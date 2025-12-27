//! Symbol snapshot capture and rendering.

use super::{Snapshot, SnapshotContext};
use std::fmt::Write as _;

/// Snapshot of all symbols in the compilation context.
#[derive(Clone)]
pub struct SymbolsSnapshot {
    entries: Vec<SymbolEntry>,
}

#[derive(Clone)]
struct SymbolEntry {
    unit: usize,
    id: u32,
    kind: String,
    name: String,
    is_global: bool,
}

impl Snapshot for SymbolsSnapshot {
    fn capture(ctx: SnapshotContext<'_>) -> Self {
        let symbols = ctx.cc.get_all_symbols();
        let interner = &ctx.cc.interner;

        let mut entries = Vec::with_capacity(symbols.len());
        for symbol in symbols {
            let name_str = interner
                .resolve_owned(symbol.name)
                .unwrap_or_else(|| "?".to_string());

            entries.push(SymbolEntry {
                unit: symbol.unit_index().unwrap_or_default(),
                id: symbol.id().0 as u32,
                kind: format!("{:?}", symbol.kind()),
                name: name_str,
                is_global: symbol.is_global(),
            });
        }

        // Sort by unit, then id, then kind, then name
        entries.sort_by(|a, b| {
            a.unit
                .cmp(&b.unit)
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.name.cmp(&b.name))
        });

        Self { entries }
    }

    fn render(&self) -> String {
        if self.entries.is_empty() {
            return "none\n".to_string();
        }

        let label_width = self
            .entries
            .iter()
            .map(|e| format!("u{}:{}", e.unit, e.id).len())
            .max()
            .unwrap_or(0);
        let kind_width = self.entries.iter().map(|e| e.kind.len()).max().unwrap_or(0);
        let name_width = self.entries.iter().map(|e| e.name.len()).max().unwrap_or(0);
        let global_width = if self.entries.iter().any(|e| e.is_global) {
            "[global]".len()
        } else {
            0
        };

        let mut buf = String::new();
        for entry in &self.entries {
            let label = format!("u{}:{}", entry.unit, entry.id);
            let _ = writeln!(
                buf,
                "{:<label_width$} | {:kind_width$} | {:name_width$} | {:global_width$}",
                label,
                entry.kind,
                entry.name,
                if entry.is_global { "[global]" } else { "" },
                label_width = label_width,
                kind_width = kind_width,
                name_width = name_width,
                global_width = global_width,
            );
        }
        buf
    }

    fn normalize(text: &str) -> String {
        let canonical = text
            .replace("\r\n", "\n")
            .trim_end_matches('\n')
            .to_string();

        normalize_symbols(&canonical)
    }
}

/// Parse a "uN:M" label into (unit, id).
fn parse_unit_and_id(token: &str) -> (usize, u32) {
    if let Some(stripped) = token.strip_prefix('u')
        && let Some((unit_str, id_str)) = stripped.split_once(':')
        && let (Ok(unit), Ok(id)) = (unit_str.parse::<usize>(), id_str.parse::<u32>())
    {
        return (unit, id);
    }
    (usize::MAX, u32::MAX)
}

fn normalize_symbols(text: &str) -> String {
    let mut rows: Vec<(usize, u32, String)> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }

            let parts: Vec<_> = line.split('|').map(|part| part.trim()).collect();
            if parts.is_empty() {
                return None;
            }

            let label = parts[0];
            let (unit, id) = parse_unit_and_id(label);
            let kind = parts.get(1).copied().unwrap_or("");
            let name = parts.get(2).copied().unwrap_or("");
            let global = parts.get(3).copied().unwrap_or("");

            let canonical = format!("{label} | {kind} | {name} | {global}");
            // Trim trailing whitespace from the row (e.g., when global is empty)
            let canonical = canonical.trim_end().to_string();
            Some((unit, id, canonical))
        })
        .collect();

    rows.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    rows.into_iter()
        .map(|(_, _, row)| row)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_symbols_sorts_by_unit_and_id() {
        let input = "u1:5 | Fn | foo |\nu0:3 | Struct | Bar |";
        let normalized = SymbolsSnapshot::normalize(input);
        assert!(normalized.starts_with("u0:3"));
    }

    #[test]
    fn test_normalize_symbols_trims_whitespace() {
        let input = "u0:1 | Fn | test |   ";
        let normalized = SymbolsSnapshot::normalize(input);
        assert!(!normalized.ends_with(' '));
    }
}
