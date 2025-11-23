use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use llmcc_core::ProjectGraph;
use llmcc_core::block::reset_block_id_counter;
use llmcc_core::context::{CompileCtxt, CompileUnit};
use llmcc_core::graph_builder::{BlockId, BlockRelation, GraphBuildOption, build_llmcc_graph};
use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
use llmcc_core::lang_def::LanguageTraitImpl;
use llmcc_core::symbol::reset_symbol_id_counter;

use llmcc_resolver::{ResolverOption, bind_symbols_with, collect_symbols_with};
use llmcc_rust::LangRust;
use similar::TextDiff;
use tempfile::TempDir;
use walkdir::WalkDir;

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

#[derive(Clone)]
struct SymbolDependencySnapshot {
    label: String,
    depends_on: Vec<String>,
    depended_by: Vec<String>,
}

#[derive(Clone)]
struct BlockSnapshot {
    label: String,
    kind: String,
    name: String,
}

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub filter: Option<String>,
    pub update: bool,
    pub keep_temps: bool,
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
            config.keep_temps,
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

pub fn run_cases_for_file(
    file: &mut CorpusFile,
    update: bool,
    keep_temps: bool,
) -> Result<Vec<CaseOutcome>> {
    let mut matched = 0usize;
    run_cases_in_file(file, update, None, &mut matched, keep_temps)
}

fn run_cases_in_file(
    file: &mut CorpusFile,
    update: bool,
    filter: Option<&str>,
    matched: &mut usize,
    keep_temps: bool,
) -> Result<Vec<CaseOutcome>> {
    let mut file_outcomes = Vec::new();
    let mut mutated_file = false;
    let mut printed_case_header = false;
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
        let case_name = file.cases[idx].id();
        if printed_case_header {
            for _ in 0..3 {
                println!();
            }
        }
        println!(">>> running {case_name}");
        printed_case_header = true;
        let (outcome, mutated) = {
            let case = &mut file.cases[idx];
            evaluate_case(case, update, keep_temps)?
        };
        if mutated {
            file.mark_dirty();
            mutated_file = true;
        }
        file_outcomes.push(outcome);
    }
    if update && !mutated_file {
        file.mark_dirty();
    }
    Ok(file_outcomes)
}

fn evaluate_case(
    case: &mut CorpusCase,
    update: bool,
    keep_temps: bool,
) -> Result<(CaseOutcome, bool)> {
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

    reset_symbol_id_counter();
    reset_block_id_counter();
    let summary = build_pipeline_summary(case, keep_temps)?;
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

fn build_pipeline_summary(case: &CorpusCase, keep_temps: bool) -> Result<PipelineSummary> {
    let needs_symbols = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "symbols");
    let needs_graph = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "graph");
    let needs_block_reports = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "blocks" || expect.kind == "block-deps");
    let needs_block_graph = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "block-graph");
    let needs_symbol_deps = case
        .expectations
        .iter()
        .any(|expect| expect.kind == "symbol-deps");

    if !needs_symbols
        && !needs_graph
        && !needs_block_reports
        && !needs_block_graph
        && !needs_symbol_deps
    {
        return Ok(PipelineSummary::default());
    }

    let project = materialize_case(case, keep_temps)?;
    if keep_temps && project.is_persistent() {
        println!(
            "preserved materialized project for {} at {}",
            case.id(),
            project.root().display()
        );
    }
    let summary = match case.lang.as_str() {
        "rust" => collect_pipeline::<LangRust>(
            project.root(),
            needs_symbols,
            needs_graph,
            needs_block_reports,
            needs_block_graph,
            needs_symbol_deps,
        )?,
        other => {
            return Err(anyhow!(
                "unsupported lang '{}' requested by {}",
                other,
                case.id()
            ));
        }
    };
    drop(project);

    Ok(summary)
}

#[derive(Default)]
struct PipelineSummary {
    symbols: Option<Vec<SymbolSnapshot>>,
    graph_dot: Option<String>,
    block_list: Option<Vec<BlockSnapshot>>,
    block_deps: Option<Vec<SymbolDependencySnapshot>>,
    symbol_deps: Option<Vec<SymbolDependencySnapshot>>,
    block_graph: Option<String>,
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
        "blocks" => summary
            .block_list
            .as_ref()
            .map(|list| render_block_snapshot(list))
            .ok_or_else(|| {
                anyhow!(
                    "case {} requested blocks output but summary missing",
                    case_id
                )
            }),
        "block-deps" => summary
            .block_deps
            .as_ref()
            .map(|deps| render_symbol_dependencies(deps))
            .ok_or_else(|| {
                anyhow!(
                    "case {} requested block-deps output but summary missing",
                    case_id
                )
            }),
        "block-graph" => summary.block_graph.clone().ok_or_else(|| {
            anyhow!(
                "case {} requested block-graph output but summary missing",
                case_id
            )
        }),
        "symbol-deps" => {
            let deps = summary.symbol_deps.as_ref().ok_or_else(|| {
                anyhow!("case {} requested symbol-deps but summary missing", case_id)
            })?;
            Ok(render_symbol_dependencies(deps))
        }
        other => Err(anyhow!(
            "case {} uses unsupported expectation '{}'",
            case_id,
            other
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
            .then_with(|| a.fqn.cmp(&b.fqn))
    });

    let label_width = rows
        .iter()
        .map(|row| format!("u{}:{}", row.unit, row.id).len())
        .max()
        .unwrap_or(0);
    let kind_width = rows.iter().map(|row| row.kind.len()).max().unwrap_or(0);
    let name_width = rows.iter().map(|row| row.name.len()).max().unwrap_or(0);
    let fqn_width = rows.iter().map(|row| row.fqn.len()).max().unwrap_or(0);
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
            "{:<label_width$} | {:kind_width$} | {:name_width$} | {:fqn_width$} | {:global_width$}",
            label,
            row.kind,
            row.name,
            row.fqn,
            if row.is_global { "[global]" } else { "" },
            label_width = label_width,
            kind_width = kind_width,
            name_width = name_width,
            fqn_width = fqn_width,
            global_width = global_width,
        );
    }
    buf
}

use std::cmp::Ordering;

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
        "symbols" | "blocks" => normalize_symbols(&canonical),
        "symbol-deps" | "block-deps" => normalize_symbol_deps(&canonical),
        "graph" => normalize_graph(&canonical),
        "block-graph" => normalize_block_graph(&canonical),
        _ => canonical,
    }
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
            let mut fqn = parts.get(3).copied().unwrap_or("");
            let mut global = parts.get(4).copied().unwrap_or("");

            if parts.len() == 4 && fqn.ends_with("[global]") {
                if let Some(stripped) = fqn.strip_suffix(" [global]") {
                    fqn = stripped.trim_end();
                }
                global = "[global]";
            }

            let canonical = format!("{label} | {kind} | {name} | {fqn} | {global}");
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

fn normalize_symbol_deps(text: &str) -> String {
    let mut rows: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || is_empty_relation(trimmed) {
                return None;
            }
            Some(trimmed.to_string())
        })
        .collect();
    rows.sort();
    rows.join("\n")
}

fn normalize_block_graph(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    match parse_sexpr(trimmed) {
        Ok(exprs) => exprs
            .into_iter()
            .map(|expr| format_sexpr(&expr))
            .collect::<Vec<_>>()
            .join("\n"),
        Err(_) => trimmed.to_string(),
    }
}

fn is_empty_relation(line: &str) -> bool {
    if let Some((_, rhs)) = line.split_once("->")
        && rhs.trim() == "[]"
    {
        return true;
    }
    if let Some((_, rhs)) = line.split_once("<-")
        && rhs.trim() == "[]"
    {
        return true;
    }
    false
}

fn normalize_graph(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with("digraph") {
        normalize_dot_paths(trimmed)
    } else {
        trimmed.to_string()
    }
}

fn normalize_graph_path(path: &str) -> String {
    use std::path::Path;

    let (path_part, line_part) = match path.rsplit_once(':') {
        Some((p, line)) if line.chars().all(|ch| ch.is_ascii_digit()) => (p, format!(":{line}")),
        _ => (path, String::new()),
    };

    let path_obj = Path::new(path_part);
    let components: Vec<_> = path_obj
        .components()
        .filter_map(|comp| comp.as_os_str().to_str())
        .collect();

    let start = components
        .iter()
        .rposition(|comp| *comp == "src")
        .map(|idx| idx.saturating_sub(1))
        .unwrap_or(components.len().saturating_sub(3));

    let mut shortened = components[start..].join("/");
    if shortened.is_empty() {
        shortened = path_obj
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path_part)
            .to_string();
    }

    format!("{shortened}{line_part}")
}

fn normalize_dot_paths(dot: &str) -> String {
    dot.lines()
        .map(|line| {
            if let Some(start) = line.find("full_path=") {
                let prefix = &line[..start];
                let rest = &line[start..];
                if let Some((before_path, after_path)) = rest.split_once('"')
                    && let Some((path, suffix)) = after_path.split_once('"')
                {
                    let normalized = normalize_graph_path(path);
                    return format!("{}{}\"{}\"{}", prefix, before_path, normalized, suffix);
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_unit_and_id(token: &str) -> (usize, u32) {
    if let Some(stripped) = token.strip_prefix('u')
        && let Some((unit_str, id_str)) = stripped.split_once(':')
        && let (Ok(unit), Ok(id)) = (unit_str.parse::<usize>(), id_str.parse::<u32>())
    {
        return (unit, id);
    }

    (usize::MAX, u32::MAX)
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

#[derive(Debug, Clone)]
enum SExpr {
    Atom(String),
    List(Vec<SExpr>),
}

fn parse_sexpr(input: &str) -> Result<Vec<SExpr>, ()> {
    let tokens = tokenize(input);
    let mut idx = 0;
    let mut exprs = Vec::new();
    while idx < tokens.len() {
        exprs.push(parse_expr(&tokens, &mut idx)?);
    }
    Ok(exprs)
}

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '(' | ')' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
                tokens.push(ch.to_string());
            }
            '"' => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
                let mut literal = String::new();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '"' {
                        break;
                    }
                    literal.push(next);
                }
                tokens.push(literal);
            }
            _ if ch.is_whitespace() => {
                if !current.trim().is_empty() {
                    tokens.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        tokens.push(current.trim().to_string());
    }
    tokens
}

fn parse_expr(tokens: &[String], idx: &mut usize) -> Result<SExpr, ()> {
    if *idx >= tokens.len() {
        return Err(());
    }
    let token = tokens[*idx].clone();
    *idx += 1;
    match token.as_str() {
        "(" => {
            let mut items = Vec::new();
            while *idx < tokens.len() && tokens[*idx] != ")" {
                items.push(parse_expr(tokens, idx)?);
            }
            if *idx >= tokens.len() || tokens[*idx] != ")" {
                return Err(());
            }
            *idx += 1;
            Ok(SExpr::List(items))
        }
        ")" => Err(()),
        literal => Ok(SExpr::Atom(literal.to_string())),
    }
}

fn format_sexpr(expr: &SExpr) -> String {
    match expr {
        SExpr::Atom(atom) => atom.clone(),
        SExpr::List(items) => {
            let parts: Vec<String> = items.iter().map(format_sexpr).collect();
            format!("({})", parts.join(" "))
        }
    }
}

struct MaterializedProject {
    #[allow(dead_code)]
    temp_dir: Option<TempDir>,
    root_path: PathBuf,
}

impl MaterializedProject {
    fn root(&self) -> &Path {
        &self.root_path
    }

    fn is_persistent(&self) -> bool {
        self.temp_dir.is_none()
    }
}

fn materialize_case(case: &CorpusCase, keep_temps: bool) -> Result<MaterializedProject> {
    let temp_dir = tempfile::tempdir().context("failed to create temp dir for llmcc-test")?;
    let root_path = temp_dir.path().to_path_buf();

    for file in &case.files {
        let abs_path = root_path.join(Path::new(&file.path));
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
    }

    if keep_temps {
        let preserved = temp_dir.path().to_path_buf();
        return Ok(MaterializedProject {
            temp_dir: None,
            root_path: preserved,
        });
    }

    Ok(MaterializedProject {
        temp_dir: Some(temp_dir),
        root_path,
    })
}

fn collect_pipeline<L>(
    project_root: &Path,
    keep_symbols: bool,
    build_graph: bool,
    build_block_reports: bool,
    build_block_graph: bool,
    keep_symbol_deps: bool,
) -> Result<PipelineSummary>
where
    L: LanguageTraitImpl,
{
    let files = discover_language_files::<L>(project_root)?;
    let cc = CompileCtxt::from_files::<L>(&files).unwrap();
    build_llmcc_ir::<L>(&cc, IrBuildOption).unwrap();

    // Use new unified API for symbol collection with optional IR printing
    let resolver_option = ResolverOption::default()
        .with_print_ir(true)
        .with_sequential(true);
    let globals = collect_symbols_with::<L>(&cc, &resolver_option);

    // Bind symbols using new unified API
    bind_symbols_with::<L>(&cc, globals, &resolver_option);
    let mut project_graph = if build_graph || build_block_reports || build_block_graph {
        Some(ProjectGraph::new(&cc))
    } else {
        None
    };
    if let Some(project) = project_graph.as_mut() {
        let unit_graphs =
            build_llmcc_graph::<L>(&cc, GraphBuildOption::new().with_sequential(true)).unwrap();
        project.add_children(unit_graphs);
    }
    let (graph_dot, block_list, block_deps, block_graph) = if let Some(mut project) = project_graph
    {
        project.link_units();
        let graph = if build_graph {
            Some(project.render_design_graph())
        } else {
            None
        };
        let (list, deps) = if build_block_reports {
            let (blocks, deps) = render_block_reports(&project);
            (Some(blocks), Some(deps))
        } else {
            (None, None)
        };
        let block_graph = if build_block_graph {
            Some(render_block_graph(&project))
        } else {
            None
        };
        (graph, list, deps, block_graph)
    } else {
        (None, None, None, None)
    };

    let symbols = if keep_symbols {
        Some(snapshot_symbols(&cc))
    } else {
        None
    };

    let symbol_deps = if keep_symbol_deps {
        Some(snapshot_symbol_dependencies(&cc))
    } else {
        None
    };

    Ok(PipelineSummary {
        symbols,
        graph_dot,
        block_list,
        block_deps,
        symbol_deps,
        block_graph,
    })
}

fn discover_language_files<L: LanguageTraitImpl>(root: &Path) -> Result<Vec<String>> {
    let supported = L::supported_extensions();
    let mut files = Vec::new();

    for entry in WalkDir::new(root) {
        let entry = entry.with_context(|| format!("failed to walk {}", root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }

        let Some(ext) = entry.path().extension().and_then(|ext| ext.to_str()) else {
            continue;
        };

        if !supported
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        {
            continue;
        }

        files.push(entry.path().to_string_lossy().to_string());
    }

    files.sort();
    Ok(files)
}

fn render_block_graph(project: &ProjectGraph) -> String {
    let mut units: Vec<_> = project.units().iter().collect();
    if units.is_empty() {
        return "none\n".to_string();
    }

    units.sort_by_key(|unit| unit.unit_index());

    let mut sections = Vec::new();
    for unit_graph in units {
        let unit = project.cc.compile_unit(unit_graph.unit_index());
        let mut buf = String::new();
        render_block_graph_node(unit_graph.root(), unit, 0, &mut buf);
        sections.push(buf.trim_end().to_string());
    }

    if sections.is_empty() {
        "none\n".to_string()
    } else {
        let mut joined = sections.join("\n\n");
        joined.push('\n');
        joined
    }
}

fn render_block_graph_node(
    block_id: BlockId,
    unit: CompileUnit<'_>,
    depth: usize,
    buf: &mut String,
) {
    let block = unit.bb(block_id);
    let indent = "    ".repeat(depth);
    let kind = block.kind().to_string();
    let _ = write!(buf, "{}({}:{}", indent, kind, block_id.as_u32());

    if let Some(name) = block
        .base()
        .and_then(|base| base.opt_get_name())
        .filter(|name| !name.is_empty())
    {
        let _ = write!(buf, " {}", name);
    }

    let children = block.children();
    if children.is_empty() {
        buf.push_str(")\n");
        return;
    }

    buf.push('\n');
    for &child_id in children {
        render_block_graph_node(child_id, unit, depth + 1, buf);
    }
    buf.push_str(&indent);
    buf.push_str(")\n");
}

fn snapshot_symbols(cc: &CompileCtxt<'_>) -> Vec<SymbolSnapshot> {
    let symbol_map = cc.symbol_map.read();
    let interner = &cc.interner;
    let mut rows = Vec::with_capacity(symbol_map.len());
    for (_sym_id, symbol) in symbol_map.iter() {
        let fqn_str = interner
            .resolve_owned(*symbol.fqn.read())
            .unwrap_or_else(|| "?".to_string());
        let name_str = interner
            .resolve_owned(symbol.name)
            .unwrap_or_else(|| "?".to_string());

        rows.push(SymbolSnapshot {
            unit: symbol.unit_index().unwrap_or_default(),
            id: symbol.id().0 as u32,
            kind: format!("{:?}", symbol.kind()),
            name: name_str,
            fqn: fqn_str,
            is_global: symbol.is_global(),
        });
    }

    rows
}
fn snapshot_symbol_dependencies(cc: &CompileCtxt<'_>) -> Vec<SymbolDependencySnapshot> {
    use std::collections::HashMap;

    let symbol_map = cc.symbol_map.read();
    let mut cache: HashMap<u32, SymbolDependencySnapshot> = HashMap::new();

    // Build initial cache of all symbols
    for (_sym_id, symbol) in symbol_map.iter() {
        let sym_id_num = symbol.id().0 as u32;
        let label = format!(
            "u{}:{}",
            symbol.unit_index().unwrap_or_default(),
            sym_id_num
        );
        cache.insert(
            sym_id_num,
            SymbolDependencySnapshot {
                label,
                depends_on: Vec::new(),
                depended_by: Vec::new(),
            },
        );
    }

    // Fill in dependencies
    for (_sym_id, symbol) in symbol_map.iter() {
        let sym_id_num = symbol.id().0 as u32;
        let deps = symbol.depends.read().clone();
        for dep in deps {
            if let Some(_target) = symbol_map.get(&dep) {
                let dep_id_num = dep.0 as u32;
                let dep_label = format!(
                    "u{}:{}",
                    _target.unit_index().unwrap_or_default(),
                    dep_id_num
                );
                if let Some(entry) = cache.get_mut(&sym_id_num) {
                    entry.depends_on.push(dep_label.clone());
                }
                if let Some(target_entry) = cache.get_mut(&dep_id_num) {
                    target_entry.depended_by.push(format!(
                        "u{}:{}",
                        symbol.unit_index().unwrap_or_default(),
                        sym_id_num
                    ));
                }
            }
        }
    }

    let mut output: Vec<_> = cache.into_values().collect();
    for entry in &mut output {
        entry.depends_on.sort();
        entry.depended_by.sort();
    }
    output
}
fn render_block_reports(
    project: &ProjectGraph,
) -> (Vec<BlockSnapshot>, Vec<SymbolDependencySnapshot>) {
    use std::collections::BTreeMap;
    use std::collections::HashMap;

    let indexes = project.cc.block_indexes.read();
    let mut units: BTreeMap<usize, Vec<(BlockDescriptor, Vec<BlockDescriptor>)>> = BTreeMap::new();

    for unit_graph in project.units() {
        let unit_index = unit_graph.unit_index();
        let mut entries = Vec::new();

        for (_name_opt, kind, block_id) in indexes.find_by_unit(unit_index) {
            let Some(mut desc) = describe_block(block_id, &indexes) else {
                continue;
            };
            desc.kind = kind.to_string();

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
            entries.push((desc, dep_descs));
        }

        entries.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        if !entries.is_empty() {
            units.insert(unit_index, entries);
        }
    }

    let mut block_rows = Vec::new();
    let mut dep_map: HashMap<String, SymbolDependencySnapshot> = HashMap::new();

    for (_unit, blocks) in units {
        for (block, deps) in blocks {
            let label = format!("u{}:{}", block.unit, block.id.as_u32());
            block_rows.push(BlockSnapshot {
                label: label.clone(),
                kind: block.kind.clone(),
                name: block.name.clone(),
            });

            {
                let entry = dep_map
                    .entry(label.clone())
                    .or_insert(SymbolDependencySnapshot {
                        label: label.clone(),
                        depends_on: Vec::new(),
                        depended_by: Vec::new(),
                    });

                for dep in &deps {
                    let dep_label = format!("u{}:{}", dep.unit, dep.id.as_u32());
                    entry.depends_on.push(dep_label.clone());
                }
            }

            for dep in deps {
                let dep_label = format!("u{}:{}", dep.unit, dep.id.as_u32());
                dep_map
                    .entry(dep_label.clone())
                    .or_insert(SymbolDependencySnapshot {
                        label: dep_label.clone(),
                        depends_on: Vec::new(),
                        depended_by: Vec::new(),
                    })
                    .depended_by
                    .push(label.clone());
            }
        }
    }

    for snapshot in dep_map.values_mut() {
        snapshot.depends_on.sort();
        snapshot.depended_by.sort();
    }

    block_rows.sort_by(|a, b| a.label.cmp(&b.label));
    let mut deps: Vec<_> = dep_map.into_values().collect();
    deps.sort_by(|a, b| a.label.cmp(&b.label));

    (block_rows, deps)
}

#[derive(Clone)]
struct BlockDescriptor {
    name: String,
    kind: String,
    unit: usize,
    id: llmcc_core::graph_builder::BlockId,
}

fn describe_block(
    block_id: llmcc_core::graph_builder::BlockId,
    indexes: &llmcc_core::block_rel::BlockIndexMaps,
) -> Option<BlockDescriptor> {
    let (unit, name, kind) = indexes.get_block_info(block_id)?;
    let name = name.unwrap_or_else(|| format!("block#{block_id}"));
    Some(BlockDescriptor {
        name,
        kind: kind.to_string(),
        unit,
        id: block_id,
    })
}
