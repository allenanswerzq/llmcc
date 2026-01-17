//! llmcc command-line interface.
//!
pub mod discovery;
pub mod options;
pub mod output;
pub mod pipeline;
pub mod profile;

use llmcc_core::Result;
use llmcc_core::lang_def::LanguageTraitImpl;
use llmcc_dot::ComponentDepth;

pub use options::{CommonTestOptions, GraphOptions, ProcessingOptions};
pub use pipeline::process_files;
pub use profile::profile_phase;

/// Options for running llmcc.
pub struct LlmccOptions {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub output: Option<String>,
    pub print_ir: bool,
    pub print_block: bool,
    pub graph: bool,
    pub component_depth: ComponentDepth,
    pub pagerank_top_k: Option<usize>,
    pub cluster_by_crate: bool,
    pub short_labels: bool,
}

/// Main entry point
pub fn run_main<L: LanguageTraitImpl>(opts: &LlmccOptions) -> Result<Option<String>> {
    let extensions: std::collections::HashSet<&str> =
        L::supported_extensions().iter().copied().collect();

    let files = discovery::discover_files(opts, &extensions)?;

    if files.is_empty() {
        return Ok(None);
    }

    process_files::<L>(opts, &files)
}
