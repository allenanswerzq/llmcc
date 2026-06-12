use std::collections::HashSet;
use std::io;
use std::time::Instant;

use ignore::WalkBuilder;
use tracing::info;

use llmcc_core::context::{BuildMetrics, FileOrder};
use llmcc_core::graph::ProjectGraph;
use llmcc_core::lang_def::Language as CoreLanguage;
use llmcc_core::{CompileCtxt, Error, ResolveOptions, Result, print_llmcc_graph};
use llmcc_core::{GraphBuildOptions, build_graphs};
use llmcc_cpp::LangCpp;
use llmcc_dot::{RenderOptions, render_graph_with_options};
use llmcc_resolver::{bind_symbols_with, build_and_collect_symbols};
use llmcc_rust::LangRust;
use llmcc_ts::LangTypeScript;

use crate::{Language, RunnerOptions};

pub struct Runner {
    lang: Language,
    options: RunnerOptions,
}

impl Runner {
    pub fn new(lang: Language, options: RunnerOptions) -> Self {
        Self { lang, options }
    }

    pub fn execute(self) -> Result<()> {
        let started = Instant::now();
        if let Err(err) = self.do_execute() {
            tracing::error!(error = %err, "execution failed");
            return Err(err);
        }

        let total_secs = started.elapsed().as_secs_f64();
        tracing::info!(total_secs, "complete");
        eprintln!("Total time: {total_secs:.2}s");
        Ok(())
    }

    fn do_execute(&self) -> Result<()> {
        let output = match self.lang {
            Language::Rust => self.process_language::<LangRust>(),
            Language::Typescript => self.process_language::<LangTypeScript>(),
            Language::Cpp => self.process_language::<LangCpp>(),
        }?;

        self.emit_output(output)
    }

    fn process_language<L: CoreLanguage>(&self) -> Result<Option<String>> {
        let files = self.discover_files(L::extensions())?;

        let lang_name = L::extensions().first().unwrap_or(&"unknown");

        let parse_start = Instant::now();
        info!("Parsing {} {} files", files.len(), lang_name);

        let cc = CompileCtxt::from_files_with_order::<L>(&files, FileOrder::BySizeDescending)?;

        info!(
            "Parsing & tree-sitter: {:.2}s",
            parse_start.elapsed().as_secs_f64()
        );
        Self::log_parse_metrics(cc.build_metrics());

        let build_start = Instant::now();
        let resolve_options = ResolveOptions::default()
            .with_print_ir(self.options.print_ir)
            .with_sequential(false);

        let globals = build_and_collect_symbols::<L>(&cc, &resolve_options)?;

        info!(
            "IR build + Symbol collection: {:.2}s",
            build_start.elapsed().as_secs_f64()
        );

        let bind_start = Instant::now();
        bind_symbols_with::<L>(&cc, globals, &resolve_options)?;
        info!("Symbol binding: {:.2}s", bind_start.elapsed().as_secs_f64());

        let graph_start = Instant::now();
        let mut project_graph = ProjectGraph::new(&cc);
        let unit_graphs = build_graphs::<L>(&cc, GraphBuildOptions::new())?;
        project_graph.add_children(unit_graphs);
        info!(
            "Graph building: {:.2}s",
            graph_start.elapsed().as_secs_f64()
        );

        let link_start = Instant::now();
        project_graph.connect_blocks();
        info!("Linking units: {:.2}s", link_start.elapsed().as_secs_f64());

        if self.options.print_block {
            for unit_graph in project_graph.units() {
                let unit = cc.compile_unit(unit_graph.unit_index());
                let _ = print_llmcc_graph(unit_graph.root(), unit);
            }
        }

        if !self.options.graph {
            return Ok(None);
        }

        let render_start = Instant::now();
        let render_options = RenderOptions {
            show_orphan_nodes: false,
            pagerank_top_k: self.options.pagerank_top_k,
            cluster_by_crate: self.options.cluster_by_crate,
            short_labels: self.options.short_labels,
        };
        let output = render_graph_with_options(
            &project_graph,
            self.options.component_depth,
            &render_options,
        );

        info!(
            "Graph rendering: {:.2}s",
            render_start.elapsed().as_secs_f64()
        );

        Ok(Some(output))
    }

    fn discover_files(&self, extensions: &[&str]) -> Result<Vec<String>> {
        let started = Instant::now();
        let threads = std::thread::available_parallelism().map_or(1, |count| count.get());
        let mut seen = HashSet::new();
        let mut files = Vec::new();

        let mut push_file = |path: String| {
            if seen.insert(path.clone()) {
                files.push(path);
            }
        };

        for file in &self.options.files {
            push_file(file.clone());
        }

        for dir in &self.options.dirs {
            let mut builder = WalkBuilder::new(dir);
            builder
                .standard_filters(true)
                .follow_links(false)
                .threads(threads)
                .add_custom_ignore_filename(".rgignore")
                .add_custom_ignore_filename(".ripignore")
                .add_custom_ignore_filename(".llmccignore");

            for entry in builder.build() {
                let entry = entry.map_err(|err| {
                    io::Error::other(format!("failed to walk directory {dir}: {err}"))
                })?;

                if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                    continue;
                }

                let path = entry.path();
                let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                    continue;
                };

                if extensions.contains(&extension) {
                    push_file(path.to_string_lossy().into_owned());
                }
            }
        }

        info!(
            "File discovery: {:.2}s ({} files)",
            started.elapsed().as_secs_f64(),
            files.len()
        );

        if files.is_empty() {
            return Err(Error::invalid_argument(
                "No input files found. Check that the directory contains supported file types.",
            ));
        }

        Ok(files)
    }

    fn log_parse_metrics(metrics: &BuildMetrics) {
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

    fn emit_output(&self, output: Option<String>) -> Result<()> {
        let Some(output) = output else {
            return Ok(());
        };

        if let Some(path) = self.options.output.as_deref() {
            std::fs::write(path, &output)?;
            tracing::info!(path, "output written");
        } else {
            println!("{output}");
        }

        Ok(())
    }
}
