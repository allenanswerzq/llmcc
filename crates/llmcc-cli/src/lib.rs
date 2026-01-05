pub mod options;

use std::collections::HashSet;
use std::io;
use std::time::Instant;

use ignore::WalkBuilder;
use tracing::{info, warn};

use llmcc_core::graph_builder::{GraphBuildOption, build_llmcc_graph};
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
