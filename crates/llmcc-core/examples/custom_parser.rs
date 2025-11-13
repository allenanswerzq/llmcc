// Example: Custom Parser Implementation
// This demonstrates how to add a non-tree-sitter parser to llmcc
//
// NOTE: This file is for documentation purposes. See the examples in the comments
// at the bottom for how to implement your own custom parser.

use std::any::Any;
use llmcc_core::lang_def::{LanguageTrait, ParseTree, HirKind};
use llmcc_core::graph_builder::BlockKind;

// ============================================================================
// 1. Define Your Custom AST Representation
// ============================================================================

/// Simple custom AST node for demonstration
#[derive(Debug, Clone)]
pub struct AstNode {
    pub kind: String,
    pub text_range: (usize, usize), // start, end bytes
    pub children: Vec<AstNode>,
}

/// Your custom parse tree implementation
pub struct CustomParseTree {
    pub root: AstNode,
    pub source_len: usize,
}

// ============================================================================
// 2. Implement ParseTree Trait
// ============================================================================

impl ParseTree for CustomParseTree {
    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }

    fn debug_info(&self) -> String {
        format!(
            "Custom(root: {}, src_len: {}, nodes: {})",
            self.root.kind,
            self.source_len,
            Self::count_nodes(&self.root)
        )
    }
}

impl CustomParseTree {
    fn count_nodes(node: &AstNode) -> usize {
        1 + node.children.iter().map(Self::count_nodes).sum::<usize>()
    }
}

// ============================================================================
// 3. Implement Your Custom Parser
// ============================================================================

mod custom_parser {
    use super::*;

    /// Simple custom parser (example: very basic parsing logic)
    pub fn parse_source(source: &[u8]) -> Option<AstNode> {
        let text = std::str::from_utf8(source).ok()?;

        // Very simple parser: just identify "fn" keyword positions
        let mut root = AstNode {
            kind: "root".to_string(),
            text_range: (0, source.len()),
            children: Vec::new(),
        };

        let mut pos = 0;
        while let Some(idx) = text[pos..].find("fn ") {
            let abs_idx = pos + idx;
            let end = text[abs_idx..].find('(').unwrap_or(source.len() - abs_idx) + abs_idx;

            root.children.push(AstNode {
                kind: "function".to_string(),
                text_range: (abs_idx, end),
                children: vec![],
            });

            pos = end;
        }

        Some(root)
    }
}

// ============================================================================
// 4. Define Your Language Type
// ============================================================================

#[derive(Debug)]
pub struct LangCustom {}

// ============================================================================
// 5. Implement LanguageTrait
// ============================================================================

impl LanguageTrait for LangCustom {
    /// Parse returns a generic ParseTree (in this case, our custom one)
    fn parse(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        let source = text.as_ref();
        let root = custom_parser::parse_source(source)?;

        Some(Box::new(CustomParseTree {
            root,
            source_len: source.len(),
        }))
    }

    /// Map custom token IDs to HIR kinds
    /// (Same as define_tokens! macro would generate)
    fn hir_kind(kind_id: u16) -> HirKind {
        match kind_id {
            0 => HirKind::File,
            1 => HirKind::Scope,      // functions
            2 => HirKind::Identifier, // identifiers
            _ => HirKind::Internal,
        }
    }

    fn block_kind(kind_id: u16) -> BlockKind {
        match kind_id {
            1 => BlockKind::Func,
            _ => BlockKind::Undefined,
        }
    }

    fn token_str(kind_id: u16) -> Option<&'static str> {
        match kind_id {
            0 => Some("root"),
            1 => Some("function"),
            2 => Some("identifier"),
            _ => None,
        }
    }

    fn is_valid_token(kind_id: u16) -> bool {
        matches!(kind_id, 0 | 1 | 2)
    }

    fn name_field() -> u16 {
        2 // identifier kind ID
    }

    fn type_field() -> u16 {
        2 // reuse identifier for demo
    }

    fn supported_extensions() -> &'static [&'static str] {
        &["custom"]
    }
}

// ============================================================================
// 6. Usage Example
// ============================================================================

#[cfg(test)]
mod examples {
    use super::*;
    use llmcc_core::context::CompileCtxt;

    #[test]
    fn example_using_custom_parser() {
        let source = b"fn hello() { } fn world() { }".to_vec();

        // Create compilation context with custom parser
        let cc = CompileCtxt::from_sources::<LangCustom>(&[source]);

        // Access the generic parse tree
        if let Some(parse_tree) = cc.get_parse_tree(0) {
            // Downcast to our custom implementation
            if let Some(custom) = parse_tree.as_any().downcast_ref::<CustomParseTree>() {
                println!("Debug info: {}", parse_tree.debug_info());
                println!("Found {} functions", custom.root.children.len());

                for child in &custom.root.children {
                    println!("  - {} at bytes {:?}", child.kind, child.text_range);
                }
            }
        }
    }

    #[test]
    fn example_parser_properties() {
        // Can use standard LanguageTrait methods
        assert_eq!(LangCustom::hir_kind(0), HirKind::File);
        assert_eq!(LangCustom::hir_kind(1), HirKind::Scope);
        assert!(LangCustom::is_valid_token(1));
        assert!(!LangCustom::is_valid_token(999));
    }
}

// ============================================================================
// Key Takeaways
// ============================================================================

// 1. Custom ParseTree just needs to implement the trait
//    - Must be Send + Sync for parallel parsing
//    - Must implement as_any() for downcasting
//
// 2. LanguageTrait implementation can use any parser
//    - Return Box<dyn ParseTree> instead of tree-sitter Tree
//    - Semantic mappings (hir_kind, token_str, etc.) remain the same
//
// 3. CompileCtxt works with both:
//    - cc.get_tree(i) -> Option<&Tree>        (tree-sitter only)
//    - cc.get_parse_tree(i) -> Option<&Box<dyn ParseTree>> (any parser)
//
// 4. Downcasting to specific type:
//    - pt.as_any().downcast_ref::<YourCustomTree>()
//    - Safe: returns Option, no panics
//
// 5. No changes needed to:
//    - Visitor trait generation
//    - HIR building
//    - Symbol resolution
//    - All higher-level infrastructure
