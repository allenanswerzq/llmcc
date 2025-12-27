//! Snapshot capture and rendering for test verification.
//!
//! This module provides a unified interface for capturing and rendering
//! different aspects of the compilation pipeline for golden-file testing.

mod block_graph;
mod block_relations;
mod symbols;

pub use block_graph::BlockGraphSnapshot;
pub use block_relations::BlockRelationsSnapshot;
pub use symbols::SymbolsSnapshot;

/// A snapshot that can be captured from compilation context and rendered to text.
pub trait Snapshot: Sized {
    /// Capture a snapshot from the compilation context.
    fn capture(ctx: SnapshotContext<'_>) -> Self;

    /// Render the snapshot to a string for comparison.
    fn render(&self) -> String;

    /// Normalize text for comparison (handles whitespace, sorting, etc.).
    fn normalize(text: &str) -> String;
}

/// Context passed to snapshot capture methods.
pub struct SnapshotContext<'a> {
    pub cc: &'a llmcc_core::context::CompileCtxt<'a>,
    pub project_graph: Option<&'a llmcc_core::ProjectGraph<'a>>,
}

impl<'a> SnapshotContext<'a> {
    pub fn new(cc: &'a llmcc_core::context::CompileCtxt<'a>) -> Self {
        Self {
            cc,
            project_graph: None,
        }
    }

    pub fn with_project_graph(mut self, pg: &'a llmcc_core::ProjectGraph<'a>) -> Self {
        self.project_graph = Some(pg);
        self
    }
}

/// Format a label like "u0:42" for unit index and ID.
#[allow(dead_code)]
pub fn format_unit_label(unit: usize, id: u32) -> String {
    format!("u{}:{}", unit, id)
}

/// Parse a label like "u0:42" into (unit, id).
#[allow(dead_code)]
pub fn parse_unit_label(label: &str) -> Option<(usize, u32)> {
    let stripped = label.strip_prefix('u')?;
    let (unit_str, id_str) = stripped.split_once(':')?;
    let unit = unit_str.parse().ok()?;
    let id = id_str.parse().ok()?;
    Some((unit, id))
}

/// Helper to build aligned tabular output.
#[allow(dead_code)]
pub struct TableBuilder {
    rows: Vec<Vec<String>>,
    widths: Vec<usize>,
}

impl TableBuilder {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            widths: Vec::new(),
        }
    }

    pub fn add_row(&mut self, cells: Vec<String>) {
        // Update column widths
        for (i, cell) in cells.iter().enumerate() {
            if i >= self.widths.len() {
                self.widths.push(cell.len());
            } else {
                self.widths[i] = self.widths[i].max(cell.len());
            }
        }
        self.rows.push(cells);
    }

    #[allow(dead_code)]
    pub fn render(&self) -> String {
        use std::fmt::Write;
        let mut buf = String::new();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i > 0 {
                    buf.push_str(" | ");
                }
                let width = self.widths.get(i).copied().unwrap_or(0);
                let _ = write!(buf, "{:<width$}", cell, width = width);
            }
            // Trim trailing whitespace from each line
            let trimmed = buf.trim_end();
            buf.truncate(trimmed.len());
            buf.push('\n');
        }
        buf
    }
}

impl Default for TableBuilder {
    fn default() -> Self {
        Self::new()
    }
}
