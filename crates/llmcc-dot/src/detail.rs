//! File-level detail rendering with hierarchical clustering.

use std::collections::BTreeSet;
use std::fmt::Write;

use llmcc_core::{CollectedEdge, CollectedEdgeKind, CollectedNode};

use crate::dot::{escape_label, sanitize_id, shape_for_kind, write_indent};
use crate::types::{ComponentDepth, ComponentTree};

/// Build a ComponentTree from nodes based on package/namespace/file hierarchy.
///
/// This is used for File-level depth where we show individual nodes
/// clustered by package -> namespace -> file.
pub fn build_component_tree(nodes: &[CollectedNode], _depth: ComponentDepth) -> ComponentTree {
    let mut tree = ComponentTree::default();
    for (idx, node) in nodes.iter().enumerate() {
        let mut path: Vec<(String, &'static str)> = Vec::new();

        if let Some(package_name) = node.package() {
            path.push((package_name.to_owned(), "package"));
        }

        if let Some(namespace) = node.namespace() {
            path.push((namespace.to_owned(), "namespace"));
        }

        if let Some(file_name) = node.file_name() {
            path.push((file_name, "file"));
        }

        tree.insert(&path, idx);
    }
    tree
}

/// Render the graph to DOT format with file-level detail.
pub fn render_dot(
    nodes: &[CollectedNode],
    edges: &BTreeSet<CollectedEdge>,
    tree: &ComponentTree,
) -> String {
    let estimated_size = nodes.len() * 150 + edges.len() * 80 + 200;
    let mut output = String::with_capacity(estimated_size);

    output.push_str("digraph architecture {\n");

    output.push_str("  rankdir=TB;\n");
    output.push_str("  ranksep=0.6;\n");
    output.push_str("  nodesep=0.3;\n");
    output.push_str("  splines=ortho;\n");
    output.push_str("  compound=true;\n");
    output.push('\n');

    // Node and edge styling
    output.push_str("  node [shape=box, style=rounded];\n");
    output.push_str("  edge [arrowsize=0.7];\n\n");

    // Render nodes grouped in clusters
    render_tree_recursive(&mut output, tree, nodes, 1);

    output.push('\n');

    // Render edges
    for edge in edges {
        let labels = DotEdgeLabels::from_edge(edge);
        let _ = writeln!(
            output,
            "  n{} -> n{} [from=\"{}\", to=\"{}\"];",
            edge.from_id.as_u32(),
            edge.to_id.as_u32(),
            labels.from,
            labels.to,
        );
    }

    output.push_str("}\n");
    output
}

struct DotEdgeLabels {
    from: &'static str,
    to: &'static str,
}

impl DotEdgeLabels {
    fn from_edge(edge: &CollectedEdge) -> Self {
        let (from, to) = match edge.kind {
            CollectedEdgeKind::Field => ("field_type", "container"),
            CollectedEdgeKind::NestedField => ("type_dep", "container"),
            CollectedEdgeKind::TypeArg => ("type_arg", "generic"),
            CollectedEdgeKind::Call => ("caller", "callee"),
            CollectedEdgeKind::Param => ("input", "func"),
            CollectedEdgeKind::Return => ("func", "output"),
            CollectedEdgeKind::Conformance => ("contract", "conforms"),
            CollectedEdgeKind::Specialization => ("base", "specializes"),
            CollectedEdgeKind::TypeDep => ("source", "type_dep"),
            CollectedEdgeKind::ImplArg => ("type_arg", "implementation"),
            CollectedEdgeKind::Annotation => ("annotation", "annotates"),
        };

        Self { from, to }
    }
}

/// Recursively render the component tree as nested subgraph clusters.
fn render_tree_recursive(
    output: &mut String,
    tree: &ComponentTree,
    nodes: &[CollectedNode],
    indent_level: usize,
) {
    // Render child subtrees
    for (component_name, (level_type, subtree)) in &tree.children {
        let cluster_id = sanitize_id(component_name);

        write_indent(output, indent_level);
        let _ = writeln!(output, "subgraph cluster_{cluster_id} {{");

        write_indent(output, indent_level + 1);
        let _ = writeln!(output, "label=\"{}\";", escape_label(component_name));

        // Style based on level type
        write_indent(output, indent_level + 1);
        output.push_str("style=rounded;\n");
        write_indent(output, indent_level + 1);
        match level_type.as_str() {
            "package" => {
                write_indent(output, indent_level + 1);
                output.push_str("bgcolor=\"#f8f8f8\";\n");
            }
            "namespace" => {
                write_indent(output, indent_level + 1);
                output.push_str("bgcolor=\"#f5f5f5\";\n");
            }
            _ => {
                write_indent(output, indent_level + 1);
                output.push_str("bgcolor=\"#fafafa\";\n");
            }
        }
        output.push('\n');

        render_tree_recursive(output, subtree, nodes, indent_level + 1);

        write_indent(output, indent_level);
        output.push_str("}\n\n");
    }

    // Render nodes at this level
    let mut sorted_indices = tree.node_indices.clone();
    sorted_indices.sort_by(|&a, &b| {
        let node_a = &nodes[a];
        let node_b = &nodes[b];
        node_a
            .path_text()
            .cmp(&node_b.path_text())
            .then_with(|| node_a.source_line.cmp(&node_b.source_line))
            .then_with(|| node_a.name.cmp(&node_b.name))
            .then_with(|| node_a.block_id.as_u32().cmp(&node_b.block_id.as_u32()))
    });

    for idx in sorted_indices {
        let node = &nodes[idx];
        render_node(output, node, indent_level);
    }
}

/// Render a single node.
fn render_node(output: &mut String, node: &CollectedNode, indent_level: usize) {
    write_indent(output, indent_level);

    let _ = write!(
        output,
        "n{}[label=\"{}\"",
        node.block_id.as_u32(),
        escape_label(&node.name)
    );

    if let Some(location) = node.location() {
        let _ = write!(output, ", path=\"{}\"", escape_label(&location));
    }

    if let Some(symbol_kind) = &node.symbol_kind {
        let _ = write!(output, ", sym_ty=\"{symbol_kind:?}\"");
        let shape = shape_for_kind(Some(*symbol_kind));
        // Always include shape for clarity
        let _ = write!(output, ", shape={shape}");
    }

    output.push_str("];\n");
}
