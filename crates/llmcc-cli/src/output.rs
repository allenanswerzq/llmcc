//! Output generation (DOT graphs).

use std::fmt::Write;
use std::time::Instant;

use tracing::info;

use llmcc_core::graph::ProjectGraph;
use llmcc_dot::{RenderOptions, render_graph_with_options};

use crate::LlmccOptions;

/// Generate DOT output for a project graph.
pub fn generate_dot_output<'tcx>(
    opts: &LlmccOptions,
    pg: &'tcx ProjectGraph<'tcx>,
) -> Option<String> {
    if !opts.graph {
        return None;
    }

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
}

/// Merge multiple DOT graph outputs into a single graph.
pub fn merge_dot_outputs(outputs: &[String]) -> String {
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

    // Extract content from each DOT file (skip header and closing brace)
    for output in outputs {
        let mut in_content = false;
        for line in output.lines() {
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
