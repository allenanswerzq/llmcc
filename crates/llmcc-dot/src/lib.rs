//! DOT graph rendering for architecture visualization.
//!
//! The public API is intentionally small: create a [`DotGraph`] from a
//! [`CollectedGraph`] and render it at the requested [`ViewDepth`].

mod component;
mod emit;
mod file;

use component::ComponentViewTree;
use emit::DotEmitter;
use file::FileViewTree;
use llmcc_core::CollectedGraph;

pub use llmcc_core::ViewDepth;

/// Options that affect DOT layout and labeling.
///
/// These options are renderer-specific. Graph filtering and ranking belong on
/// [`CollectedGraph`] before rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RenderOptions {
    /// Cluster module-level components under their parent package.
    ///
    /// This only affects [`ViewDepth::Module`] aggregate rendering.
    pub cluster_by_package: bool,
    /// Use module-only labels instead of `package::module` labels.
    ///
    /// This only affects [`ViewDepth::Module`] aggregate rendering.
    pub short_labels: bool,
    /// Optimize output for AI agent consumption.
    ///
    /// When true, visual styling (colors, shapes, fonts, layout hints) is
    /// omitted. The output retains only structural information: nodes with
    /// labels/paths, edges with semantic roles, and cluster grouping.
    pub ai: bool,
    /// Emit nodes flat instead of nesting them in DOT subgraph clusters.
    ///
    /// This is useful for agent-oriented output where ownership metadata on
    /// each node is easier to consume than Graphviz cluster syntax.
    pub flat: bool,
}

impl RenderOptions {
    /// Create render options with default behavior.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable or disable package clustering for module-level graphs.
    #[must_use]
    pub fn with_cluster_by_package(mut self, enabled: bool) -> Self {
        self.cluster_by_package = enabled;
        self
    }

    /// Enable or disable short module labels.
    #[must_use]
    pub fn with_short_labels(mut self, enabled: bool) -> Self {
        self.short_labels = enabled;
        self
    }

    /// Enable or disable agent-optimized output (no visual styling).
    #[must_use]
    pub fn with_ai(mut self, enabled: bool) -> Self {
        self.ai = enabled;
        self
    }

    /// Enable or disable flat node output.
    #[must_use]
    pub fn with_flat(mut self, enabled: bool) -> Self {
        self.flat = enabled;
        self
    }
}

/// Renderable DOT view over a collected graph.
///
/// `DotGraph` borrows the graph and builds a small DOT-oriented intermediate
/// representation each time [`render`](Self::render) is called.
#[must_use]
pub struct DotGraph<'graph> {
    graph: &'graph CollectedGraph,
}

impl<'graph> DotGraph<'graph> {
    /// Create a DOT rendering view over a collected graph.
    pub fn new(graph: &'graph CollectedGraph) -> Self {
        Self { graph }
    }

    /// Render the graph to DOT format at the given architecture level.
    #[must_use]
    pub fn render(&self, depth: ViewDepth, options: &RenderOptions) -> String {
        if self.graph.is_empty() {
            return DotEmitter::empty();
        }

        let document = match depth {
            ViewDepth::File => self.build_file_view(),
            level => self.build_component_view(level, options),
        };

        DotEmitter::emit(&document, options)
    }

    fn build_file_view(&self) -> DotDocument {
        FileViewTree::from_nodes(self.graph.nodes()).to_document(self.graph)
    }

    fn build_component_view(&self, depth: ViewDepth, options: &RenderOptions) -> DotDocument {
        ComponentViewTree::from_graph(self.graph, depth, options).to_document(options, depth)
    }
}

/// Render a collected graph to DOT format (convenience wrapper).
#[must_use]
pub fn render(graph: &CollectedGraph, depth: ViewDepth, options: &RenderOptions) -> String {
    DotGraph::new(graph).render(depth, options)
}

// Intermediate representation.

pub(crate) struct DotDocument {
    pub(crate) clusters: Vec<DotCluster>,
    pub(crate) free_nodes: Vec<DotNode>,
    pub(crate) edges: Vec<DotEdge>,
}

pub(crate) struct DotCluster {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: ClusterKind,
    pub(crate) nodes: Vec<DotNode>,
    pub(crate) children: Vec<DotCluster>,
}

pub(crate) struct DotNode {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) attrs: Vec<(&'static str, String)>,
}

pub(crate) struct DotEdge {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) attrs: Vec<(&'static str, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClusterKind {
    Package,
    Namespace,
    File,
}

// Shared helpers.

pub(crate) fn child_cluster_id(parent_id: &str, sibling_index: usize, label: &str) -> String {
    format!("{parent_id}_{sibling_index}_{}", sanitize_id(label))
}

pub(crate) fn sanitize_id(input: &str) -> String {
    input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

pub(crate) fn normalize_path(input: &str) -> String {
    let normalized = input.replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_owned()
}
