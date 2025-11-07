use llmcc_core::define_tokens;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::HirKind;
use llmcc_core::paste;

include!(concat!(env!("OUT_DIR"), "/rust_tokens.rs"));

impl LangRust {
    pub const SUPPORTED_EXTENSIONS: &'static [&'static str] = &["rs"];
}
