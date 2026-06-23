mod collect;
mod output;

use llmcc_core::ViewDepth;
use llmcc_cpp::LangCpp;
use llmcc_error::{Error, ErrorKind, Result};
use llmcc_rust::LangRust;
use llmcc_ts::LangTypeScript;

use crate::corpus::CorpusCase;
use crate::materialize::materialize_case;

use self::collect::{collect_pipeline, collect_pipeline_auto};
pub(crate) use self::output::render_expectation;

#[derive(Clone)]
#[allow(dead_code)]
struct SymbolSnapshot {
    unit: usize,
    id: u32,
    kind: String,
    name: String,
    is_global: bool,
    type_of: Option<String>,
    block_id: Option<String>,
}

#[derive(Clone)]
struct SymbolDependencySnapshot {
    label: String,
    depends_on: Vec<String>,
    depended_by: Vec<String>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct BlockSnapshot {
    label: String,
    kind: String,
    name: String,
}

#[derive(Clone)]
struct BlockRelationSnapshot {
    label: String,
    kind: String,
    name: String,
    relations: Vec<(String, Vec<String>)>,
}

#[derive(Default)]
#[allow(dead_code)]
pub(crate) struct PipelineSummary {
    symbols: Option<Vec<SymbolSnapshot>>,
    symbol_types: Option<Vec<SymbolSnapshot>>,
    block_relations: Option<Vec<BlockRelationSnapshot>>,
    dep_graph_dot: Option<String>,
    arch_graph_dot: Option<String>,
    arch_graph_depth_0: Option<String>,
    arch_graph_depth_1: Option<String>,
    arch_graph_depth_2: Option<String>,
    arch_graph_depth_3: Option<String>,
    block_list: Option<Vec<BlockSnapshot>>,
    block_deps: Option<Vec<SymbolDependencySnapshot>>,
    symbol_deps: Option<Vec<SymbolDependencySnapshot>>,
    block_graph: Option<String>,
    temp_dir_path: Option<String>,
}

impl PipelineSummary {
    pub(crate) fn temp_dir_path(&self) -> Option<&str> {
        self.temp_dir_path.as_deref()
    }
}

/// Options for configuring the pipeline collection process.
#[derive(Debug, Clone)]
pub struct PipelineOptions {
    /// File paths to process (in declaration order).
    pub file_paths: Vec<String>,
    /// Whether to collect symbol information.
    pub keep_symbols: bool,
    /// Whether to collect symbol type resolution info.
    pub keep_symbol_types: bool,
    /// Whether to collect block relations.
    pub keep_block_relations: bool,
    /// Whether to build the dependency graph.
    pub build_dep_graph: bool,
    /// Whether to build the architecture graph.
    pub build_arch_graph: bool,
    /// Whether to build depth-0 (Project) arch graph.
    pub build_arch_graph_depth_0: bool,
    /// Whether to build depth-1 (Crate) arch graph.
    pub build_arch_graph_depth_1: bool,
    /// Whether to build depth-2 (Module) arch graph.
    pub build_arch_graph_depth_2: bool,
    /// Whether to build depth-3 (File) arch graph.
    pub build_arch_graph_depth_3: bool,
    /// Whether to build block reports (blocks and block-deps).
    pub build_block_reports: bool,
    /// Whether to build the block graph.
    pub build_block_graph: bool,
    /// Whether to collect symbol dependencies.
    pub keep_symbol_deps: bool,
    /// When true, may process files in parallel.
    pub parallel: bool,
    /// Whether to print IR during symbol resolution.
    pub print_ir: bool,
    /// Component grouping depth for graph visualization.
    pub view_depth: ViewDepth,
    /// Number of top PageRank nodes to include (None = all nodes).
    pub pagerank_top_k: Option<usize>,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            file_paths: Vec::new(),
            keep_symbols: false,
            keep_symbol_types: false,
            keep_block_relations: false,
            build_dep_graph: false,
            build_arch_graph: false,
            build_arch_graph_depth_0: false,
            build_arch_graph_depth_1: false,
            build_arch_graph_depth_2: false,
            build_arch_graph_depth_3: false,
            build_block_reports: false,
            build_block_graph: false,
            keep_symbol_deps: false,
            parallel: false,
            print_ir: false,
            view_depth: ViewDepth::File,
            pagerank_top_k: None,
        }
    }
}

impl PipelineOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_file_paths(mut self, paths: Vec<String>) -> Self {
        self.file_paths = paths;
        self
    }

    pub fn with_keep_symbols(mut self, keep: bool) -> Self {
        self.keep_symbols = keep;
        self
    }

    pub fn with_build_dep_graph(mut self, build: bool) -> Self {
        self.build_dep_graph = build;
        self
    }

    pub fn with_build_arch_graph(mut self, build: bool) -> Self {
        self.build_arch_graph = build;
        self
    }

    pub fn with_build_arch_graph_depth_0(mut self, build: bool) -> Self {
        self.build_arch_graph_depth_0 = build;
        self
    }

    pub fn with_build_arch_graph_depth_1(mut self, build: bool) -> Self {
        self.build_arch_graph_depth_1 = build;
        self
    }

    pub fn with_build_arch_graph_depth_2(mut self, build: bool) -> Self {
        self.build_arch_graph_depth_2 = build;
        self
    }

    pub fn with_build_arch_graph_depth_3(mut self, build: bool) -> Self {
        self.build_arch_graph_depth_3 = build;
        self
    }

    pub fn with_build_block_reports(mut self, build: bool) -> Self {
        self.build_block_reports = build;
        self
    }

    pub fn with_build_block_graph(mut self, build: bool) -> Self {
        self.build_block_graph = build;
        self
    }

    pub fn with_keep_symbol_deps(mut self, keep: bool) -> Self {
        self.keep_symbol_deps = keep;
        self
    }

    pub fn with_keep_symbol_types(mut self, keep: bool) -> Self {
        self.keep_symbol_types = keep;
        self
    }

    pub fn with_keep_block_relations(mut self, keep: bool) -> Self {
        self.keep_block_relations = keep;
        self
    }

    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    pub fn with_print_ir(mut self, print: bool) -> Self {
        self.print_ir = print;
        self
    }

    pub fn with_view_depth(mut self, depth: ViewDepth) -> Self {
        self.view_depth = depth;
        self
    }

    pub fn with_pagerank_top_k(mut self, top_k: Option<usize>) -> Self {
        self.pagerank_top_k = top_k;
        self
    }
}

pub(crate) fn build_pipeline_summary(
    case: &CorpusCase,
    keep_temps: bool,
    parallel: bool,
    print_ir: bool,
    view_depth: ViewDepth,
    pagerank_top_k: Option<usize>,
) -> Result<PipelineSummary> {
    let required = RequiredOutputs::from_case(case);
    if required.is_empty() {
        return Ok(PipelineSummary::default());
    }

    let project = materialize_case(case, keep_temps)?;
    let temp_dir_path = project.root().to_string_lossy().to_string();
    if keep_temps && project.is_persistent() {
        println!(
            "preserved materialized project for {} at {}",
            case.id(),
            project.root().display()
        );
    }

    let options = required.pipeline_options(parallel, print_ir, view_depth, pagerank_top_k);
    let mut summary = match case.lang.as_str() {
        "rust" => collect_pipeline::<LangRust>(project.root(), &options)?,
        "typescript" | "ts" => collect_pipeline::<LangTypeScript>(project.root(), &options)?,
        "cpp" | "c++" | "c" => collect_pipeline::<LangCpp>(project.root(), &options)?,
        "auto" => collect_pipeline_auto(project.root(), &options)?,
        other => {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                format!("unsupported lang '{}' requested by {}", other, case.id()),
            ));
        }
    };
    summary.temp_dir_path = Some(temp_dir_path);
    drop(project);

    Ok(summary)
}

#[derive(Default)]
struct RequiredOutputs {
    symbols: bool,
    dep_graph: bool,
    arch_graph: bool,
    arch_graph_depth_0: bool,
    arch_graph_depth_1: bool,
    arch_graph_depth_2: bool,
    arch_graph_depth_3: bool,
    block_reports: bool,
    block_graph: bool,
    symbol_deps: bool,
    symbol_types: bool,
    block_relations: bool,
}

impl RequiredOutputs {
    fn from_case(case: &CorpusCase) -> Self {
        let mut outputs = Self::default();
        for expect in &case.expectations {
            match expect.kind.as_str() {
                "symbols" => outputs.symbols = true,
                "dep-graph" => outputs.dep_graph = true,
                "arch-graph" => outputs.arch_graph = true,
                "arch-graph-depth-0" => outputs.arch_graph_depth_0 = true,
                "arch-graph-depth-1" => outputs.arch_graph_depth_1 = true,
                "arch-graph-depth-2" => outputs.arch_graph_depth_2 = true,
                "arch-graph-depth-3" => outputs.arch_graph_depth_3 = true,
                "blocks" | "block-deps" => outputs.block_reports = true,
                "block-graph" => outputs.block_graph = true,
                "symbol-deps" => outputs.symbol_deps = true,
                "symbol-types" => outputs.symbol_types = true,
                "block-relations" => outputs.block_relations = true,
                _ => {}
            }
        }
        outputs
    }

    fn is_empty(&self) -> bool {
        !self.symbols
            && !self.symbol_types
            && !self.block_relations
            && !self.dep_graph
            && !self.needs_any_arch_graph()
            && !self.block_reports
            && !self.block_graph
            && !self.symbol_deps
    }

    fn needs_any_arch_graph(&self) -> bool {
        self.arch_graph
            || self.arch_graph_depth_0
            || self.arch_graph_depth_1
            || self.arch_graph_depth_2
            || self.arch_graph_depth_3
    }

    fn pipeline_options(
        &self,
        parallel: bool,
        print_ir: bool,
        view_depth: ViewDepth,
        pagerank_top_k: Option<usize>,
    ) -> PipelineOptions {
        PipelineOptions::new()
            .with_keep_symbols(self.symbols)
            .with_keep_symbol_types(self.symbol_types)
            .with_keep_block_relations(self.block_relations)
            .with_build_dep_graph(self.dep_graph)
            .with_build_arch_graph(self.needs_any_arch_graph())
            .with_build_arch_graph_depth_0(self.arch_graph_depth_0)
            .with_build_arch_graph_depth_1(self.arch_graph_depth_1)
            .with_build_arch_graph_depth_2(self.arch_graph_depth_2)
            .with_build_arch_graph_depth_3(self.arch_graph_depth_3)
            .with_build_block_reports(self.block_reports)
            .with_build_block_graph(self.block_graph)
            .with_keep_symbol_deps(self.symbol_deps)
            .with_parallel(parallel)
            .with_print_ir(print_ir)
            .with_view_depth(view_depth)
            .with_pagerank_top_k(pagerank_top_k)
    }
}
