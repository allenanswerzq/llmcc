//! llmcc command-line interface.
//!
pub mod runner;

use llmcc_core::ViewDepth;

pub use runner::Runner;

/// Options for running llmcc.
pub struct RunnerOptions {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub output: Option<String>,
    pub print_ir: bool,
    pub print_block: bool,
    pub graph: bool,
    pub view_depth: ViewDepth,
    pub top_k: Option<usize>,
    pub cluster_by_package: bool,
    pub short_labels: bool,
    pub ai: bool,
    pub flat: bool,
}
