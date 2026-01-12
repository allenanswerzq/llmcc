pub mod options;

use std::collections::{HashMap, HashSet};
use std::io;
use std::sync::Arc;
use std::time::Instant;

use ignore::WalkBuilder;
use tracing::{info, warn};

use llmcc_core::context::FileOrder;
use llmcc_core::graph_builder::{GraphBuildOption, build_llmcc_graph};
use llmcc_core::ir_builder::IrBuildOption;
use llmcc_core::lang_def::{LanguageTrait, LanguageTraitImpl};
use llmcc_core::*;
use llmcc_dot::{ComponentDepth, RenderOptions, render_graph_with_options};
use llmcc_resolver::{ResolverOption, bind_symbols_with, build_and_collect_symbols};

pub use options::{CommonTestOptions, GraphOptions, ProcessingOptions};

#[cfg(feature = "profile")]
use std::fs::File;

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "test"
            | "tests"
            | "testing"
            | "example"
            | "examples"
            | "doc"
            | "docs"
            | "bench"
            | "benches"
            | "benchmark"
            | "benchmarks"
            // Build output directories
            | "target"
            | "build"
            | "dist"
            | "out"
            // Vendor/dependency directories
            | "vendor"
            | "node_modules"
            | "third_party"
    )
}

/// Check if a file should be skipped due to size.
/// Returns Some(reason) if the file should be skipped, None otherwise.
fn should_skip_file(_path: &std::path::Path) -> Option<String> {
    None
}

/// Generate a flamegraph from CPU profiling data
#[cfg(feature = "profile")]
pub fn profile_phase<F, R>(name: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    use pprof::ProfilerGuard;

    // Use 1000Hz for higher resolution
    let guard = ProfilerGuard::new(1000).expect("Failed to start profiler");
    let result = f();

    if let Ok(report) = guard.report().build() {
        let filename = format!("{}.svg", name);
        let file = File::create(&filename).expect("Failed to create flamegraph file");
        report.flamegraph(file).expect("Failed to write flamegraph");
        info!("Flamegraph saved to {}", filename);
    }

    result
}

#[cfg(not(feature = "profile"))]
pub fn profile_phase<F, R>(_name: &str, f: F) -> R
where
    F: FnOnce() -> R,
{
    f()
}

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

// ============================================================================
// Language Processor Registry
// ============================================================================
//
// This registry-based approach allows any number of languages to be registered
// dynamically, solving the scalability problem of generic parameters like
// `run_main_multi<L1, L2, L3, ..., L100>`.
//
// Usage:
//   let mut registry = LangProcessorRegistry::new();
//   registry.register::<LangRust>("rust");
//   registry.register::<LangTypeScript>("typescript");
//   registry.register::<LangGo>("go");  // Add as many as needed
//   run_main_auto(&opts, &registry)
//
// ============================================================================

/// Type alias for a language processing function.
/// Takes options and files, returns DOT output or error.
pub type LangProcessor = Arc<dyn Fn(&LlmccOptions, &[String]) -> Result<Option<String>, DynError> + Send + Sync>;

/// A registered language with its processor.
pub struct RegisteredLang {
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub processor: LangProcessor,
}

/// Registry of language processors for multi-language support.
/// This allows any number of languages to be registered dynamically.
pub struct LangProcessorRegistry {
    languages: Vec<RegisteredLang>,
    extension_map: HashMap<&'static str, usize>, // ext -> index
}

impl Default for LangProcessorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl LangProcessorRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            languages: Vec::new(),
            extension_map: HashMap::new(),
        }
    }

    /// Register a language by its LanguageTraitImpl type
    pub fn register<L: LanguageTraitImpl + 'static>(&mut self, name: &'static str) {
        let idx = self.languages.len();
        let extensions = L::supported_extensions();

        // Create a type-erased processor closure
        let processor: LangProcessor = Arc::new(move |opts, files| {
            run_main_with_files::<L>(opts, files)
        });

        self.languages.push(RegisteredLang {
            name,
            extensions,
            processor,
        });

        for ext in extensions {
            self.extension_map.insert(*ext, idx);
        }
    }

    /// Get all supported extensions across all languages
    pub fn all_extensions(&self) -> Vec<&'static str> {
        self.extension_map.keys().copied().collect()
    }

    /// Partition files by language
    pub fn partition_files(&self, files: &[String]) -> HashMap<&'static str, Vec<String>> {
        let mut partitions: HashMap<&'static str, Vec<String>> = HashMap::new();

        for file in files {
            let path = std::path::Path::new(file);
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if let Some(&idx) = self.extension_map.get(ext) {
                    partitions
                        .entry(self.languages[idx].name)
                        .or_default()
                        .push(file.clone());
                }
            }
        }

        partitions
    }

    /// Get the processor for a language by name
    pub fn get_processor(&self, name: &str) -> Option<&LangProcessor> {
        self.languages.iter()
            .find(|l| l.name == name)
            .map(|l| &l.processor)
    }

    /// Get number of registered languages
    pub fn len(&self) -> usize {
        self.languages.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.languages.is_empty()
    }
}

/// Run llmcc with automatic multi-language detection using registry.
/// Discovers files, partitions by language, processes each, and merges results.
pub fn run_main_auto(opts: &LlmccOptions, registry: &LangProcessorRegistry) -> Result<Option<String>, DynError> {
    validate_options(opts)?;

    // Gather all extensions from the registry
    let all_exts: std::collections::HashSet<&str> = registry.all_extensions().into_iter().collect();

    // Discover all files
    let all_files = discover_files_with_extensions(opts, &all_exts)?;

    // Partition files by language
    let partitions = registry.partition_files(&all_files);

    info!(
        "Multi-language mode: {} languages, {} total files",
        partitions.len(),
        all_files.len()
    );
    for (lang, files) in &partitions {
        info!("  {}: {} files", lang, files.len());
    }

    // Process each language and collect DOT outputs
    let mut dot_outputs: Vec<String> = Vec::new();

    for (lang_name, files) in &partitions {
        if files.is_empty() {
            continue;
        }

        let processor = registry.get_processor(lang_name)
            .ok_or_else(|| format!("No processor for language: {}", lang_name))?;

        let lang_opts = LlmccOptions {
            files: Vec::new(),
            dirs: Vec::new(), // Already discovered
            output: None,
            print_ir: opts.print_ir,
            print_block: opts.print_block,
            graph: opts.graph,
            component_depth: opts.component_depth,
            pagerank_top_k: opts.pagerank_top_k,
            cluster_by_crate: opts.cluster_by_crate,
            short_labels: opts.short_labels,
        };

        if let Some(output) = processor(&lang_opts, files)? {
            dot_outputs.push(output);
        }
    }

    // Merge DOT outputs if we have multiple
    if dot_outputs.is_empty() {
        Ok(None)
    } else if dot_outputs.len() == 1 {
        Ok(Some(dot_outputs.into_iter().next().unwrap()))
    } else {
        Ok(Some(merge_dot_outputs(&dot_outputs)))
    }
}

/// Discover files matching any of the given extensions
fn discover_files_with_extensions(
    opts: &LlmccOptions,
    extensions: &std::collections::HashSet<&str>,
) -> Result<Vec<String>, DynError> {
    let discovery_start = Instant::now();

    let mut seen = std::collections::HashSet::new();
    let mut requested_files = Vec::new();
    let mut skipped_count = 0usize;

    let mut add_path = |path: &str| {
        if seen.contains(path) {
            return;
        }

        if let Some(reason) = should_skip_file(std::path::Path::new(path)) {
            warn!("Skipping {}: {}", path, reason);
            skipped_count += 1;
            return;
        }

        seen.insert(path.to_string());
        requested_files.push(path.to_string());
    };

    for file in &opts.files {
        add_path(file);
    }

    if !opts.dirs.is_empty() {
        let walker_threads = std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(1);

        for dir in &opts.dirs {
            let mut builder = WalkBuilder::new(dir);
            builder
                .standard_filters(true)
                .follow_links(false)
                .threads(walker_threads)
                .filter_entry(|entry| {
                    if entry.depth() == 0 {
                        return true;
                    }

                    let Some(file_type) = entry.file_type() else {
                        return true;
                    };

                    if !file_type.is_dir() {
                        return true;
                    }

                    let Some(name) = entry.file_name().to_str() else {
                        return true;
                    };

                    let lowered = name.to_ascii_lowercase();
                    !should_skip_dir(lowered.as_str())
                });

            let walker = builder.build();
            for entry in walker {
                let entry = entry.map_err(|e| {
                    io::Error::other(format!("Failed to walk directory {dir}: {e}"))
                })?;

                if !entry
                    .file_type()
                    .map(|file_type| file_type.is_file())
                    .unwrap_or(false)
                {
                    continue;
                }

                let path = entry.path();
                let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
                    continue;
                };

                // Accept files with any registered extension
                if extensions.contains(ext) {
                    add_path(&path.to_string_lossy());
                }
            }
        }
    }

    if skipped_count > 0 {
        info!("Skipped {} files due to size limits", skipped_count);
    }

    info!(
        "File discovery: {:.2}s ({} files)",
        discovery_start.elapsed().as_secs_f64(),
        requested_files.len()
    );

    if requested_files.is_empty() {
        return Err("No input files found. Check that the directory contains supported file types.".into());
    }

    Ok(requested_files)
}

/// Run llmcc with automatic multi-language detection.
/// Partitions files by extension and processes each language separately.
/// DEPRECATED: Use run_main_auto with LangProcessorRegistry for N languages.
pub fn run_main_multi<L1, L2>(opts: &LlmccOptions) -> Result<Option<String>, DynError>
where
    L1: LanguageTraitImpl + 'static,
    L2: LanguageTraitImpl + 'static,
{
    // Build registry with the two languages
    let mut registry = LangProcessorRegistry::new();
    registry.register::<L1>(L1::supported_extensions().first().unwrap_or(&"L1"));
    registry.register::<L2>(L2::supported_extensions().first().unwrap_or(&"L2"));

    run_main_auto(opts, &registry)
}

/// Merge multiple DOT graph outputs into a single graph.
fn merge_dot_outputs(outputs: &[String]) -> String {
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

    // Extract content from each DOT file (skip the header and closing brace)
    for output in outputs {
        let lines: Vec<&str> = output.lines().collect();
        let mut in_content = false;
        for line in &lines {
            let trimmed = line.trim();
            // Skip header lines
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
            // Skip closing brace
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

/// Internal helper to run with pre-discovered files
fn run_main_with_files<L>(opts: &LlmccOptions, files: &[String]) -> Result<Option<String>, DynError>
where
    L: LanguageTraitImpl,
{
    let parse_start = Instant::now();
    info!("Parsing {} {} files", files.len(), L::supported_extensions().first().unwrap_or(&"unknown"));

    let cc = profile_phase("parsing", || {
        CompileCtxt::from_files_with_order::<L>(files, FileOrder::BySizeDescending)
    })?;
    info!(
        "Parsing & tree-sitter: {:.2}s",
        parse_start.elapsed().as_secs_f64()
    );
    log_parse_metrics(&cc.build_metrics);

    // Fused IR build + symbol collection
    let build_collect_start = Instant::now();
    let resolver_option = ResolverOption::default()
        .with_print_ir(opts.print_ir)
        .with_sequential(false);
    let globals = profile_phase("build_and_collect", || {
        build_and_collect_symbols::<L>(&cc, IrBuildOption::default(), &resolver_option)
    })?;
    info!(
        "IR build + Symbol collection: {:.2}s",
        build_collect_start.elapsed().as_secs_f64()
    );

    let mut pg = ProjectGraph::new(&cc);

    let bind_start = Instant::now();
    profile_phase("binding", || {
        bind_symbols_with::<L>(&cc, globals, &resolver_option);
    });
    info!("Symbol binding: {:.2}s", bind_start.elapsed().as_secs_f64());

    let graph_build_start = Instant::now();
    let unit_graphs = build_llmcc_graph::<L>(&cc, GraphBuildOption::new())?;
    pg.add_children(unit_graphs);
    info!(
        "Graph building: {:.2}s",
        graph_build_start.elapsed().as_secs_f64()
    );

    let link_start = Instant::now();
    pg.connect_blocks();
    info!("Linking units: {:.2}s", link_start.elapsed().as_secs_f64());

    if opts.print_block {
        for unit_graph in pg.units() {
            let unit = cc.compile_unit(unit_graph.unit_index());
            let _ = print_llmcc_graph(unit_graph.root(), unit);
        }
    }

    let output = generate_outputs(opts, &pg);
    Ok(output)
}

pub fn run_main<L>(opts: &LlmccOptions) -> Result<Option<String>, DynError>
where
    L: LanguageTraitImpl,
{
    validate_options(opts)?;

    let requested_files = discover_requested_files::<L>(opts)?;

    let parse_start = Instant::now();
    info!("Parsing total {} files", requested_files.len());
    // Use size-based ordering for better parallel load balancing in production
    let cc = profile_phase("parsing", || {
        CompileCtxt::from_files_with_order::<L>(&requested_files, FileOrder::BySizeDescending)
    })?;
    info!(
        "Parsing & tree-sitter: {:.2}s",
        parse_start.elapsed().as_secs_f64()
    );
    log_parse_metrics(&cc.build_metrics);

    // Fused IR build + symbol collection (eliminates gap between phases)
    let build_collect_start = Instant::now();
    let resolver_option = ResolverOption::default()
        .with_print_ir(opts.print_ir)
        .with_sequential(false);
    let globals = profile_phase("build_and_collect", || {
        build_and_collect_symbols::<L>(&cc, IrBuildOption::default(), &resolver_option)
    })?;
    info!(
        "IR build + Symbol collection: {:.2}s",
        build_collect_start.elapsed().as_secs_f64()
    );

    let mut pg = ProjectGraph::new(&cc);

    let bind_start = Instant::now();
    profile_phase("binding", || {
        bind_symbols_with::<L>(&cc, globals, &resolver_option);
    });
    info!("Symbol binding: {:.2}s", bind_start.elapsed().as_secs_f64());

    let graph_build_start = Instant::now();
    let unit_graphs = build_llmcc_graph::<L>(&cc, GraphBuildOption::new())?;
    pg.add_children(unit_graphs);
    info!(
        "Graph building: {:.2}s",
        graph_build_start.elapsed().as_secs_f64()
    );

    let link_start = Instant::now();
    pg.connect_blocks();
    info!("Linking units: {:.2}s", link_start.elapsed().as_secs_f64());

    // Print blocks after connect_blocks so that all resolved references are shown
    if opts.print_block {
        for unit_graph in pg.units() {
            let unit = cc.compile_unit(unit_graph.unit_index());
            let _ = print_llmcc_graph(unit_graph.root(), unit);
        }
    }

    let output = generate_outputs(opts, &pg);

    Ok(output)
}

fn validate_options(_opts: &LlmccOptions) -> Result<(), DynError> {
    // Validation simplified - files/dirs conflict handled by clap
    Ok(())
}

fn discover_requested_files<L>(opts: &LlmccOptions) -> Result<Vec<String>, DynError>
where
    L: LanguageTrait,
{
    let discovery_start = Instant::now();

    let mut seen = HashSet::new();
    let mut requested_files = Vec::new();
    let mut skipped_count = 0usize;

    let mut add_path = |path: &str| {
        if seen.contains(path) {
            return;
        }

        // Check if file should be skipped due to size
        if let Some(reason) = should_skip_file(std::path::Path::new(path)) {
            warn!("Skipping {}: {}", path, reason);
            skipped_count += 1;
            return;
        }

        seen.insert(path.to_string());
        requested_files.push(path.to_string());
    };

    for file in &opts.files {
        add_path(file);
    }

    if !opts.dirs.is_empty() {
        let supported_exts = L::supported_extensions();
        let walker_threads = std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(1);

        for dir in &opts.dirs {
            let mut builder = WalkBuilder::new(dir);
            builder
                .standard_filters(true)
                .follow_links(false)
                .threads(walker_threads)
                .filter_entry(|entry| {
                    if entry.depth() == 0 {
                        return true;
                    }

                    let Some(file_type) = entry.file_type() else {
                        return true;
                    };

                    if !file_type.is_dir() {
                        return true;
                    }

                    let Some(name) = entry.file_name().to_str() else {
                        return true;
                    };

                    let lowered = name.to_ascii_lowercase();
                    !should_skip_dir(lowered.as_str())
                });

            let walker = builder.build();
            for entry in walker {
                let entry = entry.map_err(|e| {
                    io::Error::other(format!("Failed to walk directory {dir}: {e}"))
                })?;

                if !entry
                    .file_type()
                    .map(|file_type| file_type.is_file())
                    .unwrap_or(false)
                {
                    continue;
                }

                let path = entry.path();
                let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
                    continue;
                };

                if supported_exts.contains(&ext) {
                    add_path(&path.to_string_lossy());
                }
            }
        }
    }

    if skipped_count > 0 {
        info!("Skipped {} files due to size limits", skipped_count);
    }

    info!(
        "File discovery: {:.2}s ({} files)",
        discovery_start.elapsed().as_secs_f64(),
        requested_files.len()
    );

    if requested_files.is_empty() {
        return Err("No input files provided. --lang not set correct maybe".into());
    }

    Ok(requested_files)
}

fn log_parse_metrics(metrics: &llmcc_core::context::BuildMetrics) {
    if metrics.file_read_seconds > 0.0 {
        info!("  File I/O: {:.2}s", metrics.file_read_seconds);
    }
    if metrics.parse_wall_seconds > 0.0 {
        info!(
            "  Tree-sitter wall: {:.2}s (cpu {:.2}s across {} files, avg {:.4}s)",
            metrics.parse_wall_seconds,
            metrics.parse_cpu_seconds,
            metrics.parse_file_count,
            metrics.parse_avg_seconds
        );
    }
    if !metrics.parse_slowest.is_empty() {
        info!("  Slowest parses:");
        for metric in &metrics.parse_slowest {
            info!("    {:.2}s {}", metric.seconds, metric.path);
        }
    }
}

fn generate_outputs<'tcx>(opts: &LlmccOptions, pg: &'tcx ProjectGraph<'tcx>) -> Option<String> {
    if opts.graph {
        let render_start = Instant::now();
        let render_options = RenderOptions {
            show_orphan_nodes: false,
            pagerank_top_k: opts.pagerank_top_k,
            cluster_by_crate: opts.cluster_by_crate,
            short_labels: opts.short_labels,
        };
        let result = render_graph_with_options(pg, opts.component_depth, &render_options);
        info!(
            "Graph rendering: {:.2}s",
            render_start.elapsed().as_secs_f64()
        );
        Some(result)
    } else {
        None
    }
}
