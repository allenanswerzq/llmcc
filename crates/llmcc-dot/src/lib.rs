//! Graph rendering module for producing DOT format output.
//!
//! This module transforms a `ProjectGraph` into DOT format for visualization.
//! Nodes are grouped hierarchically by crate/module/file into nested subgraph clusters.
//!
//! # Module Structure
//!
//! - [`dot`]: DOT format utilities and helpers
//! - [`aggregate`]: Aggregated graph rendering (crate/module/project level)
//! - [`detail`]: File-level detail rendering with clustering

mod aggregate;
mod detail;
mod dot;

use std::collections::{BTreeSet, HashSet};

use llmcc_collect::{collect_edges, collect_nodes};
use llmcc_core::BlockId;
use llmcc_core::graph::ProjectGraph;
use llmcc_core::pagerank::PageRanker;

// Re-export public types from llmcc-collect
pub use dot::DotBuilder;
pub use llmcc_collect::{ComponentDepth, RenderEdge, RenderNode, RenderOptions};

// ============================================================================
// Public API
// ============================================================================

/// Render the project graph to DOT format for visualization.
///
/// - `depth`: Component abstraction level
///   - Project (0): Show project-level dependencies
///   - Crate (1): Show crate-level dependencies
///   - Module (2): Show module-level dependencies
///   - File (3): Show individual nodes with file clustering
pub fn render_graph(project: &ProjectGraph, depth: ComponentDepth) -> String {
    render_graph_with_options(project, depth, &RenderOptions::default())
}

/// Render the project graph with optional PageRank filtering.
///
/// - `pagerank_top_k`: If Some(k), only show the top k nodes by PageRank score
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

/// Render the project graph to DOT format with custom options.
///
/// For aggregated views (depth < File), nodes are aggregated into components
/// and edges show dependencies between those components.
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

    // For aggregated views, use aggregated rendering
    if depth.is_aggregated() {
        return aggregate::render_aggregated_graph(&nodes, &edges, depth, project, options);
    }

    // For file-level detail, use clustered rendering
    render_file_level(&nodes, edges, project, options)
}

// ============================================================================
// File-Level Rendering
// ============================================================================

fn render_file_level(
    nodes: &[RenderNode],
    edges: BTreeSet<RenderEdge>,
    project: &ProjectGraph,
    options: &RenderOptions,
) -> String {
    let mut filtered_nodes = nodes.to_vec();

    // Apply PageRank filtering if requested
    if let Some(top_k) = options.pagerank_top_k {
        let ranker = PageRanker::new(project);
        let all_ranked = ranker.rank();

        let node_ids: HashSet<BlockId> = filtered_nodes.iter().map(|n| n.block_id).collect();

        let top_architecture_ids: HashSet<BlockId> = all_ranked
            .blocks
            .into_iter()
            .filter(|r| node_ids.contains(&r.node.block_id))
            .take(top_k)
            .map(|r| r.node.block_id)
            .collect();

        filtered_nodes.retain(|n| top_architecture_ids.contains(&n.block_id));
    }

    // Filter edges to only those between filtered nodes
    let filtered_node_ids: HashSet<BlockId> = filtered_nodes.iter().map(|n| n.block_id).collect();
    let filtered_edges: BTreeSet<RenderEdge> = edges
        .into_iter()
        .filter(|e| filtered_node_ids.contains(&e.from_id) && filtered_node_ids.contains(&e.to_id))
        .collect();

    // Filter out orphan nodes unless explicitly requested
    if !options.show_orphan_nodes {
        let connected_nodes: HashSet<BlockId> = filtered_edges
            .iter()
            .flat_map(|e| [e.from_id, e.to_id])
            .collect();
        filtered_nodes.retain(|n| connected_nodes.contains(&n.block_id));
    }

    if filtered_nodes.is_empty() {
        return "digraph G {\n}\n".to_string();
    }

    let tree = detail::build_component_tree(&filtered_nodes, ComponentDepth::File);
    detail::render_dot(&filtered_nodes, &filtered_edges, &tree)
}
