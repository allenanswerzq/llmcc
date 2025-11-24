use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use shell_words::{join, split};
use walkdir::WalkDir;

const CASE_BANNER: &str =
    "===============================================================================";

fn slugify_case_name(raw: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;

    for ch in raw.chars() {
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

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "case".to_string()
    } else {
        slug
    }
}

///  Top-level corpus container discovered under a directory (e.g. `tests/corpus`).
pub struct Corpus {
    files: Vec<CorpusFile>,
}

impl Corpus {
    pub fn load(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let mut files = Vec::new();
        if !root.exists() {
            return Err(anyhow!("corpus root {} does not exist", root.display()));
        }

        for entry in WalkDir::new(&root)
            .into_iter()
            .filter_map(|res| res.ok())
            .filter(|entry| entry.file_type().is_file())
        {
            if entry.path().extension().and_then(|ext| ext.to_str()) != Some("llmcc") {
                continue;
            }

            let rel = entry
                .path()
                .strip_prefix(&root)
                .unwrap_or_else(|_| entry.path());
            let suite = rel.with_extension("").to_string_lossy().replace('\\', "/");
            let canonical = entry
                .path()
                .canonicalize()
                .with_context(|| format!("failed to resolve {}", entry.path().display()))?;
            let content = fs::read_to_string(&canonical)
                .with_context(|| format!("failed to read {}", canonical.display()))?;
            let cases = parse_corpus_file(&suite, &canonical, &content)?;
            files.push(CorpusFile {
                path: canonical,
                suite,
                cases,
                dirty: false,
            });
        }

        files.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(Self { files })
    }

    pub fn files(&self) -> &[CorpusFile] {
        &self.files
    }

    pub fn files_mut(&mut self) -> &mut [CorpusFile] {
        &mut self.files
    }

    pub fn write_updates(&mut self) -> Result<()> {
        for file in &mut self.files {
            if file.dirty {
                let rendered = file.render();
                fs::write(&file.path, rendered)
                    .with_context(|| format!("failed to update {}", file.path.display()))?;
                file.dirty = false;
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct CorpusFile {
    pub path: PathBuf,
    pub suite: String,
    pub cases: Vec<CorpusCase>,
    pub(crate) dirty: bool,
}

impl CorpusFile {
    pub fn cases(&self) -> &[CorpusCase] {
        &self.cases
    }

    pub fn cases_mut(&mut self) -> &mut [CorpusCase] {
        &mut self.cases
    }

    pub fn case_id(&self, case_name: &str) -> String {
        format!("{}::{}", self.suite, case_name)
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    fn render(&self) -> String {
        let mut buf = String::new();
        for (idx, case) in self.cases.iter().enumerate() {
            if idx > 0 {
                buf.push_str("\n\n\n");
            }
            let rendered = case.render();
            buf.push_str(rendered.trim_end_matches('\n'));
        }
        buf.push('\n');
        buf
    }
}

#[derive(Debug, Clone)]
pub struct CorpusCase {
    pub suite: String,
    pub name: String,
    pub lang: String,
    pub args: Vec<String>,
    pub files: Vec<TestFile>,
    pub expectations: Vec<CorpusCaseExpectation>,
    /// Comments lines (starting with $//)
    pub comments: Vec<String>,
}

impl CorpusCase {
    pub fn id(&self) -> String {
        format!("{}::{}", self.suite, self.name)
    }

    pub fn expectation(&self, key: &str) -> Option<&str> {
        self.expectations
            .iter()
            .find(|entry| entry.kind == key)
            .map(|entry| entry.value.as_str())
    }

    pub fn expectation_mut(&mut self, key: &str) -> Option<&mut CorpusCaseExpectation> {
        self.expectations.iter_mut().find(|entry| entry.kind == key)
    }

    pub fn ensure_expectation(&mut self, key: &str) -> &mut CorpusCaseExpectation {
        if self.expectation(key).is_none() {
            self.expectations.push(CorpusCaseExpectation {
                kind: key.to_string(),
                value: String::new(),
            });
        }
        self.expectation_mut(key).expect("expectation present")
    }

    pub fn render(&self) -> String {
        let mut buf = String::new();
        // Render comments first
        for comment in &self.comments {
            buf.push_str(comment);
            buf.push('\n');
        }
        buf.push_str(CASE_BANNER);
        buf.push('\n');
        buf.push_str(&self.name);
        buf.push('\n');
        buf.push_str(CASE_BANNER);
        buf.push('\n');
        buf.push('\n');
        if self.lang != "rust" {
            buf.push_str(&format!("lang: {}\n", self.lang));
        }
        if !self.args.is_empty() {
            buf.push_str(&format!("args: {}\n", join(&self.args)));
        }
        buf.push('\n');
        for file in &self.files {
            buf.push_str(&format!("--- file: {} ---\n", file.path));
            buf.push_str(&file.contents);
            if !file.contents.ends_with('\n') {
                buf.push('\n');
            }
            buf.push('\n');
        }

        for expect in &self.expectations {
            buf.push_str(&format!("--- expect:{} ---\n", expect.kind));
            buf.push_str(&expect.value);
            if !expect.value.ends_with('\n') {
                buf.push('\n');
            }
            buf.push('\n');
        }

        buf
    }
}

#[derive(Debug, Clone)]
pub struct TestFile {
    pub path: String,
    pub contents: String,
}

#[derive(Debug, Clone)]
pub struct CorpusCaseExpectation {
    pub kind: String,
    pub value: String,
}

fn parse_corpus_file(suite: &str, path: &Path, content: &str) -> Result<Vec<CorpusCase>> {
    let mut cases = Vec::new();
    let mut current: Option<CorpusCase> = None;
    let mut pending_section: Option<SectionHeader> = None;
    let mut section_lines: Vec<String> = Vec::new();
    let mut awaiting_banner_name = false;
    let mut awaiting_banner_close = false;
    let mut pending_comments: Vec<String> = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();

        if trimmed.starts_with("$//") {
            pending_comments.push(line.to_string());
            continue;
        }

        if awaiting_banner_close {
            if trimmed.is_empty() {
                continue;
            }
            if is_banner_line(line) {
                awaiting_banner_close = false;
                continue;
            } else {
                return Err(anyhow!(
                    "expected closing banner after case '{}' in {}",
                    current
                        .as_ref()
                        .map(|c| c.name.as_str())
                        .unwrap_or("unknown"),
                    path.display()
                ));
            }
        }

        if awaiting_banner_name {
            if trimmed.is_empty() {
                continue;
            }

            finalize_section(&mut current, &mut pending_section, &mut section_lines)?;
            if let Some(case) = current.take() {
                cases.push(case);
            }

            current = Some(CorpusCase {
                suite: suite.to_string(),
                name: slugify_case_name(trimmed),
                lang: "rust".to_string(),
                args: Vec::new(),
                files: Vec::new(),
                expectations: Vec::new(),
                comments: std::mem::take(&mut pending_comments),
            });
            awaiting_banner_name = false;
            awaiting_banner_close = true;
            continue;
        }

        if is_banner_line(line) {
            finalize_section(&mut current, &mut pending_section, &mut section_lines)?;
            if let Some(case) = current.take() {
                cases.push(case);
            }
            awaiting_banner_name = true;
            continue;
        }

        if let Some(header) = parse_case_header(line) {
            finalize_section(&mut current, &mut pending_section, &mut section_lines)?;
            if let Some(case) = current.take() {
                cases.push(case);
            }

            current = Some(CorpusCase {
                suite: suite.to_string(),
                name: header,
                lang: "rust".to_string(),
                args: Vec::new(),
                files: Vec::new(),
                expectations: Vec::new(),
                comments: std::mem::take(&mut pending_comments),
            });
            continue;
        }

        if let Some(section) = parse_section_header(line) {
            finalize_section(&mut current, &mut pending_section, &mut section_lines)?;
            pending_section = Some(section);
            continue;
        }

        if pending_section.is_some() {
            section_lines.push(line.to_string());
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        let case = current.as_mut().ok_or_else(|| {
            anyhow!(
                "content encountered before case header in {}",
                path.display()
            )
        })?;

        if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            match key {
                "lang" => case.lang = value.to_string(),
                "args" => {
                    case.args = split(value)
                        .map_err(|err| anyhow!("invalid args in {}: {}", path.display(), err))?
                }
                other => {
                    return Err(anyhow!(
                        "unsupported metadata '{}' in {} case {}",
                        other,
                        path.display(),
                        case.name
                    ));
                }
            }
        } else {
            return Err(anyhow!(
                "unexpected line '{}' in {} (within case {})",
                line,
                path.display(),
                case.name
            ));
        }
    }

    finalize_section(&mut current, &mut pending_section, &mut section_lines)?;
    if awaiting_banner_name || awaiting_banner_close {
        return Err(anyhow!(
            "unterminated banner in {} (missing case name or closing separator)",
            path.display()
        ));
    }
    if let Some(case) = current.take() {
        cases.push(case);
    }

    if cases.is_empty() {
        return Err(anyhow!(
            "corpus file {} does not contain any cases",
            path.display()
        ));
    }

    for case in &cases {
        if case.files.is_empty() {
            return Err(anyhow!(
                "case {}::{} in {} does not declare any files",
                case.suite,
                case.name,
                path.display()
            ));
        }
    }

    Ok(cases)
}

fn parse_case_header(line: &str) -> Option<String> {
    if line.starts_with("===") && line.ends_with("===") {
        let name = line.trim_matches('=').trim();
        if name.is_empty() {
            None
        } else {
            Some(slugify_case_name(name))
        }
    } else {
        None
    }
}

fn is_banner_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.chars().all(|ch| ch == '=') && trimmed.len() >= 5
}

#[derive(Debug, Clone)]
enum SectionHeader {
    File { path: String },
    Expect { kind: String },
}

fn parse_section_header(line: &str) -> Option<SectionHeader> {
    if !line.starts_with("---") || !line.ends_with("---") {
        return None;
    }

    let inner = line.trim_start_matches('-').trim_end_matches('-').trim();
    if let Some(rest) = inner.strip_prefix("file:") {
        return Some(SectionHeader::File {
            path: rest.trim().to_string(),
        });
    }

    if let Some(rest) = inner.strip_prefix("expect:") {
        return Some(SectionHeader::Expect {
            kind: rest.trim().to_string(),
        });
    }

    None
}

fn finalize_section(
    current: &mut Option<CorpusCase>,
    pending: &mut Option<SectionHeader>,
    lines: &mut Vec<String>,
) -> Result<()> {
    if pending.is_none() {
        lines.clear();
        return Ok(());
    }

    let case = current
        .as_mut()
        .ok_or_else(|| anyhow!("section declared before any case header"))?;

    let mut content = if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n")
    };
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }

    match pending.take().unwrap() {
        SectionHeader::File { path } => {
            case.files.push(TestFile {
                path,
                contents: content,
            });
        }
        SectionHeader::Expect { kind } => {
            case.expectations.push(CorpusCaseExpectation {
                kind,
                value: content,
            });
        }
    }

    lines.clear();
    Ok(())
}
