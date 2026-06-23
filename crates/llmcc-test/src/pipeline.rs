//! Pipeline execution: materialize test files, run the llmcc compiler pipeline,
//! and produce rendered outputs for comparison against expectations.

use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use llmcc_core::block::BlockKind;
use llmcc_core::context::{CompileCtxt, CompileUnit};
use llmcc_core::ir_builder::{HirBuildOptions, build_hir};
use llmcc_core::lang_def::Language;
use llmcc_core::{
    BlockId, CollectedGraph, Error, ErrorKind, GraphBuildOptions, ProjectGraph, ResolveOptions,
    Result, SupportedLang, ViewDepth, build_graphs,
};
use llmcc_cpp::LangCpp;
use llmcc_dot::RenderOptions;
use llmcc_resolver::{bind_symbols, collect_symbols};
use llmcc_rust::LangRust;
use llmcc_ts::LangTypeScript;
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::corpus::{OutputKind, TestCase};

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

// --- Compilation and rendering ---

/// Run the full pipeline for a single language and render all needed outputs.
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

    let mut outputs = BTreeMap::new();
    for &kind in needed {
        let text = render_output(kind, &cc, project.as_ref())?;
        outputs.insert(kind, text);
    }

    Ok(outputs)
}

/// Auto-detect languages by file presence and merge results.
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

// --- Output rendering ---

/// Render a single output kind from the compiled context and optional graph.
fn render_output(
    kind: OutputKind,
    cc: &CompileCtxt<'_>,
    project: Option<&ProjectGraph<'_>>,
) -> Result<String> {
    match kind {
        OutputKind::Symbols | OutputKind::SymbolTypes => Ok(render_symbols(cc)),
        OutputKind::SymbolDeps => Ok(String::new()),
        OutputKind::BlockGraph => Ok(render_block_graph(require_graph(project, kind)?)),
        OutputKind::BlockRelations => Ok(render_block_relations(require_graph(project, kind)?)),
        OutputKind::Blocks | OutputKind::BlockDeps => {
            Ok(render_block_deps(require_graph(project, kind)?))
        }
        OutputKind::ArchGraph => render_arch(project, ViewDepth::File),
        OutputKind::ArchGraphDepth0 => render_arch(project, ViewDepth::Project),
        OutputKind::ArchGraphDepth1 => render_arch(project, ViewDepth::Package),
        OutputKind::ArchGraphDepth2 => render_arch(project, ViewDepth::Module),
        OutputKind::ArchGraphDepth3 => render_arch(project, ViewDepth::File),
    }
}

/// Unwrap a project graph reference, returning an error naming the output kind.
fn require_graph<'a>(
    project: Option<&'a ProjectGraph<'_>>,
    kind: OutputKind,
) -> Result<&'a ProjectGraph<'a>> {
    project.ok_or_else(|| Error::new(ErrorKind::Unexpected, format!("{kind} requires graph")))
}

fn render_arch(project: Option<&ProjectGraph<'_>>, depth: ViewDepth) -> Result<String> {
    let pg = require_graph(project, OutputKind::ArchGraph)?;
    let graph = CollectedGraph::new(pg);
    let opts = RenderOptions::default();
    Ok(llmcc_dot::render(&graph, depth, &opts))
}

// --- Symbol rendering ---

/// One row in the symbol table output.
struct SymbolRow {
    unit: usize,
    id: u32,
    kind: String,
    name: String,
    is_global: bool,
}

impl SymbolRow {
    fn label(&self) -> String {
        format!("u{}:{}", self.unit, self.id)
    }
}

fn render_symbols(cc: &CompileCtxt<'_>) -> String {
    let symbols = cc.symbols();
    let interner = cc.interner();

    if symbols.is_empty() {
        return "none\n".to_string();
    }

    let mut rows: Vec<SymbolRow> = symbols
        .iter()
        .map(|sym| SymbolRow {
            unit: sym.unit_index().unwrap_or_default(),
            id: sym.id().0 as u32,
            kind: format!("{:?}", sym.kind()),
            name: interner
                .try_resolve(sym.name)
                .unwrap_or_else(|| "?".to_string()),
            is_global: sym.is_global(),
        })
        .collect();

    rows.sort_by(|a, b| a.unit.cmp(&b.unit).then(a.id.cmp(&b.id)));

    let label_w = rows.iter().map(|r| r.label().len()).max().unwrap_or(0);
    let kind_w = rows.iter().map(|r| r.kind.len()).max().unwrap_or(0);
    let name_w = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
    let has_globals = rows.iter().any(|r| r.is_global);

    let mut buf = String::new();
    for row in &rows {
        let label = row.label();
        let global = if row.is_global { "[global]" } else { "" };
        if has_globals {
            let _ = writeln!(
                buf,
                "{label:<label_w$} | {:<kind_w$} | {:<name_w$} | {global:8}",
                row.kind, row.name
            );
        } else {
            let _ = writeln!(
                buf,
                "{label:<label_w$} | {:<kind_w$} | {:<name_w$} |",
                row.kind, row.name
            );
        }
    }
    buf
}

// --- Block graph rendering ---

fn render_block_graph(project: &ProjectGraph<'_>) -> String {
    let mut units: Vec<_> = project.units().iter().collect();
    if units.is_empty() {
        return "none\n".to_string();
    }
    units.sort_by_key(|u| u.unit_index());

    let mut sections = Vec::new();
    for unit_graph in units {
        let unit = project.context().compile_unit(unit_graph.unit_index());
        let mut buf = String::new();
        render_block_node(unit_graph.root(), unit, 0, &mut buf);
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

fn render_block_node(id: BlockId, unit: CompileUnit<'_>, depth: usize, buf: &mut String) {
    let block = unit.block(id);
    let indent = "  ".repeat(depth);
    let label = block.to_string();
    let deps = block.dependency_labels(unit);

    let _ = write!(buf, "{indent}({label}");

    let children = block.children();
    if children.is_empty() && deps.is_empty() {
        buf.push_str(")\n");
        return;
    }

    buf.push('\n');
    for child_id in children {
        if unit.block(child_id).kind() == BlockKind::Call {
            continue;
        }
        render_block_node(child_id, unit, depth + 1, buf);
    }
    let child_indent = "  ".repeat(depth + 1);
    for dep in deps {
        let _ = writeln!(buf, "{child_indent}({dep})");
    }
    buf.push_str(&indent);
    buf.push_str(")\n");
}

// --- Block relations rendering ---

/// One row in the block-relations output.
struct RelationRow {
    source: String,
    kind: String,
    name: String,
    relation: String,
    targets: Vec<String>,
}

impl fmt::Display for RelationRow {
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

fn render_block_relations(project: &ProjectGraph<'_>) -> String {
    let cc = project.context();
    let related_map = cc.block_relations();
    let mut rows: Vec<RelationRow> = Vec::new();

    for block_id in related_map.blocks() {
        let Some(info) = cc.block_info(block_id) else {
            continue;
        };

        let relations = related_map.relations_from(block_id);
        if relations.is_empty() {
            continue;
        }

        let source = format!("u{}:{}", info.unit_index, block_id.as_u32());

        for rel in relations.iter() {
            let mut targets: Vec<String> = rel
                .targets
                .iter()
                .map(|tid| {
                    let tunit = cc
                        .block_info(*tid)
                        .map(|e| e.unit_index)
                        .unwrap_or_default();
                    format!("u{tunit}:{}", tid.as_u32())
                })
                .collect();
            targets.sort();

            rows.push(RelationRow {
                source: source.clone(),
                kind: info.kind.to_string(),
                name: info.name.clone().unwrap_or_default(),
                relation: rel.relation.to_string(),
                targets,
            });
        }
    }

    render_sorted_lines(&rows)
}

// --- Block deps rendering ---

/// One row in the blocks/block-deps output.
struct BlockRow {
    label: String,
    kind: String,
    name: String,
}

impl fmt::Display for BlockRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} | {} | {}", self.label, self.kind, self.name)
    }
}

fn render_block_deps(project: &ProjectGraph<'_>) -> String {
    let cc = project.context();
    let mut rows: Vec<BlockRow> = Vec::new();

    for unit_graph in project.units() {
        for entry in cc.find_blocks_in_unit(unit_graph.unit_index()) {
            rows.push(BlockRow {
                label: format!("u{}:{}", entry.unit_index, entry.block_id.as_u32()),
                name: entry.name.clone().unwrap_or_default(),
                kind: entry.kind.to_string(),
            });
        }
    }

    render_sorted_lines(&rows)
}

/// Render a collection of `Display` items as sorted, newline-terminated text.
fn render_sorted_lines(items: &[impl fmt::Display]) -> String {
    if items.is_empty() {
        return "none\n".to_string();
    }
    let mut lines: Vec<String> = items.iter().map(|item| item.to_string()).collect();
    lines.sort();
    let mut result = lines.join("\n");
    result.push('\n');
    result
}

// --- Helpers ---

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
