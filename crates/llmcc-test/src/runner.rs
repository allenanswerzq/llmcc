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

/// Replace temp directory paths (and their final component) with `$TMP`.
///
/// Handles mismatches between the actual temp path and what appears in output
/// (forward vs backslash, Windows short names like ZHANGQ~1) by also stripping
/// any path prefix that precedes the `$TMP` placeholder after dir-name replacement.
fn replace_tmp_path(text: &str, tmp: &str) -> String {
    // Try exact full-path replacement first (handles matching separators).
    let mut s = text.replace(tmp, "$TMP");
    // Also try with forward slashes (output often uses / on Windows).
    let tmp_fwd = tmp.replace('\\', "/");
    s = s.replace(&tmp_fwd, "$TMP");

    // Replace just the temp directory name (final component).
    if let Some(dir_name) = Path::new(tmp).file_name().and_then(|n| n.to_str()) {
        s = s.replace(dir_name, "$TMP");
    }

    // Collapse any prefix before $TMP to just $TMP.
    while let Some(idx) = s.find("$TMP") {
        if idx == 0 {
            break;
        }
        // Find the start of this path token (look backward for quote or space).
        let prefix = &s[..idx];
        let boundary = prefix
            .rfind(['"', '\'', '=', ' ', '\n'])
            .map(|i| i + 1)
            .unwrap_or(0);
        if boundary < idx {
            s = format!("{}{}", &s[..boundary], &s[idx..]);
        } else {
            break;
        }
    }

    s
}

/// Prepare actual output for saving: replace temp paths and ensure trailing newline.
fn replace_tmp_and_finalize(text: &str, tmp: &str) -> String {
    let mut result = replace_tmp_path(text, tmp);
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
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
