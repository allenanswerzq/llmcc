use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;

use crate::BlockId;

const EMPTY_GRAPH_DOT: &str = "digraph DesignGraph {\n}\n";

#[derive(Clone)]
pub(crate) struct CompactNode {
    pub(crate) block_id: BlockId,
    pub(crate) unit_index: usize,
    pub(crate) name: String,
    pub(crate) location: Option<String>,
    pub(crate) group: String,
}

pub(crate) struct GraphRenderer<'a> {
    nodes: &'a [CompactNode],
}

impl<'a> GraphRenderer<'a> {
    pub(crate) fn new(nodes: &'a [CompactNode]) -> Self {
        Self { nodes }
    }

    pub(crate) fn nodes(&self) -> &'a [CompactNode] {
        self.nodes
    }

    pub(crate) fn build_node_index(&self) -> HashMap<BlockId, usize> {
        let mut node_index = HashMap::with_capacity(self.nodes.len());
        for (idx, node) in self.nodes.iter().enumerate() {
            node_index.insert(node.block_id, idx);
        }
        node_index
    }

    pub(crate) fn render(&self, edges: &BTreeSet<(usize, usize)>) -> String {
        if self.nodes.is_empty() {
            return EMPTY_GRAPH_DOT.to_string();
        }

        let pruned = prune_compact_components(self.nodes, edges);
        if pruned.nodes.is_empty() {
            return EMPTY_GRAPH_DOT.to_string();
        }

        let reduced_edges = reduce_transitive_edges(&pruned.nodes, &pruned.edges);
        render_compact_dot(&pruned.nodes, &reduced_edges)
    }
}

fn render_compact_dot(nodes: &[CompactNode], edges: &BTreeSet<(usize, usize)>) -> String {
    let mut crate_groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        crate_groups
            .entry(node.group.clone())
            .or_default()
            .push(idx);
    }

    let mut output = String::from("digraph DesignGraph {\n");

    for (subgraph_counter, (crate_path, node_indices)) in crate_groups.iter().enumerate() {
        output.push_str(&format!("  subgraph cluster_{} {{\n", subgraph_counter));
        output.push_str(&format!(
            "    label=\"{}\";\n",
            escape_dot_label(crate_path)
        ));
        output.push_str("    style=filled;\n");
        output.push_str("    color=lightgrey;\n");

        for &idx in node_indices {
            let node = &nodes[idx];
            let label = escape_dot_label(&node.name);
            let mut attrs = vec![format!("label=\"{}\"", label)];

            if let Some(location) = &node.location {
                let (_display, full) = summarize_location(location);
                let escaped_full = escape_dot_attr(&full);
                attrs.push(format!("full_path=\"{}\"", escaped_full));
            }

            output.push_str(&format!("    n{} [{}];\n", idx, attrs.join(", ")));
        }

        output.push_str("  }\n");
    }

    for &(from, to) in edges {
        output.push_str(&format!("  n{} -> n{};\n", from, to));
    }

    output.push_str("}\n");
    output
}

fn escape_dot_label(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn escape_dot_attr(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

fn summarize_location(location: &str) -> (String, String) {
    let (path_part, line_part) = location
        .rsplit_once(':')
        .map(|(path, line)| (path, Some(line)))
        .unwrap_or((location, None));

    let path = Path::new(path_part);
    let components: Vec<_> = path
        .components()
        .filter_map(|comp| comp.as_os_str().to_str())
        .collect();

    let start = components.len().saturating_sub(3);
    let mut shortened = components[start..].join("/");
    if shortened.is_empty() {
        shortened = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path_part)
            .to_string();
    }

    let display = if let Some(line) = line_part {
        format!("{shortened}:{line}")
    } else {
        shortened
    };

    (display, location.to_string())
}

fn reduce_transitive_edges(
    nodes: &[CompactNode],
    edges: &BTreeSet<(usize, usize)>,
) -> BTreeSet<(usize, usize)> {
    if nodes.is_empty() {
        return BTreeSet::new();
    }

    let mut adjacency: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(from, to) in edges.iter() {
        adjacency.entry(from).or_default().push(to);
    }

    let mut minimal_edges = BTreeSet::new();

    for &(from, to) in edges.iter() {
        if !has_alternative_path(from, to, &adjacency, (from, to)) {
            minimal_edges.insert((from, to));
        }
    }

    minimal_edges
}

fn has_alternative_path(
    start: usize,
    target: usize,
    adjacency: &HashMap<usize, Vec<usize>>,
    edge_to_skip: (usize, usize),
) -> bool {
    let mut visited = HashSet::new();
    let mut stack: Vec<usize> = adjacency
        .get(&start)
        .into_iter()
        .flat_map(|neighbors| neighbors.iter())
        .filter_map(|&neighbor| {
            if (start, neighbor) == edge_to_skip {
                None
            } else {
                Some(neighbor)
            }
        })
        .collect();

    while let Some(current) = stack.pop() {
        if !visited.insert(current) {
            continue;
        }

        if current == target {
            return true;
        }

        if let Some(neighbors) = adjacency.get(&current) {
            for &neighbor in neighbors {
                if (current, neighbor) == edge_to_skip {
                    continue;
                }
                if !visited.contains(&neighbor) {
                    stack.push(neighbor);
                }
            }
        }
    }

    false
}

struct PrunedGraph {
    nodes: Vec<CompactNode>,
    edges: BTreeSet<(usize, usize)>,
}

fn prune_compact_components(
    nodes: &[CompactNode],
    edges: &BTreeSet<(usize, usize)>,
) -> PrunedGraph {
    if nodes.is_empty() {
        return PrunedGraph {
            nodes: Vec::new(),
            edges: BTreeSet::new(),
        };
    }

    let components = find_connected_components(nodes.len(), edges);
    if components.is_empty() {
        return PrunedGraph {
            nodes: nodes.to_vec(),
            edges: edges.clone(),
        };
    }

    let mut retained_indices = HashSet::new();
    for component in components {
        if component.len() == 1 {
            let idx = component[0];
            let has_edges = edges.iter().any(|&(from, to)| from == idx || to == idx);
            if !has_edges {
                continue;
            }
        }
        retained_indices.extend(component);
    }

    if retained_indices.is_empty() {
        return PrunedGraph {
            nodes: Vec::new(),
            edges: BTreeSet::new(),
        };
    }

    let mut retained_nodes = Vec::new();
    let mut old_to_new = HashMap::new();
    for (new_idx, old_idx) in retained_indices.iter().enumerate() {
        retained_nodes.push(nodes[*old_idx].clone());
        old_to_new.insert(*old_idx, new_idx);
    }

    let mut retained_edges = BTreeSet::new();
    for &(from, to) in edges {
        if let (Some(&new_from), Some(&new_to)) = (old_to_new.get(&from), old_to_new.get(&to)) {
            retained_edges.insert((new_from, new_to));
        }
    }

    PrunedGraph {
        nodes: retained_nodes,
        edges: retained_edges,
    }
}

fn find_connected_components(
    node_count: usize,
    edges: &BTreeSet<(usize, usize)>,
) -> Vec<Vec<usize>> {
    if node_count == 0 {
        return Vec::new();
    }

    let mut graph: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(from, to) in edges.iter() {
        graph.entry(from).or_default().push(to);
        graph.entry(to).or_default().push(from);
    }

    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for node in 0..node_count {
        if visited.contains(&node) {
            continue;
        }

        let mut component = Vec::new();
        let mut stack = vec![node];

        while let Some(current) = stack.pop() {
            if !visited.insert(current) {
                continue;
            }

            component.push(current);

            if let Some(neighbors) = graph.get(&current) {
                for &neighbor in neighbors {
                    if !visited.contains(&neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
        }

        components.push(component);
    }

    components
}
