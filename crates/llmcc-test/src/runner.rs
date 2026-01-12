use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use llmcc_cli::{GraphOptions, ProcessingOptions};
use llmcc_core::ProjectGraph;
use llmcc_core::block::reset_block_id_counter;
use llmcc_core::context::{CompileCtxt, CompileUnit};
use llmcc_core::graph_builder::{BlockId, GraphBuildOption, build_llmcc_graph};
use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
use llmcc_core::lang_def::LanguageTraitImpl;
use llmcc_core::symbol::reset_symbol_id_counter;
use llmcc_dot::{ComponentDepth, render_graph};

use llmcc_resolver::{ResolverOption, bind_symbols_with, collect_symbols_with};
use llmcc_rust::LangRust;
use llmcc_ts::LangTypeScript;
use similar::TextDiff;
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::corpus::{Corpus, CorpusCase, CorpusFile};

pub use llmcc_cli::{
    GraphOptions as SharedGraphOptions, ProcessingOptions as SharedProcessingOptions,
};

#[derive(Clone)]
#[allow(dead_code)]
struct SymbolSnapshot {
    unit: usize,
    id: u32,
    kind: String,
    name: String,
    is_global: bool,
    /// Type this symbol resolves to (SymId -> name)
    type_of: Option<String>,
    /// Block this symbol is associated with
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

/// Snapshot of block relations from cc.related_map
#[derive(Clone)]
struct BlockRelationSnapshot {
    /// Block label like "u0:5"
    label: String,
    /// Block kind
    kind: String,
    /// Block name
    name: String,
    /// Relations: (relation_type, target_labels)
    relations: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub filter: Option<String>,
    pub update: bool,
    pub keep_temps: bool,
    /// Graph building and visualization options.
    pub graph: GraphOptions,
    /// Processing behavior options.
    pub processing: ProcessingOptions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseStatus {
    Passed,
    Failed,
    Updated,
    NoExpectations,
}

#[derive(Debug, Clone)]
pub struct CaseOutcome {
    pub id: String,
    pub status: CaseStatus,
    pub message: Option<String>,
}

pub fn run_cases(corpus: &mut Corpus, config: RunnerConfig) -> Result<Vec<CaseOutcome>> {
    let mut outcomes = Vec::new();
    let mut matched = 0usize;

    for file in corpus.files_mut() {
        outcomes.extend(run_cases_in_file(
            file,
            config.update,
            config.filter.as_deref(),
            &mut matched,
            config.keep_temps,
            config.processing.parallel,
            config.processing.print_ir,
            config.graph.component_depth(),
            config.graph.pagerank_top_k,
        )?);
    }

    if matched == 0 {
        return Err(anyhow!(
            "no llmcc-test cases matched filter {:?}",
            config.filter
        ));
    }

    Ok(outcomes)
}

pub fn run_cases_for_file(
    file: &mut CorpusFile,
    update: bool,
    keep_temps: bool,
) -> Result<Vec<CaseOutcome>> {
    run_cases_for_file_with_parallel(
        file,
        update,
        keep_temps,
        false,
        true,
        ComponentDepth::File,
        None,
    )
}

pub fn run_cases_for_file_with_parallel(
    file: &mut CorpusFile,
    update: bool,
    keep_temps: bool,
    _parallel: bool,
    print_ir: bool,
    component_depth: ComponentDepth,
    pagerank_top_k: Option<usize>,
) -> Result<Vec<CaseOutcome>> {
    let mut matched = 0usize;
    run_cases_in_file(
        file,
        update,
        None,
        &mut matched,
        keep_temps,
        false,
        print_ir,
        component_depth,
        pagerank_top_k,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_cases_in_file(
    file: &mut CorpusFile,
    update: bool,
    filter: Option<&str>,
    matched: &mut usize,
    keep_temps: bool,
    _parallel: bool,
    print_ir: bool,
    component_depth: ComponentDepth,
    pagerank_top_k: Option<usize>,
) -> Result<Vec<CaseOutcome>> {
    let mut file_outcomes = Vec::new();
    let mut mutated_file = false;
    for idx in 0..file.cases.len() {
        let run_case = {
            let case = &file.cases[idx];
            if let Some(filter_term) = filter {
                case.id().contains(filter_term)
            } else {
                true
            }
        };

        if !run_case {
            continue;
        }

        *matched += 1;
        let case_name = file.cases[idx].id();
        print!("  {case_name} ... ");
        // Flush to ensure the test name appears before we run
        use std::io::Write;
        let _ = std::io::stdout().flush();

        let (outcome, mutated) = {
            let case = &mut file.cases[idx];
            evaluate_case(
                case,
                update,
                keep_temps,
                _parallel,
                print_ir,
                component_depth,
                pagerank_top_k,
            )?
        };

        // Print result immediately after test completes
        match outcome.status {
            CaseStatus::Passed => println!("ok"),
            CaseStatus::Updated => println!("updated"),
            CaseStatus::Failed => {
                println!("FAILED");
                if let Some(message) = &outcome.message {
                    for line in message.lines() {
                        println!("        {line}");
                    }
                }
            }
            CaseStatus::NoExpectations => println!("skipped (no expectations)"),
        }

        if mutated {
            file.mark_dirty();
            mutated_file = true;
        }
        file_outcomes.push(outcome);
    }
    if update && !mutated_file {
        file.mark_dirty();
    }
    Ok(file_outcomes)
}

fn evaluate_case(
    case: &mut CorpusCase,
    update: bool,
    keep_temps: bool,
    _parallel: bool,
    print_ir: bool,
    component_depth: ComponentDepth,
    pagerank_top_k: Option<usize>,
) -> Result<(CaseOutcome, bool)> {
    let case_id = case.id();

    if case.expectations.is_empty() {
        return Ok((
            CaseOutcome {
                id: case_id,
                status: CaseStatus::NoExpectations,
                message: Some("no expectation blocks declared".to_string()),
            },
            false,
        ));
    }

    reset_symbol_id_counter();
    reset_block_id_counter();
    let summary = build_pipeline_summary(
        case,
        keep_temps,
        _parallel,
        print_ir,
        component_depth,
        pagerank_top_k,
    )?;
    let mut mutated = false;
    let mut status = CaseStatus::Passed;
    let mut failures = Vec::new();

    let temp_dir_path = summary.temp_dir_path.as_deref();
    for expect in &mut case.expectations {
        let kind = expect.kind.as_str();
        let actual = render_expectation(kind, &summary, &case_id)?;
        let expected_norm = normalize(kind, &expect.value, None);
        let actual_norm = normalize(kind, &actual, temp_dir_path);

        if expected_norm == actual_norm {
            continue;
        }

        if update {
            // Save the actual formatted output (with alignment/formatting preserved)
            // We apply temp_dir replacement if needed
            let actual_to_save = if let Some(tmp_path) = temp_dir_path {
                let mut result = actual.replace(tmp_path, "$TMP");
                // Also replace just the directory name (for relative paths in graph output)
                if let Some(dir_name) = std::path::Path::new(tmp_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                {
                    result = result.replace(dir_name, "$TMP");
                }
                result
            } else {
                actual.clone()
            };
            expect.value = ensure_trailing_newline(actual_to_save);
            mutated = true;
            status = CaseStatus::Updated;
        } else {
            status = CaseStatus::Failed;
            // Use normalized values for diff so $TMP replacement is visible
            failures.push(format_expectation_diff(
                &expect.kind,
                &expected_norm,
                &actual_norm,
            ));
        }
    }

    let message = if failures.is_empty() {
        None
    } else {
        Some(failures.join("\n"))
    };

    Ok((
        CaseOutcome {
            id: case_id,
            status,
            message,
        },
        mutated,
    ))
}

fn build_pipeline_summary(
    case: &CorpusCase,
    keep_temps: bool,
    parallel: bool,
    print_ir: bool,
    component_depth: ComponentDepth,
    pagerank_top_k: Option<usize>,
) -> Result<PipelineSummary> {
    let needs_symbols = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "symbols");
    let needs_dep_graph = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "dep-graph");
    let needs_arch_graph = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "arch-graph");
    // Check for depth-specific arch graph expectations
    let needs_arch_graph_depth_0 = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "arch-graph-depth-0");
    let needs_arch_graph_depth_1 = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "arch-graph-depth-1");
    let needs_arch_graph_depth_2 = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "arch-graph-depth-2");
    let needs_arch_graph_depth_3 = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "arch-graph-depth-3");
    let needs_any_arch_graph = needs_arch_graph
        || needs_arch_graph_depth_0
        || needs_arch_graph_depth_1
        || needs_arch_graph_depth_2
        || needs_arch_graph_depth_3;
    let needs_block_reports = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "blocks" || expect.kind == "block-deps");
    let needs_block_graph = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "block-graph");
    let needs_symbol_deps = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "symbol-deps");
    let needs_symbol_types = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "symbol-types");
    let needs_block_relations = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "block-relations");

    if !needs_symbols
        && !needs_symbol_types
        && !needs_block_relations
        && !needs_dep_graph
        && !needs_any_arch_graph
        && !needs_block_reports
        && !needs_block_graph
        && !needs_symbol_deps
    {
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

    // Don't provide file_paths - let collect_pipeline use discover_language_files
    // which handles the prefixed filenames we created in materialize_case
    let options = PipelineOptions::new()
        // file_paths is empty, so discover_language_files will be used
        .with_keep_symbols(needs_symbols)
        .with_keep_symbol_types(needs_symbol_types)
        .with_keep_block_relations(needs_block_relations)
        .with_build_dep_graph(needs_dep_graph)
        .with_build_arch_graph(needs_any_arch_graph)
        .with_build_arch_graph_depth_0(needs_arch_graph_depth_0)
        .with_build_arch_graph_depth_1(needs_arch_graph_depth_1)
        .with_build_arch_graph_depth_2(needs_arch_graph_depth_2)
        .with_build_arch_graph_depth_3(needs_arch_graph_depth_3)
        .with_build_block_reports(needs_block_reports)
        .with_build_block_graph(needs_block_graph)
        .with_keep_symbol_deps(needs_symbol_deps)
        .with_parallel(parallel)
        .with_print_ir(print_ir)
        .with_component_depth(component_depth)
        .with_pagerank_top_k(pagerank_top_k);

    let mut summary = match case.lang.as_str() {
        "rust" => collect_pipeline::<LangRust>(project.root(), &options)?,
        "typescript" | "ts" => collect_pipeline::<LangTypeScript>(project.root(), &options)?,
        "auto" => collect_pipeline_auto(project.root(), &options)?,
        other => {
            return Err(anyhow!(
                "unsupported lang '{}' requested by {}",
                other,
                case.id()
            ));
        }
    };
    summary.temp_dir_path = Some(temp_dir_path);
    drop(project);

    Ok(summary)
}

#[derive(Default)]
#[allow(dead_code)]
struct PipelineSummary {
    symbols: Option<Vec<SymbolSnapshot>>,
    symbol_types: Option<Vec<SymbolSnapshot>>,
    block_relations: Option<Vec<BlockRelationSnapshot>>,
    dep_graph_dot: Option<String>,
    arch_graph_dot: Option<String>,
    /// Depth-specific arch graphs (Project, Crate, Module, File levels)
    arch_graph_depth_0: Option<String>,
    arch_graph_depth_1: Option<String>,
    arch_graph_depth_2: Option<String>,
    arch_graph_depth_3: Option<String>,
    block_list: Option<Vec<BlockSnapshot>>,
    block_deps: Option<Vec<SymbolDependencySnapshot>>,
    symbol_deps: Option<Vec<SymbolDependencySnapshot>>,
    block_graph: Option<String>,
    /// The temp directory path used for this test case.
    /// Used to replace actual paths with $TMP placeholder.
    temp_dir_path: Option<String>,
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
    pub component_depth: ComponentDepth,
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
            component_depth: ComponentDepth::File,
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

    pub fn with_component_depth(mut self, depth: ComponentDepth) -> Self {
        self.component_depth = depth;
        self
    }

    pub fn with_pagerank_top_k(mut self, top_k: Option<usize>) -> Self {
        self.pagerank_top_k = top_k;
        self
    }
}

fn render_expectation(kind: &str, summary: &PipelineSummary, case_id: &str) -> Result<String> {
    match kind {
        "symbols" => {
            let symbols = summary
                .symbols
                .as_ref()
                .ok_or_else(|| anyhow!("case {case_id} requested symbols but summary missing"))?;
            Ok(render_symbol_snapshot(symbols))
        }
        "symbol-types" => {
            // let symbols = summary
            //     .symbol_types
            //     .as_ref()
            //     .ok_or_else(|| anyhow!("case {} requested symbol-types but summary missing", case_id))?;
            // Ok(render_symbol_types_snapshot(symbols))
            Ok("symbol-types snapshot not yet implemented\n".to_string())
        }
        "block-relations" => {
            let relations = summary.block_relations.as_ref().ok_or_else(|| {
                anyhow!("case {case_id} requested block-relations but summary missing")
            })?;
            Ok(render_block_relations_snapshot(relations))
        }
        "dep-graph" => summary.dep_graph_dot.clone().ok_or_else(|| {
            anyhow!("case {case_id} requested dep-graph output but summary missing")
        }),
        "arch-graph" => summary.arch_graph_dot.clone().ok_or_else(|| {
            anyhow!("case {case_id} requested arch-graph output but summary missing")
        }),
        "arch-graph-depth-0" => summary.arch_graph_depth_0.clone().ok_or_else(|| {
            anyhow!("case {case_id} requested arch-graph-depth-0 output but summary missing")
        }),
        "arch-graph-depth-1" => summary.arch_graph_depth_1.clone().ok_or_else(|| {
            anyhow!("case {case_id} requested arch-graph-depth-1 output but summary missing")
        }),
        "arch-graph-depth-2" => summary.arch_graph_depth_2.clone().ok_or_else(|| {
            anyhow!("case {case_id} requested arch-graph-depth-2 output but summary missing")
        }),
        "arch-graph-depth-3" => summary.arch_graph_depth_3.clone().ok_or_else(|| {
            anyhow!("case {case_id} requested arch-graph-depth-3 output but summary missing")
        }),
        "blocks" => {
            // summary
            // .block_list
            // .as_ref()
            // .map(|list| render_block_snapshot(list))
            // .ok_or_else(|| {
            //     anyhow!(
            //         "case {} requested blocks output but summary missing",
            //         case_id
            //     )
            // }),
            Ok("symbol-types snapshot not yet implemented\n".to_string())
        }
        "block-deps" => summary
            .block_deps
            .as_ref()
            .map(|deps| render_symbol_dependencies(deps))
            .ok_or_else(|| {
                anyhow!("case {case_id} requested block-deps output but summary missing")
            }),
        "block-graph" => summary.block_graph.clone().ok_or_else(|| {
            anyhow!("case {case_id} requested block-graph output but summary missing")
        }),
        "symbol-deps" => {
            let deps = summary.symbol_deps.as_ref().ok_or_else(|| {
                anyhow!("case {case_id} requested symbol-deps but summary missing")
            })?;
            Ok(render_symbol_dependencies(deps))
        }
        other => Err(anyhow!(
            "case {case_id} uses unsupported expectation '{other}'"
        )),
    }
}

fn render_symbol_snapshot(entries: &[SymbolSnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    let mut rows = entries.to_vec();
    rows.sort_by(|a, b| {
        a.unit
            .cmp(&b.unit)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });

    let label_width = rows
        .iter()
        .map(|row| format!("u{}:{}", row.unit, row.id).len())
        .max()
        .unwrap_or(0);
    let kind_width = rows.iter().map(|row| row.kind.len()).max().unwrap_or(0);
    let name_width = rows.iter().map(|row| row.name.len()).max().unwrap_or(0);
    let global_width = if rows.iter().any(|row| row.is_global) {
        "[global]".len()
    } else {
        0
    };

    let mut buf = String::new();
    for row in rows {
        let label = format!("u{}:{}", row.unit, row.id);
        let _ = writeln!(
            buf,
            "{:<label_width$} | {:kind_width$} | {:name_width$} | {:global_width$}",
            label,
            row.kind,
            row.name,
            if row.is_global { "[global]" } else { "" },
            label_width = label_width,
            kind_width = kind_width,
            name_width = name_width,
            global_width = global_width,
        );
    }
    buf
}

/// Render symbol types snapshot showing type resolution.
/// Format: label | kind | name | -> type_label (type_name)
#[allow(dead_code)]
fn render_symbol_types_snapshot(entries: &[SymbolSnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    let mut rows = entries.to_vec();
    rows.sort_by(|a, b| {
        a.unit
            .cmp(&b.unit)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });

    let label_width = rows
        .iter()
        .map(|row| format!("u{}:{}", row.unit, row.id).len())
        .max()
        .unwrap_or(0);
    let kind_width = rows.iter().map(|row| row.kind.len()).max().unwrap_or(0);
    let name_width = rows.iter().map(|row| row.name.len()).max().unwrap_or(0);

    let mut buf = String::new();
    for row in rows {
        let label = format!("u{}:{}", row.unit, row.id);
        let type_info = if let Some(type_of) = &row.type_of {
            format!("-> {type_of}")
        } else {
            String::new()
        };
        let block_info = if let Some(block_id) = &row.block_id {
            format!("[{block_id}]")
        } else {
            String::new()
        };
        let _ = writeln!(
            buf,
            "{:<label_width$} | {:kind_width$} | {:name_width$} | {} {}",
            label,
            row.kind,
            row.name,
            type_info,
            block_info,
            label_width = label_width,
            kind_width = kind_width,
            name_width = name_width,
        );
    }
    buf
}

/// Render block relations snapshot.
/// Uses a clean edge-based format, filtering out redundant relations.
/// Format: name:id (kind)  --relation-->  name:id (kind)
fn render_block_relations_snapshot(entries: &[BlockRelationSnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    // Relations to skip (they're either redundant or shown in block-graph)
    let skip_relations = ["contains", "contained_by"];

    // Collect all edges as (source, relation, target) tuples
    let mut edges: Vec<(String, String, String)> = Vec::new();

    for entry in entries {
        let id = entry.label.replace("u0:", "");
        let source = format!("{}:{} ({})", entry.name, id, entry.kind);

        for (rel_type, targets) in &entry.relations {
            // Skip redundant relations
            if skip_relations.contains(&rel_type.as_str()) {
                continue;
            }

            for target_label in targets {
                // Find target entry to get its name and kind
                let (target_name, target_kind) = entries
                    .iter()
                    .find(|e| e.label == *target_label)
                    .map(|e| (e.name.as_str(), e.kind.as_str()))
                    .unwrap_or(("?", "?"));
                let target_id = target_label.replace("u0:", "");
                let target = format!("{target_name}:{target_id} ({target_kind})");

                edges.push((source.clone(), rel_type.clone(), target));
            }
        }
    }

    if edges.is_empty() {
        return "none\n".to_string();
    }

    // Sort edges for deterministic output
    edges.sort();

    // Calculate column widths for alignment
    let source_width = edges.iter().map(|(s, _, _)| s.len()).max().unwrap_or(0);
    let rel_width = edges.iter().map(|(_, r, _)| r.len()).max().unwrap_or(0);

    let mut buf = String::new();
    for (source, rel, target) in &edges {
        let _ = writeln!(
            buf,
            "{source:<source_width$}  --{rel:^rel_width$}-->  {target}",
        );
    }
    buf
}

use std::cmp::Ordering;

#[allow(dead_code)]
fn render_block_snapshot(entries: &[BlockSnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    let mut rows = entries.to_vec();
    rows.sort_by(compare_block_snapshots);

    let label_width = rows.iter().map(|row| row.label.len()).max().unwrap_or(0);
    let kind_width = rows.iter().map(|row| row.kind.len()).max().unwrap_or(0);
    let name_width = rows.iter().map(|row| row.name.len()).max().unwrap_or(0);

    let mut buf = String::new();
    for row in rows {
        let _ = writeln!(
            buf,
            "{:<label_width$} | {:kind_width$} | {:name_width$}",
            row.label,
            row.kind,
            row.name,
            label_width = label_width,
            kind_width = kind_width,
            name_width = name_width,
        );
    }
    buf
}

#[allow(dead_code)]
fn compare_block_snapshots(a: &BlockSnapshot, b: &BlockSnapshot) -> Ordering {
    match (parse_block_label(&a.label), parse_block_label(&b.label)) {
        (Some(ka), Some(kb)) => ka
            .cmp(&kb)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a
            .label
            .cmp(&b.label)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name)),
    }
}

#[allow(dead_code)]
fn parse_block_label(label: &str) -> Option<(usize, usize)> {
    let mut parts = label.split(':');
    let unit_part = parts.next()?.strip_prefix('u')?;
    let block_part = parts.next()?;
    let unit = unit_part.parse().ok()?;
    let block = block_part.parse().ok()?;
    Some((unit, block))
}

fn render_symbol_dependencies(entries: &[SymbolDependencySnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    let mut rows = entries.to_vec();
    rows.sort_by(|a, b| a.label.cmp(&b.label));

    let mut buf = String::new();
    for row in rows {
        let mut depends = row.depends_on.clone();
        depends.sort();
        let mut depended = row.depended_by.clone();
        depended.sort();
        if !depends.is_empty() {
            let _ = writeln!(buf, "{} -> [{}]", row.label, depends.join(", "));
        }
        if !depended.is_empty() {
            let _ = writeln!(buf, "{} <- [{}]", row.label, depended.join(", "));
        }
    }
    buf
}

fn format_expectation_diff(kind: &str, expected: &str, actual: &str) -> String {
    let diff = TextDiff::from_lines(expected, actual);
    let mut buf = String::new();
    let _ = writeln!(buf, "Expectation '{kind}' mismatch:");
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        let _ = write!(buf, "{sign}{change}");
    }
    buf
}

fn normalize(kind: &str, text: &str, temp_dir_path: Option<&str>) -> String {
    let canonical = text
        .replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_string();

    // Replace temp directory path with $TMP placeholder (for actual output)
    // or replace $TMP with temp directory path (for expected value)
    let canonical = if let Some(tmp_path) = temp_dir_path {
        // Replace the full path first
        let mut result = canonical.replace(tmp_path, "$TMP");
        // Also replace just the directory name (for relative paths in graph output)
        if let Some(dir_name) = std::path::Path::new(tmp_path)
            .file_name()
            .and_then(|s| s.to_str())
        {
            result = result.replace(dir_name, "$TMP");
        }
        result
    } else {
        canonical
    };

    match kind {
        "symbols" | "blocks" | "symbol-types" => normalize_symbols(&canonical),
        "symbol-deps" | "block-deps" => normalize_symbol_deps(&canonical),
        "block-relations" => normalize_block_relations(&canonical),
        "dep-graph" | "arch-graph" | "arch-graph-depth-0" | "arch-graph-depth-1"
        | "arch-graph-depth-2" | "arch-graph-depth-3" => normalize_graph(&canonical),
        "block-graph" => normalize_block_graph(&canonical),
        _ => canonical,
    }
}

fn normalize_symbols(text: &str) -> String {
    let mut rows: Vec<(usize, u32, String)> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }

            let parts: Vec<_> = line.split('|').map(|part| part.trim()).collect();
            if parts.is_empty() {
                return None;
            }

            let label = parts[0];
            let (unit, id) = parse_unit_and_id(label);
            let kind = parts.get(1).copied().unwrap_or("");
            let name = parts.get(2).copied().unwrap_or("");
            let global = parts.get(3).copied().unwrap_or("");

            let canonical = format!("{label} | {kind} | {name} | {global}");
            // Trim trailing whitespace from the row (e.g., when global is empty)
            let canonical = canonical.trim_end().to_string();
            Some((unit, id, canonical))
        })
        .collect();

    rows.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    rows.into_iter()
        .map(|(_, _, row)| row)
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_symbol_deps(text: &str) -> String {
    let mut rows: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || is_empty_relation(trimmed) {
                return None;
            }
            Some(trimmed.to_string())
        })
        .collect();
    rows.sort();
    rows.join("\n")
}

fn normalize_block_relations(text: &str) -> String {
    // Simple line-based format now: each line is an edge like
    // "source (id) --relation--> target (id)"
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    lines.sort();
    lines.join("\n")
}
fn normalize_block_graph(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Parse and re-format to ensure consistent indentation
    match parse_sexpr(trimmed) {
        Ok(exprs) => exprs
            .into_iter()
            .map(|expr| format_sexpr(&expr))
            .collect::<Vec<_>>()
            .join("\n\n"),
        Err(_) => trimmed.to_string(),
    }
}

fn is_empty_relation(line: &str) -> bool {
    if let Some((_, rhs)) = line.split_once("->")
        && rhs.trim() == "[]"
    {
        return true;
    }
    if let Some((_, rhs)) = line.split_once("<-")
        && rhs.trim() == "[]"
    {
        return true;
    }
    false
}

fn normalize_graph(text: &str) -> String {
    // Parse graph and sort edges for deterministic comparison
    // Filter out empty lines and cosmetic styling lines for flexible comparison
    let mut lines: Vec<&str> = text
        .trim()
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Skip empty lines
            if trimmed.is_empty() {
                return false;
            }
            // Skip cosmetic styling lines that don't affect graph semantics
            if trimmed.starts_with("//")
                || trimmed.starts_with("rankdir=")
                || trimmed.starts_with("ranksep=")
                || trimmed.starts_with("nodesep=")
                || trimmed.starts_with("splines=")
                || trimmed.starts_with("compound=")
                || trimmed.starts_with("concentrate=")
                || trimmed.starts_with("fontsize=")
                || trimmed.starts_with("fontname=")
                || trimmed.starts_with("labelloc=")
                || trimmed.starts_with("node [")
                || trimmed.starts_with("edge [")
                || trimmed.starts_with("style=")
                || trimmed.starts_with("color=")
                || trimmed.starts_with("bgcolor=")
                || trimmed.starts_with("label=\"")
            {
                return false;
            }
            true
        })
        .collect();

    // Find where edges start (after closing brace of last subgraph)
    // Edges are lines like "  n1 -> n2;" or "  n1 -> n2 [...];"
    let edge_re = regex::Regex::new(r"^\s*n\d+\s*->\s*n\d+").unwrap();

    let mut edge_start = None;
    let mut edge_end = None;

    for (i, line) in lines.iter().enumerate() {
        if edge_re.is_match(line) {
            if edge_start.is_none() {
                edge_start = Some(i);
            }
            edge_end = Some(i);
        }
    }

    // Sort the edges if found
    if let (Some(start), Some(end)) = (edge_start, edge_end) {
        let edges = &mut lines[start..=end];
        edges.sort();
    }

    lines.join("\n")
}

fn parse_unit_and_id(token: &str) -> (usize, u32) {
    if let Some(stripped) = token.strip_prefix('u')
        && let Some((unit_str, id_str)) = stripped.split_once(':')
        && let (Ok(unit), Ok(id)) = (unit_str.parse::<usize>(), id_str.parse::<u32>())
    {
        return (unit, id);
    }

    (usize::MAX, u32::MAX)
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

#[derive(Debug, Clone)]
enum SExpr {
    Atom(String),
    List(Vec<SExpr>),
}

fn parse_sexpr(input: &str) -> Result<Vec<SExpr>, ()> {
    let tokens = tokenize(input);
    let mut idx = 0;
    let mut exprs = Vec::new();
    while idx < tokens.len() {
        exprs.push(parse_expr(&tokens, &mut idx)?);
    }
    Ok(exprs)
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '(' | ')' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
                tokens.push(ch.to_string());
            }
            '"' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
                let mut literal = String::new();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '"' {
                        break;
                    }
                    literal.push(next);
                }
                tokens.push(literal);
            }
            _ if ch.is_whitespace() => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

fn parse_expr(tokens: &[String], idx: &mut usize) -> Result<SExpr, ()> {
    if *idx >= tokens.len() {
        return Err(());
    }
    let token = tokens[*idx].clone();
    *idx += 1;
    match token.as_str() {
        "(" => {
            let mut items = Vec::new();
            while *idx < tokens.len() && tokens[*idx] != ")" {
                items.push(parse_expr(tokens, idx)?);
            }
            if *idx >= tokens.len() || tokens[*idx] != ")" {
                return Err(());
            }
            *idx += 1;
            Ok(SExpr::List(items))
        }
        ")" => Err(()),
        literal => Ok(SExpr::Atom(literal.to_string())),
    }
}

fn format_sexpr(expr: &SExpr) -> String {
    format_sexpr_indented(expr, 0)
}

fn format_sexpr_indented(expr: &SExpr, depth: usize) -> String {
    match expr {
        SExpr::Atom(atom) => atom.clone(),
        SExpr::List(items) => {
            if items.is_empty() {
                return "()".to_string();
            }

            // Get the head (first atom, e.g., "root:1" or "root:1 main")
            let head_parts: Vec<String> = items
                .iter()
                .take_while(|item| matches!(item, SExpr::Atom(_)))
                .map(format_sexpr)
                .collect();
            let head = head_parts.join(" ");

            // Get child lists
            let children: Vec<&SExpr> = items.iter().skip(head_parts.len()).collect();

            if children.is_empty() {
                format!("({head})")
            } else {
                let indent = "  ".repeat(depth);
                let child_indent = "  ".repeat(depth + 1);
                let mut buf = format!("({head}\n");
                for child in children {
                    buf.push_str(&child_indent);
                    buf.push_str(&format_sexpr_indented(child, depth + 1));
                    buf.push('\n');
                }
                buf.push_str(&indent);
                buf.push(')');
                buf
            }
        }
    }
}

struct MaterializedProject {
    #[allow(dead_code)]
    temp_dir: Option<TempDir>,
    root_path: PathBuf,
}

impl MaterializedProject {
    fn root(&self) -> &Path {
        &self.root_path
    }

    fn is_persistent(&self) -> bool {
        self.temp_dir.is_none()
    }
}

fn materialize_case(case: &CorpusCase, keep_temps: bool) -> Result<MaterializedProject> {
    let temp_dir = tempfile::tempdir().context("failed to create temp dir for llmcc-test")?;
    let root_path = temp_dir.path().to_path_buf();

    for (idx, file) in case.files.iter().enumerate() {
        let original_path = Path::new(&file.path);
        let file_name_str = original_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        // Don't add prefix to Cargo.toml or package.json - they need to be findable by parse_crate_name/parse_package_name
        let final_path = if file_name_str == "Cargo.toml" || file_name_str == "package.json" {
            original_path.to_path_buf()
        } else {
            // Add numeric prefix to filename to preserve declaration order after WalkDir + sort
            let prefixed_filename = format!("{idx:03}_{file_name_str}");
            original_path
                .parent()
                .map(|p| p.join(&prefixed_filename))
                .unwrap_or_else(|| PathBuf::from(&prefixed_filename))
        };

        let abs_path = root_path.join(&final_path);
        if let Some(parent) = abs_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&abs_path, file.contents.as_bytes()).with_context(|| {
            format!(
                "failed to write virtual file {} for {}",
                abs_path.display(),
                case.id()
            )
        })?;
    }

    if keep_temps {
        // Use keep() to consume TempDir without deleting the directory
        let preserved = temp_dir.keep();
        return Ok(MaterializedProject {
            temp_dir: None,
            root_path: preserved,
        });
    }

    Ok(MaterializedProject {
        temp_dir: Some(temp_dir),
        root_path,
    })
}

fn collect_pipeline<L>(project_root: &Path, options: &PipelineOptions) -> Result<PipelineSummary>
where
    L: LanguageTraitImpl,
{
    // Use provided file paths (preserves declaration order) or discover them
    let files: Vec<(String, String)> = if options.file_paths.is_empty() {
        discover_language_files::<L>(project_root, options.parallel)?
    } else {
        // Filter to only include files with supported extensions
        // For explicitly provided paths, use the same path as both physical and logical
        let supported = L::supported_extensions();
        options
            .file_paths
            .iter()
            .filter(|path| {
                Path::new(path)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| supported.iter().any(|s| s.eq_ignore_ascii_case(ext)))
                    .unwrap_or(false)
            })
            .map(|path| (path.clone(), path.clone()))
            .collect()
    };

    let cc = CompileCtxt::from_files_with_logical::<L>(&files).unwrap();

    // Use sequential mode when not parallel to ensure stable ordering
    let sequential = !options.parallel;
    let ir_option = IrBuildOption::new().with_sequential(sequential);
    build_llmcc_ir::<L>(&cc, ir_option).unwrap();

    let resolver_option = ResolverOption::default()
        .with_print_ir(options.print_ir)
        .with_sequential(sequential);
    let globals = collect_symbols_with::<L>(&cc, &resolver_option);

    // Bind symbols using new unified API
    bind_symbols_with::<L>(&cc, globals, &resolver_option);
    let mut project_graph = if options.build_block_reports
        || options.build_block_graph
        || options.keep_block_relations
        || options.build_arch_graph
    {
        let graph = ProjectGraph::new(&cc);
        Some(graph)
    } else {
        None
    };
    if let Some(project) = project_graph.as_mut() {
        let unit_graphs =
            build_llmcc_graph::<L>(&cc, GraphBuildOption::new().with_sequential(sequential))
                .unwrap();
        project.add_children(unit_graphs);
    }
    let (
        dep_graph_dot,
        arch_graph_dot,
        arch_graph_depth_0,
        arch_graph_depth_1,
        arch_graph_depth_2,
        arch_graph_depth_3,
        block_list,
        block_deps,
        block_graph,
        block_relations,
    ) = if let Some(project) = project_graph {
        project.connect_blocks();
        // Graph visualization
        let dep_graph: Option<String> = None; // TODO: implement dep_graph rendering
        let arch_graph: Option<String> = if options.build_arch_graph {
            Some(render_graph(&project, options.component_depth))
        } else {
            None
        };
        // Depth-specific arch graphs
        let arch_graph_d0: Option<String> = if options.build_arch_graph_depth_0 {
            Some(render_graph(&project, ComponentDepth::Project))
        } else {
            None
        };
        let arch_graph_d1: Option<String> = if options.build_arch_graph_depth_1 {
            Some(render_graph(&project, ComponentDepth::Crate))
        } else {
            None
        };
        let arch_graph_d2: Option<String> = if options.build_arch_graph_depth_2 {
            Some(render_graph(&project, ComponentDepth::Module))
        } else {
            None
        };
        let arch_graph_d3: Option<String> = if options.build_arch_graph_depth_3 {
            Some(render_graph(&project, ComponentDepth::File))
        } else {
            None
        };
        let (list, deps) = if options.build_block_reports {
            let (blocks, deps) = render_block_reports(&project);
            (Some(blocks), Some(deps))
        } else {
            (None, None)
        };
        let block_graph = if options.build_block_graph {
            Some(render_block_graph(&project))
        } else {
            None
        };
        let block_relations = if options.keep_block_relations {
            Some(snapshot_block_relations(&project))
        } else {
            None
        };
        (
            dep_graph,
            arch_graph,
            arch_graph_d0,
            arch_graph_d1,
            arch_graph_d2,
            arch_graph_d3,
            list,
            deps,
            block_graph,
            block_relations,
        )
    } else {
        (None, None, None, None, None, None, None, None, None, None)
    };

    let symbols = if options.keep_symbols {
        Some(snapshot_symbols(&cc))
    } else {
        None
    };

    let symbol_types = if options.keep_symbol_types {
        Some(snapshot_symbols(&cc))
    } else {
        None
    };

    let symbol_deps = if options.keep_symbol_deps {
        Some(snapshot_symbol_dependencies(&cc))
    } else {
        None
    };

    Ok(PipelineSummary {
        symbols,
        symbol_types,
        block_relations,
        dep_graph_dot,
        arch_graph_dot,
        arch_graph_depth_0,
        arch_graph_depth_1,
        arch_graph_depth_2,
        arch_graph_depth_3,
        block_list,
        block_deps,
        symbol_deps,
        block_graph,
        temp_dir_path: None,
    })
}

fn discover_language_files<L: LanguageTraitImpl>(
    root: &Path,
    _parallel: bool,
) -> Result<Vec<(String, String)>> {
    let supported = L::supported_extensions();
    let mut files = Vec::new();

    for entry in WalkDir::new(root) {
        let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }

        let Some(ext) = entry.path().extension().and_then(|ext| ext.to_str()) else {
            continue;
        };

        if !supported
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        {
            continue;
        }

        files.push(entry.path().to_string_lossy().to_string());
    }

    // Sort by numeric prefix in filename to preserve declaration order
    // e.g., "002_lib.rs" should come before "005_lib.rs" regardless of directory
    files.sort_by(|a, b| {
        let get_prefix = |path: &str| -> Option<usize> {
            let filename = Path::new(path).file_name()?.to_str()?;
            let prefix_end = filename.find('_')?;
            filename[..prefix_end].parse::<usize>().ok()
        };
        get_prefix(a).cmp(&get_prefix(b))
    });

    // Return both physical path (with prefix) and logical path (without prefix)
    let files: Vec<(String, String)> = files
        .into_iter()
        .map(|physical_path| {
            let logical_path = strip_numeric_prefix_from_path(&physical_path);
            (physical_path, logical_path)
        })
        .collect();

    Ok(files)
}

/// Strips numeric prefix like "000_" from the filename component of a path.
/// For example: "/tmp/src/000_main.rs" -> "/tmp/src/main.rs"
fn strip_numeric_prefix_from_path(path: &str) -> String {
    let path = Path::new(path);
    let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
        return path.to_string_lossy().to_string();
    };

    // Check for pattern: 3 digits followed by underscore
    if filename.len() > 4
        && filename[..3].chars().all(|c| c.is_ascii_digit())
        && &filename[3..4] == "_"
    {
        let stripped_filename = &filename[4..];
        if let Some(parent) = path.parent() {
            return parent.join(stripped_filename).to_string_lossy().to_string();
        } else {
            return stripped_filename.to_string();
        }
    }

    path.to_string_lossy().to_string()
}

/// Auto-mode pipeline that processes both Rust and TypeScript files
/// and merges their architecture graphs.
fn collect_pipeline_auto(
    project_root: &Path,
    options: &PipelineOptions,
) -> Result<PipelineSummary> {
    // Check if we have files for each language (don't pass file_paths,
    // let each pipeline discover its own files to preserve logical path handling)
    let rust_files = discover_language_files::<LangRust>(project_root, options.parallel)?;
    let ts_files = discover_language_files::<LangTypeScript>(project_root, options.parallel)?;

    // We need to process each language separately and merge the arch graphs
    let mut arch_graphs: Vec<String> = Vec::new();
    let mut arch_graphs_d0: Vec<String> = Vec::new();
    let mut arch_graphs_d1: Vec<String> = Vec::new();
    let mut arch_graphs_d2: Vec<String> = Vec::new();
    let mut arch_graphs_d3: Vec<String> = Vec::new();

    // Process Rust files if any exist (don't set file_paths to preserve logical paths)
    if !rust_files.is_empty() {
        let summary = collect_pipeline::<LangRust>(project_root, options)?;
        if let Some(graph) = summary.arch_graph_dot {
            arch_graphs.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_0 {
            arch_graphs_d0.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_1 {
            arch_graphs_d1.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_2 {
            arch_graphs_d2.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_3 {
            arch_graphs_d3.push(graph);
        }
    }

    // Process TypeScript files if any exist
    if !ts_files.is_empty() {
        let summary = collect_pipeline::<LangTypeScript>(project_root, options)?;
        if let Some(graph) = summary.arch_graph_dot {
            arch_graphs.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_0 {
            arch_graphs_d0.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_1 {
            arch_graphs_d1.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_2 {
            arch_graphs_d2.push(graph);
        }
        if let Some(graph) = summary.arch_graph_depth_3 {
            arch_graphs_d3.push(graph);
        }
    }

    // Merge arch graphs
    let merged_arch = if arch_graphs.is_empty() {
        None
    } else if arch_graphs.len() == 1 {
        Some(arch_graphs.into_iter().next().unwrap())
    } else {
        Some(merge_dot_graphs(&arch_graphs))
    };

    let merged_d0 = merge_if_multiple(arch_graphs_d0);
    let merged_d1 = merge_if_multiple(arch_graphs_d1);
    let merged_d2 = merge_if_multiple(arch_graphs_d2);
    let merged_d3 = merge_if_multiple(arch_graphs_d3);

    Ok(PipelineSummary {
        symbols: None,
        symbol_types: None,
        block_relations: None,
        dep_graph_dot: None,
        arch_graph_dot: merged_arch,
        arch_graph_depth_0: merged_d0,
        arch_graph_depth_1: merged_d1,
        arch_graph_depth_2: merged_d2,
        arch_graph_depth_3: merged_d3,
        block_list: None,
        block_deps: None,
        symbol_deps: None,
        block_graph: None,
        temp_dir_path: None,
    })
}

fn merge_if_multiple(graphs: Vec<String>) -> Option<String> {
    if graphs.is_empty() {
        None
    } else if graphs.len() == 1 {
        Some(graphs.into_iter().next().unwrap())
    } else {
        Some(merge_dot_graphs(&graphs))
    }
}

/// Merge multiple DOT graphs into one
fn merge_dot_graphs(graphs: &[String]) -> String {
    use std::fmt::Write;

    let mut merged = String::new();
    let _ = writeln!(merged, "digraph architecture {{");
    let _ = writeln!(merged, "  rankdir=TB;");
    let _ = writeln!(merged, "  ranksep=0.8;");
    let _ = writeln!(merged, "  nodesep=0.4;");
    let _ = writeln!(merged, "  splines=ortho;");
    let _ = writeln!(merged, "  concentrate=true;");
    let _ = writeln!(merged);
    let _ = writeln!(
        merged,
        r##"  node [shape=box, style="rounded,filled", fillcolor="#f0f0f0", fontname="Helvetica"];"##
    );
    let _ = writeln!(merged, r##"  edge [color="#888888", arrowsize=0.7];"##);
    let _ = writeln!(merged);
    let _ = writeln!(merged, "  labelloc=t;");
    let _ = writeln!(merged, "  fontsize=16;");
    let _ = writeln!(merged);

    for output in graphs {
        let lines: Vec<&str> = output.lines().collect();
        let mut in_content = false;
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.starts_with("digraph")
                || trimmed.starts_with("rankdir")
                || trimmed.starts_with("ranksep")
                || trimmed.starts_with("nodesep")
                || trimmed.starts_with("splines")
                || trimmed.starts_with("concentrate")
                || trimmed.starts_with("node [")
                || trimmed.starts_with("edge [")
                || trimmed.starts_with("labelloc")
                || trimmed.starts_with("fontsize")
                || trimmed.is_empty()
            {
                in_content = true;
                continue;
            }
            if trimmed == "}" {
                continue;
            }
            if in_content {
                let _ = writeln!(merged, "{}", line);
            }
        }
        let _ = writeln!(merged);
    }

    let _ = writeln!(merged, "}}");
    merged
}

fn render_block_graph(project: &ProjectGraph) -> String {
    let mut units: Vec<_> = project.units().iter().collect();
    if units.is_empty() {
        return "none\n".to_string();
    }

    units.sort_by_key(|unit| unit.unit_index());

    let mut sections = Vec::new();
    for unit_graph in units {
        let unit = project.cc.compile_unit(unit_graph.unit_index());
        let mut buf = String::new();
        render_block_graph_node(unit_graph.root(), unit, 0, &mut buf);
        sections.push(buf.trim_end().to_string());
    }

    if sections.is_empty() {
        "none\n".to_string()
    } else {
        let mut joined = sections.join("\n\n");
        joined.push('\n');
        joined
    }
}

fn render_block_graph_node(
    block_id: BlockId,
    unit: CompileUnit<'_>,
    depth: usize,
    buf: &mut String,
) {
    let block = unit.bb(block_id);
    let indent = "  ".repeat(depth);

    // Use the block's format methods for consistent output
    let label = block.format_block(unit);
    let suffix = block.format_suffix();
    let deps = block.format_deps(unit);

    let _ = write!(buf, "{indent}({label}");

    let children = block.children();
    if children.is_empty() && deps.is_empty() {
        buf.push(')');
        // Add suffix after closing paren (e.g., "@type i32")
        if let Some(suffix) = suffix {
            buf.push(' ');
            buf.push_str(&suffix);
        }
        buf.push('\n');
        return;
    }

    buf.push('\n');
    for child_id in children {
        // Skip Call blocks - they are now represented by @fdep/@tdep entries
        if unit.bb(child_id).kind() == llmcc_core::block::BlockKind::Call {
            continue;
        }
        render_block_graph_node(child_id, unit, depth + 1, buf);
    }
    // Render deps as pseudo-children (after real children)
    let child_indent = "  ".repeat(depth + 1);
    for dep in deps {
        let _ = writeln!(buf, "{child_indent}({dep})");
    }
    buf.push_str(&indent);
    buf.push_str(")\n");
}

fn snapshot_symbols<'a>(cc: &'a CompileCtxt<'a>) -> Vec<SymbolSnapshot> {
    let symbols = cc.get_all_symbols();
    let interner = &cc.interner;
    let mut rows = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        let name_str = interner
            .resolve_owned(symbol.name)
            .unwrap_or_else(|| "?".to_string());

        // Get type_of info
        let type_of = symbol.type_of().and_then(|sym_id| {
            cc.opt_get_symbol(sym_id).map(|type_sym| {
                let type_name = interner
                    .resolve_owned(type_sym.name)
                    .unwrap_or_else(|| "?".to_string());
                let type_unit = type_sym.unit_index().unwrap_or_default();
                format!("u{}:{} ({})", type_unit, sym_id.0 as u32, type_name)
            })
        });

        // Get block_id info
        let block_id = symbol.block_id().map(|bid| {
            format!(
                "u{}:{}",
                symbol.unit_index().unwrap_or_default(),
                bid.as_u32()
            )
        });

        rows.push(SymbolSnapshot {
            unit: symbol.unit_index().unwrap_or_default(),
            id: symbol.id().0 as u32,
            kind: format!("{:?}", symbol.kind()),
            name: name_str,
            is_global: symbol.is_global(),
            type_of,
            block_id,
        });
    }

    rows
}

fn snapshot_symbol_dependencies<'a>(_cc: &'a CompileCtxt<'a>) -> Vec<SymbolDependencySnapshot> {
    // Symbol dependency tracking is currently disabled
    // TODO: Implement via block relation traversal when needed
    Vec::new()
}

/// Snapshot block relations from the ProjectGraph.
fn snapshot_block_relations(project: &ProjectGraph) -> Vec<BlockRelationSnapshot> {
    use std::collections::BTreeMap;

    let cc = project.cc;
    let related_map = &cc.related_map;

    // Group relations by block
    let mut block_map: BTreeMap<BlockId, BlockRelationSnapshot> = BTreeMap::new();

    // First, collect all blocks that have relations
    for block_id in related_map.get_connected_blocks() {
        let Some(desc) = describe_block(block_id, cc) else {
            continue;
        };

        let label = format!("u{}:{}", desc.unit, block_id.as_u32());

        // Get all relations for this block
        let relations = related_map.get_all_relations(block_id);

        // Convert relations to grouped format
        let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (relation, target_ids) in relations.iter() {
            let rel_name = relation.to_string();

            for target_id in target_ids {
                // Get target label
                let target_label = if let Some(target_desc) = describe_block(*target_id, cc) {
                    format!("u{}:{}", target_desc.unit, target_id.as_u32())
                } else {
                    format!("?:{}", target_id.as_u32())
                };

                grouped
                    .entry(rel_name.clone())
                    .or_default()
                    .push(target_label);
            }
        }

        // Sort targets within each relation type
        for targets in grouped.values_mut() {
            targets.sort();
        }

        let relations_vec: Vec<(String, Vec<String>)> = grouped.into_iter().collect();

        block_map.insert(
            block_id,
            BlockRelationSnapshot {
                label,
                kind: desc.kind.clone(),
                name: desc.name.clone(),
                relations: relations_vec,
            },
        );
    }

    // Convert to vector, sorted by block id
    block_map.into_values().collect()
}

fn render_block_reports(
    project: &ProjectGraph,
) -> (Vec<BlockSnapshot>, Vec<SymbolDependencySnapshot>) {
    use std::collections::BTreeMap;

    let mut units: BTreeMap<usize, Vec<BlockDescriptor>> = BTreeMap::new();

    for unit_graph in project.units() {
        let unit_index = unit_graph.unit_index();
        let mut entries = Vec::new();

        for (_name_opt, kind, block_id) in project.cc.find_blocks_in_unit(unit_index) {
            let Some(mut desc) = describe_block(block_id, project.cc) else {
                continue;
            };
            desc.kind = kind.to_string();
            entries.push(desc);
        }

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        if !entries.is_empty() {
            units.insert(unit_index, entries);
        }
    }

    let mut block_rows = Vec::new();
    // Block deps tracking is currently disabled - return empty deps
    let deps: Vec<SymbolDependencySnapshot> = Vec::new();

    for (_unit, blocks) in units {
        for block in blocks {
            let label = format!("u{}:{}", block.unit, block.id.as_u32());
            block_rows.push(BlockSnapshot {
                label,
                kind: block.kind.clone(),
                name: block.name.clone(),
            });
        }
    }

    block_rows.sort_by(|a, b| a.label.cmp(&b.label));
    (block_rows, deps)
}

#[derive(Clone)]
struct BlockDescriptor {
    name: String,
    kind: String,
    unit: usize,
    id: llmcc_core::graph_builder::BlockId,
}

fn describe_block<'a>(
    block_id: llmcc_core::graph_builder::BlockId,
    cc: &'a llmcc_core::context::CompileCtxt<'a>,
) -> Option<BlockDescriptor> {
    let (unit, name, kind) = cc.get_block_info(block_id)?;

    // Try to get a proper name from multiple sources:
    // 1. From block info (indexed name)
    // 2. From the block's specific type (e.g., BlockField.name)
    // 3. From the block's base (HIR node)
    // 4. From first identifier child node
    // 5. From associated symbol
    // 6. Fall back to block#N
    let name = name
        .or_else(|| {
            // Try to get name from specific block types using DashMap lookup
            cc.block_arena.get_bb(block_id.0 as usize).and_then(|bb| {
                // Try field name
                if let Some(field) = bb.as_field()
                    && !field.name.is_empty()
                {
                    return Some(field.name.clone());
                }
                // Try base name
                bb.base()
                    .and_then(|base| base.opt_get_name())
                    .filter(|n| !n.is_empty())
                    .map(|s| s.to_string())
            })
        })
        .or_else(|| {
            // Try to find first identifier child of the node
            cc.block_arena.get_bb(block_id.0 as usize).and_then(|bb| {
                let node = bb.base()?.node;
                // Recursively search for first identifier in children
                find_first_ident_name(cc, &node)
            })
        })
        .or_else(|| {
            // Try to get name from associated symbol
            cc.find_symbol_by_block_id(block_id)
                .and_then(|sym| cc.interner.resolve_owned(sym.name))
        })
        .unwrap_or_else(|| format!("block#{block_id}"));

    Some(BlockDescriptor {
        name,
        kind: kind.to_string(),
        unit,
        id: block_id,
    })
}

/// Recursively find the first identifier name in a node's children
fn find_first_ident_name<'a>(
    cc: &'a llmcc_core::context::CompileCtxt<'a>,
    node: &llmcc_core::ir::HirNode<'a>,
) -> Option<String> {
    use llmcc_core::ir::HirKind;

    // Check if this node itself is an identifier
    if node.is_kind(HirKind::Identifier)
        && let Some(ident) = node.as_ident()
    {
        return Some(ident.name.to_string());
    }

    // Search through children
    for child_id in node.child_ids() {
        if let Some(child_node) = cc.get_hir_node(*child_id) {
            if child_node.is_kind(HirKind::Identifier)
                && let Some(ident) = child_node.as_ident()
            {
                return Some(ident.name.to_string());
            }
            // Recurse into internal nodes
            if child_node.is_kind(HirKind::Internal)
                && let Some(name) = find_first_ident_name(cc, &child_node)
            {
                return Some(name);
            }
        }
    }
    None
}
