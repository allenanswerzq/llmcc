//! Language definition framework for multi-language AST support.

use crate::Result;
use crate::context::{CompileCtxt, CompileUnit};
use crate::graph_builder::BlockKind;
use crate::ir::HirKind;
use crate::ir::HirNode;
use crate::resolve::ResolveOptions;
use crate::scope::{Scope, ScopeStack};

/// Sentinel used when a parse node has no parent field.
pub const NO_FIELD_ID: u16 = u16::MAX;

/// A parse child paired with its parent field id.
pub struct ParseChild<'a> {
    /// Child parse node.
    pub node: Box<dyn ParseNode + 'a>,
    /// Parent field id, or `NO_FIELD_ID` when absent.
    pub field_id: u16,
}

/// Generic trait for parse tree representation.
///
/// Implementations can wrap tree-sitter trees, custom ASTs, or other parse representations.
/// This abstraction decouples language definitions from specific parser implementations.
pub trait ParseTree: Send + Sync + 'static {
    /// Root parse node.
    fn root(&self) -> Box<dyn ParseNode + '_>;

    /// Human-readable parser/backend description.
    fn debug_label(&self) -> String;
}

/// Default implementation wrapping tree-sitter Tree
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

/// Generic trait for parse tree nodes (individual AST nodes).
///
/// Implementations can wrap tree-sitter nodes, custom AST nodes, or other parse representations.
/// This abstraction allows IR building to work with any parser backend.
///
/// Note: Unlike ParseTree, ParseNode can have lifetime parameters to match the lifetime
/// of the underlying parser's borrowed nodes (e.g., tree-sitter::Node<'tree>).
pub trait ParseNode: Send + Sync {
    /// Get the node's kind name.
    fn kind_name(&self) -> &str;

    /// Get the node's kind ID (language-specific token ID)
    fn kind_id(&self) -> u16;

    /// Get the start byte offset of this node in the source
    fn start_byte(&self) -> usize;

    /// Get the end byte offset of this node in the source
    fn end_byte(&self) -> usize;

    /// Get the 1-indexed line number where this node starts
    fn start_line(&self) -> usize;

    /// Get the number of children this node has
    fn child_count(&self) -> usize;

    /// Get the child at the specified index
    fn child(&self, index: usize) -> Option<Box<dyn ParseNode + '_>>;

    /// Get the field name of the child at the specified index (if available)
    fn child_field_name(&self, _index: usize) -> Option<&str> {
        None
    }

    /// Get the field ID of this node within its parent (if available).
    /// Returns None if the node has no parent or the field ID cannot be determined.
    fn field_id(&self) -> Option<u16> {
        None
    }

    /// Collect all children with their field IDs in a single pass.
    /// This is more efficient than calling child() + field_id() separately
    /// because it uses a cursor to get field_id during iteration.
    ///
    /// Default implementation falls back to child() + field_id() for each child.
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

    /// Get a child by field name (if supported by the parser)
    fn child_by_field_name(&self, field_name: &str) -> Option<Box<dyn ParseNode + '_>>;

    /// Get a child by field ID (if supported by the parser)
    fn child_by_field_id(&self, _field_id: u16) -> Option<Box<dyn ParseNode + '_>> {
        None
    }

    /// Check if this node represents a parse error
    fn is_error(&self) -> bool {
        false
    }

    /// Check if this node is "extra" (typically whitespace/comments)
    fn is_extra(&self) -> bool {
        false
    }

    /// Check if this node is missing (e.g., implicit tokens)
    fn is_missing(&self) -> bool {
        false
    }

    /// Check if this node is a named token (vs anonymous)
    fn is_named(&self) -> bool {
        true
    }

    /// Get the parent node if available
    fn parent(&self) -> Option<Box<dyn ParseNode + '_>> {
        None
    }

    /// Debug representation of this node.
    fn debug_label(&self) -> String;

    /// Format a label for this node suitable for debugging and rendering.
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

/// Wrapper implementation of ParseNode for tree-sitter nodes
pub struct TreeSitterParseNode<'tree> {
    node: ::tree_sitter::Node<'tree>,
}

impl<'tree> TreeSitterParseNode<'tree> {
    /// Create a new wrapper around a tree-sitter node
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
        // tree-sitter's start_position().row is 0-indexed, add 1 for 1-indexed line
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
        // Walk up to parent and find this node's field ID
        // NOTE: This is O(n) per call - prefer children_with_fields for bulk access.
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

/// Static language contract used by the core pipeline.
pub trait Language {
    /// Get the manifest file name for this language (e.g., "Cargo.toml", "package.json").
    fn manifest_name() -> &'static str;

    /// Get the container directories that don't add semantic meaning.
    /// These directories are skipped in module detection (e.g., "src", "lib").
    fn container_dirs() -> &'static [&'static str];

    /// Check if a directory name is a container directory.
    fn is_container(name: &str) -> bool {
        Self::container_dirs().contains(&name)
    }

    /// Parse source code and return a generic parse tree.
    fn parse(_text: impl AsRef<[u8]>) -> Result<Box<dyn ParseTree>>;

    /// Map a token kind ID to its corresponding HIR kind.
    fn hir_kind(kind_id: u16) -> HirKind;

    /// Map a token kind ID to its corresponding block kind.
    fn block_kind(kind_id: u16) -> BlockKind;

    /// Map a token kind ID to its corresponding block kind with parent context.
    /// This allows languages to create blocks based on the parent node's kind.
    /// For example, types inside tuple struct definitions become Field blocks.
    /// Default implementation ignores parent and delegates to block_kind.
    fn block_kind_with_parent(kind_id: u16, field_id: u16, _parent_kind_id: u16) -> BlockKind {
        let field_kind = Self::block_kind(field_id);
        if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            Self::block_kind(kind_id)
        }
    }

    /// Check if a parse node is a test-related attribute that should cause the next item to be skipped.
    /// This is used to filter out test functions and modules from the HIR at build time.
    /// Takes the parse node and source bytes to extract and check the attribute text.
    /// Default implementation returns false (no filtering).
    fn is_test_attribute(node: &dyn ParseNode, source: &[u8]) -> bool {
        let _ = (node, source);
        false
    }

    /// Get the string representation of a token ID.
    fn token_str(kind_id: u16) -> Option<&'static str>;

    /// Validate whether a kind ID corresponds to a defined token.
    fn is_valid_token(kind_id: u16) -> bool;

    /// Get the field ID that represents the "name" of a construct.
    fn name_field() -> u16;

    /// Get the field ID that represents the "type" of a construct.
    fn type_field() -> u16;

    /// Get the field ID that represents the "trait" in impl blocks.
    /// Used for `impl Trait for Type { }` to identify the trait being implemented.
    fn trait_field() -> u16;

    /// Get the list of file extensions this language supports.
    fn extensions() -> &'static [&'static str];

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

/// Implementation hooks used by `define_lang!` to build a `Language` impl.
pub trait LanguageHooks: Language {
    /// Parse source bytes for this language.
    fn parse_source(text: impl AsRef<[u8]>) -> Result<Box<dyn ParseTree>>;

    /// Supported file extensions for this language.
    fn file_extensions() -> &'static [&'static str];

    /// The manifest file name for this language (e.g., "Cargo.toml", "package.json").
    fn manifest_file() -> &'static str;

    /// Container directories that don't add semantic meaning (e.g., "src", "lib").
    fn container_dirs() -> &'static [&'static str];

    /// Language-specific block kind with parent context.
    /// Override this to handle context-dependent block creation.
    /// Default implementation delegates to the trait's default.
    fn block_kind_for_child(kind_id: u16, field_id: u16, _parent_kind_id: u16) -> BlockKind {
        // Default: use the trait's default implementation
        let field_kind = Self::block_kind(field_id);
        if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            Self::block_kind(kind_id)
        }
    }

    fn initial_scopes<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> ScopeStack<'tcx> {
        ScopeStack::new(cc.arena(), &cc.interner)
    }

    /// Check if a parse node is a test attribute that should cause the next item to be skipped.
    /// Override this for language-specific test attribute detection.
    /// Default implementation returns false.
    fn is_test_attribute(node: &dyn ParseNode, source: &[u8]) -> bool {
        let _ = (node, source);
        false
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
                fn manifest_name() -> &'static str {
                    <Self as $crate::lang_def::LanguageHooks>::manifest_file()
                }

                fn container_dirs() -> &'static [&'static str] {
                    <Self as $crate::lang_def::LanguageHooks>::container_dirs()
                }

                fn parse(text: impl AsRef<[u8]>) -> $crate::Result<Box<dyn $crate::lang_def::ParseTree>> {
                    <Self as $crate::lang_def::LanguageHooks>::parse_source(text.as_ref())
                }

                fn collect_init<'tcx>(cc: &'tcx $crate::context::CompileCtxt<'tcx>) -> $crate::scope::ScopeStack<'tcx> {
                    <Self as $crate::lang_def::LanguageHooks>::initial_scopes(cc)
                }

                fn collect_symbols<'tcx>(
                    unit: $crate::context::CompileUnit<'tcx>,
                    node: $crate::ir::HirNode<'tcx>,
                    scope_stack: $crate::scope::ScopeStack<'tcx>,
                    options: &$crate::resolve::ResolveOptions,
                ) -> &'tcx $crate::scope::Scope<'tcx> {
                    <Self as $crate::lang_def::LanguageHooks>::collect_symbols(unit, node, scope_stack, options)
                }

                fn bind_symbols<'tcx>(
                    unit: $crate::context::CompileUnit<'tcx>,
                    node: $crate::ir::HirNode<'tcx>,
                    globals: &'tcx $crate::scope::Scope<'tcx>,
                    options: &$crate::resolve::ResolveOptions,
                ) {
                    <Self as $crate::lang_def::LanguageHooks>::bind_symbols(unit, node, globals, options);
                }

                fn extensions() -> &'static [&'static str] {
                    <Self as $crate::lang_def::LanguageHooks>::file_extensions()
                }

                fn hir_kind(kind_id: u16) -> $crate::ir::HirKind {
                    match kind_id {
                        $(
                            Self::$const => $kind,
                        )*
                        _ => $crate::ir::HirKind::Internal,
                    }
                }

                fn block_kind(kind_id: u16) -> $crate::graph_builder::BlockKind {
                    match kind_id {
                        $(
                            Self::$const => define_lang!(@unwrap_block $($block)?),
                        )*
                        _ => $crate::graph_builder::BlockKind::Undefined,
                    }
                }

                fn block_kind_with_parent(kind_id: u16, field_id: u16, parent_kind_id: u16) -> $crate::graph_builder::BlockKind {
                    <Self as $crate::lang_def::LanguageHooks>::block_kind_for_child(kind_id, field_id, parent_kind_id)
                }

                fn is_test_attribute(node: &dyn $crate::lang_def::ParseNode, source: &[u8]) -> bool {
                    <Self as $crate::lang_def::LanguageHooks>::is_test_attribute(node, source)
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
                /// Visit a node, dispatching to the appropriate method based on token ID
                /// NOTE: scope stack is for lookup convenience, the actual namespace in
                /// which names should be mangled and declared.
                /// So namespace is semantic home scope for name resolution/mangling,
                /// independent of the push stack.
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

                /// Visit all children of a node
                fn visit_children(
                    &mut self,
                    unit: &$crate::context::CompileUnit<'a>,
                    node: &$crate::ir::HirNode<'a>,
                    scopes: &mut T,
                    namespace: &'a $crate::scope::Scope<'a>,
                    parent: Option<&$crate::symbol::Symbol>,
                ) {
                    // Iterate directly over child IDs to avoid Vec/SmallVec allocation
                    for &child_id in node.child_ids() {
                        let child = unit.hir_node(child_id);
                        self.visit_node(unit, &child, scopes, namespace, parent);
                    }
                }

                /// Handle unknown/unrecognized token types
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

                // Generate visit methods for each token type with visit_ prefix
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
    (@unwrap_block) => { $crate::graph_builder::BlockKind::Undefined };
}
