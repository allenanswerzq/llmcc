//! JSON graph format for machine-readable llmcc architecture output.
//!
//! The format is intentionally separate from DOT: DOT remains optimized for
//! humans, while this crate exposes stable graph facts for tests and agents.

use std::collections::{BTreeMap, BTreeSet};

use llmcc_core::graph::ProjectGraph;
use llmcc_core::{BlockId, CollectedEdge, CollectedEdgeKind, CollectedGraph, CollectedNode};
use llmcc_error::{Error, ErrorKind, Result};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString, FromRepr, IntoStaticStr};

pub const GRAPH_SCHEMA: &str = "llmcc.graph";
pub const GRAPH_SCHEMA_VERSION: u32 = 1;

/// Component grouping depth for JSON architecture graph output.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Deserialize,
    Serialize,
    Display,
    EnumString,
    FromRepr,
    IntoStaticStr,
)]
#[repr(usize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum GraphDepth {
    /// Aggregate every node into one project component.
    #[strum(serialize = "project", serialize = "0")]
    Project,
    /// Aggregate nodes by package or library boundary.
    #[strum(serialize = "package", serialize = "1")]
    Package,
    /// Aggregate nodes by namespace, module, or equivalent grouping.
    #[strum(serialize = "namespace", serialize = "2")]
    Namespace,
    /// Keep individual collected graph nodes.
    #[default]
    #[strum(serialize = "file", serialize = "3")]
    File,
}

impl GraphDepth {
    pub fn is_aggregated(self) -> bool {
        !matches!(self, Self::File)
    }
}

impl From<usize> for GraphDepth {
    fn from(value: usize) -> Self {
        Self::from_repr(value).unwrap_or_default()
    }
}

impl From<GraphDepth> for usize {
    fn from(value: GraphDepth) -> Self {
        value as usize
    }
}

/// Versioned graph document serialized by [`render_graph`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GraphDocument {
    pub schema: String,
    pub schema_version: u32,
    pub depth: GraphDepth,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

/// Node in the machine-readable graph.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub block_ids: Vec<u32>,
    pub node_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceLocation>,
}

/// Source location metadata that avoids absolute paths in stable outputs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
}

/// Directed dependency edge in the machine-readable graph.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub weight: usize,
}

/// Render a project graph to pretty JSON.
pub fn render_graph(project: &ProjectGraph, depth: GraphDepth) -> Result<String> {
    serde_json::to_string_pretty(&format_graph(project, depth)).map_err(|error| {
        Error::new(
            ErrorKind::SerializationFailed,
            format!("failed to serialize llmcc graph JSON: {error}"),
        )
    })
}

/// Build the typed graph document without serializing it.
pub fn format_graph(project: &ProjectGraph, depth: GraphDepth) -> GraphDocument {
    let graph = CollectedGraph::new(project);
    if depth.is_aggregated() {
        return aggregate_graph(graph.nodes(), graph.edges(), depth);
    }

    file_graph(graph.nodes(), graph.edges())
}

fn file_graph(nodes: &[CollectedNode], edges: &BTreeSet<CollectedEdge>) -> GraphDocument {
    let mut node_ids = BTreeMap::new();
    let nodes = nodes
        .iter()
        .map(|node| {
            let id = block_node_id(node.block_id);
            node_ids.insert(node.block_id, id.clone());
            GraphNode {
                id,
                label: node.name.clone(),
                kind: node
                    .symbol_kind
                    .map(|kind| {
                        let value: &'static str = kind.into();
                        value
                    })
                    .unwrap_or("unknown")
                    .to_string(),
                block_ids: vec![node.block_id.as_u32()],
                node_count: 1,
                package: node.package().map(ToOwned::to_owned),
                namespace: node.namespace().map(ToOwned::to_owned),
                source: node.file_name().map(|file| SourceLocation {
                    file,
                    line: node.source_line,
                }),
            }
        })
        .collect();

    GraphDocument {
        schema: GRAPH_SCHEMA.to_string(),
        schema_version: GRAPH_SCHEMA_VERSION,
        depth: GraphDepth::File,
        nodes,
        edges: render_edges(edges, |block_id| node_ids.get(&block_id).cloned()),
    }
}

fn aggregate_graph(
    nodes: &[CollectedNode],
    edges: &BTreeSet<CollectedEdge>,
    depth: GraphDepth,
) -> GraphDocument {
    let mut block_to_component = BTreeMap::new();
    let mut components = BTreeMap::new();

    for node in nodes {
        let key = ComponentKey::from_node(node, depth);
        block_to_component.insert(node.block_id, key.id.clone());
        components
            .entry(key.id.clone())
            .and_modify(|component: &mut GraphNode| {
                component.block_ids.push(node.block_id.as_u32());
                component.node_count += 1;
            })
            .or_insert_with(|| GraphNode {
                id: key.id,
                label: key.label,
                kind: key.kind,
                block_ids: vec![node.block_id.as_u32()],
                node_count: 1,
                package: key.package,
                namespace: key.namespace,
                source: None,
            });
    }

    let mut nodes: Vec<_> = components.into_values().collect();
    for node in &mut nodes {
        node.block_ids.sort_unstable();
    }

    GraphDocument {
        schema: GRAPH_SCHEMA.to_string(),
        schema_version: GRAPH_SCHEMA_VERSION,
        depth,
        nodes,
        edges: render_edges(edges, |block_id| block_to_component.get(&block_id).cloned()),
    }
}

fn render_edges(
    edges: &BTreeSet<CollectedEdge>,
    node_id: impl Fn(BlockId) -> Option<String>,
) -> Vec<GraphEdge> {
    let mut grouped = BTreeMap::<(String, String, String), usize>::new();

    for edge in edges {
        let (source_id, target_id) = dependency_direction(edge);
        let Some(source) = node_id(source_id) else {
            continue;
        };
        let Some(target) = node_id(target_id) else {
            continue;
        };
        if source == target {
            continue;
        }

        let kind = edge.kind.to_string();
        *grouped.entry((source, target, kind)).or_insert(0) += 1;
    }

    grouped
        .into_iter()
        .map(|((source, target, kind), weight)| GraphEdge {
            source,
            target,
            kind,
            weight,
        })
        .collect()
}

fn dependency_direction(edge: &CollectedEdge) -> (BlockId, BlockId) {
    if reverses_for_dependency(edge.kind) {
        (edge.to_id, edge.from_id)
    } else {
        (edge.from_id, edge.to_id)
    }
}

fn reverses_for_dependency(kind: CollectedEdgeKind) -> bool {
    matches!(
        kind,
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

fn block_node_id(block_id: BlockId) -> String {
    format!("block:{}", block_id.as_u32())
}

struct ComponentKey {
    id: String,
    label: String,
    kind: String,
    package: Option<String>,
    namespace: Option<String>,
}

impl ComponentKey {
    fn from_node(node: &CollectedNode, depth: GraphDepth) -> Self {
        match depth {
            GraphDepth::Project => Self {
                id: "project:project".to_string(),
                label: "project".to_string(),
                kind: "project".to_string(),
                package: None,
                namespace: None,
            },
            GraphDepth::Package => {
                let package = node.package().unwrap_or("unknown").to_string();
                Self {
                    id: format!("package:{package}"),
                    label: package.clone(),
                    kind: "package".to_string(),
                    package: Some(package),
                    namespace: None,
                }
            }
            GraphDepth::Namespace => {
                let package = node.package().unwrap_or("unknown").to_string();
                let namespace = node
                    .namespace()
                    .map(ToOwned::to_owned)
                    .or_else(|| node.file_stem())
                    .unwrap_or_else(|| "unknown".to_string());
                Self {
                    id: format!("namespace:{package}::{namespace}"),
                    label: format!("{package}::{namespace}"),
                    kind: "namespace".to_string(),
                    package: Some(package),
                    namespace: Some(namespace),
                }
            }
            GraphDepth::File => Self {
                id: block_node_id(node.block_id),
                label: node.name.clone(),
                kind: "node".to_string(),
                package: node.package().map(ToOwned::to_owned),
                namespace: node.namespace().map(ToOwned::to_owned),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_depth_numeric_conversion_defaults_to_file() {
        assert_eq!(GraphDepth::from(0), GraphDepth::Project);
        assert_eq!(GraphDepth::from(1), GraphDepth::Package);
        assert_eq!(GraphDepth::from(2), GraphDepth::Namespace);
        assert_eq!(GraphDepth::from(3), GraphDepth::File);
        assert_eq!(GraphDepth::from(99), GraphDepth::File);

        assert_eq!(usize::from(GraphDepth::Project), 0);
        assert_eq!(usize::from(GraphDepth::Package), 1);
        assert_eq!(usize::from(GraphDepth::Namespace), 2);
        assert_eq!(usize::from(GraphDepth::File), 3);
    }

    #[test]
    fn graph_depth_string_conversions_are_derived() {
        assert_eq!(GraphDepth::Project.to_string(), "project");
        assert_eq!("project".parse::<GraphDepth>(), Ok(GraphDepth::Project));
        assert_eq!("PACKAGE".parse::<GraphDepth>(), Ok(GraphDepth::Package));
        assert_eq!("2".parse::<GraphDepth>(), Ok(GraphDepth::Namespace));

        let value: &'static str = GraphDepth::File.into();
        assert_eq!(value, "file");
    }

    #[test]
    fn graph_document_round_trips_through_json() {
        let document = GraphDocument {
            schema: GRAPH_SCHEMA.to_string(),
            schema_version: GRAPH_SCHEMA_VERSION,
            depth: GraphDepth::File,
            nodes: vec![GraphNode {
                id: "block:1".to_string(),
                label: "run".to_string(),
                kind: "function".to_string(),
                block_ids: vec![1],
                node_count: 1,
                package: None,
                namespace: None,
                source: Some(SourceLocation {
                    file: "main.rs".to_string(),
                    line: 1,
                }),
            }],
            edges: Vec::new(),
        };

        let json = serde_json::to_string(&document).unwrap();
        let parsed: GraphDocument = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed, document);
    }
}
