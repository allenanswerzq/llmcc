use std::collections::{BTreeMap, BTreeSet, HashMap};

use llmcc_core::{BlockId, CollectedEdge, CollectedGraph, CollectedNode, ViewDepth};

use crate::{
    ClusterKind, DotCluster, DotDocument, DotEdge, DotNode, RenderOptions, child_cluster_id,
    sanitize_id,
};

/// Mapping from block ids to their component id.
type BlockComponentMap = HashMap<BlockId, String>;

/// Ordered map of component id → component data.
type ComponentMap = BTreeMap<String, Component>;

/// Component-level view of a collected graph.
///
/// Groups individual graph nodes into architectural components (project,
/// package, or module) and aggregates edges between them with weight counts.
/// Built once from a [`CollectedGraph`] at a given [`ViewDepth`].
pub(crate) struct ComponentViewTree {
    components: ComponentMap,
    edges: BTreeMap<(String, String), usize>,
}

impl ComponentViewTree {
    /// Build a component view from a collected graph.
    ///
    /// `depth` controls the grouping granularity. `options` affects labeling
    /// and other render-time behavior.
    pub(crate) fn from_graph(
        graph: &CollectedGraph,
        depth: ViewDepth,
        options: &RenderOptions,
    ) -> Self {
        let (block_map, components) = Self::map_nodes_to_components(graph.nodes(), depth, options);
        let edges = Self::merge_edges(graph.edges(), &block_map);
        Self { components, edges }
    }

    /// Build a complete DOT document for component-level rendering.
    pub(crate) fn to_document(&self, options: &RenderOptions, depth: ViewDepth) -> DotDocument {
        let dot_edges = self.edges_to_dot();

        if options.cluster_by_package && depth == ViewDepth::Module {
            DotDocument {
                clusters: self.clusters_by_package(),
                free_nodes: vec![],
                edges: dot_edges,
            }
        } else {
            DotDocument {
                clusters: vec![],
                free_nodes: self.nodes_to_dot(),
                edges: dot_edges,
            }
        }
    }

    /// Convert components to flat DOT nodes.
    fn nodes_to_dot(&self) -> Vec<DotNode> {
        self.components.values().map(DotNode::from).collect()
    }

    /// Convert weighted edges to DOT edges.
    fn edges_to_dot(&self) -> Vec<DotEdge> {
        self.edges
            .iter()
            .map(|((from, to), weight)| DotEdge {
                from: from.clone(),
                to: to.clone(),
                attrs: vec![("weight", weight.to_string())],
            })
            .collect()
    }

    /// Group components by package into DOT clusters.
    fn clusters_by_package(&self) -> Vec<DotCluster> {
        let mut by_package: BTreeMap<String, Vec<DotNode>> = BTreeMap::new();

        for component in self.components.values() {
            let package = component.package().unwrap_or("unknown").to_owned();
            by_package
                .entry(package)
                .or_default()
                .push(DotNode::from(component));
        }

        by_package
            .into_iter()
            .enumerate()
            .map(|(index, (package, nodes))| DotCluster {
                id: child_cluster_id("packages", index, &package),
                label: package,
                kind: ClusterKind::Package,
                nodes,
                children: vec![],
            })
            .collect()
    }

    /// Classify each collected node into a component and build the component map.
    ///
    /// When multiple nodes map to the same component, the first non-None folder
    /// wins. Labels and ids are stable: they depend only on the node's metadata
    /// and the architecture level, not on insertion order.
    fn map_nodes_to_components(
        nodes: &[CollectedNode],
        depth: ViewDepth,
        options: &RenderOptions,
    ) -> (BlockComponentMap, ComponentMap) {
        let mut block_map = HashMap::with_capacity(nodes.len());
        let mut components = BTreeMap::new();

        for node in nodes {
            let component = Self::classify_node(node, depth, options);
            block_map.insert(node.block_id, component.id.clone());
            components
                .entry(component.id.clone())
                .and_modify(|existing: &mut Component| {
                    // Keep the first folder we see for this component.
                    if existing.folder.is_none() {
                        existing.folder = component.folder.clone();
                    }
                })
                .or_insert(component);
        }

        (block_map, components)
    }

    /// Merge collected edges into weighted component-level dependency edges.
    ///
    /// Edges between nodes in the same component are dropped. Edge direction
    /// is normalized to dependency order via `reverses_for_dependency()`.
    fn merge_edges(
        edges: &BTreeSet<CollectedEdge>,
        block_map: &BlockComponentMap,
    ) -> BTreeMap<(String, String), usize> {
        let mut merged = BTreeMap::new();

        for edge in edges {
            let (Some(from), Some(to)) = (block_map.get(&edge.from_id), block_map.get(&edge.to_id))
            else {
                continue;
            };
            if from == to {
                continue;
            }

            // Normalize to dependency direction.
            let (source, target) = if edge.kind.reverses_for_dependency() {
                (to, from)
            } else {
                (from, to)
            };

            *merged.entry((source.clone(), target.clone())).or_default() += 1;
        }

        merged
    }

    /// Classify a single node into a component based on architecture level.
    fn classify_node(node: &CollectedNode, depth: ViewDepth, options: &RenderOptions) -> Component {
        match depth {
            ViewDepth::Project => Component {
                id: "project".into(),
                label: "project".into(),
                package: None,
                folder: None,
            },
            ViewDepth::Package => Self::package_component(node),
            ViewDepth::Module => Self::module_component(node, options),
            ViewDepth::File => unreachable!("file-level rendering uses FileViewTree"),
        }
    }

    fn package_component(node: &CollectedNode) -> Component {
        let package = node.package().unwrap_or("unknown").to_owned();
        let id = format!("package_{}", sanitize_id(&package));
        let folder = node.package_root();
        Component {
            id,
            label: package.clone(),
            package: Some(package),
            folder,
        }
    }

    fn module_component(node: &CollectedNode, options: &RenderOptions) -> Component {
        let package = node.package().unwrap_or("unknown").to_owned();

        let (segment, folder) = if let Some(namespace) = node.namespace() {
            (
                namespace.to_owned(),
                node.namespace_root().or_else(|| node.dir()),
            )
        } else {
            (
                node.file_stem().unwrap_or_else(|| "unknown".into()),
                node.dir(),
            )
        };

        let id = format!(
            "namespace_{}_{}",
            sanitize_id(&package),
            sanitize_id(&segment),
        );
        let label = if options.short_labels {
            segment
        } else {
            format!("{package}::{segment}")
        };

        Component {
            id,
            label,
            package: Some(package),
            folder,
        }
    }
}

/// A component node in an aggregate DOT view.
///
/// Represents a group of collected nodes at a single architecture level
/// (project, package, or module).
pub(crate) struct Component {
    id: String,
    label: String,
    package: Option<String>,
    folder: Option<String>,
}

impl Component {
    /// Stable DOT node identifier for this component.
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    /// Human-readable display label.
    pub(crate) fn label(&self) -> &str {
        &self.label
    }

    /// Package name, if this component belongs to a package.
    pub(crate) fn package(&self) -> Option<&str> {
        self.package.as_deref()
    }

    /// Filesystem folder path, if known.
    pub(crate) fn folder(&self) -> Option<&str> {
        self.folder.as_deref()
    }
}

impl From<&Component> for DotNode {
    fn from(component: &Component) -> Self {
        let mut attrs = Vec::new();
        if let Some(folder) = component.folder() {
            attrs.push(("path", folder.to_owned()));
        }
        Self {
            id: component.id().to_owned(),
            label: component.label().to_owned(),
            attrs,
        }
    }
}
