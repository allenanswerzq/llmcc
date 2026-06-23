//! llmcc command-line interface.
//!
use clap::ValueEnum;

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
    pub component_depth: ViewDepth,
    pub pagerank_top_k: Option<usize>,
    pub cluster_by_package: bool,
    pub short_labels: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[value(rename_all = "lower")]
pub enum Language {
    Rust,
    #[value(alias = "ts")]
    Typescript,
    #[value(alias = "c++", alias = "c")]
    Cpp,
}
