use clap::Args;
use llmcc_core::ViewDepth;

#[derive(Args, Debug, Clone, Default)]
pub struct GraphOptions {
    /// Component grouping depth for graph visualization.
    /// - 0/flat: No grouping (flat graph, no clusters)
    /// - 1/package: Package level only
    /// - 2/namespace: Namespace level
    /// - 3/file: File level (default)
    #[arg(long = "component-depth", default_value = "3")]
    component_depth_num: usize,

    /// Number of top PageRank nodes to include.
    #[arg(long = "pagerank-top-k")]
    pub pagerank_top_k: Option<usize>,

    /// Generate architecture graph instead of dependency graph.
    #[arg(long = "arch-graph")]
    pub architecture_graph: bool,

    /// Cluster namespaces by their parent package.
    #[arg(long = "cluster-by-package")]
    pub cluster_by_package: bool,

    /// Use shortened labels.
    #[arg(long = "short-labels")]
    pub short_labels: bool,
}

impl GraphOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn component_depth(&self) -> ViewDepth {
        ViewDepth::from_repr(self.component_depth_num as u8).unwrap_or_default()
    }

    pub fn with_component_depth(mut self, depth: ViewDepth) -> Self {
        self.component_depth_num = depth as usize;
        self
    }

    pub fn with_pagerank_top_k(mut self, top_k: Option<usize>) -> Self {
        self.pagerank_top_k = top_k;
        self
    }
}

#[derive(Args, Debug, Clone, Default)]
pub struct ProcessingOptions {
    /// Process files in parallel.
    #[arg(long)]
    pub parallel: bool,

    /// Print IR during symbol resolution.
    #[arg(long = "print-ir")]
    pub print_ir: bool,
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

#[derive(Args, Debug, Clone, Default)]
pub struct CommonTestOptions {
    #[command(flatten)]
    pub graph: GraphOptions,

    #[command(flatten)]
    pub processing: ProcessingOptions,

    /// Keep the temporary project directory for inspection.
    #[arg(long = "keep-temps")]
    pub keep_temps: bool,

    /// Update expectation sections with current output.
    #[arg(long)]
    pub update: bool,
}
