//! Parse `.llmcc` corpus files into test cases.

use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use llmcc_core::{Error, ErrorKind, Result, SupportedLang};
use strum_macros::{Display, EnumString};
use walkdir::WalkDir;

/// Output expectation kinds that can appear in `.llmcc` files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Display, EnumString)]
#[strum(serialize_all = "kebab-case", ascii_case_insensitive)]
pub enum OutputKind {
    Symbols,
    SymbolTypes,
    SymbolDeps,
    BlockGraph,
    BlockRelations,
    Blocks,
    BlockDeps,
    #[strum(serialize = "file", serialize = "arch-graph")]
    File,
    #[strum(serialize = "project", serialize = "arch-graph-depth0")]
    Project,
    #[strum(serialize = "package", serialize = "arch-graph-depth1")]
    Package,
    #[strum(
        serialize = "namespace",
        serialize = "arch-graph-depth2",
        serialize = "arch-graph-depth3"
    )]
    Namespace,
}

impl OutputKind {
    /// Whether this output kind requires a project graph to be built.
    pub fn needs_graph(self) -> bool {
        matches!(
            self,
            Self::BlockGraph
                | Self::BlockRelations
                | Self::Blocks
                | Self::BlockDeps
                | Self::File
                | Self::Project
                | Self::Package
                | Self::Namespace
        )
    }

    /// Whether this output kind uses line-sorted normalization.
    pub fn sorts_lines(self) -> bool {
        matches!(
            self,
            Self::Symbols
                | Self::SymbolTypes
                | Self::Blocks
                | Self::SymbolDeps
                | Self::BlockDeps
                | Self::BlockRelations
        )
    }
}

/// A collection of `.llmcc` test files discovered under a root directory.
pub struct Corpus {
    pub files: Vec<CorpusFile>,
}

/// One `.llmcc` file containing one or more test cases.
pub struct CorpusFile {
    pub path: PathBuf,
    pub suite: String,
    pub cases: Vec<TestCase>,
    pub dirty: bool,
}

/// A single test case parsed from a `.llmcc` file.
#[derive(Clone)]
pub struct TestCase {
    pub name: String,
    pub lang: SupportedLang,
    /// Source files to materialize: `(relative_path, content)`.
    pub files: Vec<(String, String)>,
    /// Expected outputs.
    pub expectations: Vec<(OutputKind, String)>,
    /// Raw metadata for re-serialization.
    pub(crate) comments: Vec<String>,
    pub(crate) description: Vec<String>,
    pub(crate) args: Vec<String>,
}

impl TestCase {
    /// Full test identifier: `suite::case-name`.
    pub fn qualified_name(&self, suite: &str) -> String {
        format!("{suite}::{}", self.name)
    }
}

impl Corpus {
    /// Walk `root` for `.llmcc` files and parse them.
    pub fn load(root: &Path) -> Result<Self> {
        let mut files = Vec::new();

        for entry in WalkDir::new(root)
            .into_iter()
            .filter_map(|r| r.ok())
            .filter(|e| e.file_type().is_file())
        {
            if entry.path().extension().and_then(|e| e.to_str()) != Some("llmcc") {
                continue;
            }

            let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let suite = rel.with_extension("").to_string_lossy().replace('\\', "/");

            let path = entry.path().canonicalize()?;
            let content = fs::read_to_string(&path)?;
            let cases = parse_file(&suite, &content)?;
            files.push(CorpusFile {
                path,
                suite,
                cases,
                dirty: false,
            });
        }

        files.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self { files })
    }

    /// Write back any files that were modified by `--update`.
    pub fn write_updates(&self) -> Result<()> {
        for file in &self.files {
            if file.dirty {
                let rendered = render_file(file);
                fs::write(&file.path, rendered)?;
            }
        }
        Ok(())
    }
}

// ─── Parsing ────────────────────────────────────────────────────────────────

const BANNER: &str =
    "===============================================================================";

fn detect_lang(suite: &str) -> SupportedLang {
    if suite.starts_with("typescript/") || suite.starts_with("ts/") {
        SupportedLang::Typescript
    } else if suite.starts_with("cpp/") {
        SupportedLang::Cpp
    } else if suite.starts_with("auto/") {
        SupportedLang::Auto
    } else {
        SupportedLang::Rust
    }
}

fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(ch.to_ascii_lowercase());
            pending_dash = false;
        } else if !slug.is_empty() {
            pending_dash = true;
        }
    }
    if slug.is_empty() { "case".into() } else { slug }
}

fn parse_file(suite: &str, content: &str) -> Result<Vec<TestCase>> {
    let default_lang = detect_lang(suite);
    let mut cases = Vec::new();
    let mut current: Option<TestCase> = None;
    let mut section: Option<Section> = None;
    let mut section_lines: Vec<String> = Vec::new();
    let mut state = ParseState::BetweenCases;
    let mut pending_comments: Vec<String> = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();

        // Collect comment lines before a case starts.
        if trimmed.starts_with("$//") {
            pending_comments.push(line.to_string());
            continue;
        }

        match state {
            ParseState::BetweenCases => {
                if is_banner(line) {
                    flush_section(&mut current, &mut section, &mut section_lines);
                    if let Some(case) = current.take() {
                        cases.push(case);
                    }
                    state = ParseState::AwaitingName;
                }
                // Ignore other lines between cases.
            }

            ParseState::AwaitingName => {
                if trimmed.is_empty() {
                    continue;
                }
                current = Some(TestCase {
                    name: slugify(trimmed),
                    lang: default_lang,
                    files: Vec::new(),
                    expectations: Vec::new(),
                    comments: std::mem::take(&mut pending_comments),
                    description: Vec::new(),
                    args: Vec::new(),
                });
                state = ParseState::AwaitingClose;
            }

            ParseState::AwaitingClose => {
                if trimmed.is_empty() {
                    continue;
                }
                if is_banner(line) {
                    state = ParseState::CaseBody;
                    continue;
                }
                return Err(Error::new(
                    ErrorKind::ParseFailed,
                    format!("expected closing banner in suite {suite}"),
                ));
            }

            ParseState::CaseBody => {
                // Transition to next case on a new banner.
                if is_banner(line) {
                    flush_section(&mut current, &mut section, &mut section_lines);
                    if let Some(case) = current.take() {
                        cases.push(case);
                    }
                    state = ParseState::AwaitingName;
                    continue;
                }

                // Section header.
                if let Some(s) = parse_section_header(line) {
                    flush_section(&mut current, &mut section, &mut section_lines);
                    section = Some(s);
                    continue;
                }

                // Inside a section: accumulate content lines.
                if section.is_some() {
                    section_lines.push(line.to_string());
                    continue;
                }

                if trimmed.is_empty() {
                    continue;
                }

                // Metadata area (before first section header).
                if let Some(ref mut case) = current {
                    parse_metadata_line(case, line, trimmed, default_lang);
                }
            }
        }
    }

    flush_section(&mut current, &mut section, &mut section_lines);
    if let Some(case) = current.take() {
        cases.push(case);
    }

    Ok(cases)
}

/// Parser state machine states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseState {
    /// Between test cases (or at start of file).
    BetweenCases,
    /// Saw opening banner, waiting for the case name line.
    AwaitingName,
    /// Saw the case name, waiting for closing banner.
    AwaitingClose,
    /// Inside a case body (metadata, file sections, expect sections).
    CaseBody,
}

/// Parse a key:value metadata line within a test case header.
fn parse_metadata_line(
    case: &mut TestCase,
    line: &str,
    trimmed: &str,
    default_lang: SupportedLang,
) {
    if let Some((key, value)) = trimmed.split_once(':') {
        let key = key.trim();
        let value = value.trim();
        match key {
            "lang" => case.lang = SupportedLang::from_str(value).unwrap_or(default_lang),
            "args" => case.args = shell_words::split(value).unwrap_or_default(),
            _ => case.description.push(line.to_string()),
        }
    } else {
        case.description.push(line.to_string());
    }
}

enum Section {
    File(String),
    Expect(OutputKind),
}

fn is_banner(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 20 && trimmed.chars().all(|c| c == '=')
}

fn parse_section_header(line: &str) -> Option<Section> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix("---")?.trim();
    let inner = inner.strip_suffix("---").unwrap_or(inner).trim();

    inner
        .strip_prefix("file:")
        .map(|path| Section::File(path.trim().to_string()))
        .or_else(|| {
            inner
                .strip_prefix("expect:")
                .and_then(|kind| OutputKind::from_str(kind.trim()).ok())
                .map(Section::Expect)
        })
}

fn flush_section(
    current: &mut Option<TestCase>,
    section: &mut Option<Section>,
    lines: &mut Vec<String>,
) {
    let Some(case) = current.as_mut() else {
        lines.clear();
        *section = None;
        return;
    };
    let Some(sec) = section.take() else {
        lines.clear();
        return;
    };

    let text = join_section_lines(lines);
    lines.clear();

    match sec {
        Section::File(path) => case.files.push((path, text)),
        Section::Expect(kind) => case.expectations.push((kind, text)),
    }
}

fn join_section_lines(lines: &[String]) -> String {
    // Trim trailing empty lines but keep content intact
    let mut end = lines.len();
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    let mut text = lines[..end].join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    text
}

// ─── Rendering (for --update) ───────────────────────────────────────────────

fn render_file(file: &CorpusFile) -> String {
    let mut buf = String::new();
    for (idx, case) in file.cases.iter().enumerate() {
        if idx > 0 {
            buf.push_str("\n\n\n");
        }
        render_case(&mut buf, case);
    }
    // Ensure file ends with exactly one newline
    let trimmed = buf.trim_end_matches('\n');
    let mut result = trimmed.to_string();
    result.push('\n');
    result
}

fn render_case(buf: &mut String, case: &TestCase) {
    for comment in &case.comments {
        buf.push_str(comment);
        buf.push('\n');
    }
    buf.push_str(BANNER);
    buf.push('\n');
    buf.push_str(&case.name);
    buf.push('\n');
    buf.push_str(BANNER);
    buf.push('\n');

    for line in &case.description {
        buf.push_str(line);
        buf.push('\n');
    }
    if case.description.is_empty() {
        buf.push('\n');
    }
    if case.lang != SupportedLang::Rust {
        buf.push_str(&format!("lang: {}\n", case.lang));
    }
    if !case.args.is_empty() {
        buf.push_str(&format!("args: {}\n", shell_words::join(&case.args)));
    }
    buf.push('\n');

    for (path, content) in &case.files {
        buf.push_str(&format!("--- file: {path} ---\n"));
        buf.push_str(content);
        if !content.ends_with('\n') {
            buf.push('\n');
        }
        buf.push('\n');
    }

    for (kind, value) in &case.expectations {
        buf.push_str(&format!("--- expect:{kind} ---\n"));
        buf.push_str(value);
        if !value.ends_with('\n') {
            buf.push('\n');
        }
        buf.push('\n');
    }
}
