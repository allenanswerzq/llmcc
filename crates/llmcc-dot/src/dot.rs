//! DOT format utilities for graph rendering.

use std::fmt::Write;

use llmcc_core::symbol::SymKind;

/// Map SymKind to DOT shape.
pub fn shape_for_kind(kind: Option<SymKind>) -> &'static str {
    match kind {
        // Types: rectangle (box)
        Some(
            SymKind::Struct
            | SymKind::Enum
            | SymKind::Trait
            | SymKind::Interface
            | SymKind::TypeAlias,
        ) => "box",
        // Modules/Files: folder shape
        Some(SymKind::Module | SymKind::File | SymKind::Namespace | SymKind::Crate) => "folder",
        // Fields/Variables: plain text (minimal)
        Some(SymKind::Field | SymKind::Variable) => "plaintext",
        // Constants: diamond
        Some(SymKind::Const | SymKind::Static) => "diamond",
        // Functions/Methods/Closures: ellipse (oval)
        _ => "ellipse",
    }
}

/// Sanitize a string to be a valid DOT identifier.
/// Replaces any non-alphanumeric character with underscore.
pub fn sanitize_id(input: &str) -> String {
    input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Escape special characters for DOT labels.
pub fn escape_label(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Write indentation to output.
pub fn write_indent(output: &mut String, level: usize) {
    for _ in 0..level {
        output.push_str("  ");
    }
}

/// A DOT graph builder for constructing valid DOT output.
#[allow(dead_code)]
pub struct DotBuilder {
    output: String,
    indent: usize,
}

impl DotBuilder {
    /// Create a new DOT graph with the given name.
    pub fn new(name: &str) -> Self {
        let mut output = String::with_capacity(4096);
        let _ = writeln!(output, "digraph {name} {{");
        Self { output, indent: 1 }
    }

    /// Add a graph attribute.
    pub fn attr(&mut self, key: &str, value: &str) -> &mut Self {
        write_indent(&mut self.output, self.indent);
        let _ = writeln!(self.output, "{}=\"{}\";", key, escape_label(value));
        self
    }

    /// Add a node style default.
    pub fn node_style(&mut self, attrs: &str) -> &mut Self {
        write_indent(&mut self.output, self.indent);
        let _ = writeln!(self.output, "node [{attrs}];");
        self
    }

    /// Add a blank line for readability.
    pub fn blank(&mut self) -> &mut Self {
        self.output.push('\n');
        self
    }

    /// Add a simple node with just an ID and label.
    pub fn node(&mut self, id: &str, label: &str) -> &mut Self {
        write_indent(&mut self.output, self.indent);
        let _ = writeln!(self.output, "{}[label=\"{}\"];", id, escape_label(label));
        self
    }

    /// Add a node with full attributes.
    pub fn node_full(&mut self, id: &str, attrs: &[(&str, &str)]) -> &mut Self {
        write_indent(&mut self.output, self.indent);
        let _ = write!(self.output, "{id}[");
        for (i, (key, value)) in attrs.iter().enumerate() {
            if i > 0 {
                self.output.push_str(", ");
            }
            let _ = write!(self.output, "{}=\"{}\"", key, escape_label(value));
        }
        self.output.push_str("];\n");
        self
    }

    /// Add an edge.
    pub fn edge(&mut self, from: &str, to: &str) -> &mut Self {
        write_indent(&mut self.output, self.indent);
        let _ = writeln!(self.output, "{from} -> {to};");
        self
    }

    /// Add an edge with attributes.
    pub fn edge_with_attrs(&mut self, from: &str, to: &str, attrs: &[(&str, &str)]) -> &mut Self {
        write_indent(&mut self.output, self.indent);
        let _ = write!(self.output, "{from} -> {to} [");
        for (i, (key, value)) in attrs.iter().enumerate() {
            if i > 0 {
                self.output.push_str(", ");
            }
            let _ = write!(self.output, "{key}=\"{value}\"");
        }
        self.output.push_str("];\n");
        self
    }


    /// Start a subgraph cluster.
    pub fn start_cluster(&mut self, id: &str, label: &str) -> &mut Self {
        write_indent(&mut self.output, self.indent);
        let _ = writeln!(self.output, "subgraph cluster_{} {{", sanitize_id(id));
        self.indent += 1;
        write_indent(&mut self.output, self.indent);
        let _ = writeln!(self.output, "label=\"{}\";", escape_label(label));
        self
    }

    /// End the current subgraph cluster.
    pub fn end_cluster(&mut self) -> &mut Self {
        self.indent -= 1;
        write_indent(&mut self.output, self.indent);
        self.output.push_str("}\n\n");
        self
    }

    /// Finish building and return the DOT string.
    pub fn build(mut self) -> String {
        self.output.push_str("}\n");
        self.output
    }

    /// Get current output (for appending raw content).
    pub fn output_mut(&mut self) -> &mut String {
        &mut self.output
    }

    /// Get current indent level.
    pub fn indent(&self) -> usize {
        self.indent
    }
}
