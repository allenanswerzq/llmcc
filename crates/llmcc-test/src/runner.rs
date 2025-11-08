use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use llmcc_core::context::CompileCtxt;
use llmcc_core::graph_builder::{build_llmcc_graph, BlockRelation, GraphBuildConfig, ProjectGraph};
use llmcc_core::ir_builder::{build_llmcc_ir, IrBuildConfig};
use llmcc_core::lang_def::LanguageTrait;
use llmcc_resolver::apply_collected_symbols;
use llmcc_resolver::collector::CollectedSymbols;
use llmcc_rust::LangRust;
use similar::TextDiff;
use tempfile::TempDir;

use crate::corpus::{Corpus, CorpusCase, CorpusFile};

#[derive(Clone)]
struct SymbolSnapshot {
    unit: usize,
    id: u32,
    kind: String,
    name: String,
    fqn: String,
    is_global: bool,
}

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub filter: Option<String>,
    pub update: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseStatus {
    Passed,
    Failed,
    Updated,
    NoExpectations,
}

#[derive(Debug, Clone)]
pub struct CaseOutcome {
    pub id: String,
    pub status: CaseStatus,
    pub message: Option<String>,
}

pub fn run_cases(corpus: &mut Corpus, config: RunnerConfig) -> Result<Vec<CaseOutcome>> {
    let mut outcomes = Vec::new();
    let mut matched = 0usize;

    for file in corpus.files_mut() {
        outcomes.extend(run_cases_in_file(
            file,
            config.update,
            config.filter.as_deref(),
            &mut matched,
        )?);
    }

    if matched == 0 {
        return Err(anyhow!(
            "no llmcc-test cases matched filter {:?}",
            config.filter
        ));
    }

    Ok(outcomes)
}

pub fn run_cases_for_file(file: &mut CorpusFile, update: bool) -> Result<Vec<CaseOutcome>> {
    let mut matched = 0usize;
    run_cases_in_file(file, update, None, &mut matched)
}

fn run_cases_in_file(
    file: &mut CorpusFile,
    update: bool,
    filter: Option<&str>,
    matched: &mut usize,
) -> Result<Vec<CaseOutcome>> {
    let mut file_outcomes = Vec::new();
    for idx in 0..file.cases.len() {
        let run_case = {
            let case = &file.cases[idx];
            if let Some(filter_term) = filter {
                case.id().contains(filter_term)
            } else {
                true
            }
        };

        if !run_case {
            continue;
        }

        *matched += 1;
        let (outcome, mutated) = {
            let case = &mut file.cases[idx];
            evaluate_case(case, update)?
        };
        if mutated {
            file.mark_dirty();
        }
        file_outcomes.push(outcome);
    }
    Ok(file_outcomes)
}

fn evaluate_case(case: &mut CorpusCase, update: bool) -> Result<(CaseOutcome, bool)> {
    let case_id = case.id();

    if case.expectations.is_empty() {
        return Ok((
            CaseOutcome {
                id: case_id,
                status: CaseStatus::NoExpectations,
                message: Some("no expectation blocks declared".to_string()),
            },
            false,
        ));
    }

    let summary = build_pipeline_summary(case)?;
    let mut mutated = false;
    let mut status = CaseStatus::Passed;
    let mut failures = Vec::new();

    for expect in &mut case.expectations {
        let kind = expect.kind.as_str();
        let actual = render_expectation(kind, &summary, &case_id)?;
        let expected_norm = normalize(kind, &expect.value);
        let actual_norm = normalize(kind, &actual);

        if expected_norm == actual_norm {
            continue;
        }

        if update {
            expect.value = ensure_trailing_newline(actual);
            mutated = true;
            status = CaseStatus::Updated;
        } else {
            status = CaseStatus::Failed;
            failures.push(format_expectation_diff(
                &expect.kind,
                &expect.value,
                &actual,
            ));
        }
    }

    let message = if failures.is_empty() {
        None
    } else {
        Some(failures.join("\n"))
    };

    Ok((
        CaseOutcome {
            id: case_id,
            status,
            message,
        },
        mutated,
    ))
}

fn build_pipeline_summary(case: &CorpusCase) -> Result<PipelineSummary> {
    let needs_symbols = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "symbols");
    let needs_graph = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "graph");
    let needs_bind = case.expectations.iter().any(|expect| expect.kind == "bind");

    if !needs_symbols && !needs_graph && !needs_bind {
        return Ok(PipelineSummary::default());
    }

    let project = materialize_case(case)?;
    let summary = match case.lang.as_str() {
        "rust" => {
            collect_pipeline::<LangRust>(&project.paths, needs_symbols, needs_graph, needs_bind)?
        }
        other => {
            return Err(anyhow!(
                "unsupported lang '{}' requested by {}",
                other,
                case.id()
            ))
        }
    };
    drop(project);

    Ok(summary)
}

#[derive(Default)]
struct PipelineSummary {
    symbols: Option<Vec<SymbolSnapshot>>,
    graph_dot: Option<String>,
    bindings: Option<String>,
}

fn render_expectation(kind: &str, summary: &PipelineSummary, case_id: &str) -> Result<String> {
    match kind {
        "symbols" => {
            let symbols = summary
                .symbols
                .as_ref()
                .ok_or_else(|| anyhow!("case {} requested symbols but summary missing", case_id))?;
            Ok(render_symbol_snapshot(symbols))
        }
        "graph" => summary.graph_dot.clone().ok_or_else(|| {
            anyhow!(
                "case {} requested graph output but summary missing",
                case_id
            )
        }),
        "bind" => summary.bindings.clone().ok_or_else(|| {
            anyhow!(
                "case {} requested binding output but summary missing",
                case_id
            )
        }),
        other => Err(anyhow!(
            "case {} uses unsupported expectation '{}'",
            case_id,
            other
        )),
    }
}

fn render_symbol_snapshot(entries: &[SymbolSnapshot]) -> String {
    if entries.is_empty() {
        return "<no-symbols>\n".to_string();
    }

    let mut rows = entries.to_vec();
    rows.sort_by(|a, b| {
        a.unit
            .cmp(&b.unit)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.fqn.cmp(&b.fqn))
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
        let fqn_display = if row.is_global {
            format!("{} [global]", row.fqn)
        } else {
            row.fqn.clone()
        };
        let label = format!("u{}:{}", row.unit, row.id);
        let _ = writeln!(
            buf,
            "{:<label_width$} | {:kind_width$} | {:name_width$} | {}",
            label,
            row.kind,
            row.name,
            fqn_display,
            label_width = label_width,
            kind_width = kind_width,
            name_width = name_width,
        );
    }
    buf
}

fn format_expectation_diff(kind: &str, expected: &str, actual: &str) -> String {
    let diff = TextDiff::from_lines(expected, actual);
    let mut buf = String::new();
    let _ = writeln!(buf, "Expectation '{kind}' mismatch:");
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        let _ = write!(buf, "{sign}{}", change);
    }
    buf
}

fn normalize(kind: &str, text: &str) -> String {
    let canonical = text
        .replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_string();

    match kind {
        "symbols" => normalize_symbols(&canonical),
        _ => canonical,
    }
}

fn normalize_symbols(text: &str) -> String {
    let mut rows: Vec<(usize, u32, String)> = text
        .lines()
        .filter_map(|line| {
            if line.trim().is_empty() {
                return None;
            }

            let parts: Vec<_> = line.split('|').map(|part| part.trim()).collect();
            if parts.is_empty() {
                return None;
            }

            let head = parts[0];
            let (unit, id) = parse_unit_and_id(head);
            let canonical = parts.join(" | ");
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

fn parse_unit_and_id(token: &str) -> (usize, u32) {
    if let Some(stripped) = token.strip_prefix('u') {
        if let Some((unit_str, id_str)) = stripped.split_once(':') {
            if let (Ok(unit), Ok(id)) = (unit_str.parse::<usize>(), id_str.parse::<u32>()) {
                return (unit, id);
            }
        }
    }

    (usize::MAX, u32::MAX)
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

struct MaterializedProject {
    #[allow(dead_code)]
    temp_dir: TempDir,
    paths: Vec<String>,
}

fn materialize_case(case: &CorpusCase) -> Result<MaterializedProject> {
    let temp_dir = tempfile::tempdir().context("failed to create temp dir for llmcc-test")?;
    let mut paths = Vec::with_capacity(case.files.len());

    for file in &case.files {
        let abs_path = temp_dir.path().join(Path::new(&file.path));
        if let Some(parent) = abs_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::write(&abs_path, file.contents.as_bytes()).with_context(|| {
            format!(
                "failed to write virtual file {} for {}",
                abs_path.display(),
                case.id()
            )
        })?;
        paths.push(abs_path.to_string_lossy().to_string());
    }

    Ok(MaterializedProject { temp_dir, paths })
}

fn collect_pipeline<L>(
    files: &[String],
    keep_symbols: bool,
    build_graph: bool,
    build_bindings: bool,
) -> Result<PipelineSummary>
where
    L: LanguageTrait<SymbolCollection = CollectedSymbols>,
{
    let cc = CompileCtxt::from_files::<L>(files)
        .with_context(|| format!("failed to build compile context for {:?}", files))?;
    build_llmcc_ir::<L>(&cc, IrBuildConfig).map_err(|err| anyhow!(err))?;
    let globals = cc.create_globals();
    let unit_count = cc.get_files().len();
    let mut collections = Vec::with_capacity(unit_count);
    for index in 0..unit_count {
        let unit = cc.compile_unit(index);
        let collected = L::collect_symbols(unit);
        apply_collected_symbols(unit, globals, &collected);
        collections.push(collected);
    }
    let mut project_graph = if build_graph || build_bindings {
        Some(ProjectGraph::new(&cc))
    } else {
        None
    };
    for (index, collection) in collections.iter().enumerate() {
        let unit = cc.compile_unit(index);
        L::bind_symbols(unit, globals, collection);
        if let Some(project) = project_graph.as_mut() {
            let unit_graph = build_llmcc_graph::<L>(unit, index, GraphBuildConfig)
                .map_err(|err| anyhow!(err))?;
            project.add_child(unit_graph);
        }
    }
    let (graph_dot, bindings) = if let Some(mut project) = project_graph {
        project.link_units();
        let graph = if build_graph {
            Some(project.render_design_graph())
        } else {
            None
        };
        let binding = if build_bindings {
            Some(render_binding_summary(&project))
        } else {
            None
        };
        (graph, binding)
    } else {
        (None, None)
    };

    let symbols = if keep_symbols {
        Some(snapshot_symbols(&cc))
    } else {
        None
    };

    Ok(PipelineSummary {
        symbols,
        graph_dot,
        bindings,
    })
}

fn snapshot_symbols(cc: &CompileCtxt<'_>) -> Vec<SymbolSnapshot> {
    let symbol_map = cc.symbol_map.read();
    let mut rows = Vec::with_capacity(symbol_map.len());
    for (sym_id, symbol) in symbol_map.iter() {
        let mut fqn = symbol.fqn_name.read().clone();
        if fqn.is_empty() {
            fqn = symbol.name.clone();
        }

        rows.push(SymbolSnapshot {
            unit: symbol.unit_index().unwrap_or_default(),
            id: sym_id.0,
            kind: format!("{:?}", symbol.kind()),
            name: symbol.name.clone(),
            fqn,
            is_global: symbol.is_global(),
        });
    }

    rows
}

fn render_binding_summary(project: &ProjectGraph) -> String {
    use std::collections::BTreeMap;

    let indexes = project.cc.block_indexes.read();
    let mut units: BTreeMap<usize, Vec<(BlockDescriptor, Vec<BlockDescriptor>)>> = BTreeMap::new();

    for unit_graph in project.units() {
        let unit_index = unit_graph.unit_index();
        let mut entries = Vec::new();
        for block_id in unit_graph.edges().get_connected_blocks() {
            let Some(src_desc) = describe_block(block_id, &indexes) else {
                continue;
            };
            let mut deps = unit_graph
                .edges()
                .get_related(block_id, BlockRelation::DependsOn);
            deps.sort_unstable_by_key(|id| id.as_u32());
            deps.dedup();
            let mut dep_descs: Vec<BlockDescriptor> = deps
                .into_iter()
                .filter_map(|id| describe_block(id, &indexes))
                .collect();
            dep_descs.sort_by(|a, b| (a.unit, &a.name).cmp(&(b.unit, &b.name)));
            entries.push((src_desc, dep_descs));
        }
        entries.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        if !entries.is_empty() {
            units.insert(unit_index, entries);
        }
    }

    if units.is_empty() {
        return "(bindings)\n".to_string();
    }

    let mut out = String::new();
    out.push_str("(bindings\n");
    for (unit, blocks) in units {
        let _ = writeln!(out, "  (unit {unit}");
        for (block, deps) in blocks {
            let _ = writeln!(
                out,
                "    (block {} {} {}",
                quote(&block.name),
                block.kind,
                block.unit
            );
            if deps.is_empty() {
                out.push_str("      (depends_on))\n");
            } else {
                out.push_str("      (depends_on\n");
                for dep in deps {
                    let _ = writeln!(
                        out,
                        "        ({} {} {})",
                        quote(&dep.name),
                        dep.kind,
                        dep.unit
                    );
                }
                out.push_str("      ))\n");
            }
        }
        out.push_str("  )\n");
    }
    out.push_str(")\n");
    out
}

#[derive(Clone)]
struct BlockDescriptor {
    name: String,
    kind: String,
    unit: usize,
}

fn describe_block(
    block_id: llmcc_core::graph_builder::BlockId,
    indexes: &llmcc_core::context::BlockIndexMaps,
) -> Option<BlockDescriptor> {
    let (unit, name, kind) = indexes.get_block_info(block_id)?;
    let name = name.unwrap_or_else(|| format!("block#{block_id}"));
    Some(BlockDescriptor {
        name,
        kind: kind.to_string(),
        unit,
    })
}

fn quote(text: &str) -> String {
    let escaped = text.replace('"', "\\\"");
    format!("\"{escaped}\"")
}
