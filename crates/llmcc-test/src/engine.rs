use std::fs;
use std::path::{Component, Path, PathBuf};

use llmcc_core::ir_builder::{HirBuildOptions, build_hir};
use llmcc_core::lang_def::Language;
use llmcc_core::reset_symbol_id_counter;
use llmcc_core::{CompileCtxt, GraphBuildOptions, ProjectGraph, ResolveOptions, build_graphs};
use llmcc_core::{reset_block_id_counter, reset_hir_id_counter, reset_scope_id_counter};
use llmcc_cpp::LangCpp;
use llmcc_error::{Error, ErrorKind, Result};
use llmcc_format::{GraphDocument, format_graph};
use llmcc_resolver::{bind_symbols, collect_symbols};
use llmcc_rust::LangRust;
use llmcc_ts::LangTypeScript;
use strum_macros::{Display, EnumString, IntoStaticStr};
use tempfile::TempDir;

use crate::case::{CaseLanguage, JsonCase, SourceFile, load_suite_files};

#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    pub filter: Option<String>,
    pub update: bool,
    pub keep_temps: bool,
    pub parallel: bool,
    pub print_ir: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, IntoStaticStr)]
#[strum(serialize_all = "snake_case", ascii_case_insensitive)]
pub enum CaseStatus {
    Passed,
    Failed,
    Updated,
}

#[derive(Debug, Clone)]
pub struct CaseOutcome {
    pub id: String,
    pub status: CaseStatus,
    pub message: Option<String>,
}

#[derive(Debug, Default)]
pub struct RunReport {
    pub outcomes: Vec<CaseOutcome>,
}

impl RunReport {
    pub fn passed(&self) -> usize {
        self.count(CaseStatus::Passed)
    }

    pub fn failed(&self) -> usize {
        self.count(CaseStatus::Failed)
    }

    pub fn updated(&self) -> usize {
        self.count(CaseStatus::Updated)
    }

    fn count(&self, status: CaseStatus) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.status == status)
            .count()
    }
}

pub fn run_path(path: &Path, options: RunOptions) -> Result<RunReport> {
    let mut suites = load_suite_files(path)?;
    let mut report = RunReport::default();
    let mut matched = 0usize;

    for suite_file in &mut suites {
        for case in &mut suite_file.suite.cases {
            if let Some(filter) = &options.filter
                && !case.id.contains(filter)
            {
                continue;
            }

            matched += 1;
            let outcome = run_case(case, &options)?;
            if outcome.status == CaseStatus::Updated {
                suite_file.dirty = true;
            }
            report.outcomes.push(outcome);
        }
    }

    if matched == 0 {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "no llmcc JSON cases matched the requested path/filter",
        ));
    }

    if options.update {
        for suite_file in &suites {
            suite_file.write_if_dirty()?;
        }
    }

    Ok(report)
}

fn run_case(case: &mut JsonCase, options: &RunOptions) -> Result<CaseOutcome> {
    reset_ids();
    let actual = evaluate_case(case, options)?;

    if case.expect.as_ref() == Some(&actual) {
        return Ok(CaseOutcome {
            id: case.id.clone(),
            status: CaseStatus::Passed,
            message: None,
        });
    }

    if options.update {
        case.expect = Some(actual);
        return Ok(CaseOutcome {
            id: case.id.clone(),
            status: CaseStatus::Updated,
            message: None,
        });
    }

    let Some(expected) = &case.expect else {
        return Ok(CaseOutcome {
            id: case.id.clone(),
            status: CaseStatus::Failed,
            message: Some(
                "missing expected graph document; rerun with --update to bless it".to_string(),
            ),
        });
    };

    Ok(CaseOutcome {
        id: case.id.clone(),
        status: CaseStatus::Failed,
        message: Some(format_mismatch(expected, &actual)?),
    })
}

fn evaluate_case(case: &JsonCase, options: &RunOptions) -> Result<GraphDocument> {
    let project = materialize_case(case, options.keep_temps)?;

    match case.language {
        CaseLanguage::Rust => build_document::<LangRust>(case, project.root(), options),
        CaseLanguage::Cpp => build_document::<LangCpp>(case, project.root(), options),
        CaseLanguage::TypeScript => build_document::<LangTypeScript>(case, project.root(), options),
    }
}

fn build_document<L>(case: &JsonCase, root: &Path, options: &RunOptions) -> Result<GraphDocument>
where
    L: Language,
{
    let files = source_paths::<L>(root, &case.files)?;
    let context = CompileCtxt::from_files::<L>(&files)?;
    let sequential = !options.parallel;

    build_hir::<L>(&context, HirBuildOptions::new().with_sequential(sequential))?;
    let resolve_options = ResolveOptions::default()
        .with_print_ir(options.print_ir)
        .with_sequential(sequential);
    let globals = collect_symbols::<L>(&context, &resolve_options)?;
    bind_symbols::<L>(&context, globals, &resolve_options)?;

    let mut project = ProjectGraph::new(&context);
    let unit_graphs = build_graphs::<L>(
        &context,
        GraphBuildOptions::new().with_sequential(sequential),
    )?;
    project.add_units(unit_graphs);
    project.link_blocks();

    Ok(format_graph(&project, case.depth))
}

fn source_paths<L>(root: &Path, files: &[SourceFile]) -> Result<Vec<String>>
where
    L: Language,
{
    let supported = L::extensions();
    let mut paths = Vec::new();

    for file in files {
        let relative = safe_relative_path(&file.path)?;
        let is_supported = relative
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                supported
                    .iter()
                    .any(|supported| supported.eq_ignore_ascii_case(extension))
            });

        if is_supported {
            paths.push(root.join(relative).to_string_lossy().to_string());
        }
    }

    if paths.is_empty() {
        return Err(Error::new(
            ErrorKind::InvalidFormat,
            "case does not contain any source files for its language",
        ));
    }

    Ok(paths)
}

struct MaterializedProject {
    temp_dir: Option<TempDir>,
    root: PathBuf,
}

impl MaterializedProject {
    fn root(&self) -> &Path {
        &self.root
    }
}

impl Drop for MaterializedProject {
    fn drop(&mut self) {
        let _ = self.temp_dir.as_ref();
    }
}

fn materialize_case(case: &JsonCase, keep_temps: bool) -> Result<MaterializedProject> {
    let temp_dir = tempfile::tempdir().map_err(|error| {
        Error::new(
            ErrorKind::IoFailed,
            format!("failed to create temp project for '{}': {error}", case.id),
        )
    })?;
    let root = temp_dir.path().to_path_buf();

    for file in &case.files {
        let relative = safe_relative_path(&file.path)?;
        let path = root.join(relative);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                Error::new(
                    ErrorKind::IoFailed,
                    format!("failed to create {}: {error}", parent.display()),
                )
            })?;
        }

        fs::write(&path, file.contents.as_bytes()).map_err(|error| {
            Error::new(
                ErrorKind::IoFailed,
                format!("failed to write {}: {error}", path.display()),
            )
        })?;
    }

    if keep_temps {
        let root = temp_dir.keep();
        return Ok(MaterializedProject {
            temp_dir: None,
            root,
        });
    }

    Ok(MaterializedProject {
        temp_dir: Some(temp_dir),
        root,
    })
}

fn safe_relative_path(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if path.is_absolute() {
        return Err(Error::new(
            ErrorKind::InvalidFormat,
            format!("test file path '{}' must be relative", path.display()),
        ));
    }

    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(Error::new(
            ErrorKind::InvalidFormat,
            format!("test file path '{}' escapes the case root", path.display()),
        ));
    }

    Ok(path.to_path_buf())
}

fn format_mismatch(expected: &GraphDocument, actual: &GraphDocument) -> Result<String> {
    let expected = pretty_graph(expected)?;
    let actual = pretty_graph(actual)?;
    Ok(format!(
        "expected JSON graph:\n{expected}\nactual JSON graph:\n{actual}"
    ))
}

fn pretty_graph(document: &GraphDocument) -> Result<String> {
    serde_json::to_string_pretty(document).map_err(|error| {
        Error::new(
            ErrorKind::SerializationFailed,
            format!("failed to serialize graph document: {error}"),
        )
    })
}

fn reset_ids() {
    reset_block_id_counter();
    reset_hir_id_counter();
    reset_scope_id_counter();
    reset_symbol_id_counter();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_absolute_test_file_paths() {
        let result = safe_relative_path("C:/tmp/main.rs");

        assert!(result.is_err());
    }

    #[test]
    fn rejects_parent_directory_paths() {
        let result = safe_relative_path("../main.rs");

        assert!(result.is_err());
    }

    #[test]
    fn case_status_string_conversions_are_derived() {
        assert_eq!(CaseStatus::Passed.to_string(), "passed");
        assert_eq!("FAILED".parse::<CaseStatus>(), Ok(CaseStatus::Failed));

        let value: &'static str = CaseStatus::Updated.into();
        assert_eq!(value, "updated");
    }
}
