//! Core processing pipeline: parse → build IR → bind symbols → build graph.

use std::time::Instant;

use tracing::info;

use llmcc_core::context::FileOrder;
use llmcc_core::graph::ProjectGraph;
use llmcc_core::graph_builder::{GraphBuildOption, build_llmcc_graph};
use llmcc_core::ir_builder::IrBuildOption;
use llmcc_core::lang_def::LanguageTraitImpl;
use llmcc_core::{CompileCtxt, Result, print_llmcc_graph};
use llmcc_resolver::{ResolverOption, bind_symbols_with, build_and_collect_symbols};

use crate::LlmccOptions;
use crate::output::generate_dot_output;
use crate::profile::profile_phase;

/// Process a set of files for a single language.
///
/// This is the core pipeline:
/// 1. Parse files with tree-sitter
/// 2. Build IR and collect symbols
/// 3. Bind symbols (resolve references)
/// 4. Build the graph
/// 5. Generate output (if requested)
pub fn process_files<L>(opts: &LlmccOptions, files: &[String]) -> Result<Option<String>>
where
    L: LanguageTraitImpl,
{
    let lang_name = L::supported_extensions().first().unwrap_or(&"unknown");

    // 1. Parse
    let parse_start = Instant::now();
    info!("Parsing {} {} files", files.len(), lang_name);

    let cc = profile_phase("parsing", || {
        CompileCtxt::from_files_with_order::<L>(files, FileOrder::BySizeDescending)
    })?;

    info!(
        "Parsing & tree-sitter: {:.2}s",
        parse_start.elapsed().as_secs_f64()
    );
    log_parse_metrics(&cc.build_metrics);

    // 2. Build IR + collect symbols (fused for efficiency)
    let build_start = Instant::now();
    let resolver_option = ResolverOption::default()
        .with_print_ir(opts.print_ir)
        .with_sequential(false);

    let globals = profile_phase("build_and_collect", || {
        build_and_collect_symbols::<L>(&cc, IrBuildOption::default(), &resolver_option)
    })?;

    info!(
        "IR build + Symbol collection: {:.2}s",
        build_start.elapsed().as_secs_f64()
    );

    // 3. Bind symbols
    let bind_start = Instant::now();
    profile_phase("binding", || {
        bind_symbols_with::<L>(&cc, globals, &resolver_option);
    });
    info!("Symbol binding: {:.2}s", bind_start.elapsed().as_secs_f64());

    // 4. Build graph
    let graph_start = Instant::now();
    let mut pg = ProjectGraph::new(&cc);
    let unit_graphs = build_llmcc_graph::<L>(&cc, GraphBuildOption::new())?;
    pg.add_children(unit_graphs);
    info!(
        "Graph building: {:.2}s",
        graph_start.elapsed().as_secs_f64()
    );

    // 5. Link cross-file references
    let link_start = Instant::now();
    pg.connect_blocks();
    info!("Linking units: {:.2}s", link_start.elapsed().as_secs_f64());

    // Debug output
    if opts.print_block {
        for unit_graph in pg.units() {
            let unit = cc.compile_unit(unit_graph.unit_index());
            let _ = print_llmcc_graph(unit_graph.root(), unit);
        }
    }

    // 6. Generate output
    Ok(generate_dot_output(opts, &pg))
}

/// Log parsing performance metrics.
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
