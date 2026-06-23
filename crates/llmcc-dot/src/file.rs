use std::collections::BTreeMap;

use llmcc_core::symbol::SymKind;
use llmcc_core::{CollectedGraph, CollectedNode};

use crate::{
    ClusterKind, DotCluster, DotDocument, DotEdge, DotNode, child_cluster_id, normalize_path,
};

/// Package/namespace/file tree used by file-level DOT rendering.
#[derive(Default)]
pub(crate) struct FileViewTree {
    node_indices: Vec<usize>,
    children: BTreeMap<String, (ClusterKind, FileViewTree)>,
}

impl FileViewTree {
    /// Build the file view tree from collected graph nodes.
    pub(crate) fn from_nodes(nodes: &[CollectedNode]) -> Self {
        let mut tree = Self::default();
        for (idx, node) in nodes.iter().enumerate() {
            let file = node.file_name();
            tree.insert(node.package(), node.namespace(), file.as_deref(), idx);
        }
        tree
    }

    /// Build a complete DOT document for file-level rendering.
    pub(crate) fn to_document(&self, graph: &CollectedGraph) -> DotDocument {
        let nodes = graph.nodes();

        DotDocument {
            clusters: self.to_clusters(nodes, "root"),
            free_nodes: dot_nodes_sorted(&self.node_indices, nodes),
            edges: graph
                .edges()
                .iter()
                .map(|edge| {
                    let (from_role, to_role) = edge.kind.role_labels();
                    let rel: &'static str = edge.kind.into();
                    DotEdge {
                        from: format!("n{}", edge.from_id.as_u32()),
                        to: format!("n{}", edge.to_id.as_u32()),
                        attrs: vec![
                            ("rel", rel.into()),
                            ("from", from_role.into()),
                            ("to", to_role.into()),
                        ],
                    }
                })
                .collect(),
        }
    }

    /// Recursively convert tree children into DOT clusters.
    fn to_clusters(&self, nodes: &[CollectedNode], parent_id: &str) -> Vec<DotCluster> {
        self.children
            .iter()
            .enumerate()
            .map(|(index, (name, (kind, subtree)))| {
                let id = child_cluster_id(parent_id, index, name);
                DotCluster {
                    children: subtree.to_clusters(nodes, &id),
                    id,
                    label: name.clone(),
                    kind: *kind,
                    nodes: dot_nodes_sorted(subtree.node_indices(), nodes),
                }
            })
            .collect()
    }

    /// Node indices at this tree level.
    fn node_indices(&self) -> &[usize] {
        &self.node_indices
    }

    fn insert(
        &mut self,
        package: Option<&str>,
        namespace: Option<&str>,
        file: Option<&str>,
        node_idx: usize,
    ) {
        let mut current = self;
        if let Some(package) = package {
            current = current.child(package, ClusterKind::Package);
        }
        if let Some(namespace) = namespace {
            current = current.child(namespace, ClusterKind::Namespace);
        }
        if let Some(file) = file {
            current = current.child(file, ClusterKind::File);
        }
        current.node_indices.push(node_idx);
    }

    fn child(&mut self, name: &str, kind: ClusterKind) -> &mut FileViewTree {
        &mut self
            .children
            .entry(name.to_owned())
            .or_insert_with(|| (kind, FileViewTree::default()))
            .1
    }
}

/// Convert node indices into sorted DOT nodes.
fn dot_nodes_sorted(indices: &[usize], nodes: &[CollectedNode]) -> Vec<DotNode> {
    let mut sorted = indices.to_vec();
    sorted.sort_by(|&a, &b| {
        nodes[a]
            .source_line
            .cmp(&nodes[b].source_line)
            .then_with(|| nodes[a].name.cmp(&nodes[b].name))
            .then_with(|| nodes[a].block_id.as_u32().cmp(&nodes[b].block_id.as_u32()))
    });
    sorted
        .iter()
        .map(|&idx| DotNode::from(&nodes[idx]))
        .collect()
}

/// Convert a single collected node into a DOT node with attrs.
impl From<&CollectedNode> for DotNode {
    fn from(node: &CollectedNode) -> Self {
        let mut attrs = Vec::new();
        if let Some(package) = node.package() {
            attrs.push(("package", package.to_owned()));
        }
        if let Some(module) = node.namespace() {
            attrs.push(("module", module.to_owned()));
        }
        if let Some(file) = node.file_name() {
            attrs.push(("file", file));
        }
        if let Some(location) = node.location() {
            attrs.push(("path", normalize_path(&location)));
        }
        if let Some(kind) = node.symbol_kind {
            let kind_name: &'static str = kind.into();
            attrs.push(("kind", kind_name.to_owned()));
            attrs.push(("shape", shape_for_kind(kind).to_owned()));
        }
        Self {
            id: format!("n{}", node.block_id.as_u32()),
            label: node.name.clone(),
            attrs,
        }
    }
}

fn shape_for_kind(kind: SymKind) -> &'static str {
    match kind {
        SymKind::Struct
        | SymKind::Enum
        | SymKind::Trait
        | SymKind::Interface
        | SymKind::TypeAlias => "box",
        SymKind::Module | SymKind::File | SymKind::Namespace | SymKind::Package => "folder",
        SymKind::Field | SymKind::Variable => "plaintext",
        SymKind::Const | SymKind::Static => "diamond",
        _ => "ellipse",
    }
}
