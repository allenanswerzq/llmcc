//! Language and parser abstractions used by the core pipeline.

use crate::Result;
use crate::block::BlockKind;
use crate::context::{CompileCtxt, CompileUnit};
use crate::ir::HirKind;
use crate::ir::HirNode;
use crate::resolve::ResolveOptions;
use crate::scope::{Scope, ScopeStack};

/// Field id used when a parse child has no named parent field.
pub const NO_FIELD_ID: u16 = u16::MAX;

/// Parse child plus the field id assigned by its parent.
pub struct ParseChild<'a> {
    /// Child node.
    pub node: Box<dyn ParseNode + 'a>,
    /// Parent field id, or `NO_FIELD_ID`.
    pub field_id: u16,
}

/// Language-specific lowering decision for a parse node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBuildAction {
    /// Lower this node normally.
    Build,
    /// Omit this node.
    Skip,
    /// Omit this node and its next sibling.
    SkipNextSibling,
}

/// Parser-independent source tree.
pub trait ParseTree: Send + Sync + 'static {
    /// Root node.
    fn root(&self) -> Box<dyn ParseNode + '_>;

    /// Short diagnostic label.
    fn debug_label(&self) -> String;
}

/// `tree-sitter` tree wrapper.
#[derive(Debug, Clone)]
pub struct TreeSitterParseTree {
    tree: ::tree_sitter::Tree,
}

impl TreeSitterParseTree {
    pub fn new(tree: ::tree_sitter::Tree) -> Self {
        Self { tree }
    }
}

impl ParseTree for TreeSitterParseTree {
    fn root(&self) -> Box<dyn ParseNode + '_> {
        Box::new(TreeSitterParseNode::new(self.tree.root_node()))
    }

    fn debug_label(&self) -> String {
        format!("TreeSitter(root_id: {})", self.tree.root_node().id())
    }
}

/// Parser-independent source node.
pub trait ParseNode: Send + Sync {
    /// Node kind name.
    fn kind_name(&self) -> &str;

    /// Language-specific token id.
    fn kind_id(&self) -> u16;

    /// Start byte in the source file.
    fn start_byte(&self) -> usize;

    /// End byte in the source file.
    fn end_byte(&self) -> usize;

    /// 1-indexed starting line.
    fn start_line(&self) -> usize;

    /// Number of child nodes.
    fn child_count(&self) -> usize;

    /// Child at `index`.
    fn child(&self, index: usize) -> Option<Box<dyn ParseNode + '_>>;

    /// Field name for the child at `index`.
    fn child_field_name(&self, _index: usize) -> Option<&str> {
        None
    }

    /// Field id assigned by this node's parent.
    fn field_id(&self) -> Option<u16> {
        None
    }

    /// Children paired with parent field ids.
    fn children_with_fields(&self) -> Vec<ParseChild<'_>> {
        let mut result = Vec::with_capacity(self.child_count());
        for i in 0..self.child_count() {
            if let Some(child) = self.child(i) {
                let field_id = child.field_id().unwrap_or(NO_FIELD_ID);
                result.push(ParseChild {
                    node: child,
                    field_id,
                });
            }
        }
        result
    }

    /// Child for a parser field name.
    fn child_by_field_name(&self, field_name: &str) -> Option<Box<dyn ParseNode + '_>>;

    /// Child for a parser field id.
    fn child_by_field_id(&self, _field_id: u16) -> Option<Box<dyn ParseNode + '_>> {
        None
    }

    /// True for parser error nodes.
    fn is_error(&self) -> bool {
        false
    }

    /// True for parser extras such as comments or whitespace.
    fn is_extra(&self) -> bool {
        false
    }

    /// True for parser-inserted missing nodes.
    fn is_missing(&self) -> bool {
        false
    }

    /// True for named parser nodes.
    fn is_named(&self) -> bool {
        true
    }

    /// Parent node, if available.
    fn parent(&self) -> Option<Box<dyn ParseNode + '_>> {
        None
    }

    /// Short diagnostic label.
    fn debug_label(&self) -> String;

    /// Label used by AST debug rendering.
    fn label(&self, field_name: Option<&str>) -> String {
        let kind_id = self.kind_id();
        let mut label = String::new();

        if let Some(fname) = field_name {
            label.push_str(&format!("|{fname}|_ "));
        }

        label.push_str(&format!("{} [{kind_id}]", self.kind_name()));

        if self.is_error() {
            label.push_str(" [ERROR]");
        } else if self.is_extra() {
            label.push_str(" [EXTRA]");
        } else if self.is_missing() {
            label.push_str(" [MISSING]");
        }

        label
    }
}

/// `tree-sitter` node wrapper.
pub struct TreeSitterParseNode<'tree> {
    node: ::tree_sitter::Node<'tree>,
}

impl<'tree> TreeSitterParseNode<'tree> {
    /// Wrap a `tree-sitter` node.
    pub fn new(node: ::tree_sitter::Node<'tree>) -> Self {
        Self { node }
    }
}

impl<'tree> ParseNode for TreeSitterParseNode<'tree> {
    fn kind_name(&self) -> &str {
        self.node.kind()
    }

    fn kind_id(&self) -> u16 {
        self.node.kind_id()
    }

    fn start_byte(&self) -> usize {
        self.node.start_byte()
    }

    fn end_byte(&self) -> usize {
        self.node.end_byte()
    }

    fn start_line(&self) -> usize {
        // tree-sitter rows are zero-based.
        self.node.start_position().row + 1
    }

    fn child_count(&self) -> usize {
        self.node.child_count()
    }

    fn child(&self, index: usize) -> Option<Box<dyn ParseNode + '_>> {
        self.node
            .child(index)
            .map(|child| Box::new(TreeSitterParseNode::new(child)) as Box<dyn ParseNode + '_>)
    }

    fn child_field_name(&self, index: usize) -> Option<&str> {
        self.node.field_name_for_child(index as u32)
    }

    fn field_id(&self) -> Option<u16> {
        let parent = self.node.parent()?;
        let mut cursor = parent.walk();

        if !cursor.goto_first_child() {
            return None;
        }

        loop {
            if cursor.node().id() == self.node.id() {
                return cursor.field_id().map(|id| id.get());
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }

        None
    }

    fn children_with_fields(&self) -> Vec<ParseChild<'_>> {
        let mut result = Vec::with_capacity(self.node.child_count());
        let mut cursor = self.node.walk();

        if !cursor.goto_first_child() {
            return result;
        }

        loop {
            let child_node = cursor.node();
            let field_id = cursor.field_id().map(|id| id.get()).unwrap_or(NO_FIELD_ID);
            result.push(ParseChild {
                node: Box::new(TreeSitterParseNode::new(child_node)),
                field_id,
            });

            if !cursor.goto_next_sibling() {
                break;
            }
        }

        result
    }

    fn child_by_field_name(&self, field_name: &str) -> Option<Box<dyn ParseNode + '_>> {
        self.node
            .child_by_field_name(field_name)
            .map(|child| Box::new(TreeSitterParseNode::new(child)) as Box<dyn ParseNode + '_>)
    }

    fn child_by_field_id(&self, field_id: u16) -> Option<Box<dyn ParseNode + '_>> {
        self.node
            .child_by_field_id(field_id)
            .map(|child| Box::new(TreeSitterParseNode::new(child)) as Box<dyn ParseNode + '_>)
    }

    fn is_error(&self) -> bool {
        self.node.is_error()
    }

    fn is_extra(&self) -> bool {
        self.node.is_extra()
    }

    fn is_missing(&self) -> bool {
        self.node.is_missing()
    }

    fn is_named(&self) -> bool {
        self.node.is_named()
    }

    fn parent(&self) -> Option<Box<dyn ParseNode + '_>> {
        self.node
            .parent()
            .map(|parent| Box::new(TreeSitterParseNode::new(parent)) as Box<dyn ParseNode + '_>)
    }

    fn debug_label(&self) -> String {
        format!(
            "TreeSitterNode(kind: {}, kind_id: {}, bytes: {}..{})",
            self.node.kind(),
            self.node.kind_id(),
            self.start_byte(),
            self.end_byte()
        )
    }
}

/// Supported source languages.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, strum_macros::Display, strum_macros::EnumString,
)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum SupportedLang {
    Rust,
    #[strum(serialize = "typescript", serialize = "ts")]
    Typescript,
    #[strum(serialize = "cpp", serialize = "c++", serialize = "c")]
    Cpp,
    #[strum(serialize = "csharp", serialize = "cs", serialize = "c#")]
    CSharp,
    Go,
    Java,
    #[strum(serialize = "javascript", serialize = "js")]
    JavaScript,
    #[strum(serialize = "python", serialize = "py")]
    Python,
    Auto,
}

impl SupportedLang {
    /// Manifest file names recognized for this language.
    pub fn manifest_names(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["Cargo.toml"],
            Self::Typescript | Self::JavaScript => &["package.json"],
            Self::Cpp => &["CMakeLists.txt"],
            Self::CSharp => &["project.csproj"],
            Self::Go => &["go.mod"],
            Self::Java => &["pom.xml", "build.gradle"],
            Self::Python => &["pyproject.toml", "setup.py"],
            Self::Auto => &[
                "Cargo.toml",
                "package.json",
                "CMakeLists.txt",
                "go.mod",
                "pom.xml",
                "pyproject.toml",
                "project.csproj",
            ],
        }
    }

    /// Source file extensions recognized for this language.
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["rs"],
            Self::Typescript => &["ts", "mts", "cts"],
            Self::Cpp => &[
                "c", "h", "cpp", "hpp", "cc", "hh", "cxx", "hxx", "c++", "h++", "C", "H", "ipp",
                "inl", "tpp",
            ],
            Self::CSharp => &["cs"],
            Self::Go => &["go"],
            Self::Java => &["java"],
            Self::JavaScript => &["js", "mjs", "cjs"],
            Self::Python => &["py", "pyi"],
            Self::Auto => &[
                "rs", "ts", "mts", "cts", "c", "h", "cpp", "hpp", "cc", "hh", "cxx", "hxx", "c++",
                "h++", "C", "H", "ipp", "inl", "tpp", "cs", "go", "java", "js", "mjs", "cjs", "py",
                "pyi",
            ],
        }
    }

    /// Directory names ignored when deriving module paths for this language.
    pub fn container_dirs(self) -> &'static [&'static str] {
        match self {
            Self::Rust => &["src"],
            Self::Typescript | Self::JavaScript => {
                &["src", "lib", "dist", "build", "out", "source"]
            }
            Self::Cpp => &[
                "src", "source", "sources", "lib", "include", "inc", "headers",
            ],
            Self::CSharp => &["src", "source"],
            Self::Go => &["cmd", "internal", "pkg"],
            Self::Java => &["src", "main", "java"],
            Self::Python => &["src", "lib", "app", "python"],
            Self::Auto => &[
                "src", "lib", "dist", "build", "out", "source", "sources", "include", "inc",
                "headers", "cmd", "internal", "pkg", "main", "java", "app", "python",
            ],
        }
    }
}

/// Public language contract consumed by the pipeline.
pub trait Language {
    /// Supported source language.
    fn supported_lang() -> SupportedLang;

    /// Parse source bytes.
    fn parse(_text: impl AsRef<[u8]>) -> Result<Box<dyn ParseTree>>;

    /// Map parser token id to HIR kind.
    fn hir_kind(kind_id: u16) -> HirKind;

    /// Map parser token id to graph block kind.
    fn block_kind(kind_id: u16) -> BlockKind;

    /// Map parser token id to graph block kind with parent context.
    fn block_kind_with_parent(kind_id: u16, field_id: u16, _parent_kind_id: u16) -> BlockKind {
        let field_kind = Self::block_kind(field_id);
        if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            Self::block_kind(kind_id)
        }
    }

    /// Graph block kind for a HIR node, preferring field-id mappings over node-kind mappings.
    fn block_kind_for_node(node: HirNode<'_>) -> BlockKind {
        let field_kind = Self::block_kind(node.field_id());
        if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            Self::block_kind(node.kind_id())
        }
    }

    /// Materialized graph block kind for a HIR node.
    fn try_block_kind_for_node(node: HirNode<'_>) -> Option<BlockKind> {
        let kind = Self::block_kind_for_node(node);
        kind.is_materialized().then_some(kind)
    }

    /// Materialized graph block kind for a HIR child with parent context.
    fn try_block_kind_in_parent(child: HirNode<'_>, parent: HirNode<'_>) -> Option<BlockKind> {
        let kind =
            Self::block_kind_with_parent(child.kind_id(), child.field_id(), parent.kind_id());
        kind.is_materialized().then_some(kind)
    }

    /// Decide how the HIR builder handles a parse node.
    fn hir_build_action(node: &dyn ParseNode, source: &[u8]) -> HirBuildAction {
        let _ = (node, source);
        HirBuildAction::Build
    }

    /// Parser token text for `kind_id`.
    fn token_str(kind_id: u16) -> Option<&'static str>;

    /// True when `kind_id` is a known parser token.
    fn is_valid_token(kind_id: u16) -> bool;

    /// Field id for declaration names.
    fn name_field() -> u16;

    /// Field id for type annotations or type references.
    fn type_field() -> u16;

    /// Field id for implemented traits or interfaces.
    fn trait_field() -> u16;

    fn collect_init<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> ScopeStack<'tcx>;

    fn collect_symbols<'tcx>(
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        scope_stack: ScopeStack<'tcx>,
        options: &ResolveOptions,
    ) -> &'tcx Scope<'tcx>;

    fn bind_symbols<'tcx>(
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        globals: &'tcx Scope<'tcx>,
        options: &ResolveOptions,
    );
}

/// Manual language definition consumed by `define_lang!`.
///
/// The macro generates token-derived `Language` methods. Language crates
/// implement this trait for parsing, project layout, filtering, and symbol
/// passes.
pub trait LanguageDefinition {
    /// Supported source language.
    fn supported_lang() -> SupportedLang;

    /// Parse source bytes.
    fn parse_source(text: impl AsRef<[u8]>) -> Result<Box<dyn ParseTree>>;

    /// Context-specific block override.
    ///
    /// Return `None` to use the generated field/node default. Return
    /// `Some(BlockKind::Undefined)` to intentionally suppress block creation.
    fn block_kind_for_child(
        _kind_id: u16,
        _field_id: u16,
        _parent_kind_id: u16,
    ) -> Option<BlockKind> {
        None
    }

    fn initial_scopes<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> ScopeStack<'tcx> {
        ScopeStack::new(cc.arena(), cc.interner())
    }

    fn hir_build_action(node: &dyn ParseNode, source: &[u8]) -> HirBuildAction {
        let _ = (node, source);
        HirBuildAction::Build
    }

    fn collect_symbols<'tcx>(
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        scope_stack: ScopeStack<'tcx>,
        options: &ResolveOptions,
    ) -> &'tcx Scope<'tcx>;

    fn bind_symbols<'tcx>(
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        globals: &'tcx Scope<'tcx>,
        options: &ResolveOptions,
    );
}

#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! define_lang {
    (
        $suffix:ident,
        $( ($const:ident, $id:expr, $str:expr, $kind:expr $(, $block:expr)? ) ),* $(,)?
    ) => {
        $crate::paste::paste! {
            #[derive(Debug, Clone, Copy, Default)]
            pub struct [<Lang $suffix>];

            #[allow(non_upper_case_globals)]
            impl [<Lang $suffix>] {
                pub const fn new() -> Self {
                    Self
                }

                $(
                    pub const $const: u16 = $id;
                )*
            }

            impl $crate::lang_def::Language for [<Lang $suffix>] {
                fn supported_lang() -> $crate::lang_def::SupportedLang {
                    <Self as $crate::lang_def::LanguageDefinition>::supported_lang()
                }

                fn parse(text: impl AsRef<[u8]>) -> $crate::Result<Box<dyn $crate::lang_def::ParseTree>> {
                    <Self as $crate::lang_def::LanguageDefinition>::parse_source(text.as_ref())
                }

                fn collect_init<'tcx>(cc: &'tcx $crate::context::CompileCtxt<'tcx>) -> $crate::scope::ScopeStack<'tcx> {
                    <Self as $crate::lang_def::LanguageDefinition>::initial_scopes(cc)
                }

                fn collect_symbols<'tcx>(
                    unit: $crate::context::CompileUnit<'tcx>,
                    node: $crate::ir::HirNode<'tcx>,
                    scope_stack: $crate::scope::ScopeStack<'tcx>,
                    options: &$crate::resolve::ResolveOptions,
                ) -> &'tcx $crate::scope::Scope<'tcx> {
                    <Self as $crate::lang_def::LanguageDefinition>::collect_symbols(unit, node, scope_stack, options)
                }

                fn bind_symbols<'tcx>(
                    unit: $crate::context::CompileUnit<'tcx>,
                    node: $crate::ir::HirNode<'tcx>,
                    globals: &'tcx $crate::scope::Scope<'tcx>,
                    options: &$crate::resolve::ResolveOptions,
                ) {
                    <Self as $crate::lang_def::LanguageDefinition>::bind_symbols(unit, node, globals, options);
                }

                fn hir_kind(kind_id: u16) -> $crate::ir::HirKind {
                    match kind_id {
                        $(
                            Self::$const => $kind,
                        )*
                        _ => $crate::ir::HirKind::Internal,
                    }
                }

                fn block_kind(kind_id: u16) -> $crate::block::BlockKind {
                    match kind_id {
                        $(
                            Self::$const => define_lang!(@unwrap_block $($block)?),
                        )*
                        _ => $crate::block::BlockKind::Undefined,
                    }
                }

                fn block_kind_with_parent(kind_id: u16, field_id: u16, parent_kind_id: u16) -> $crate::block::BlockKind {
                    if let Some(kind) = <Self as $crate::lang_def::LanguageDefinition>::block_kind_for_child(kind_id, field_id, parent_kind_id) {
                        kind
                    } else {
                        let field_kind = Self::block_kind(field_id);
                        if field_kind != $crate::block::BlockKind::Undefined {
                            field_kind
                        } else {
                            Self::block_kind(kind_id)
                        }
                    }
                }

                fn hir_build_action(node: &dyn $crate::lang_def::ParseNode, source: &[u8]) -> $crate::lang_def::HirBuildAction {
                    <Self as $crate::lang_def::LanguageDefinition>::hir_build_action(node, source)
                }

                fn token_str(kind_id: u16) -> Option<&'static str> {
                    match kind_id {
                        $(
                            Self::$const => Some($str),
                        )*
                        _ => None,
                    }
                }

                fn is_valid_token(kind_id: u16) -> bool {
                    matches!(kind_id, $(Self::$const)|*)
                }

                fn name_field() -> u16 {
                    Self::field_name
                }

                fn type_field() -> u16 {
                    Self::field_type
                }

                fn trait_field() -> u16 {
                    Self::field_trait
                }
            }

            pub trait [<AstVisitor $suffix>]<'a, T> {
                /// Visit a node and dispatch by token id.
                fn visit_node(
                    &mut self,
                    unit: &$crate::context::CompileUnit<'a>,
                    node: &$crate::ir::HirNode<'a>,
                    scopes: &mut T,
                    namespace: &'a $crate::scope::Scope<'a>,
                    parent: Option<&$crate::symbol::Symbol>,
                ) {
                    match node.kind_id() {
                        $(
                            [<Lang $suffix>]::$const => $crate::paste::paste! {{
                                self.[<visit_ $const>](unit, node, scopes, namespace, parent)
                            }},
                        )*
                        _ => self.visit_unknown(unit, node, scopes, namespace, parent),
                    }
                }

                /// Visit all children.
                fn visit_children(
                    &mut self,
                    unit: &$crate::context::CompileUnit<'a>,
                    node: &$crate::ir::HirNode<'a>,
                    scopes: &mut T,
                    namespace: &'a $crate::scope::Scope<'a>,
                    parent: Option<&$crate::symbol::Symbol>,
                ) {
                    for &child_id in node.child_ids() {
                        let child = unit.hir_node(child_id);
                        self.visit_node(unit, &child, scopes, namespace, parent);
                    }
                }

                /// Default handler for unrecognized token ids.
                fn visit_unknown(
                    &mut self,
                    unit: &$crate::context::CompileUnit<'a>,
                    node: &$crate::ir::HirNode<'a>,
                    scopes: &mut T,
                    namespace: &'a $crate::scope::Scope<'a>,
                    parent: Option<&$crate::symbol::Symbol>,
                ) {
                    self.visit_children(unit, node, scopes, namespace, parent);
                }

                $(
                    $crate::paste::paste! {
                        fn [<visit_ $const>](
                            &mut self,
                            unit: &$crate::context::CompileUnit<'a>,
                            node: &$crate::ir::HirNode<'a>,
                            scopes: &mut T,
                            namespace: &'a $crate::scope::Scope<'a>,
                            parent: Option<&$crate::symbol::Symbol>,
                        ) {
                            self.visit_children(unit, node, scopes, namespace, parent);
                        }
                    }
                )*
            }
        }
    };

    (@unwrap_block $block:expr) => { $block };
    (@unwrap_block) => { $crate::block::BlockKind::Undefined };
}
