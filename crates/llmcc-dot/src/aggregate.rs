//! Aggregated graph rendering for crate/module/project level views.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write;

use llmcc_core::BlockId;
use llmcc_core::graph::ProjectGraph;
use llmcc_core::pagerank::PageRanker;

use super::dot::sanitize_id;
use super::types::{AggregatedNode, ComponentDepth, RenderEdge, RenderNode};

/// Get the component key for a node at a given depth level.
///
/// Returns (component_id, component_label, component_type).
pub fn get_component_key(
    node: &RenderNode,
    depth: ComponentDepth,
) -> (String, String, &'static str) {
    match depth {
        ComponentDepth::Project => ("project".to_string(), "project".to_string(), "project"),
        ComponentDepth::Crate => {
            let crate_name = node
                .crate_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let id = format!("crate_{}", sanitize_id(&crate_name));
            (id, crate_name, "crate")
        }
        ComponentDepth::Module => {
            let crate_name = node
                .crate_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string());
            let module_path = node.module_path.clone();

            let (label, id) = if let Some(ref module) = module_path {
                let label = format!("{}::{}", crate_name, module);
                let id = format!("mod_{}_{}", sanitize_id(&crate_name), sanitize_id(module));
                (label, id)
            } else {
                let file_name = node
                    .file_name
                    .clone()
                    .map(|f| {
                        std::path::Path::new(&f)
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or(&f)
                            .to_string()
                    })
                    .unwrap_or_else(|| "unknown".to_string());
                let label = format!("{}::{}", crate_name, file_name);
                let id = format!(
                    "mod_{}_{}",
                    sanitize_id(&crate_name),
                    sanitize_id(&file_name)
                );
                (label, id)
            };
            (id, label, "module")
        }
        ComponentDepth::File => {
            let name = node.name.clone();
            let id = format!("node_{}", node.block_id.as_u32());
            (id, name, "node")
        }
    }
}

/// Render an aggregated graph where nodes represent components (crates/modules/projects)
/// and edges represent dependencies between those components.
pub fn render_aggregated_graph(
    nodes: &[RenderNode],
    edges: &BTreeSet<RenderEdge>,
    depth: ComponentDepth,
    project: &ProjectGraph,
    pagerank_top_k: Option<usize>,
) -> String {
    // Build mapping from BlockId to component key
    let (block_to_component, component_nodes) = build_component_mapping(nodes, depth);

    // Apply PageRank filtering if requested
    let pagerank_components =
        compute_pagerank_components(project, &block_to_component, pagerank_top_k);

    // Aggregate edges between components
    let mut component_edges = aggregate_edges(edges, &block_to_component);

    // Detect and mark bidirectional edges
    let bidirectional_pairs = detect_bidirectional_edges(&component_edges);

    // Remove reverse edges from bidirectional pairs
    for (a, b) in &bidirectional_pairs {
        component_edges.remove(&(b.clone(), a.clone()));
    }

    // Filter weak edges by weight threshold
    let component_edges = filter_weak_edges(component_edges);

    // Determine which components to show
    let components_to_show = determine_visible_components(&component_edges, &pagerank_components);

    // Filter edges and nodes
    let filtered_edges = filter_edges_by_components(&component_edges, &components_to_show);
    let filtered_nodes = filter_nodes_by_edges(&component_nodes, &filtered_edges);

    // Render to DOT format
    render_to_dot(
        depth,
        &filtered_nodes,
        &filtered_edges,
        &bidirectional_pairs,
    )
}

// ============================================================================
// Component Mapping
// ============================================================================

fn build_component_mapping(
    nodes: &[RenderNode],
    depth: ComponentDepth,
) -> (
    std::collections::HashMap<BlockId, String>,
    BTreeMap<String, AggregatedNode>,
) {
    let mut block_to_component = std::collections::HashMap::new();
    let mut component_nodes = BTreeMap::new();

    for node in nodes {
        let (id, label, component_type) = get_component_key(node, depth);
        block_to_component.insert(node.block_id, id.clone());

        component_nodes
            .entry(id.clone())
            .and_modify(|n: &mut AggregatedNode| n.node_count += 1)
            .or_insert(AggregatedNode {
                id,
                label,
                component_type,
                node_count: 1,
            });
    }

    (block_to_component, component_nodes)
}

// ============================================================================
// PageRank Filtering
// ============================================================================

fn compute_pagerank_components(
    project: &ProjectGraph,
    block_to_component: &std::collections::HashMap<BlockId, String>,
    pagerank_top_k: Option<usize>,
) -> Option<HashSet<String>> {
    let top_k = pagerank_top_k?;
    let ranker = PageRanker::new(project);
    let scores = ranker.scores();

    // Aggregate scores by component
    let mut component_scores: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    for (block_id, score) in &scores {
        if let Some(component) = block_to_component.get(block_id) {
            *component_scores.entry(component.clone()).or_insert(0.0) += score;
        }
    }

    // Sort and take top-K
    let mut sorted: Vec<_> = component_scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let top_components: HashSet<String> = sorted
        .into_iter()
        .take(top_k)
        .map(|(component, _)| component)
        .collect();

    Some(top_components)
}

// ============================================================================
// Edge Aggregation
// ============================================================================

/// Aggregate edges between components with correct dependency direction.
fn aggregate_edges(
    edges: &BTreeSet<RenderEdge>,
    block_to_component: &std::collections::HashMap<BlockId, String>,
) -> BTreeMap<(String, String), usize> {
    let mut component_edges = BTreeMap::new();

    for edge in edges {
        let from_component = block_to_component.get(&edge.from_id);
        let to_component = block_to_component.get(&edge.to_id);

        if let (Some(from), Some(to)) = (from_component, to_component) {
            if from == to {
                continue;
            }

            // Determine correct dependency direction based on edge type
            let (dep_from, dep_to) = match (edge.from_label, edge.to_label) {
                // These edges need flipping: to_component depends on from_component
                ("field_type", _)
                | ("input", _)
                | ("trait", _)
                | ("type_arg", _)
                | ("type_dep", _) => (to.clone(), from.clone()),
                // These edges keep direction
                ("caller", "callee") | ("func", "output") | ("func", "type_dep") => {
                    (from.clone(), to.clone())
                }
                // Default: keep raw direction
                _ => (from.clone(), to.clone()),
            };

            *component_edges.entry((dep_from, dep_to)).or_insert(0) += 1;
        }
    }

    component_edges
}

/// Detect bidirectional edge pairs.
fn detect_bidirectional_edges(
    component_edges: &BTreeMap<(String, String), usize>,
) -> HashSet<(String, String)> {
    let mut pairs = HashSet::new();
    for (from, to) in component_edges.keys() {
        if component_edges.contains_key(&(to.clone(), from.clone())) {
            let canonical = if from < to {
                (from.clone(), to.clone())
            } else {
                (to.clone(), from.clone())
            };
            pairs.insert(canonical);
        }
    }
    pairs
}

/// Filter edges by weight threshold (75th percentile).
fn filter_weak_edges(
    component_edges: BTreeMap<(String, String), usize>,
) -> BTreeMap<(String, String), usize> {
    let weights: Vec<usize> = component_edges.values().copied().collect();

    let threshold = if weights.len() > 10 {
        let mut sorted = weights;
        sorted.sort_unstable();
        sorted[sorted.len() * 3 / 4]
    } else {
        1
    };

    component_edges
        .into_iter()
        .filter(|(_, weight)| *weight >= threshold)
        .collect()
}

// ============================================================================
// Filtering
// ============================================================================

fn determine_visible_components(
    component_edges: &BTreeMap<(String, String), usize>,
    pagerank_components: &Option<HashSet<String>>,
) -> HashSet<String> {
    let components_with_edges: HashSet<String> = component_edges
        .keys()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();

    if let Some(pr_components) = pagerank_components {
        pr_components
            .intersection(&components_with_edges)
            .cloned()
            .collect()
    } else {
        components_with_edges
    }
}

fn filter_edges_by_components(
    component_edges: &BTreeMap<(String, String), usize>,
    components_to_show: &HashSet<String>,
) -> Vec<(String, String)> {
    component_edges
        .keys()
        .filter(|(from, to)| components_to_show.contains(from) && components_to_show.contains(to))
        .map(|(from, to)| (from.clone(), to.clone()))
        .collect()
}

fn filter_nodes_by_edges<'a>(
    component_nodes: &'a BTreeMap<String, AggregatedNode>,
    filtered_edges: &[(String, String)],
) -> Vec<&'a AggregatedNode> {
    let nodes_with_edges: HashSet<&String> = filtered_edges
        .iter()
        .flat_map(|(from, to)| [from, to])
        .collect();

    component_nodes
        .values()
        .filter(|n| nodes_with_edges.contains(&n.id))
        .collect()
}

// ============================================================================
// DOT Rendering
// ============================================================================

fn render_to_dot(
    depth: ComponentDepth,
    nodes: &[&AggregatedNode],
    edges: &[(String, String)],
    bidirectional_pairs: &HashSet<(String, String)>,
) -> String {
    let mut output = String::with_capacity(nodes.len() * 100 + edges.len() * 50);

    output.push_str("digraph architecture {\n");
    output.push_str("  node [shape=box];\n\n");

    // Add title
    let title = match depth {
        ComponentDepth::Project => "project graph",
        ComponentDepth::Crate => "crate graph",
        ComponentDepth::Module => "module graph",
        ComponentDepth::File => "architecture graph",
    };
    output.push_str(&format!("  label=\"{}\";\n", title));
    output.push_str("  labelloc=t;\n\n");

    // Render nodes
    for node in nodes {
        let _ = writeln!(output, "  {}[label=\"{}\"];", node.id, node.label);
    }

    output.push('\n');

    // Render edges
    for (from, to) in edges {
        if bidirectional_pairs.contains(&(from.clone(), to.clone())) {
            let _ = writeln!(output, "  {} -> {} [dir=both];", from, to);
        } else {
            let _ = writeln!(output, "  {} -> {};", from, to);
        }
    }

    output.push_str("}\n");
    output
}
