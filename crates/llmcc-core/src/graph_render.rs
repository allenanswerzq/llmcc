use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::path::Path;

use crate::BlockId;
use crate::symbol::{DepKind, SymId, SymKind};

/// Edge with labeled from/to DepKind for architecture graphs
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LabeledEdge {
    pub from_idx: usize,
    pub to_idx: usize,
    pub from_kind: &'static str,
    pub to_kind: &'static str,
}

impl LabeledEdge {
    pub fn new(from_idx: usize, to_idx: usize, kind: DepKind) -> Self {
        let (from_kind, to_kind) = match kind {
            DepKind::ParamType => ("input", "func"),
            DepKind::ReturnType => ("func", "output"),
            DepKind::Calls => ("caller", "callee"),
            DepKind::Implements => ("trait", "impl"),
            DepKind::FieldType => ("struct", "field"),
            DepKind::Instantiates => ("caller", "type"),
            DepKind::TypeBound => ("bound", "generic"),
            DepKind::Uses => ("user", "used"),
            DepKind::Used => ("user", "used"),
        };
        Self {
            from_idx,
            to_idx,
            from_kind,
            to_kind,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CompactNode {
    pub(crate) block_id: BlockId,
    pub(crate) unit_index: usize,
    pub(crate) name: String,
    pub(crate) location: Option<String>,
    /// Fully qualified name for hierarchical grouping
    pub(crate) fqn: String,
    pub(crate) sym_id: Option<SymId>,
    pub(crate) sym_kind: Option<SymKind>,
    /// Whether the symbol is public (for filtering private helpers in arch-graph)
    pub(crate) is_public: bool,
}

impl CompactNode {
    /// Extract component path from FQN at given depth
    /// FQN: "_c::data::entity::User"
    /// depth=1 → ["_c"]
    /// depth=2 → ["_c", "data"]
    /// depth=3 → ["_c", "data", "entity"]
    pub(crate) fn component_path(&self, depth: usize) -> Vec<String> {
        if depth == 0 {
            return vec![];
        }
        let parts: Vec<&str> = self.fqn.split("::").collect();
        // Take up to `depth` parts, excluding the symbol name itself
        let module_parts = if parts.len() > 1 {
            &parts[..parts.len() - 1]
        } else {
            &parts[..]
        };
        module_parts
            .iter()
            .take(depth)
            .map(|s| s.to_string())
            .collect()
    }
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

    pub(crate) fn render(
        &self,
        edges: &BTreeSet<(usize, usize)>,
        component_depth: usize,
    ) -> String {
        self.render_with_title(edges, component_depth, "project")
    }

    pub(crate) fn render_with_title(
        &self,
        edges: &BTreeSet<(usize, usize)>,
        component_depth: usize,
        title: &str,
    ) -> String {
        if self.nodes.is_empty() {
            return format!("digraph {} {{\n}}\n", title);
        }

        if self.nodes.is_empty() {
            return format!("digraph {} {{\n}}\n", title);
        }

        render_nested_dot_with_title(self.nodes, edges, component_depth, title)

        // TODO:
        // let pruned = prune_compact_components(self.nodes, edges);
        // if self.nodes.is_empty() {
        //     return format!("digraph {} {{\n}}\n", title);
        // }
        // let reduced_edges = reduce_transitive_edges(&pruned.nodes, &pruned.edges);
        // render_nested_dot_with_title(&pruned.nodes, &reduced_edges, component_depth, title)
    }

    /// Render architecture graph with labeled edges showing DepKind
    pub(crate) fn render_arch(
        &self,
        edges: &BTreeSet<LabeledEdge>,
        component_depth: usize,
    ) -> String {
        if self.nodes.is_empty() {
            return "digraph architecture {\n}\n".to_string();
        }
        render_arch_dot(self.nodes, edges, component_depth)
    }
}

/// A tree structure for organizing nodes by their component paths
#[derive(Default)]
struct ComponentTree {
    /// Direct child nodes at this level
    node_indices: Vec<usize>,
    /// Child component subtrees
    children: BTreeMap<String, ComponentTree>,
}

impl ComponentTree {
    fn insert(&mut self, path: &[String], node_idx: usize) {
        if path.is_empty() {
            self.node_indices.push(node_idx);
        } else {
            let child = self.children.entry(path[0].clone()).or_default();
            child.insert(&path[1..], node_idx);
        }
    }
}

fn render_nested_dot_with_title(
    nodes: &[CompactNode],
    edges: &BTreeSet<(usize, usize)>,
    component_depth: usize,
    title: &str,
) -> String {
    // Pre-allocate output buffer based on expected size
    // Rough estimate: 100 bytes per node + 50 bytes per edge + base overhead
    let estimated_size = nodes.len() * 100 + edges.len() * 50 + 200;
    let mut output = String::with_capacity(estimated_size);

    // Build component tree from node paths derived from FQN
    let mut tree = ComponentTree::default();
    for (idx, node) in nodes.iter().enumerate() {
        let path = node.component_path(component_depth);
        tree.insert(&path, idx);
    }

    let _ = writeln!(output, "digraph {} {{", title);
    output.push_str("  graph [fontname=\"Helvetica Bold\", fontsize=12];\n\n");

    let mut counter = 0usize;

    // Render the tree recursively, starting at depth 0
    render_component_tree(&mut output, &tree, nodes, &mut counter, 1, 0);

    // Render edges
    for &(from, to) in edges {
        let _ = writeln!(
            output,
            "  n{} -> n{};",
            nodes[from].block_id.as_u32(),
            nodes[to].block_id.as_u32()
        );
    }

    output.push_str("}\n");
    output
}

/// Render architecture graph with labeled edges showing from/to DepKind
fn render_arch_dot(
    nodes: &[CompactNode],
    edges: &BTreeSet<LabeledEdge>,
    component_depth: usize,
) -> String {
    // Pre-allocate output buffer based on expected size
    let estimated_size = nodes.len() * 150 + edges.len() * 80 + 200;
    let mut output = String::with_capacity(estimated_size);

    // Build component tree from node paths derived from FQN
    let mut tree = ComponentTree::default();
    for (idx, node) in nodes.iter().enumerate() {
        let path = node.component_path(component_depth);
        tree.insert(&path, idx);
    }

    output.push_str("digraph architecture {\n");
    output.push_str("  graph [fontname=\"Helvetica Bold\", fontsize=12];\n\n");

    let mut counter = 0usize;

    // Render the tree recursively with sym_ty attribute
    render_arch_component_tree(&mut output, &tree, nodes, &mut counter, 1, 0);

    // Render labeled edges with from/to attributes
    for edge in edges {
        let _ = writeln!(
            output,
            "  n{} -> n{} [from=\"{}\", to=\"{}\"];",
            nodes[edge.from_idx].block_id.as_u32(),
            nodes[edge.to_idx].block_id.as_u32(),
            edge.from_kind,
            edge.to_kind
        );
    }

    output.push_str("}\n");
    output
}

fn render_arch_component_tree(
    output: &mut String,
    tree: &ComponentTree,
    nodes: &[CompactNode],
    counter: &mut usize,
    indent_level: usize,
    depth: usize,
) {
    let (fill_color, border_color) = get_depth_colors(depth);

    // Render child subtrees (nested subgraphs)
    for (component_name, subtree) in &tree.children {
        let cluster_id = *counter;
        *counter += 1;

        // Write subgraph header
        for _ in 0..indent_level {
            output.push_str("  ");
        }
        let _ = writeln!(output, "subgraph cluster_{} {{", cluster_id);

        // Write label
        for _ in 0..indent_level {
            output.push_str("  ");
        }
        let _ = writeln!(output, "  label=\"{}\";", escape_dot_label(component_name));

        if depth != 0 {
            for _ in 0..indent_level {
                output.push_str("  ");
            }
            output.push_str("  style=\"filled\";\n");

            for _ in 0..indent_level {
                output.push_str("  ");
            }
            let _ = writeln!(output, "  fillcolor=\"{}\";", fill_color);

            for _ in 0..indent_level {
                output.push_str("  ");
            }
            let _ = writeln!(output, "  color=\"{}\";", border_color);
        }

        // Recursively render children with increased depth
        render_arch_component_tree(output, subtree, nodes, counter, indent_level + 1, depth + 1);

        for _ in 0..indent_level {
            output.push_str("  ");
        }
        output.push_str("}\n");
    }

    // Render nodes at this level with sym_ty attribute
    let mut sorted_indices = tree.node_indices.clone();
    sorted_indices.sort_by(|&a, &b| {
        let node_a = &nodes[a];
        let node_b = &nodes[b];

        node_a
            .location
            .as_ref()
            .cmp(&node_b.location.as_ref())
            .then_with(|| node_a.name.cmp(&node_b.name))
            .then_with(|| node_a.block_id.as_u32().cmp(&node_b.block_id.as_u32()))
    });

    for idx in sorted_indices {
        let node = &nodes[idx];

        // Write indent
        for _ in 0..indent_level {
            output.push_str("  ");
        }

        // Build node line directly
        let _ = write!(
            output,
            "n{}[label=\"{}\"",
            node.block_id.as_u32(),
            escape_dot_label(&node.name)
        );

        if let Some(location) = &node.location {
            let (_display, full) = summarize_location(location);
            let _ = write!(output, ", full_path=\"{}\"", escape_dot_attr(&full));
        }

        if let Some(sym_kind) = &node.sym_kind {
            let _ = write!(output, ", sym_ty=\"{:?}\"", sym_kind);
            // Use box shape for type-like symbols (Struct, Trait, Enum)
            if matches!(sym_kind, SymKind::Struct | SymKind::Enum | SymKind::Trait) {
                output.push_str(", shape=box");
            }
        }

        output.push_str("];\n");
    }
}

/// Color palette for different nesting depths
/// Returns (fill_color, border_color) for the subgraph
fn get_depth_colors(depth: usize) -> (&'static str, &'static str) {
    match depth % 5 {
        0 => ("#F5F5F5", "#757575"), // Light grey / Dark grey
        1 => ("#EEEEEE", "#616161"), // Lighter grey / Medium grey
        2 => ("#E0E0E0", "#424242"), // Medium grey / Darker grey
        3 => ("#FAFAFA", "#9E9E9E"), // Near white / Grey
        4 => ("#F0F0F0", "#808080"), // Soft grey / Neutral grey
        _ => ("#F5F5F5", "#9E9E9E"), // Light grey (fallback)
    }
}

fn render_component_tree(
    output: &mut String,
    tree: &ComponentTree,
    nodes: &[CompactNode],
    counter: &mut usize,
    indent_level: usize,
    depth: usize,
) {
    let (fill_color, border_color) = get_depth_colors(depth);

    // Render child subtrees (nested subgraphs)
    for (component_name, subtree) in &tree.children {
        let cluster_id = *counter;
        *counter += 1;

        // Write subgraph header
        for _ in 0..indent_level {
            output.push_str("  ");
        }
        let _ = writeln!(output, "subgraph cluster_{} {{", cluster_id);

        // Write label
        for _ in 0..indent_level {
            output.push_str("  ");
        }
        let _ = writeln!(output, "  label=\"{}\";", escape_dot_label(component_name));

        if depth != 0 {
            for _ in 0..indent_level {
                output.push_str("  ");
            }
            output.push_str("  style=\"filled\";\n");

            for _ in 0..indent_level {
                output.push_str("  ");
            }
            let _ = writeln!(output, "  fillcolor=\"{}\";", fill_color);

            for _ in 0..indent_level {
                output.push_str("  ");
            }
            let _ = writeln!(output, "  color=\"{}\";", border_color);
        }

        // Recursively render children with increased depth
        render_component_tree(output, subtree, nodes, counter, indent_level + 1, depth + 1);

        for _ in 0..indent_level {
            output.push_str("  ");
        }
        output.push_str("}\n");
    }

    // Render nodes at this level
    let mut sorted_indices = tree.node_indices.clone();
    sorted_indices.sort_by(|&a, &b| {
        let node_a = &nodes[a];
        let node_b = &nodes[b];

        node_a
            .location
            .as_ref()
            .cmp(&node_b.location.as_ref())
            .then_with(|| node_a.name.cmp(&node_b.name))
            .then_with(|| node_a.block_id.as_u32().cmp(&node_b.block_id.as_u32()))
    });

    for idx in sorted_indices {
        let node = &nodes[idx];

        // Write indent
        for _ in 0..indent_level {
            output.push_str("  ");
        }

        // Build node line directly
        let _ = write!(
            output,
            "n{}[label=\"{}\"",
            node.block_id.as_u32(),
            escape_dot_label(&node.name)
        );

        if let Some(location) = &node.location {
            let (_display, full) = summarize_location(location);
            let _ = write!(output, ", full_path=\"{}\"", escape_dot_attr(&full));
        }

        output.push_str("];\n");
    }
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

    let mut shortened = if let Some(idx) = components.iter().rposition(|comp| *comp == "src") {
        let start = idx.saturating_sub(1);
        components[start..].join("/")
    } else {
        components[components.len().saturating_sub(3)..].join("/")
    };
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

#[allow(dead_code)]
fn reduce_transitive_edges(
    nodes: &[CompactNode],
    edges: &BTreeSet<(usize, usize)>,
) -> BTreeSet<(usize, usize)> {
    // Temporarily skip transitive reduction to avoid dropping edges like
    // main -> render. TODO: reinstate smarter reduction once edge retention
    // rules are clarified.
    if nodes.is_empty() {
        BTreeSet::new()
    } else {
        edges.clone()
    }
}

#[allow(dead_code)]
struct PrunedGraph {
    nodes: Vec<CompactNode>,
    edges: BTreeSet<(usize, usize)>,
}

#[allow(dead_code)]
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

    let mut retained_nodes = Vec::with_capacity(retained_indices.len());
    let mut old_to_new = HashMap::new();

    let mut ordered_indices: Vec<usize> = retained_indices.into_iter().collect();
    ordered_indices.sort_unstable_by(|a, b| {
        let node_a = &nodes[*a];
        let node_b = &nodes[*b];
        node_a
            .unit_index
            .cmp(&node_b.unit_index)
            .then_with(|| node_a.location.as_ref().cmp(&node_b.location.as_ref()))
            .then_with(|| node_a.name.cmp(&node_b.name))
            .then_with(|| node_a.block_id.as_u32().cmp(&node_b.block_id.as_u32()))
    });

    for (new_idx, old_idx) in ordered_indices.iter().enumerate() {
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

#[allow(dead_code)]
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
