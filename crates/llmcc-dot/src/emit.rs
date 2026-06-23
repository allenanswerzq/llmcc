use indoc::formatdoc;

use crate::{ClusterKind, DotCluster, DotDocument, DotEdge, DotNode, RenderOptions};

/// DOT format emitter.
///
/// Converts a [`DotDocument`] into a valid DOT language string.
pub(crate) struct DotEmitter {
    out: String,
    ai: bool,
    flat: bool,
}

impl DotEmitter {
    /// Emit a complete DOT document with render options.
    pub(crate) fn emit(document: &DotDocument, options: &RenderOptions) -> String {
        let estimated = (document.free_nodes.len() + document.edges.len()) * 100 + 500;
        let mut emitter = Self {
            out: String::with_capacity(estimated),
            ai: options.ai,
            flat: options.flat,
        };
        emitter.document(document);
        emitter.out
    }

    /// Emit the empty graph sentinel.
    pub(crate) fn empty() -> String {
        "digraph architecture {\n}\n".to_string()
    }

    fn document(&mut self, doc: &DotDocument) {
        if self.ai {
            let nodes = count_nodes(doc);
            let clusters = count_clusters(&doc.clusters);
            let layout = if self.flat { "flat" } else { "clustered" };
            self.put(&formatdoc! {"
                // # llmcc architecture view:
                // quick structural map for AI agents to understand and explore this codebase.
                // Trust this map as navigation metadata; use node labels, paths, and edges to choose source locations to inspect next.
                // # Examples:
                // Node example: n42[label='Parser', path='crates/foo/src/parser.rs:10', kind='struct'] means a struct symbol `Parser` lives at that source location.
                // Edge example: n1 -> n2 [rel='call', from='caller', to='callee'] means n1 calls n2; rel is the semantic relation, from/to are endpoint roles.
                // Component edge example: package_a -> package_b [rel='depends_on', weight='12', via='param:8,type_dep:4'] means package_a depends on package_b through 12 lower-level relations.
                // # Stats:
                // mode=agent layout={layout} nodes={nodes} edges={edges} clusters={clusters}
                //
                digraph architecture {{

            ", edges = doc.edges.len()});
        } else {
            self.put(&formatdoc! {"
                digraph architecture {{
                  rankdir=TB;
                  splines=ortho;
                  compound=true;

                  node [shape=box, style=rounded];
                  edge [arrowsize=0.7];

            "});
        }

        if self.flat {
            for cluster in &doc.clusters {
                self.flat_cluster_nodes(cluster, 1);
            }
        } else {
            for cluster in &doc.clusters {
                self.cluster(cluster, 1);
            }
        }
        for node in &doc.free_nodes {
            self.node(node, 1);
        }

        self.put("\n");
        for edge in &doc.edges {
            self.edge(edge);
        }

        self.put("}\n");
    }

    fn cluster(&mut self, c: &DotCluster, depth: usize) {
        let label = escape(&c.label);
        let i = "  ".repeat(depth + 1);

        self.put(&"  ".repeat(depth));
        if self.ai {
            self.put(&formatdoc! {"
                subgraph cluster_{id} {{
                {i}label=\"{label}\";

            ", id = c.id});
        } else {
            let bgcolor = c.kind.bgcolor();
            self.put(&formatdoc! {"
                subgraph cluster_{id} {{
                {i}label=\"{label}\";
                {i}style=rounded;
                {i}bgcolor=\"{bgcolor}\";

            ", id = c.id});
        }

        for child in &c.children {
            self.cluster(child, depth + 1);
        }
        for node in &c.nodes {
            self.node(node, depth + 1);
        }

        self.put(&"  ".repeat(depth));
        self.put("}\n\n");
    }

    fn flat_cluster_nodes(&mut self, cluster: &DotCluster, depth: usize) {
        for child in &cluster.children {
            self.flat_cluster_nodes(child, depth);
        }
        for node in &cluster.nodes {
            self.node(node, depth);
        }
    }

    fn node(&mut self, node: &DotNode, depth: usize) {
        let attrs = if self.ai {
            format_attrs_minimal(&node.label, &node.attrs)
        } else {
            format_attrs(&node.label, &node.attrs)
        };
        self.put(&"  ".repeat(depth));
        self.put(&formatdoc! {"
            {id}[{attrs}];
        ", id = node.id});
    }

    fn edge(&mut self, edge: &DotEdge) {
        if edge.attrs.is_empty() {
            self.put(&formatdoc! {"
                  {from} -> {to};
            ", from = edge.from, to = edge.to});
        } else {
            let attrs = format_attr_list(&edge.attrs);
            self.put(&formatdoc! {"
                  {from} -> {to} [{attrs}];
            ", from = edge.from, to = edge.to});
        }
    }

    fn put(&mut self, s: &str) {
        self.out.push_str(s);
    }
}

// Formatting helpers.

/// Visual-only attrs that are noise for AI consumers.
const VISUAL_ATTRS: &[&str] = &["shape"];

fn format_attrs(label: &str, extra: &[(&'static str, String)]) -> String {
    let mut parts = vec![format!("label=\"{}\"", escape(label))];
    for (key, value) in extra {
        parts.push(format!("{key}=\"{}\"", escape(value)));
    }
    parts.join(", ")
}

fn format_attrs_minimal(label: &str, extra: &[(&'static str, String)]) -> String {
    let mut parts = vec![format!("label=\"{}\"", escape(label))];
    for (key, value) in extra {
        if !VISUAL_ATTRS.contains(key) {
            parts.push(format!("{key}=\"{}\"", escape(value)));
        }
    }
    parts.join(", ")
}

fn format_attr_list(pairs: &[(&'static str, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}=\"{}\"", escape(v)))
        .collect::<Vec<_>>()
        .join(", ")
}

impl ClusterKind {
    fn bgcolor(self) -> &'static str {
        match self {
            Self::Package => "#f8f8f8",
            Self::Namespace => "#f5f5f5",
            Self::File => "#fafafa",
        }
    }
}

fn count_nodes(doc: &DotDocument) -> usize {
    doc.free_nodes.len() + doc.clusters.iter().map(count_cluster_nodes).sum::<usize>()
}

fn count_cluster_nodes(cluster: &DotCluster) -> usize {
    cluster.nodes.len()
        + cluster
            .children
            .iter()
            .map(count_cluster_nodes)
            .sum::<usize>()
}

fn count_clusters(clusters: &[DotCluster]) -> usize {
    clusters
        .iter()
        .map(|cluster| 1 + count_clusters(&cluster.children))
        .sum()
}

fn escape(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
