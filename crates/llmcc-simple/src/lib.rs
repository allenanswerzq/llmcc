//! Simple test language for llmcc testing.
//!
//! Provides a minimal language implementation with custom parser for use in tests.
//! This crate is designed to be a lightweight, publicly accessible test language
//! that can be used across all llmcc crates without cfg(test) restrictions.

#[macro_use]
extern crate llmcc_core;

use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::HirKind;
use llmcc_core::lang_def::{LanguageTraitExt, ParseNode, ParseTree};
use std::any::Any;

// ============================================================================
// PART 1: Define the Simple Language Using the Macro
// ============================================================================

llmcc_core::define_lang!(
    Simple,
    (module, 0, "module", HirKind::File),
    (function, 1, "function", HirKind::Scope, BlockKind::Func),
    (identifier, 2, "identifier", HirKind::Identifier),
    (statement, 3, "statement", HirKind::Scope),
    (field_name, 10, "field_name", HirKind::Identifier),
    (field_type, 11, "field_type", HirKind::Identifier),
);

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
