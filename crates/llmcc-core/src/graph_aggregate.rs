//! Aggregate transform for collected graph facts.

use std::collections::{BTreeMap, BTreeSet};

use crate::graph_collect::{
    CollectedEdge, CollectedEdgeKind, CollectedGraphVisitor, CollectedNode,
};
use crate::{ArchitectureLevel, BlockId};
use strum_macros::{Display, EnumString, IntoStaticStr};

/// Graph produced by aggregating collected graph facts at an architecture level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregatedGraph {
    level: ArchitectureLevel,
    nodes: Vec<AggregatedNode>,
    edges: BTreeSet<AggregatedEdge>,
}

/// Visitor for aggregated graph facts.
pub trait AggregatedGraphVisitor {
    /// Visit one aggregated component node in deterministic order.
    fn visit_node(&mut self, node: &AggregatedNode);

    /// Visit one weighted aggregated edge in deterministic order.
    fn visit_edge(&mut self, edge: &AggregatedEdge);
}

impl AggregatedGraph {
    fn new(
        level: ArchitectureLevel,
        nodes: Vec<AggregatedNode>,
        edges: BTreeSet<AggregatedEdge>,
    ) -> Self {
        Self {
            level,
            nodes,
            edges,
        }
    }

    /// Architecture level used to aggregate this graph.
    pub fn level(&self) -> ArchitectureLevel {
        self.level
    }

    /// Return true when no aggregated nodes were produced.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Return aggregated component nodes.
    pub fn nodes(&self) -> &[AggregatedNode] {
        &self.nodes
    }

    /// Return weighted dependency edges between aggregated components.
    pub fn edges(&self) -> &BTreeSet<AggregatedEdge> {
        &self.edges
    }

    /// Traverse aggregated nodes followed by weighted edges.
    pub fn visit(&self, visitor: &mut impl AggregatedGraphVisitor) {
        for node in &self.nodes {
            visitor.visit_node(node);
        }
        for edge in &self.edges {
            visitor.visit_edge(edge);
        }
    }

    /// Split the graph into owned level, node, and edge collections.
    pub fn into_parts(
        self,
    ) -> (
        ArchitectureLevel,
        Vec<AggregatedNode>,
        BTreeSet<AggregatedEdge>,
    ) {
        (self.level, self.nodes, self.edges)
    }
}

/// Aggregated component category.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Display, EnumString, IntoStaticStr,
)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum AggregatedNodeKind {
    /// Whole-project component.
    Project,
    /// Package or library boundary.
    Package,
    /// Namespace, module, or equivalent source grouping.
    Namespace,
    /// Individual collected graph node.
    Node,
}

/// Node in an aggregated graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AggregatedNode {
    /// Stable component id.
    pub id: String,
    /// Display label for this component.
    pub label: String,
    /// Component category.
    pub kind: AggregatedNodeKind,
    /// Source block ids represented by this component.
    pub block_ids: Vec<BlockId>,
    /// Number of collected nodes represented by this component.
    pub node_count: usize,
    /// Package name for package and namespace components.
    pub package: Option<String>,
    /// Namespace or module name for namespace components.
    pub namespace: Option<String>,
}

/// Weighted edge between aggregated components.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AggregatedEdge {
    /// Source component id.
    pub source_id: String,
    /// Target component id.
    pub target_id: String,
    /// Semantic edge category.
    pub kind: CollectedEdgeKind,
    /// Number of collected edges represented by this edge.
    pub weight: usize,
}

/// Transform visitor that groups collected graph facts by architecture level.
#[derive(Debug, Clone)]
pub struct AggregateVisitor {
    level: ArchitectureLevel,
    node_ids: BTreeMap<BlockId, String>,
    nodes: BTreeMap<String, AggregatedNode>,
    edges: BTreeMap<(String, String, CollectedEdgeKind), usize>,
}

impl AggregateVisitor {
    /// Create an aggregate transform for an architecture level.
    pub fn new(level: ArchitectureLevel) -> Self {
        Self {
            level,
            node_ids: BTreeMap::new(),
            nodes: BTreeMap::new(),
            edges: BTreeMap::new(),
        }
    }

    /// Architecture level this visitor aggregates at.
    pub fn level(&self) -> ArchitectureLevel {
        self.level
    }
}

impl CollectedGraphVisitor for AggregateVisitor {
    type Output = AggregatedGraph;

    fn visit_node(&mut self, node: &CollectedNode) {
        let key = AggregateNodeKey::from_node(node, self.level);
        self.node_ids.insert(node.block_id, key.id.clone());
        self.nodes
            .entry(key.id.clone())
            .and_modify(|component| {
                component.block_ids.push(node.block_id);
                component.node_count += 1;
            })
            .or_insert_with(|| AggregatedNode {
                id: key.id,
                label: key.label,
                kind: key.kind,
                block_ids: vec![node.block_id],
                node_count: 1,
                package: key.package,
                namespace: key.namespace,
            });
    }

    fn visit_edge(&mut self, edge: &CollectedEdge) {
        let (source_id, target_id) = edge.dependency_ids();
        let Some(source) = self.node_ids.get(&source_id).cloned() else {
            return;
        };
        let Some(target) = self.node_ids.get(&target_id).cloned() else {
            return;
        };
        if source == target {
            return;
        }

        *self.edges.entry((source, target, edge.kind)).or_insert(0) += 1;
    }

    fn finish(self) -> Self::Output {
        let mut nodes: Vec<_> = self.nodes.into_values().collect();
        for node in &mut nodes {
            node.block_ids.sort_unstable();
        }

        let edges = self
            .edges
            .into_iter()
            .map(|((source_id, target_id, kind), weight)| AggregatedEdge {
                source_id,
                target_id,
                kind,
                weight,
            })
            .collect();

        AggregatedGraph::new(self.level, nodes, edges)
    }
}

struct AggregateNodeKey {
    id: String,
    label: String,
    kind: AggregatedNodeKind,
    package: Option<String>,
    namespace: Option<String>,
}

impl AggregateNodeKey {
    fn from_node(node: &CollectedNode, level: ArchitectureLevel) -> Self {
        match level {
            ArchitectureLevel::Project => Self {
                id: "project:project".to_string(),
                label: "project".to_string(),
                kind: AggregatedNodeKind::Project,
                package: None,
                namespace: None,
            },
            ArchitectureLevel::Package => {
                let package = package_name(node);
                Self {
                    id: format!("package:{package}"),
                    label: package.clone(),
                    kind: AggregatedNodeKind::Package,
                    package: Some(package),
                    namespace: None,
                }
            }
            ArchitectureLevel::Module => {
                let package = package_name(node);
                let namespace = node
                    .namespace()
                    .map(ToOwned::to_owned)
                    .or_else(|| node.file_stem())
                    .unwrap_or_else(|| "unknown".to_string());
                Self {
                    id: format!("namespace:{package}::{namespace}"),
                    label: format!("{package}::{namespace}"),
                    kind: AggregatedNodeKind::Namespace,
                    package: Some(package),
                    namespace: Some(namespace),
                }
            }
            ArchitectureLevel::File => {
                let package = package_name(node);
                let namespace = node.namespace().map(ToOwned::to_owned);
                let file = node.file_name().unwrap_or_else(|| "unknown".to_string());
                Self {
                    id: block_node_id(node.block_id),
                    label: file,
                    kind: AggregatedNodeKind::Node,
                    package: Some(package),
                    namespace,
                }
            }
        }
    }
}

fn package_name(node: &CollectedNode) -> String {
    node.package()
        .or(node.unit_meta.project_name.as_deref())
        .unwrap_or("unknown")
        .to_string()
}

fn block_node_id(block_id: BlockId) -> String {
    format!("block:{}", block_id.as_u32())
}
