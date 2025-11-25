//! Shared CLI options for llmcc tools.
//!
//! This module defines common command-line options used across different llmcc binaries
//! (llmcc and llmcc-test) to ensure consistent behavior and reduce code duplication.

use clap::Args;

/// Common options for graph building and visualization.
#[derive(Args, Debug, Clone, Default)]
pub struct GraphOptions {
    /// Component grouping depth for graph visualization.
    /// - 0: No grouping (flat graph, no clusters)
    /// - 1: Crate level only
    /// - 2: Top-level modules (data, service, api)
    /// - 3+: Deeper sub-modules
    #[arg(long = "component-depth", default_value = "2")]
    pub component_depth: usize,

    /// Number of top PageRank nodes to include (enables pagerank filtering).
    /// When set, only the top K most important nodes are shown.
    #[arg(long = "pagerank-top-k")]
    pub pagerank_top_k: Option<usize>,
}

/// Common options for controlling processing behavior.
#[derive(Args, Debug, Clone, Default)]
pub struct ProcessingOptions {
    /// Process files in parallel (default: false for stable ordering).
    #[arg(long)]
    pub parallel: bool,

    /// Print IR during symbol resolution.
    #[arg(long = "print-ir", default_value = "false")]
    pub print_ir: bool,
}

/// Combined common options for test runners.
#[derive(Args, Debug, Clone, Default)]
pub struct CommonTestOptions {
    #[command(flatten)]
    pub graph: GraphOptions,

    #[command(flatten)]
    pub processing: ProcessingOptions,

    /// Keep the temporary project directory for inspection.
    #[arg(long = "keep-temps")]
    pub keep_temps: bool,

    /// Update expectation sections with current output (bless).
    #[arg(long)]
    pub update: bool,
}

impl GraphOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_component_depth(mut self, depth: usize) -> Self {
        self.component_depth = depth;
        self
    }

    pub fn with_pagerank_top_k(mut self, top_k: Option<usize>) -> Self {
        self.pagerank_top_k = top_k;
        self
    }
}

impl ProcessingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    pub fn with_print_ir(mut self, print_ir: bool) -> Self {
        self.print_ir = print_ir;
        self
    }
}
