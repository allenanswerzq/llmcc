use std::collections::HashSet;
use std::io;
use std::sync::Once;
use std::time::Instant;

use ignore::WalkBuilder;
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use tracing::info;

use llmcc_core::lang_def::LanguageTrait;
use llmcc_core::symbol::Scope;
use llmcc_core::*;
use llmcc_resolver::{CollectedSymbols, apply_collected_symbols};

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
    )
}

static RAYON_INIT: Once = Once::new();

fn init_rayon_pool() {
    RAYON_INIT.call_once(|| {
        let available = std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(1);
        let target = available.clamp(1, 12);
        if let Err(err) = ThreadPoolBuilder::new()
            .num_threads(target)
            .thread_name(|index| format!("llmcc-worker-{index}"))
            .build_global()
        {
            tracing::debug!(?err, "Rayon global pool already initialized");
        } else {
            tracing::debug!(threads = target, "Initialized Rayon global thread pool");
        }
    });
}

pub struct LlmccOptions {
    pub files: Vec<String>,
    pub dirs: Vec<String>,
    pub print_ir: bool,
    pub print_block: bool,
    pub design_graph: bool,
    pub pagerank: bool,
    pub top_k: Option<usize>,
    pub query: Option<String>,
    pub depends: bool,
    pub dependents: bool,
    pub recursive: bool,
    pub summary: bool,
}

pub fn run_main<L>(opts: &LlmccOptions) -> Result<Option<String>, DynError>
where
    L: LanguageTrait<SymbolCollection = CollectedSymbols>,
{
    let total_start = Instant::now();

    init_rayon_pool();

    validate_options(opts)?;

    let requested_files = discover_requested_files::<L>(opts)?;

    let parse_start = Instant::now();
    let cc = CompileCtxt::from_files::<L>(&requested_files)?;
    info!(
        "Parsing & tree-sitter: {:.2}s",
        parse_start.elapsed().as_secs_f64()
    );
    log_parse_metrics(&cc.build_metrics);

    let files = cc.get_files();

    let ir_start = Instant::now();
    build_llmcc_ir::<L>(&cc, IrBuildOption)?;
    info!("IR building: {:.2}s", ir_start.elapsed().as_secs_f64());

    let globals = cc.create_globals();

    if opts.print_ir {
        for (index, _) in files.iter().enumerate() {
            let unit = cc.compile_unit(index);
            print_llmcc_ir(unit);
        }
    }

    let symbols_start = Instant::now();
    let collections = collect_and_apply_symbols::<L>(&cc, globals, files.len());
    info!(
        "Symbol collection: {:.2}s",
        symbols_start.elapsed().as_secs_f64()
    );

    let mut pg = ProjectGraph::new(&cc);
    let graph_config = GraphBuildConfig;

    let graph_build_start = Instant::now();
    bind_symbols_for_units::<L>(&cc, globals, &collections);

    let unit_graphs = build_unit_graphs::<L>(&cc, graph_config, files.len())?;
    for (index, unit_graph) in unit_graphs {
        let unit = cc.compile_unit(index);
        if opts.print_block {
            print_llmcc_graph(unit_graph.root(), unit);
        }
        pg.add_child(unit_graph);
    }
    info!(
        "Graph building: {:.2}s",
        graph_build_start.elapsed().as_secs_f64()
    );

    let link_start = Instant::now();
    pg.link_units();
    info!("Linking units: {:.2}s", link_start.elapsed().as_secs_f64());

    let output = generate_outputs(opts, &mut pg);
    info!("Total time: {:.2}s", total_start.elapsed().as_secs_f64());

    Ok(output)
}

fn validate_options(opts: &LlmccOptions) -> Result<(), DynError> {
    if !opts.files.is_empty() && !opts.dirs.is_empty() {
        return Err("Specify either --file or --dir, not both".into());
    }

    if opts.pagerank && !opts.design_graph {
        return Err("--pagerank requires --design-graph".into());
    }

    if opts.depends && opts.dependents {
        return Err("--depends and --dependents are mutually exclusive".into());
    }

    Ok(())
}

fn discover_requested_files<L>(opts: &LlmccOptions) -> Result<Vec<String>, DynError>
where
    L: LanguageTrait,
{
    let discovery_start = Instant::now();

    let mut seen = HashSet::new();
    let mut requested_files = Vec::new();

    let mut add_path = |path: String| {
        if seen.insert(path.clone()) {
            requested_files.push(path);
        }
    };

    for file in &opts.files {
        add_path(file.clone());
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
                    add_path(path.to_string_lossy().into_owned());
                }
            }
        }
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

fn collect_and_apply_symbols<'tcx, L>(
    cc: &'tcx CompileCtxt<'tcx>,
    globals: &'tcx Scope<'tcx>,
    unit_count: usize,
) -> Vec<CollectedSymbols>
where
    L: LanguageTrait<SymbolCollection = CollectedSymbols>,
{
    let mut indexed_collections: Vec<_> = (0..unit_count)
        .into_par_iter()
        .map(|index| {
            let unit = cc.compile_unit(index);
            (index, L::collect_symbols(unit))
        })
        .collect();

    indexed_collections.sort_by_key(|(index, _)| *index);

    let mut collections = Vec::with_capacity(indexed_collections.len());
    for (index, collected) in indexed_collections {
        let unit = cc.compile_unit(index);
        apply_collected_symbols(unit, globals, &collected);
        collections.push(collected);
    }

    collections
}

fn bind_symbols_for_units<'tcx, L>(
    cc: &'tcx CompileCtxt<'tcx>,
    globals: &'tcx Scope<'tcx>,
    collections: &[CollectedSymbols],
) where
    L: LanguageTrait<SymbolCollection = CollectedSymbols>,
{
    for (index, collection) in collections.iter().enumerate() {
        let unit = cc.compile_unit(index);
        L::bind_symbols(unit, globals, collection);
    }
}

fn build_unit_graphs<'tcx, L>(
    cc: &'tcx CompileCtxt<'tcx>,
    graph_config: GraphBuildConfig,
    unit_count: usize,
) -> Result<Vec<(usize, llmcc_core::graph_builder::UnitGraph)>, DynError>
where
    L: LanguageTrait<SymbolCollection = CollectedSymbols>,
{
    let mut unit_graph_results: Vec<_> = (0..unit_count)
        .into_par_iter()
        .map(|index| {
            let unit = cc.compile_unit(index);
            (index, build_llmcc_graph::<L>(unit, index, graph_config))
        })
        .collect();

    unit_graph_results.sort_by_key(|(index, _)| *index);

    let mut graphs = Vec::with_capacity(unit_graph_results.len());
    for (index, graph_result) in unit_graph_results {
        graphs.push((index, graph_result?));
    }

    Ok(graphs)
}

fn generate_outputs<'tcx>(opts: &LlmccOptions, pg: &'tcx mut ProjectGraph<'tcx>) -> Option<String> {
    if opts.design_graph {
        if opts.pagerank {
            let limit = Some(opts.top_k.unwrap_or(80));
            pg.set_compact_rank_limit(limit);

            let pagerank_start = Instant::now();
            let result = pg.render_design_graph();
            info!(
                "PageRank & graph rendering: {:.2}s",
                pagerank_start.elapsed().as_secs_f64()
            );
            Some(result)
        } else {
            let render_start = Instant::now();
            let result = pg.render_design_graph();
            info!(
                "Graph rendering: {:.2}s",
                render_start.elapsed().as_secs_f64()
            );
            Some(result)
        }
    } else if let Some(name) = opts.query.as_ref() {
        let query = ProjectQuery::new(&*pg);
        let query_result = if opts.dependents {
            if opts.recursive {
                query.find_depended_recursive(name)
            } else {
                query.find_depended(name)
            }
        } else if opts.recursive {
            query.find_depends_recursive(name)
        } else {
            query.find_depends(name)
        };
        let formatted = if opts.summary {
            query_result.format_summary()
        } else {
            query_result.format_for_llm()
        };
        Some(formatted)
    } else {
        None
    }
}
