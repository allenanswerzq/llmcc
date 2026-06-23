use std::cmp::Ordering;
use std::fmt::Write as _;

use llmcc_error::{Error, ErrorKind, Result};

use super::{
    BlockRelationSnapshot, BlockSnapshot, PipelineSummary, SymbolDependencySnapshot, SymbolSnapshot,
};

pub(crate) fn render_expectation(
    kind: &str,
    summary: &PipelineSummary,
    case_id: &str,
) -> Result<String> {
    match kind {
        "symbols" => {
            let symbols = summary.symbols.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidArgument,
                    format!("case {case_id} requested symbols but summary missing"),
                )
            })?;
            Ok(render_symbol_snapshot(symbols))
        }
        "symbol-types" => Ok("symbol-types snapshot not yet implemented\n".to_string()),
        "block-relations" => {
            let relations = summary.block_relations.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidArgument,
                    format!("case {case_id} requested block-relations but summary missing"),
                )
            })?;
            Ok(render_block_relations_snapshot(relations))
        }
        "dep-graph" => summary.dep_graph_dot.clone().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidArgument,
                format!("case {case_id} requested dep-graph output but summary missing"),
            )
        }),
        "arch-graph" => summary.arch_graph_dot.clone().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidArgument,
                format!("case {case_id} requested arch-graph output but summary missing"),
            )
        }),
        "arch-graph-depth-0" => summary.arch_graph_depth_0.clone().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidArgument,
                format!("case {case_id} requested arch-graph-depth-0 output but summary missing"),
            )
        }),
        "arch-graph-depth-1" => summary.arch_graph_depth_1.clone().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidArgument,
                format!("case {case_id} requested arch-graph-depth-1 output but summary missing"),
            )
        }),
        "arch-graph-depth-2" => summary.arch_graph_depth_2.clone().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidArgument,
                format!("case {case_id} requested arch-graph-depth-2 output but summary missing"),
            )
        }),
        "arch-graph-depth-3" => summary.arch_graph_depth_3.clone().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidArgument,
                format!("case {case_id} requested arch-graph-depth-3 output but summary missing"),
            )
        }),
        "blocks" => Ok("blocks snapshot not yet implemented\n".to_string()),
        "block-deps" => summary
            .block_deps
            .as_ref()
            .map(|deps| render_symbol_dependencies(deps))
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidArgument,
                    format!("case {case_id} requested block-deps output but summary missing"),
                )
            }),
        "block-graph" => summary.block_graph.clone().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidArgument,
                format!("case {case_id} requested block-graph output but summary missing"),
            )
        }),
        "symbol-deps" => {
            let deps = summary.symbol_deps.as_ref().ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidArgument,
                    format!("case {case_id} requested symbol-deps but summary missing"),
                )
            })?;
            Ok(render_symbol_dependencies(deps))
        }
        other => Err(Error::new(
            ErrorKind::InvalidArgument,
            format!("case {case_id} uses unsupported expectation '{other}'"),
        )),
    }
}

fn render_symbol_snapshot(entries: &[SymbolSnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    let mut rows = entries.to_vec();
    rows.sort_by(|a, b| {
        a.unit
            .cmp(&b.unit)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });

    let label_width = rows
        .iter()
        .map(|row| format!("u{}:{}", row.unit, row.id).len())
        .max()
        .unwrap_or(0);
    let kind_width = rows.iter().map(|row| row.kind.len()).max().unwrap_or(0);
    let name_width = rows.iter().map(|row| row.name.len()).max().unwrap_or(0);
    let global_width = if rows.iter().any(|row| row.is_global) {
        "[global]".len()
    } else {
        0
    };

    let mut buf = String::new();
    for row in rows {
        let label = format!("u{}:{}", row.unit, row.id);
        let _ = writeln!(
            buf,
            "{:<label_width$} | {:kind_width$} | {:name_width$} | {:global_width$}",
            label,
            row.kind,
            row.name,
            if row.is_global { "[global]" } else { "" },
            label_width = label_width,
            kind_width = kind_width,
            name_width = name_width,
            global_width = global_width,
        );
    }
    buf
}

#[allow(dead_code)]
fn render_symbol_types_snapshot(entries: &[SymbolSnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    let mut rows = entries.to_vec();
    rows.sort_by(|a, b| {
        a.unit
            .cmp(&b.unit)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
    });

    let label_width = rows
        .iter()
        .map(|row| format!("u{}:{}", row.unit, row.id).len())
        .max()
        .unwrap_or(0);
    let kind_width = rows.iter().map(|row| row.kind.len()).max().unwrap_or(0);
    let name_width = rows.iter().map(|row| row.name.len()).max().unwrap_or(0);

    let mut buf = String::new();
    for row in rows {
        let label = format!("u{}:{}", row.unit, row.id);
        let type_info = if let Some(type_of) = &row.type_of {
            format!("-> {type_of}")
        } else {
            String::new()
        };
        let block_info = if let Some(block_id) = &row.block_id {
            format!("[{block_id}]")
        } else {
            String::new()
        };
        let _ = writeln!(
            buf,
            "{:<label_width$} | {:kind_width$} | {:name_width$} | {} {}",
            label,
            row.kind,
            row.name,
            type_info,
            block_info,
            label_width = label_width,
            kind_width = kind_width,
            name_width = name_width,
        );
    }
    buf
}

fn render_block_relations_snapshot(entries: &[BlockRelationSnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    let skip_relations = ["contains", "contained_by"];
    let mut edges: Vec<(String, String, String)> = Vec::new();

    for entry in entries {
        let id = entry.label.replace("u0:", "");
        let source = format!("{}:{} ({})", entry.name, id, entry.kind);

        for (rel_type, targets) in &entry.relations {
            if skip_relations.contains(&rel_type.as_str()) {
                continue;
            }

            for target_label in targets {
                let (target_name, target_kind) = entries
                    .iter()
                    .find(|e| e.label == *target_label)
                    .map(|e| (e.name.as_str(), e.kind.as_str()))
                    .unwrap_or(("?", "?"));
                let target_id = target_label.replace("u0:", "");
                let target = format!("{target_name}:{target_id} ({target_kind})");
                edges.push((source.clone(), rel_type.clone(), target));
            }
        }
    }

    if edges.is_empty() {
        return "none\n".to_string();
    }

    edges.sort();

    let source_width = edges.iter().map(|(s, _, _)| s.len()).max().unwrap_or(0);
    let rel_width = edges.iter().map(|(_, r, _)| r.len()).max().unwrap_or(0);

    let mut buf = String::new();
    for (source, rel, target) in &edges {
        let _ = writeln!(
            buf,
            "{source:<source_width$}  --{rel:^rel_width$}-->  {target}",
        );
    }
    buf
}

#[allow(dead_code)]
fn render_block_snapshot(entries: &[BlockSnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    let mut rows = entries.to_vec();
    rows.sort_by(compare_block_snapshots);

    let label_width = rows.iter().map(|row| row.label.len()).max().unwrap_or(0);
    let kind_width = rows.iter().map(|row| row.kind.len()).max().unwrap_or(0);
    let name_width = rows.iter().map(|row| row.name.len()).max().unwrap_or(0);

    let mut buf = String::new();
    for row in rows {
        let _ = writeln!(
            buf,
            "{:<label_width$} | {:kind_width$} | {:name_width$}",
            row.label,
            row.kind,
            row.name,
            label_width = label_width,
            kind_width = kind_width,
            name_width = name_width,
        );
    }
    buf
}

#[allow(dead_code)]
fn compare_block_snapshots(a: &BlockSnapshot, b: &BlockSnapshot) -> Ordering {
    match (parse_block_label(&a.label), parse_block_label(&b.label)) {
        (Some(ka), Some(kb)) => ka
            .cmp(&kb)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a
            .label
            .cmp(&b.label)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name)),
    }
}

#[allow(dead_code)]
fn parse_block_label(label: &str) -> Option<(usize, usize)> {
    let mut parts = label.split(':');
    let unit_part = parts.next()?.strip_prefix('u')?;
    let block_part = parts.next()?;
    let unit = unit_part.parse().ok()?;
    let block = block_part.parse().ok()?;
    Some((unit, block))
}

fn render_symbol_dependencies(entries: &[SymbolDependencySnapshot]) -> String {
    if entries.is_empty() {
        return "none\n".to_string();
    }

    let mut rows = entries.to_vec();
    rows.sort_by(|a, b| a.label.cmp(&b.label));

    let mut buf = String::new();
    for row in rows {
        let mut depends = row.depends_on.clone();
        depends.sort();
        let mut depended = row.depended_by.clone();
        depended.sort();
        if !depends.is_empty() {
            let _ = writeln!(buf, "{} -> [{}]", row.label, depends.join(", "));
        }
        if !depended.is_empty() {
            let _ = writeln!(buf, "{} <- [{}]", row.label, depended.join(", "));
        }
    }
    buf
}
