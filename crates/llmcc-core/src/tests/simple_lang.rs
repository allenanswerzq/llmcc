//! Simple test language for llmcc testing.
//!
//! Provides a minimal language implementation with custom parser for use in tests.
//! Can be used in any test file by importing from the common test module.

use crate::graph_builder::BlockKind;
use crate::ir::HirKind;
use crate::lang_def::{LanguageTrait, LanguageTraitExt, ParseNode, ParseTree};
use std::any::Any;

// ============================================================================
// PART 1: Define the Simple Language Using the Macro
// ============================================================================

crate::define_lang!(
    Simple,
    (module, 0, "module", HirKind::File),
    (function, 1, "function", HirKind::Scope, BlockKind::Func),
    (identifier, 2, "identifier", HirKind::Identifier),
    (statement, 3, "statement", HirKind::Scope),
    (field_name, 10, "field_name", HirKind::Identifier),
    (field_type, 11, "field_type", HirKind::Identifier),
);

// Define supported extensions for the Simple language
impl LangSimple {
    pub const SUPPORTED_EXTENSIONS: &'static [&'static str] = &["simple"];
}

// ============================================================================
// PART 2: Simple Custom Parse Node Implementation
// ============================================================================

/// Simple custom AST node representation for testing.
///
/// This is a minimal implementation suitable for tests that don't require
/// actual language features but do need a parse tree structure.
#[derive(Debug, Clone)]
pub struct SimpleParseNode {
    pub kind_id: u16,
    pub text: String,
    pub children: Vec<SimpleParseNode>,
}

impl ParseNode for SimpleParseNode {
    fn kind_id(&self) -> u16 {
        self.kind_id
    }

    fn start_byte(&self) -> usize {
        0 // Simplified for example
    }

    fn end_byte(&self) -> usize {
        self.text.len()
    }

    fn child_count(&self) -> usize {
        self.children.len()
    }

    fn child(&self, index: usize) -> Option<Box<dyn ParseNode + '_>> {
        self.children
            .get(index)
            .map(|child| Box::new(child.clone()) as Box<dyn ParseNode + '_>)
    }

    fn child_by_field_name(&self, _field_name: &str) -> Option<Box<dyn ParseNode + '_>> {
        None // Simplified for example
    }

    fn debug_info(&self) -> String {
        format!(
            "SimpleParseNode(kind_id: {}, text: {})",
            self.kind_id, self.text
        )
    }
}

// ============================================================================
// PART 3: Simple Custom Parse Tree Implementation
// ============================================================================

/// Custom parse tree for simple language.
///
/// Wraps the simple AST structure for IR building in tests.
pub struct SimpleParseTree {
    pub root: SimpleParseNode,
}

impl ParseTree for SimpleParseTree {
    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn debug_info(&self) -> String {
        format!(
            "SimpleParse(kind: {}, text_len: {})",
            self.root.kind_id,
            self.root.text.len()
        )
    }

    fn root_node(&self) -> Option<Box<dyn ParseNode + '_>> {
        Some(Box::new(self.root.clone()))
    }
}

// ============================================================================
// PART 4: Simple Parser Implementation
// ============================================================================

/// Very simple parser: tokenize by lines and basic keywords.
///
/// This parser recognizes:
/// - "fn " prefix as function definitions
/// - Non-empty, non-comment lines as statements
/// - "//" prefix as comments (ignored)
mod simple_parser {
    use super::*;

    pub fn parse(source: &[u8]) -> Option<SimpleParseNode> {
        let text = std::str::from_utf8(source).ok()?;
        let mut children = Vec::new();

        // Parse "fn" keyword lines as functions
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("fn ") {
                let func_name = trimmed.split('(').next().unwrap_or("").replace("fn ", "");

                children.push(SimpleParseNode {
                    kind_id: LangSimple::function,
                    text: trimmed.to_string(),
                    children: vec![SimpleParseNode {
                        kind_id: LangSimple::identifier,
                        text: func_name,
                        children: Vec::new(),
                    }],
                });
            } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
                // Other non-empty, non-comment lines as statements
                children.push(SimpleParseNode {
                    kind_id: LangSimple::statement,
                    text: trimmed.to_string(),
                    children: Vec::new(),
                });
            }
        }

        Some(SimpleParseNode {
            kind_id: LangSimple::module,
            text: text.to_string(),
            children,
        })
    }
}

// ============================================================================
// PART 5: Implement Custom Parser via LanguageTraitExt
// ============================================================================

/// Extend LangSimple with custom parser implementation.
impl LanguageTraitExt for LangSimple {
    /// Custom parse implementation for this test language.
    ///
    /// Recognizes simple "fn" declarations and statement lines.
    fn parse_impl(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        let source = text.as_ref();
        let root = simple_parser::parse(source)?;

        Some(Box::new(SimpleParseTree { root }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CompileCtxt;

    #[test]
    fn test_simple_language_macro_and_parser() {
        // Verify language constants are defined
        assert_eq!(LangSimple::module, 0);
        assert_eq!(LangSimple::function, 1);
        assert_eq!(LangSimple::identifier, 2);

        // Verify HIR kind mapping
        assert_eq!(LangSimple::hir_kind(LangSimple::module), HirKind::File);
        assert_eq!(LangSimple::hir_kind(LangSimple::function), HirKind::Scope);
        assert_eq!(
            LangSimple::hir_kind(LangSimple::identifier),
            HirKind::Identifier
        );

        // Verify block kind mapping
        assert_eq!(
            LangSimple::block_kind(LangSimple::function),
            BlockKind::Func
        );

        // Verify token string mapping
        assert_eq!(LangSimple::token_str(LangSimple::module), Some("module"));
        assert_eq!(
            LangSimple::token_str(LangSimple::function),
            Some("function")
        );
        assert_eq!(
            LangSimple::token_str(LangSimple::identifier),
            Some("identifier")
        );

        // Test parsing
        let source = b"fn main() {}\nfn helper() {}\nlet x = 42;";
        let parse_tree = LangSimple::parse(source).expect("Parsing should succeed");
        assert!(!parse_tree.debug_info().is_empty());
    }

    #[test]
    fn test_simple_parse_node_and_tree() {
        // Create a simple parse tree manually
        let root = SimpleParseNode {
            kind_id: LangSimple::module,
            text: "test module".to_string(),
            children: vec![
                SimpleParseNode {
                    kind_id: LangSimple::function,
                    text: "fn test()".to_string(),
                    children: vec![SimpleParseNode {
                        kind_id: LangSimple::identifier,
                        text: "test".to_string(),
                        children: Vec::new(),
                    }],
                },
                SimpleParseNode {
                    kind_id: LangSimple::statement,
                    text: "let x = 1;".to_string(),
                    children: Vec::new(),
                },
            ],
        };

        let tree = SimpleParseTree { root };

        // Verify tree methods
        assert_eq!(tree.debug_info(), "SimpleParse(kind: 0, text_len: 11)");

        // Verify node structure
        if let Some(root_node) = tree.root_node() {
            assert_eq!(root_node.kind_id(), LangSimple::module);
            assert_eq!(root_node.child_count(), 2);

            if let Some(child) = root_node.child(0) {
                assert_eq!(child.kind_id(), LangSimple::function);
                assert_eq!(child.child_count(), 1);
            }
        }
    }
}
