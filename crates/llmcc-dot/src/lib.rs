//! DOT graph rendering for architecture visualization.

mod aggregate;
mod detail;
mod dot;

use std::collections::{BTreeSet, HashSet};

use llmcc_collect::{collect_edges, collect_nodes};
use llmcc_core::BlockId;
use llmcc_core::graph::ProjectGraph;
use llmcc_core::pagerank::PageRanker;

pub use dot::DotBuilder;
pub use llmcc_collect::{ComponentDepth, RenderEdge, RenderNode, RenderOptions};

/// Render the project graph to DOT format.
pub fn render_graph(project: &ProjectGraph, depth: ComponentDepth) -> String {
    render_graph_with_options(project, depth, &RenderOptions::default())
}

/// Render the project graph with PageRank filtering.
pub fn render_graph_with_pagerank(
    project: &ProjectGraph,
    depth: ComponentDepth,
    pagerank_top_k: Option<usize>,
) -> String {
    let options = RenderOptions {
        show_orphan_nodes: false,
        pagerank_top_k,
        cluster_by_crate: false,
        short_labels: false,
    };
    render_graph_with_options(project, depth, &options)
}

/// Render the project graph with custom options.
pub fn render_graph_with_options(
    project: &ProjectGraph,
    depth: ComponentDepth,
    options: &RenderOptions,
) -> String {
    let nodes = collect_nodes(project);
    if nodes.is_empty() {
        return "digraph G {\n}\n".to_string();
    }

    let node_set: HashSet<BlockId> = nodes.iter().map(|n| n.block_id).collect();
    let edges = collect_edges(project, &node_set);

    if depth.is_aggregated() {
        return aggregate::render_aggregated_graph(&nodes, &edges, depth, project, options);
    }

    render_file_level(&nodes, edges, project, options)
}

fn render_file_level(
    nodes: &[RenderNode],
    edges: BTreeSet<RenderEdge>,
    project: &ProjectGraph,
    options: &RenderOptions,
) -> String {
    let mut filtered_nodes = nodes.to_vec();

    if let Some(top_k) = options.pagerank_top_k {
        let ranker = PageRanker::new(project);
        let all_ranked = ranker.rank();
        let node_ids: HashSet<BlockId> = filtered_nodes.iter().map(|n| n.block_id).collect();

        let top_ids: HashSet<BlockId> = all_ranked
            .blocks
            .into_iter()
            .filter(|r| node_ids.contains(&r.node.block_id))
            .take(top_k)
            .map(|r| r.node.block_id)
            .collect();

        filtered_nodes.retain(|n| top_ids.contains(&n.block_id));
    }

    let filtered_node_ids: HashSet<BlockId> = filtered_nodes.iter().map(|n| n.block_id).collect();
    let filtered_edges: BTreeSet<RenderEdge> = edges
        .into_iter()
        .filter(|e| filtered_node_ids.contains(&e.from_id) && filtered_node_ids.contains(&e.to_id))
        .collect();

    if !options.show_orphan_nodes {
        let connected: HashSet<BlockId> = filtered_edges
            .iter()
            .flat_map(|e| [e.from_id, e.to_id])
            .collect();
        filtered_nodes.retain(|n| connected.contains(&n.block_id));
    }

    if filtered_nodes.is_empty() {
        return "digraph G {\n}\n".to_string();
    }

    let tree = detail::build_component_tree(&filtered_nodes, ComponentDepth::File);
    detail::render_dot(&filtered_nodes, &filtered_edges, &tree)
}
