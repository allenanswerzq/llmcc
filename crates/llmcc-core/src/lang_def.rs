//! Language definition framework for multi-language AST support.
//!
//! This module provides the core infrastructure for defining language-specific AST handling
//! in llmcc. It abstracts language-specific details behind the `LanguageTrait` interface
//! and provides macros for rapid language definition.
//!
//! # Architecture
//!
//! ## Language Trait
//! The [`LanguageTrait`] defines the interface that every supported language must implement:
//! - **Parsing**: Convert source code to tree-sitter parse trees
//! - **Type mapping**: Map tree-sitter kind IDs to HIR kinds
//! - **Token lookup**: Query token names and validity
//! - **Field resolution**: Get standard field IDs (name, type fields)
//! - **File extensions**: Declare supported file types
//!
//! ## Macro-Based Definition
//! The [`define_lang!`] macro enables declarative language definition:
//! ```ignore
//! define_lang!(
//!     Rust,
//!     (function_item, 0, "function_item", HirKind::Scope),
//!     (identifier, 1, "identifier", HirKind::Identifier),
//!     // ... more tokens
//! );
//! ```
//!
//! ## Visitor Pattern
//! The macro generates a language-specific visitor trait (e.g., `AstVisitorRust`)
//! that enables type-safe AST traversal with token-specific dispatch.
//!
//! # Use Cases
//!
//! - **Multi-language support**: Define once, use everywhere
//! - **Type safety**: Compile-time token ID validation
//! - **Performance**: Zero-cost abstractions via static methods
//! - **Extensibility**: Add new tokens without changing core code
//!
//! # Example Language Definition
//!
//! ```ignore
//! define_lang!(
//!     Python,
//!     (module, 0, "module", HirKind::File),
//!     (function_def, 1, "function_definition", HirKind::Scope),
//!     (class_def, 2, "class_definition", HirKind::Scope),
//!     (identifier, 3, "identifier", HirKind::Identifier, BlockKind::Definition),
//! );
//!
//! // Now you can use LangPython::parse(), LangPython::hir_kind(), etc.
//! let tree = LangPython::parse(source)?;
//! ```
//!
//! # Performance
//!
//! - Token lookup: O(1) via match expressions (branch table by compiler)
//! - Field resolution: O(1) static constants
//! - Parsing: Delegated to language-specific parser (highly optimized)
//! - Memory: Zero additional overhead per language definition

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

/// Core trait defining language-specific AST handling.
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
            /// Language context for HIR processing
            #[derive(Debug)]
            pub struct [<Lang $suffix>] {}

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

                // Supported file extensions (test default)
                pub const SUPPORTED_EXTENSIONS: &'static [&'static str] = &[];
            }

            impl $crate::lang_def::LanguageTrait for [<Lang $suffix>] {
                /// Parse source code and return a generic parse tree.
                ///
                /// First tries the custom parse_impl from LanguageTraitExt.
                /// If that returns None, falls back to tree-sitter parsing if available.
                fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
                    // Try custom parser first
                    <Self as LanguageTraitExt>::parse_impl(text.as_ref())
                }

                /// Return the list of supported file extensions for this language
                fn supported_extensions() -> &'static [&'static str] {
                    paste::paste! { [<Lang $suffix>]::SUPPORTED_EXTENSIONS }
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

            /// Trait for visiting HIR nodes with type-specific dispatch
            pub trait [<AstVisitor $suffix>]<'a, T> {
                fn unit(&self) -> $crate::context::CompileUnit<'a>;

                /// Visit a node, dispatching to the appropriate method based on token ID
                fn visit_node(&mut self, node: $crate::ir::HirNode<'a>, t: &mut T,  parent: Option<&$crate::symbol::Symbol>) {
                    match node.kind_id() {
                        $(
                            [<Lang $suffix>]::$const => paste::paste! { self.[<visit_ $const>](node, t, parent) },
                        )*
                        _ => self.visit_unknown(node, t, parent),
                    }
                }

                /// Visit all children of a node
                fn visit_children(&mut self, node: &$crate::ir::HirNode<'a>, t: &mut T, parent: Option<&$crate::symbol::Symbol>) {
                    for id in node.children() {
                        let child = self.unit().hir_node(*id);
                        self.visit_node(child, t, parent);
                    }
                }

                /// Handle unknown/unrecognized token types
                fn visit_unknown(&mut self, node: $crate::ir::HirNode<'a>, t: &mut T, parent: Option<&$crate::symbol::Symbol>) {
                    self.visit_children(&node, t, parent);
                }

                // Generate visit methods for each token type with visit_ prefix
                $(
                    paste::paste! {
                        fn [<visit_ $const>](&mut self, node: $crate::ir::HirNode<'a>, t: &mut T, parent: Option<&$crate::symbol::Symbol>) {
                            self.visit_children(&node, t, parent);
                        }
                    }
               )*
            }
        }
    };

    // Helper: expand to given block or default
    (@unwrap_block $block:expr) => { $block };
    (@unwrap_block) => { BlockKind::Undefined };
}

#[cfg(test)]
mod tests {
    use super::*;
    // Import the visitor trait generated by the macro in simple_lang
    use crate::tests::simple_lang::AstVisitorSimple;
    use crate::tests::simple_lang::LangSimple;
    use crate::context::CompileCtxt;
    use crate::ir_builder::{IrBuildConfig, build_llmcc_ir};

    #[test]
    fn test_language_define_and_visitor() {
        // Create a CompileCtxt with our simple language
        let source_code = b"fn main() {}\nfn helper() {}\nlet x = 42;";
        let sources = vec![source_code.to_vec()];
        let cc = CompileCtxt::from_sources::<LangSimple>(&sources);

        // Verify files are registered
        assert_eq!(cc.files.len(), 1);
        let file = &cc.files[0];
        let file_path = file.path().unwrap_or("<no path>");
        assert!(file_path.contains("unit_0"));

        // Verify parse tree is stored
        assert!(cc.parse_trees[0].is_some());

        // Build the IR from the parse tree
        let config = IrBuildConfig::default();
        let result = build_llmcc_ir::<LangSimple>(&cc, config);
        assert!(result.is_ok(), "IR building should succeed");

        // Create a CompileUnit from the context
        let unit = cc.compile_unit(0);
        let unit_parse_tree = unit.parse_tree();
        assert!(unit_parse_tree.is_some());

        // Get interner and test string interning
        let interned = unit.intern_str("main_function");
        let resolved = unit.resolve_interned_owned(interned);
        assert_eq!(resolved, Some("main_function".to_string()));

        // Verify HIR was built
        let file_start = unit.file_start_hir_id();
        assert!(
            file_start.is_some(),
            "File start HIR ID should be set after IR building"
        );

        // Define a visitor implementation
        struct CountingVisitor<'tcx> {
            unit: crate::context::CompileUnit<'tcx>,
            function_count: usize,
        }

        impl<'tcx> CountingVisitor<'tcx> {
            fn new(unit: crate::context::CompileUnit<'tcx>) -> Self {
                Self {
                    unit,
                    function_count: 0,
                }
            }
        }

        // Holds the scopes for visiting
        struct Collector {
            func: Vec<String>,
        }

        impl Collector {
            fn new() -> Self {
                Self { func: Vec::new() }
            }

            fn add_func(&mut self, name: String) {
                self.func.push(name);
            }
        }

        impl<'tcx> AstVisitorSimple<'tcx, Collector> for CountingVisitor<'tcx> {
            fn unit(&self) -> crate::context::CompileUnit<'tcx> {
                self.unit
            }

            fn visit_function(
                &mut self,
                _node: crate::ir::HirNode<'tcx>,
                t: &mut Collector,
                _parent: Option<&crate::symbol::Symbol>,
            ) {
                t.add_func("function_visited".to_string());
                self.function_count += 1;
            }
        }

        // Create visitor and count nodes by walking the parse tree
        let mut visitor = CountingVisitor::new(unit);

        let root = file_start.unwrap();
        let node: crate::ir::HirNode<'_> = unit.hir_node(root);
        let mut collector = Collector::new();
        visitor.visit_node(node, &mut collector, None);

        assert_eq!(visitor.function_count, 2);
        assert_eq!(collector.func.len(), 2);
    }
}
