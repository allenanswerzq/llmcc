use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use llmcc_core::context::CompileCtxt;
use llmcc_core::graph_builder::{build_llmcc_graph, GraphBuildConfig, ProjectGraph};
use llmcc_core::ir_builder::{build_llmcc_ir, IrBuildConfig};
use llmcc_core::lang_def::LanguageTrait;
use llmcc_resolver::apply_collected_symbols;
use llmcc_resolver::collector::CollectedSymbols;
use llmcc_rust::LangRust;
use similar::TextDiff;
use tempfile::TempDir;

use crate::corpus::{Corpus, CorpusCase, CorpusFile};

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
        let actual = render_expectation(expect.kind.as_str(), &summary, &case_id)?;
        let expected_norm = normalize(&expect.value);
        let actual_norm = normalize(&actual);

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

    if !needs_symbols && !needs_graph {
        return Ok(PipelineSummary::default());
    }

    let project = materialize_case(case)?;
    let summary = match case.lang.as_str() {
        "rust" => collect_pipeline::<LangRust>(&project.paths, needs_symbols, needs_graph)?,
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
    symbols: Option<Vec<CollectedSymbols>>,
    graph_dot: Option<String>,
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
        other => Err(anyhow!(
            "case {} uses unsupported expectation '{}'",
            case_id,
            other
        )),
    }
}

fn render_symbol_snapshot(collections: &[CollectedSymbols]) -> String {
    #[derive(Clone)]
    struct Row {
        unit: usize,
        kind: String,
        fqn: String,
        is_global: bool,
    }

    let mut rows: Vec<Row> = collections
        .iter()
        .enumerate()
        .flat_map(|(unit, collection)| {
            collection.symbols.iter().map(move |symbol| Row {
                unit,
                kind: format!("{:?}", symbol.kind),
                fqn: if symbol.fqn.is_empty() {
                    symbol.name.clone()
                } else {
                    symbol.fqn.clone()
                },
                is_global: symbol.is_global,
            })
        })
        .collect();

    rows.sort_by(|a, b| {
        a.fqn
            .cmp(&b.fqn)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| a.unit.cmp(&b.unit))
    });

    if rows.is_empty() {
        return "<no-symbols>\n".to_string();
    }

    let mut buf = String::new();
    for row in rows {
        let _ = writeln!(
            buf,
            "{:>2} | {:<12} | {}{}",
            row.unit,
            row.kind,
            row.fqn,
            if row.is_global { " [global]" } else { "" }
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

fn normalize(text: &str) -> String {
    text.replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_string()
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
    let mut project_graph = if build_graph {
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
    let graph_dot = if let Some(mut project) = project_graph {
        project.link_units();
        Some(project.render_design_graph())
    } else {
        None
    };

    Ok(PipelineSummary {
        symbols: if keep_symbols {
            Some(collections)
        } else {
            None
        },
        graph_dot,
    })
}
