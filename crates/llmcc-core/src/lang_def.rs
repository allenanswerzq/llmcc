//! Language definition framework for multi-language AST support.
use std::any::Any;

use crate::graph_builder::BlockKind;
use crate::ir::HirKind;

/// Generic trait for parse tree representation.
///
/// Implementations can wrap tree-sitter trees, custom ASTs, or other parse representations.
/// This abstraction decouples language definitions from specific parser implementations.
pub trait ParseTree: Send + Sync + 'static {
    /// Type-erased access to underlying tree for downcasting
    fn as_any(&self) -> &(dyn Any + Send + Sync);

    /// Debug representation
    fn debug_info(&self) -> String;

    /// Get the root ParseNode of this tree
    fn root_node(&self) -> Option<Box<dyn ParseNode + '_>> {
        None
    }
}

/// Default implementation wrapping tree-sitter Tree
#[derive(Debug, Clone)]
pub struct TreeSitterParseTree {
    pub tree: ::tree_sitter::Tree,
}

impl ParseTree for TreeSitterParseTree {
    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn debug_info(&self) -> String {
        format!("TreeSitter(root_id: {})", self.tree.root_node().id())
    }

    fn root_node(&self) -> Option<Box<dyn ParseNode + '_>> {
        Some(Box::new(TreeSitterParseNode::new(self.tree.root_node())))
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
    /// Get the node's kind ID (language-specific token ID)
    fn kind_id(&self) -> u16;

    /// Get the start byte offset of this node in the source
    fn start_byte(&self) -> usize;

    /// Get the end byte offset of this node in the source
    fn end_byte(&self) -> usize;

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

    /// Debug representation of this node
    fn debug_info(&self) -> String;

    /// Format a label for this node suitable for debugging and rendering.
    fn format_node_label(&self, field_name: Option<&str>) -> String {
        // Extract kind string from debug_info
        let debug_str = self.debug_info();
        let kind_str = if let Some(start) = debug_str.find("kind: ") {
            if let Some(end) = debug_str[start + 6..].find(',') {
                &debug_str[start + 6..start + 6 + end]
            } else if let Some(end) = debug_str[start + 6..].find(')') {
                &debug_str[start + 6..start + 6 + end]
            } else {
                "unknown"
            }
        } else {
            "unknown"
        };

        let kind_id = self.kind_id();
        let mut label = String::new();

        // Add field name if provided
        if let Some(fname) = field_name {
            label.push_str(&format!("|{}|_ ", fname));
        }

        // Add kind and kind_id
        label.push_str(&format!("{} [{}]", kind_str, kind_id));

        // Add status flags
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
    fn kind_id(&self) -> u16 {
        self.node.kind_id()
    }

    fn start_byte(&self) -> usize {
        self.node.start_byte()
    }

    fn end_byte(&self) -> usize {
        self.node.end_byte()
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

    fn child_by_field_name(&self, field_name: &str) -> Option<Box<dyn ParseNode + '_>> {
        self.node
            .child_by_field_name(field_name)
            .map(|child| Box::new(TreeSitterParseNode::new(child)) as Box<dyn ParseNode + '_>)
    }

    fn child_by_field_id(&self, _field_id: u16) -> Option<Box<dyn ParseNode + '_>> {
        None
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

    fn debug_info(&self) -> String {
        format!(
            "TreeSitterNode(kind: {}, kind_id: {}, bytes: {}..{})",
            self.node.kind(),
            self.node.kind_id(),
            self.start_byte(),
            self.end_byte()
        )
    }
}

/// Scopes trait defining language-specific AST handling.
pub trait LanguageTrait {
    /// Parse source code and return a generic parse tree.
    ///
    /// # Returns
    /// A boxed `ParseTree` trait object, allowing multiple parser implementations.
    ///
    /// # Default
    /// Returns `None` by default. Languages should implement custom parsing
    /// either by overriding this method or by using `LanguageTraitExt`.
    fn parse(_text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        None
    }

    /// Map a token kind ID to its corresponding HIR kind.
    fn hir_kind(kind_id: u16) -> HirKind;

    /// Map a token kind ID to its corresponding block kind.
    fn block_kind(kind_id: u16) -> BlockKind;

    /// Get the string representation of a token ID.
    fn token_str(kind_id: u16) -> Option<&'static str>;

    /// Validate whether a kind ID corresponds to a defined token.
    fn is_valid_token(kind_id: u16) -> bool;

    /// Get the field ID that represents the "name" of a construct.
    fn name_field() -> u16;

    /// Get the field ID that represents the "type" of a construct.
    fn type_field() -> u16;

    /// Get the list of file extensions this language supports.
    fn supported_extensions() -> &'static [&'static str];
}

/// Extension trait for providing custom parse implementations.
///
/// This trait allows languages defined via `define_lang!` macro to extend
/// with custom `parse` implementations without conflicting with the macro-generated code.
///
/// # Usage
///
/// ```ignore
/// define_lang!(MyLang, ...);
///
/// impl LanguageTraitExt for LangMyLang {
///     fn parse_impl(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
///         // Custom parser logic
///     }
/// }
/// ```
pub trait LanguageTraitExt: LanguageTrait {
    /// Custom parse implementation for this language.
    ///
    /// Languages should implement this method instead of overriding `LanguageTrait::parse`.
    /// Return `None` to fall back to tree-sitter parsing (if available).
    fn parse_impl(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>>;
}

#[allow(clippy::crate_in_macro_def)]
#[macro_export]
macro_rules! define_lang {
    (
        $suffix:ident,
        $( ($const:ident, $id:expr, $str:expr, $kind:expr $(, $block:expr)? ) ),* $(,)?
    ) => {
        $crate::paste::paste! {
            // ============================================================
            // Language Struct Definition
            // ============================================================
            /// Language context for HIR processing
            #[derive(Debug)]
            pub struct [<Lang $suffix>] {}

            // ============================================================
            // Language Constants
            // ============================================================
            #[allow(non_upper_case_globals)]
            impl [<Lang $suffix>] {
                /// Create a new Language instance
                pub fn new() -> Self {
                    Self {}
                }

                // Generate token ID constants
                $(
                    pub const $const: u16 = $id;
                )*
            }

            // ============================================================
            // Language Trait Implementation
            // ============================================================
            impl $crate::lang_def::LanguageTrait for [<Lang $suffix>] {
                /// Parse source code and return a generic parse tree.
                ///
                /// First tries the custom parse_impl from LanguageTraitExt.
                /// If that returns None, falls back to tree-sitter parsing if available.
                fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn $crate::lang_def::ParseTree>> {
                    <Self as $crate::lang_def::LanguageTraitExt>::parse_impl(text.as_ref())
                }

                /// Return the list of supported file extensions for this language
                fn supported_extensions() -> &'static [&'static str] {
                    [<Lang $suffix>]::SUPPORTED_EXTENSIONS
                }

                /// Get the HIR kind for a given token ID
                fn hir_kind(kind_id: u16) -> $crate::ir::HirKind {
                    match kind_id {
                        $(
                            Self::$const => $kind,
                        )*
                        _ => $crate::ir::HirKind::Internal,
                    }
                }

                /// Get the Block kind for a given token ID
                fn block_kind(kind_id: u16) -> $crate::graph_builder::BlockKind {
                    match kind_id {
                        $(
                            Self::$const => define_lang!(@unwrap_block $($block)?),
                        )*
                        _ => $crate::graph_builder::BlockKind::Undefined,
                    }
                }

                /// Get the string representation of a token ID
                fn token_str(kind_id: u16) -> Option<&'static str> {
                    match kind_id {
                        $(
                            Self::$const => Some($str),
                        )*
                        _ => None,
                    }
                }

                /// Check if a token ID is valid
                fn is_valid_token(kind_id: u16) -> bool {
                    matches!(kind_id, $(Self::$const)|*)
                }

                fn name_field() -> u16 {
                    Self::field_name
                }

                fn type_field() -> u16 {
                    Self::field_type
                }
            }

            // ============================================================
            // Visitor Trait Definition
            // ============================================================
            /// Trait for visiting HIR nodes with type-specific dispatch
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
                                tracing::trace!("run: visit_{}", stringify!($const));
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
                    for id in node.children() {
                        let child = unit.hir_node(*id);
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

    // ================================================================
    // Helper Rules
    // ================================================================
    (@unwrap_block $block:expr) => { $block };
    (@unwrap_block) => { $crate::graph_builder::BlockKind::Undefined };
}