//! Test output renderers.
//!
//! Each renderer converts compiled IR/graph data into a deterministic text
//! format suitable for golden-file comparison. The output formats are stable
//! and intentionally simple — changes to them require updating test expectations.

use std::fmt::{self, Write};

use llmcc_core::block::BlockKind;
use llmcc_core::context::{CompileCtxt, CompileUnit};
use llmcc_core::{BlockId, CollectedGraph, ProjectGraph, ViewDepth};
use llmcc_dot::RenderOptions;

use crate::corpus::OutputKind;

/// Format a block/symbol label: `u{unit}:{id}`.
fn label(unit: usize, id: impl fmt::Display) -> String {
    format!("u{unit}:{id}")
}

// --- Dispatch ---

/// Render the text output for a given `OutputKind`.
///
/// Returns `None` if rendering requires a graph but none was built.
pub fn render(
    kind: OutputKind,
    cc: &CompileCtxt<'_>,
    project: Option<&ProjectGraph<'_>>,
) -> Option<String> {
    match kind {
        OutputKind::Symbols | OutputKind::SymbolTypes => Some(symbols(cc)),
        OutputKind::SymbolDeps => Some(String::new()),
        OutputKind::BlockGraph => Some(block_graph(project?)),
        OutputKind::BlockRelations => Some(block_relations(project?)),
        OutputKind::Blocks | OutputKind::BlockDeps => Some(block_deps(project?)),
        OutputKind::File => Some(arch_graph(project?, ViewDepth::File)),
        OutputKind::Project => Some(arch_graph(project?, ViewDepth::Project)),
        OutputKind::Package => Some(arch_graph(project?, ViewDepth::Package)),
        OutputKind::Namespace => Some(arch_graph(project?, ViewDepth::Module)),
    }
}

// --- Symbols ---

/// Tabular symbol dump: `u0:1 | Function | foo | [global]`
fn symbols(cc: &CompileCtxt<'_>) -> String {
    let syms = cc.symbols();
    let interner = cc.interner();

    if syms.is_empty() {
        return "none\n".into();
    }

    // Collect typed rows.
    let mut rows: Vec<SymRow> = syms
        .iter()
        .map(|sym| SymRow {
            unit: sym.unit_index().unwrap_or_default(),
            id: sym.id().0,
            kind: format!("{:?}", sym.kind()),
            name: interner.try_resolve(sym.name).unwrap_or_else(|| "?".into()),
            global: sym.is_global(),
        })
        .collect();
    rows.sort_by(|a, b| a.unit.cmp(&b.unit).then(a.id.cmp(&b.id)));

    // Compute column widths for aligned output.
    let col = ColWidths::from_rows(&rows);
    let has_globals = rows.iter().any(|r| r.global);

    let mut buf = String::with_capacity(rows.len() * 60);
    for row in &rows {
        let lbl = label(row.unit, row.id);
        let _ = write!(
            buf,
            "{lbl:<lw$} | {:<kw$} | {:<nw$} |",
            row.kind,
            row.name,
            lw = col.label,
            kw = col.kind,
            nw = col.name,
        );
        if has_globals {
            let flag = if row.global { " [global]" } else { "         " };
            buf.push_str(flag);
        }
        buf.push('\n');
    }
    buf
}

struct SymRow {
    unit: usize,
    id: usize,
    kind: String,
    name: String,
    global: bool,
}

struct ColWidths {
    label: usize,
    kind: usize,
    name: usize,
}

impl ColWidths {
    fn from_rows(rows: &[SymRow]) -> Self {
        Self {
            label: rows
                .iter()
                .map(|r| label(r.unit, r.id).len())
                .max()
                .unwrap_or(0),
            kind: rows.iter().map(|r| r.kind.len()).max().unwrap_or(0),
            name: rows.iter().map(|r| r.name.len()).max().unwrap_or(0),
        }
    }
}

// --- Block graph (s-expression tree) ---

/// Render the block tree as indented s-expressions.
fn block_graph(project: &ProjectGraph<'_>) -> String {
    let mut units: Vec<_> = project.units().iter().collect();
    if units.is_empty() {
        return "none\n".into();
    }
    units.sort_by_key(|u| u.unit_index());

    let mut w = SExprWriter::new();
    for (i, ug) in units.iter().enumerate() {
        if i > 0 {
            w.blank_line();
        }
        let unit = project.context().compile_unit(ug.unit_index());
        write_block_tree(&mut w, ug.root(), unit);
    }
    w.finish()
}

/// Recursive block tree writer using structured s-expression output.
fn write_block_tree(w: &mut SExprWriter, id: BlockId, unit: CompileUnit<'_>) {
    let block = unit.block(id);
    let label = block.to_string();
    let deps = block.dependency_labels(unit);
    let all_children = block.children();

    // Use the unfiltered child list for the leaf/parent decision (matches
    // the stable test format where Call-only parents still get open/close).
    if all_children.is_empty() && deps.is_empty() {
        w.leaf(&label);
    } else {
        w.open(&label);
        for child_id in all_children {
            if unit.block(child_id).kind() == BlockKind::Call {
                continue;
            }
            write_block_tree(w, child_id, unit);
        }
        for dep in deps {
            w.leaf(&dep);
        }
        w.close();
    }
}

/// Structured writer for indented s-expressions.
///
/// Produces output like:
/// ```text
/// (Module foo
///   (Function bar)
///   (dep: baz)
/// )
/// ```
struct SExprWriter {
    buf: String,
    depth: usize,
}

impl SExprWriter {
    fn new() -> Self {
        Self {
            buf: String::with_capacity(1024),
            depth: 0,
        }
    }

    /// Write a leaf node: `(label)`
    fn leaf(&mut self, label: &str) {
        let indent = "  ".repeat(self.depth);
        let _ = writeln!(self.buf, "{indent}({label})");
    }

    /// Open a node with children: `(label\n`
    fn open(&mut self, label: &str) {
        let indent = "  ".repeat(self.depth);
        let _ = writeln!(self.buf, "{indent}({label}");
        self.depth += 1;
    }

    /// Close the current node: `)`
    fn close(&mut self) {
        self.depth = self.depth.saturating_sub(1);
        let indent = "  ".repeat(self.depth);
        let _ = writeln!(self.buf, "{indent})");
    }

    /// Insert a blank separator line.
    fn blank_line(&mut self) {
        self.buf.push('\n');
    }

    /// Consume the writer and return the final string (trimmed trailing blank line).
    fn finish(mut self) -> String {
        // Trim trailing blank line but keep final newline.
        while self.buf.ends_with("\n\n") {
            self.buf.pop();
        }
        if !self.buf.ends_with('\n') {
            self.buf.push('\n');
        }
        self.buf
    }
}

// --- Block relations ---

/// One relation row: `u0:1 (Struct Foo) implements: [u0:2, u0:3]`
fn block_relations(project: &ProjectGraph<'_>) -> String {
    let cc = project.context();
    let related_map = cc.block_relations();
    let mut rows: Vec<RelRow> = Vec::new();

    for block_id in related_map.blocks() {
        let Some(info) = cc.block_info(block_id) else {
            continue;
        };
        let relations = related_map.relations_from(block_id);
        if relations.is_empty() {
            continue;
        }

        let source = label(info.unit_index, block_id.as_u32());
        let kind = info.kind.to_string();
        let name = info.name.clone().unwrap_or_default();

        for rel in relations.iter() {
            let mut targets: Vec<String> = rel
                .targets
                .iter()
                .map(|tid| {
                    let tu = cc.block_info(*tid).map(|e| e.unit_index).unwrap_or(0);
                    label(tu, tid.as_u32())
                })
                .collect();
            targets.sort();
            rows.push(RelRow {
                source: source.clone(),
                kind: kind.clone(),
                name: name.clone(),
                relation: rel.relation.to_string(),
                targets,
            });
        }
    }

    sorted_lines(&rows)
}

struct RelRow {
    source: String,
    kind: String,
    name: String,
    relation: String,
    targets: Vec<String>,
}

impl fmt::Display for RelRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({} {}) {}: [{}]",
            self.source,
            self.kind,
            self.name,
            self.relation,
            self.targets.join(", ")
        )
    }
}

// --- Block deps ---

/// Block listing: `u0:1 | Struct | Foo`
fn block_deps(project: &ProjectGraph<'_>) -> String {
    let cc = project.context();
    let mut rows: Vec<DepRow> = Vec::new();

    for ug in project.units() {
        for entry in cc.find_blocks_in_unit(ug.unit_index()) {
            rows.push(DepRow {
                label: label(entry.unit_index, entry.block_id.as_u32()),
                kind: entry.kind.to_string(),
                name: entry.name.clone().unwrap_or_default(),
            });
        }
    }

    sorted_lines(&rows)
}

struct DepRow {
    label: String,
    kind: String,
    name: String,
}

impl fmt::Display for DepRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} | {} | {}", self.label, self.kind, self.name)
    }
}

// --- Architecture graph (DOT) ---

fn arch_graph(project: &ProjectGraph<'_>, depth: ViewDepth) -> String {
    let graph = CollectedGraph::new(project);
    let opts = RenderOptions::default();
    llmcc_dot::render(&graph, depth, &opts)
}

// --- Shared helpers ---

/// Render Display items as sorted lines, or "none\n" if empty.
fn sorted_lines(items: &[impl fmt::Display]) -> String {
    if items.is_empty() {
        return "none\n".into();
    }
    let mut lines: Vec<String> = items.iter().map(|i| i.to_string()).collect();
    lines.sort();
    let mut out = lines.join("\n");
    out.push('\n');
    out
}
