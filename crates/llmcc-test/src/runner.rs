//! Test execution engine: run cases, compare outputs, report results.

use std::fmt;
use std::path::Path;

use similar::TextDiff;

use llmcc_core::Result;

use crate::corpus::{Corpus, CorpusFile, OutputKind, TestCase};
use crate::pipeline;

// --- Public API ---

/// Execute all matching test cases in a corpus.
///
/// Returns a summary of pass/fail/skip/update counts.
/// When `update` is true, mismatched expectations are overwritten with actual output.
pub fn run(corpus: &mut Corpus, filter: Option<&str>, update: bool) -> Result<RunSummary> {
    let mut summary = RunSummary::default();

    for file in &mut corpus.files {
        run_file(file, filter, update, &mut summary);
    }

    Ok(summary)
}

/// Aggregated results from a test run.
#[derive(Default)]
pub struct RunSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub updated: usize,
    pub errors: usize,
    pub skipped: usize,
}

impl RunSummary {
    /// True when no tests failed or errored.
    pub fn is_ok(&self) -> bool {
        self.failed == 0 && self.errors == 0
    }
}

impl fmt::Display for RunSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} tests: {} passed, {} failed, {} updated, {} errors, {} skipped",
            self.total, self.passed, self.failed, self.updated, self.errors, self.skipped
        )
    }
}

// --- Outcome ---

/// Outcome of executing a single test case.
enum Outcome {
    /// All expectations matched.
    Pass,
    /// Expectations were updated (bless mode).
    Updated,
    /// One or more expectations diverged.
    Fail(String),
    /// Pipeline error prevented comparison.
    Error(String),
    /// No expectations defined; nothing to check.
    Skip,
}

// --- Internal execution ---

fn run_file(file: &mut CorpusFile, filter: Option<&str>, update: bool, summary: &mut RunSummary) {
    for case in &mut file.cases {
        let case_id = case.qualified_name(&file.suite);

        if let Some(f) = filter {
            if !case_id.contains(f) {
                continue;
            }
        }

        summary.total += 1;
        print!("  {case_id} ... ");

        let outcome = execute_case(case, update);

        match &outcome {
            Outcome::Pass => {
                println!("ok");
                summary.passed += 1;
            }
            Outcome::Updated => {
                println!("updated");
                summary.updated += 1;
                file.dirty = true;
            }
            Outcome::Fail(diff) => {
                println!("FAILED");
                for line in diff.lines() {
                    println!("        {line}");
                }
                summary.failed += 1;
            }
            Outcome::Error(msg) => {
                println!("ERROR: {msg}");
                summary.errors += 1;
            }
            Outcome::Skip => {
                println!("skipped");
                summary.skipped += 1;
            }
        }
    }
}

/// Run the pipeline for one case and compare all expectations.
fn execute_case(case: &mut TestCase, update: bool) -> Outcome {
    if case.expectations.is_empty() {
        return Outcome::Skip;
    }

    let output = match pipeline::run_case(case) {
        Ok(o) => o,
        Err(e) => return Outcome::Error(e.to_string()),
    };

    let mut failures = Vec::new();
    let mut any_updated = false;

    for (kind, expected) in &mut case.expectations {
        let Some(actual) = output.outputs.get(kind) else {
            continue;
        };

        let expected_norm = normalize(*kind, expected, None);
        let actual_norm = normalize(*kind, actual, Some(&output.temp_dir));

        if expected_norm == actual_norm {
            continue;
        }

        if update {
            *expected = replace_tmp_and_finalize(actual, &output.temp_dir);
            any_updated = true;
        } else {
            failures.push(format_diff(&kind.to_string(), &expected_norm, &actual_norm));
        }
    }

    if !failures.is_empty() {
        Outcome::Fail(failures.join("\n"))
    } else if any_updated {
        Outcome::Updated
    } else {
        Outcome::Pass
    }
}

// --- Normalization ---

/// Normalize text for comparison: unify line endings, replace temp paths, apply
/// kind-specific ordering.
fn normalize(kind: OutputKind, text: &str, temp_dir: Option<&str>) -> String {
    let mut s = text.replace("\r\n", "\n");
    s = s.trim_end_matches('\n').to_string();

    if let Some(tmp) = temp_dir {
        s = replace_tmp_path(&s, tmp);
    }

    if kind.sorts_lines() {
        sort_lines(&s)
    } else if kind == OutputKind::BlockGraph {
        normalize_block_graph(&s)
    } else {
        s
    }
}

/// Replace temp directory paths with `$TMP`.
///
/// The temp path in output can differ from `tmp` because renderers normalize
/// path separators and DOT cluster ids sanitize labels. We normalize the random
/// final component first, then strip any path prefix before `$TMP`.
fn replace_tmp_path(text: &str, tmp: &str) -> String {
    let dir_name = match Path::new(tmp).file_name().and_then(|n| n.to_str()) {
        Some(name) => name.to_string(),
        None => return text.to_string(),
    };

    let mut s = text.replace(&dir_name, "$TMP");

    let sanitized_dir = dot_id_fragment(&dir_name);
    if sanitized_dir != dir_name {
        s = s.replace(&sanitized_dir, "TMP");
    }

    strip_tmp_path_prefixes(&s)
}

fn strip_tmp_path_prefixes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut remaining = text;

    while let Some(pos) = remaining.find("$TMP") {
        let before = &remaining[..pos];
        let path_start = path_prefix_start(before);
        result.push_str(&before[..path_start]);
        result.push_str("$TMP");
        remaining = &remaining[pos + "$TMP".len()..];
    }

    result.push_str(remaining);
    result
}

fn path_prefix_start(before: &str) -> usize {
    let bytes = before.as_bytes();
    let mut idx = bytes.len();
    while idx > 0 {
        let byte = bytes[idx - 1];
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'/' | b'\\' | b':' | b'.' | b'~' | b'_' | b'-')
        {
            idx -= 1;
        } else {
            break;
        }
    }
    idx
}

/// Prepare actual output for saving: replace temp paths and ensure trailing newline.
fn replace_tmp_and_finalize(text: &str, tmp: &str) -> String {
    let mut result = replace_tmp_path(text, tmp);
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn dot_id_fragment(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn sort_lines(text: &str) -> String {
    let mut lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines.sort();
    lines.join("\n")
}

fn normalize_block_graph(text: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(|l| l.trim_end().to_string()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

// --- Diff formatting ---

fn format_diff(kind: &str, expected: &str, actual: &str) -> String {
    let diff = TextDiff::from_lines(expected, actual);
    let mut buf = format!("Expectation '{kind}' mismatch:\n");
    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            similar::ChangeTag::Delete => "-",
            similar::ChangeTag::Insert => "+",
            similar::ChangeTag::Equal => " ",
        };
        buf.push_str(sign);
        buf.push_str(&change.to_string());
    }
    buf
}
