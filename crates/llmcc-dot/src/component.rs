use std::collections::{BTreeMap, BTreeSet, HashMap};

use llmcc_core::{BlockId, CollectedEdge, CollectedGraph, CollectedNode, ViewDepth};

use crate::{DotDocument, DotEdge, DotNode, RenderOptions, normalize_path, sanitize_id};

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
    edges: BTreeMap<(String, String), ComponentEdge>,
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
    pub(crate) fn to_document(&self, _options: &RenderOptions, _depth: ViewDepth) -> DotDocument {
        let dot_edges = self.edges_to_dot();

        DotDocument {
            clusters: vec![],
            free_nodes: self.nodes_to_dot(),
            edges: dot_edges,
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
            .map(|((from, to), edge)| DotEdge {
                from: from.clone(),
                to: to.clone(),
                attrs: vec![
                    ("rel", "depends_on".into()),
                    ("weight", edge.weight.to_string()),
                    ("via", edge.via()),
                ],
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
    ) -> BTreeMap<(String, String), ComponentEdge> {
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

            let relation: &'static str = edge.kind.into();
            merged
                .entry((source.clone(), target.clone()))
                .or_insert_with(ComponentEdge::default)
                .add_relation(relation);
        }

        merged
    }

    /// Classify a single node into a component based on architecture level.
    fn classify_node(node: &CollectedNode, depth: ViewDepth, options: &RenderOptions) -> Component {
        match depth {
            ViewDepth::Project => Component {
                id: "project".into(),
                label: "project".into(),
                kind: "project",
                package: None,
                module: None,
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
            kind: "package",
            package: Some(package),
            module: None,
            folder,
        }
    }

    fn module_component(node: &CollectedNode, _options: &RenderOptions) -> Component {
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
        let label = format!("{package}::{segment}");

        Component {
            id,
            label,
            kind: "module",
            package: Some(package),
            module: Some(segment),
            folder,
        }
    }
}

#[derive(Default)]
struct ComponentEdge {
    weight: usize,
    relations: BTreeMap<&'static str, usize>,
}

impl ComponentEdge {
    fn add_relation(&mut self, relation: &'static str) {
        self.weight += 1;
        *self.relations.entry(relation).or_default() += 1;
    }

    fn via(&self) -> String {
        self.relations
            .iter()
            .map(|(relation, count)| format!("{relation}:{count}"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// A component node in an aggregate DOT view.
///
/// Represents a group of collected nodes at a single architecture level
/// (project, package, or module).
pub(crate) struct Component {
    id: String,
    label: String,
    kind: &'static str,
    package: Option<String>,
    module: Option<String>,
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

    /// Architecture component kind (`project`, `package`, or `module`).
    pub(crate) fn kind(&self) -> &'static str {
        self.kind
    }

    /// Package name, if this component belongs to a package.
    pub(crate) fn package(&self) -> Option<&str> {
        self.package.as_deref()
    }

    /// Module name, if this component represents a module.
    pub(crate) fn module(&self) -> Option<&str> {
        self.module.as_deref()
    }

    /// Filesystem folder path, if known.
    pub(crate) fn folder(&self) -> Option<&str> {
        self.folder.as_deref()
    }
}

impl From<&Component> for DotNode {
    fn from(component: &Component) -> Self {
        let mut attrs = Vec::new();
        attrs.push(("kind", component.kind().to_owned()));
        if let Some(package) = component.package() {
            attrs.push(("package", package.to_owned()));
        }
        if let Some(module) = component.module() {
            attrs.push(("module", module.to_owned()));
        }
        if let Some(folder) = component.folder() {
            attrs.push(("path", normalize_path(folder)));
        }
        Self {
            id: component.id().to_owned(),
            label: component.label().to_owned(),
            attrs,
        }
    }
}
