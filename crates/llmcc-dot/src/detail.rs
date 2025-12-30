//! File-level detail rendering with hierarchical clustering.

use std::collections::BTreeSet;
use std::fmt::Write;

use super::dot::{escape_label, sanitize_id, shape_for_kind, write_indent};
use super::types::{ComponentDepth, ComponentTree, RenderEdge, RenderNode};

/// Build a ComponentTree from nodes based on crate/module/file hierarchy.
///
/// This is used for File-level depth where we show individual nodes
/// clustered by crate → module → file.
pub fn build_component_tree(nodes: &[RenderNode], _depth: ComponentDepth) -> ComponentTree {
    let mut tree = ComponentTree::default();
    for (idx, node) in nodes.iter().enumerate() {
        let mut path: Vec<(String, &'static str)> = Vec::new();

        if let Some(ref crate_name) = node.crate_name {
            path.push((crate_name.clone(), "crate"));
        }

        if let Some(ref module) = node.module_path {
            path.push((module.clone(), "module"));
        }

        if let Some(ref file) = node.file_name {
            let file_name = std::path::Path::new(file)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(file);
            path.push((file_name.to_string(), "file"));
        }

        tree.insert(&path, idx);
    }
    tree
}

/// Render the graph to DOT format with file-level detail.
pub fn render_dot(
    nodes: &[RenderNode],
    edges: &BTreeSet<RenderEdge>,
    tree: &ComponentTree,
) -> String {
    let estimated_size = nodes.len() * 150 + edges.len() * 80 + 200;
    let mut output = String::with_capacity(estimated_size);

    output.push_str("digraph architecture {\n");

    // Wrap everything in a Project cluster
    output.push_str("  subgraph cluster_project {\n");
    output.push_str("    label=\"project\";\n\n");

    // Render nodes grouped in clusters
    render_tree_recursive(&mut output, tree, nodes, 2);

    output.push_str("  }\n\n");

    // Render edges
    for edge in edges {
        let _ = writeln!(
            output,
            "  n{} -> n{} [from=\"{}\", to=\"{}\"];",
            edge.from_id.as_u32(),
            edge.to_id.as_u32(),
            edge.from_label,
            edge.to_label
        );
    }

    output.push_str("}\n");
    output
}

/// Recursively render the component tree as nested subgraph clusters.
fn render_tree_recursive(
    output: &mut String,
    tree: &ComponentTree,
    nodes: &[RenderNode],
    indent_level: usize,
) {
    // Render child subtrees
    for (component_name, (level_type, subtree)) in &tree.children {
        let cluster_id = match level_type.as_str() {
            "crate" | "module" | "file" => sanitize_id(component_name),
            _ => sanitize_id(component_name),
        };

        write_indent(output, indent_level);
        let _ = writeln!(output, "subgraph cluster_{} {{", cluster_id);

        write_indent(output, indent_level);
        let _ = writeln!(output, "  label=\"{}\";", escape_label(component_name));

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
            .location
            .as_ref()
            .cmp(&node_b.location.as_ref())
            .then_with(|| node_a.name.cmp(&node_b.name))
            .then_with(|| node_a.block_id.as_u32().cmp(&node_b.block_id.as_u32()))
    });

    for idx in sorted_indices {
        let node = &nodes[idx];
        render_node(output, node, indent_level);
    }
}

/// Render a single node.
fn render_node(output: &mut String, node: &RenderNode, indent_level: usize) {
    write_indent(output, indent_level);

    let _ = write!(
        output,
        "n{}[label=\"{}\"",
        node.block_id.as_u32(),
        escape_label(&node.name)
    );

    if let Some(location) = &node.location {
        let _ = write!(output, ", full_path=\"{}\"", escape_label(location));
    }

    if let Some(sym_kind) = &node.sym_kind {
        let _ = write!(output, ", sym_ty=\"{:?}\"", sym_kind);
        let shape = shape_for_kind(Some(*sym_kind));
        if shape != "ellipse" {
            let _ = write!(output, ", shape={}", shape);
        }
    }

    output.push_str("];\n");
}
