use indoc::formatdoc;

use crate::{ClusterKind, DotCluster, DotDocument, DotEdge, DotNode, RenderOptions};

/// DOT format emitter.
///
/// Converts a [`DotDocument`] into a valid DOT language string.
pub(crate) struct DotEmitter {
    out: String,
    minimal: bool,
}

impl DotEmitter {
    /// Emit a complete DOT document with render options.
    pub(crate) fn emit(document: &DotDocument, options: &RenderOptions) -> String {
        let estimated = (document.free_nodes.len() + document.edges.len()) * 100 + 500;
        let mut emitter = Self {
            out: String::with_capacity(estimated),
            minimal: options.for_agent,
        };
        emitter.document(document);
        emitter.out
    }

    /// Emit the empty graph sentinel.
    pub(crate) fn empty() -> String {
        "digraph architecture {\n}\n".to_string()
    }

    fn document(&mut self, doc: &DotDocument) {
        if self.minimal {
            self.put("digraph architecture {\n\n");
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

        for cluster in &doc.clusters {
            self.cluster(cluster, 1);
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
        if self.minimal {
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

    fn node(&mut self, node: &DotNode, depth: usize) {
        let attrs = if self.minimal {
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
const VISUAL_ATTRS: &[&str] = &["shape", "sym_ty"];

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

fn escape(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
