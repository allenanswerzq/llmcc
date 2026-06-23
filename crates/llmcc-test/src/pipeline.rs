//! Pipeline execution: materialize test files, run the llmcc compiler pipeline,
//! and collect rendered outputs for comparison.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use llmcc_core::context::CompileCtxt;
use llmcc_core::ir_builder::{HirBuildOptions, build_hir};
use llmcc_core::lang_def::Language;
use llmcc_core::{
    Error, ErrorKind, GraphBuildOptions, ProjectGraph, ResolveOptions, Result, SupportedLang,
    build_graphs,
};
use llmcc_cpp::LangCpp;
use llmcc_resolver::{bind_symbols, collect_symbols};
use llmcc_rust::LangRust;
use llmcc_ts::LangTypeScript;
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::corpus::{OutputKind, TestCase};
use crate::render;

// --- Public API ---

/// Output produced by running one test case through the full pipeline.
pub struct CaseOutput {
    /// Rendered outputs keyed by expectation kind.
    pub outputs: BTreeMap<OutputKind, String>,
    /// Absolute temp directory path (callers replace with `$TMP` for portability).
    pub temp_dir: String,
}

/// Execute a test case: materialize source files, compile, and render all
/// requested output kinds.
pub fn run_case(case: &TestCase) -> Result<CaseOutput> {
    let needed: Vec<OutputKind> = case.expectations.iter().map(|(k, _)| *k).collect();
    if needed.is_empty() {
        return Ok(CaseOutput {
            outputs: BTreeMap::new(),
            temp_dir: String::new(),
        });
    }

    // Reset global counters for deterministic output across test cases.
    llmcc_core::block::reset_block_id_counter();
    llmcc_core::symbol::reset_symbol_id_counter();

    let (temp_dir, root) = materialize_files(case)?;
    let temp_path = root.to_string_lossy().to_string();

    let outputs = match case.lang {
        SupportedLang::Rust => compile_and_render::<LangRust>(&root, &needed)?,
        SupportedLang::Typescript => compile_and_render::<LangTypeScript>(&root, &needed)?,
        SupportedLang::Cpp => compile_and_render::<LangCpp>(&root, &needed)?,
        SupportedLang::Auto => compile_auto(&root, &needed)?,
    };

    drop(temp_dir);
    Ok(CaseOutput {
        outputs,
        temp_dir: temp_path,
    })
}

// --- File materialization ---

/// Write test case source files into a temp directory.
///
/// Source files are prefixed with a numeric index for deterministic ordering,
/// while manifest files (Cargo.toml, package.json, etc.) retain their original
/// paths so the pipeline can detect project structure.
fn materialize_files(case: &TestCase) -> Result<(TempDir, PathBuf)> {
    let temp_dir = tempfile::tempdir()?;
    let root = temp_dir.path().to_path_buf();
    let manifests = case.lang.manifest_names();

    for (idx, (path, content)) in case.files.iter().enumerate() {
        let original = Path::new(path);
        let file_name = original.file_name().unwrap_or_default().to_string_lossy();

        let final_path = if manifests.iter().any(|m| *m == file_name) {
            original.to_path_buf()
        } else {
            let prefixed = format!("{idx:03}_{file_name}");
            original
                .parent()
                .map(|p| p.join(&prefixed))
                .unwrap_or_else(|| PathBuf::from(&prefixed))
        };

        let abs_path = root.join(&final_path);
        if let Some(parent) = abs_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&abs_path, content.as_bytes())?;
    }

    Ok((temp_dir, root))
}

// --- Compilation ---

/// Compile with a single language and render all requested outputs.
fn compile_and_render<L: Language>(
    root: &Path,
    needed: &[OutputKind],
) -> Result<BTreeMap<OutputKind, String>> {
    let files = discover_source_files::<L>(root)?;
    let cc = CompileCtxt::from_files::<L>(&files)?;

    build_hir::<L>(&cc, HirBuildOptions::new().with_sequential(true))?;

    let resolve_opts = ResolveOptions::default().with_sequential(true);
    let globals = collect_symbols::<L>(&cc, &resolve_opts)?;
    bind_symbols::<L>(&cc, globals, &resolve_opts)?;

    let project = if needed.iter().any(|k| k.needs_graph()) {
        let units = build_graphs::<L>(&cc, GraphBuildOptions::new().with_sequential(true))?;
        Some(ProjectGraph::build(&cc, units))
    } else {
        None
    };

    render_all(needed, &cc, project.as_ref())
}

/// Auto-detect languages by file presence and combine results.
fn compile_auto(root: &Path, needed: &[OutputKind]) -> Result<BTreeMap<OutputKind, String>> {
    let rust_files = discover_source_files::<LangRust>(root).unwrap_or_default();
    let ts_files = discover_source_files::<LangTypeScript>(root).unwrap_or_default();

    let mut merged: BTreeMap<OutputKind, Vec<String>> = BTreeMap::new();

    if !rust_files.is_empty() {
        for (kind, text) in compile_and_render::<LangRust>(root, needed)? {
            merged.entry(kind).or_default().push(text);
        }
    }
    if !ts_files.is_empty() {
        for (kind, text) in compile_and_render::<LangTypeScript>(root, needed)? {
            merged.entry(kind).or_default().push(text);
        }
    }

    Ok(merged
        .into_iter()
        .map(|(kind, texts)| {
            let combined = if texts.len() == 1 {
                texts.into_iter().next().unwrap()
            } else {
                texts.join("\n")
            };
            (kind, combined)
        })
        .collect())
}

/// Render all needed output kinds using the render module.
fn render_all(
    needed: &[OutputKind],
    cc: &CompileCtxt<'_>,
    project: Option<&ProjectGraph<'_>>,
) -> Result<BTreeMap<OutputKind, String>> {
    let mut outputs = BTreeMap::new();
    for &kind in needed {
        let text = render::render(kind, cc, project)
            .ok_or_else(|| Error::new(ErrorKind::Unexpected, format!("{kind} requires graph")))?;
        outputs.insert(kind, text);
    }
    Ok(outputs)
}

// --- File discovery ---

/// Discover source files matching the language's extensions under `root`.
/// Files are sorted by numeric prefix for deterministic processing order.
fn discover_source_files<L: Language>(root: &Path) -> Result<Vec<String>> {
    let extensions = L::extensions();
    let mut files = Vec::new();

    for entry in WalkDir::new(root).into_iter().filter_map(|r| r.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)) {
            files.push(entry.path().to_string_lossy().to_string());
        }
    }

    files.sort_by(|a, b| {
        let prefix = |p: &str| -> Option<usize> {
            Path::new(p)
                .file_name()?
                .to_str()?
                .split('_')
                .next()?
                .parse()
                .ok()
        };
        prefix(a).cmp(&prefix(b))
    });

    Ok(files)
}
