//! Aggregated graph rendering for project/package/namespace level views.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write;

use llmcc_core::BlockId;
use llmcc_core::graph::ProjectGraph;
use llmcc_core::pagerank::{PageRanker, RankMetric};
use llmcc_core::{CollectedEdge, CollectedEdgeKind, CollectedNode};

use crate::dot::sanitize_id;
use crate::types::{AggregatedNode, ComponentDepth, RenderOptions};

/// Get the component key for a node at a given depth level.
///
/// Returns (component_id, component_label, component_type).
#[allow(dead_code)]
pub fn get_component_key(
    node: &CollectedNode,
    depth: ComponentDepth,
) -> (String, String, &'static str) {
    let (id, label, comp_type, _package, _folder) =
        get_component_key_with_package(node, depth, false);
    (id, label, comp_type)
}

/// Get the component key for a node with optional short labels.
///
/// Returns (component_id, component_label, component_type, package_name, folder).
fn get_component_key_with_package(
    node: &CollectedNode,
    depth: ComponentDepth,
    short_labels: bool,
) -> (String, String, &'static str, Option<String>, Option<String>) {
    match depth {
        ComponentDepth::Project => (
            "project".to_string(),
            "project".to_string(),
            "project",
            None,
            None, // Project folder could be derived from any file's root
        ),
        ComponentDepth::Package => {
            let package_name = node.package().unwrap_or("unknown").to_owned();
            let id = format!("package_{}", sanitize_id(&package_name));
            let folder = node.package_root();
            (
                id,
                package_name.clone(),
                "package",
                Some(package_name),
                folder,
            )
        }
        ComponentDepth::Namespace => {
            let package_name = node.package().unwrap_or("unknown").to_owned();
            let namespace_path = node.namespace();

            let (label, id, short, folder) = if let Some(namespace) = namespace_path {
                let full_label = format!("{package_name}::{namespace}");
                let short_label = namespace.to_owned();
                let id = format!(
                    "namespace_{}_{}",
                    sanitize_id(&package_name),
                    sanitize_id(namespace)
                );
                let folder = node.namespace_root().or_else(|| node.dir());
                (full_label, id, short_label, folder)
            } else {
                let file_stem = node.file_stem().unwrap_or_else(|| "unknown".to_string());
                let full_label = format!("{package_name}::{file_stem}");
                let short_label = file_stem.clone();
                let id = format!(
                    "namespace_{}_{}",
                    sanitize_id(&package_name),
                    sanitize_id(&file_stem)
                );
                let folder = node.dir();
                (full_label, id, short_label, folder)
            };
            let display_label = if short_labels { short } else { label };
            (id, display_label, "namespace", Some(package_name), folder)
        }
        ComponentDepth::File => {
            let name = node.name.clone();
            let id = format!("node_{}", node.block_id.as_u32());
            let folder = node.dir();
            (
                id,
                name,
                "node",
                node.package().map(ToOwned::to_owned),
                folder,
            )
        }
    }
}

/// Render an aggregated graph where nodes represent project/package/namespace components
/// and edges represent dependencies between those components.
pub fn render_aggregated_graph(
    nodes: &[CollectedNode],
    edges: &BTreeSet<CollectedEdge>,
    depth: ComponentDepth,
    project: &ProjectGraph,
    options: &RenderOptions,
) -> String {
    // Build mapping from BlockId to component key
    let (block_to_component, component_nodes) =
        build_component_mapping(nodes, depth, options.short_labels);

    // Apply PageRank filtering if requested
    let pagerank_components =
        compute_pagerank_components(project, &block_to_component, options.pagerank_top_k);

    // Aggregate edges between components
    let component_edges = aggregate_edges(edges, &block_to_component);

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
        options.cluster_by_package && depth == ComponentDepth::Namespace,
    )
}

// Component Mapping

fn build_component_mapping(
    nodes: &[CollectedNode],
    depth: ComponentDepth,
    short_labels: bool,
) -> (
    std::collections::HashMap<BlockId, String>,
    BTreeMap<String, AggregatedNode>,
) {
    let mut block_to_component = std::collections::HashMap::new();
    let mut component_nodes = BTreeMap::new();

    for node in nodes {
        let (id, label, component_type, package_name, folder) =
            get_component_key_with_package(node, depth, short_labels);
        block_to_component.insert(node.block_id, id.clone());

        component_nodes
            .entry(id.clone())
            .and_modify(|n: &mut AggregatedNode| {
                n.node_count += 1;
                // Update folder if not yet set
                if n.folder.is_none() && folder.is_some() {
                    n.folder = folder.clone();
                }
            })
            .or_insert(AggregatedNode {
                id,
                label,
                component_type,
                node_count: 1,
                package_name,
                folder,
            });
    }

    (block_to_component, component_nodes)
}

// PageRank Filtering

fn compute_pagerank_components(
    project: &ProjectGraph,
    block_to_component: &std::collections::HashMap<BlockId, String>,
    pagerank_top_k: Option<usize>,
) -> Option<HashSet<String>> {
    let top_k = pagerank_top_k?;
    let ranker = PageRanker::new(project);
    let scores = ranker.rank().ok()?.scores_by_block(RankMetric::Combined);

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
    sorted.sort_by(|a, b| b.1.total_cmp(&a.1));

    let top_components: HashSet<String> = sorted
        .into_iter()
        .take(top_k)
        .map(|(component, _)| component)
        .collect();

    Some(top_components)
}

// Edge Aggregation

/// Aggregate edges between components with correct dependency direction.
fn aggregate_edges(
    edges: &BTreeSet<CollectedEdge>,
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

            let (dep_from, dep_to) = if reverses_for_aggregation(edge) {
                (to.clone(), from.clone())
            } else {
                (from.clone(), to.clone())
            };

            *component_edges.entry((dep_from, dep_to)).or_insert(0) += 1;
        }
    }

    component_edges
}

fn reverses_for_aggregation(edge: &CollectedEdge) -> bool {
    matches!(
        edge.kind,
        CollectedEdgeKind::Field
            | CollectedEdgeKind::NestedField
            | CollectedEdgeKind::TypeArg
            | CollectedEdgeKind::Param
            | CollectedEdgeKind::Conformance
            | CollectedEdgeKind::Specialization
            | CollectedEdgeKind::ImplArg
            | CollectedEdgeKind::Annotation
    )
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

// Filtering

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

// DOT Rendering

fn render_to_dot(
    depth: ComponentDepth,
    nodes: &[&AggregatedNode],
    edges: &[(String, String)],
    cluster_by_package: bool,
) -> String {
    let mut output = String::with_capacity(nodes.len() * 100 + edges.len() * 50);

    output.push_str("digraph architecture {\n");

    output.push_str("  rankdir=TB;\n"); // Top to bottom layout
    output.push_str("  ranksep=0.8;\n"); // Increase vertical spacing
    output.push_str("  nodesep=0.4;\n"); // Increase horizontal spacing
    output.push_str("  splines=ortho;\n"); // Use orthogonal edges for cleaner lines
    output.push_str("  concentrate=true;\n"); // Merge edges with same endpoints
    output.push('\n');

    // Node styling
    output.push_str("  node [shape=box, style=\"rounded,filled\", fillcolor=\"#f0f0f0\", fontname=\"Helvetica\"];\n");
    output.push_str("  edge [color=\"#888888\", arrowsize=0.7];\n\n");

    output.push_str("  labelloc=t;\n");
    output.push_str("  fontsize=16;\n\n");

    // Cluster namespaces by package if enabled.
    if cluster_by_package && depth == ComponentDepth::Namespace {
        render_clustered_nodes(&mut output, nodes);
    } else {
        // Render nodes without clustering
        for node in nodes {
            if let Some(ref folder) = node.folder {
                let _ = writeln!(
                    output,
                    "  {}[label=\"{}\", path=\"{}\"];",
                    node.id, node.label, folder
                );
            } else {
                let _ = writeln!(output, "  {}[label=\"{}\"];", node.id, node.label);
            }
        }
    }

    output.push('\n');

    // Render edges
    for (from, to) in edges {
        let _ = writeln!(output, "  {from} -> {to};");
    }

    output.push_str("}\n");
    output
}

/// Render namespace nodes clustered by package.
fn render_clustered_nodes(output: &mut String, nodes: &[&AggregatedNode]) {
    use std::collections::BTreeMap;

    // Group nodes by package.
    let mut package_groups: BTreeMap<String, Vec<&AggregatedNode>> = BTreeMap::new();
    for node in nodes {
        let package_name = node
            .package_name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        package_groups.entry(package_name).or_default().push(node);
    }

    // Render each package as a subgraph cluster.
    for (package_name, package_nodes) in &package_groups {
        let cluster_id = sanitize_id(package_name);
        let _ = writeln!(output, "  subgraph cluster_{cluster_id} {{");
        let _ = writeln!(output, "    label=\"{package_name}\";");
        output.push_str("    style=rounded;\n");
        output.push_str("    bgcolor=\"#f8f8f8\";\n\n");

        for node in package_nodes {
            if let Some(ref folder) = node.folder {
                let _ = writeln!(
                    output,
                    "    {}[label=\"{}\", path=\"{}\"];",
                    node.id, node.label, folder
                );
            } else {
                let _ = writeln!(output, "    {}[label=\"{}\"];", node.id, node.label);
            }
        }

        output.push_str("  }\n\n");
    }
}
