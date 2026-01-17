//! DOT graph rendering for architecture visualization.

mod aggregate;
mod detail;
mod dot;

use std::collections::{BTreeSet, HashMap, HashSet};

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

    let mut module_coverage_ids: HashSet<BlockId> = HashSet::new();

    if let Some(top_k) = options.pagerank_top_k {
        let ranker = PageRanker::new(project);
        let all_ranked = ranker.rank();
        let node_ids: HashSet<BlockId> = filtered_nodes.iter().map(|n| n.block_id).collect();

        let ranked_in_graph: Vec<_> = all_ranked
            .blocks
            .into_iter()
            .filter(|r| node_ids.contains(&r.node.block_id))
            .collect();

        let (top_ids, coverage_ids): (HashSet<BlockId>, HashSet<BlockId>) = if ranked_in_graph.len() <= top_k {
            let ids: HashSet<BlockId> = ranked_in_graph
                .into_iter()
                .map(|r| r.node.block_id)
                .collect();
            (ids, HashSet::new())
        } else {
            let mut module_by_block: HashMap<BlockId, String> = HashMap::new();
            for node in &filtered_nodes {
                let crate_name = node.crate_name.as_deref().unwrap_or("unknown-crate");
                let module = node.module_path.as_deref().unwrap_or("<root>");
                module_by_block.insert(node.block_id, format!("{crate_name}::{module}"));
            }

            let mut module_scores: HashMap<String, f64> = HashMap::new();
            let mut module_blocks: HashMap<String, Vec<BlockId>> = HashMap::new();

            for ranked in &ranked_in_graph {
                let module_key = module_by_block
                    .get(&ranked.node.block_id)
                    .cloned()
                    .unwrap_or_else(|| "unknown-crate::<root>".to_string());
                *module_scores.entry(module_key.clone()).or_insert(0.0) += ranked.score;
                module_blocks
                    .entry(module_key)
                    .or_default()
                    .push(ranked.node.block_id);
            }

            let mut sorted_modules: Vec<_> = module_scores.into_iter().collect();
            sorted_modules.sort_by(|a, b| {
                b.1.partial_cmp(&a.1)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            let per_module = ((top_k / 120).max(1)).min(5);
            let module_budget = ((top_k as f64) * 0.4).round() as usize;

            let mut selected_ids: HashSet<BlockId> = HashSet::new();
            let mut coverage_ids: HashSet<BlockId> = HashSet::new();
            for (module_key, _) in sorted_modules.into_iter() {
                if selected_ids.len() >= module_budget {
                    break;
                }
                if let Some(blocks) = module_blocks.get(&module_key) {
                    for block_id in blocks.iter().take(per_module) {
                        if selected_ids.len() >= module_budget {
                            break;
                        }
                        selected_ids.insert(*block_id);
                        coverage_ids.insert(*block_id);
                    }
                }
            }

            for ranked in &ranked_in_graph {
                if selected_ids.len() >= top_k {
                    break;
                }
                selected_ids.insert(ranked.node.block_id);
            }

            (selected_ids, coverage_ids)
        };

        module_coverage_ids = coverage_ids;

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
        filtered_nodes
            .retain(|n| connected.contains(&n.block_id) || module_coverage_ids.contains(&n.block_id));
    }

    if filtered_nodes.is_empty() {
        return "digraph G {\n}\n".to_string();
    }

    let tree = detail::build_component_tree(&filtered_nodes, ComponentDepth::File);
    detail::render_dot(&filtered_nodes, &filtered_edges, &tree)
}
