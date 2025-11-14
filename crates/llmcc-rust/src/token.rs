use llmcc_core::LanguageTraitExt;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::HirKind;
use llmcc_core::lang_def::{ParseTree, TreeSitterParseTree};
use tree_sitter_rust;

// Include the auto-generated language definition from build script
// The generated file contains a define_lang! call that expands to LangRust
include!(concat!(env!("OUT_DIR"), "/rust_tokens.rs"));

impl LangRust {
    pub const SUPPORTED_EXTENSIONS: &'static [&'static str] = &["rs"];
}

impl LanguageTraitExt for LangRust {
    fn parse_impl(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .ok()?;

        let bytes = text.as_ref();
        let tree = parser.parse(bytes, None)?;

        Some(Box::new(TreeSitterParseTree { tree }))
    }
}
