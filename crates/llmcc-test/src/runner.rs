use llmcc_core::ViewDepth;
use llmcc_core::block::reset_block_id_counter;
use llmcc_core::symbol::reset_symbol_id_counter;
use llmcc_error::{Error, ErrorKind, Result};

use crate::corpus::{Corpus, CorpusCase, CorpusFile};
use crate::expectation::{ensure_trailing_newline, format_expectation_diff, normalize};
use crate::pipeline::{build_pipeline_summary, render_expectation};
use crate::{GraphOptions, ProcessingOptions};

pub use crate::options::{
    GraphOptions as SharedGraphOptions, ProcessingOptions as SharedProcessingOptions,
};
pub use crate::pipeline::PipelineOptions;

#[derive(Debug, Clone)]
pub struct RunnerConfig {
    pub filter: Option<String>,
    pub update: bool,
    pub keep_temps: bool,
    /// Graph building and visualization options.
    pub graph: GraphOptions,
    /// Processing behavior options.
    pub processing: ProcessingOptions,
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
            config.processing.parallel,
            config.processing.print_ir,
            config.graph.view_depth(),
            config.graph.top_k,
        )?);
    }

    if matched == 0 {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            format!("no llmcc-test cases matched filter {:?}", config.filter),
        ));
    }

    Ok(outcomes)
}

pub fn run_cases_for_file(
    file: &mut CorpusFile,
    update: bool,
    keep_temps: bool,
) -> Result<Vec<CaseOutcome>> {
    run_cases_for_file_with_parallel(file, update, keep_temps, false, true, ViewDepth::File, None)
}

pub fn run_cases_for_file_with_parallel(
    file: &mut CorpusFile,
    update: bool,
    keep_temps: bool,
    parallel: bool,
    print_ir: bool,
    view_depth: ViewDepth,
    top_k: Option<usize>,
) -> Result<Vec<CaseOutcome>> {
    let mut matched = 0usize;
    run_cases_in_file(
        file,
        update,
        None,
        &mut matched,
        keep_temps,
        parallel,
        print_ir,
        view_depth,
        top_k,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_cases_in_file(
    file: &mut CorpusFile,
    update: bool,
    filter: Option<&str>,
    matched: &mut usize,
    keep_temps: bool,
    parallel: bool,
    print_ir: bool,
    view_depth: ViewDepth,
    top_k: Option<usize>,
) -> Result<Vec<CaseOutcome>> {
    let mut file_outcomes = Vec::new();
    let mut mutated_file = false;
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
        print!("  {case_name} ... ");
        use std::io::Write;
        let _ = std::io::stdout().flush();

        let (outcome, mutated) = {
            let case = &mut file.cases[idx];
            evaluate_case(
                case, update, keep_temps, parallel, print_ir, view_depth, top_k,
            )?
        };

        match outcome.status {
            CaseStatus::Passed => println!("ok"),
            CaseStatus::Updated => println!("updated"),
            CaseStatus::Failed => {
                println!("FAILED");
                if let Some(message) = &outcome.message {
                    for line in message.lines() {
                        println!("        {line}");
                    }
                }
            }
            CaseStatus::NoExpectations => println!("skipped (no expectations)"),
        }

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
    parallel: bool,
    print_ir: bool,
    view_depth: ViewDepth,
    top_k: Option<usize>,
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
    let summary = build_pipeline_summary(case, keep_temps, parallel, print_ir, view_depth, top_k)?;
    let mut mutated = false;
    let mut status = CaseStatus::Passed;
    let mut failures = Vec::new();

    let temp_dir_path = summary.temp_dir_path();
    for expect in &mut case.expectations {
        let kind = expect.kind.as_str();
        let actual = render_expectation(kind, &summary, &case_id)?;
        let expected_norm = normalize(kind, &expect.value, None);
        let actual_norm = normalize(kind, &actual, temp_dir_path);

        if expected_norm == actual_norm {
            continue;
        }

        if update {
            let actual_to_save = if let Some(tmp_path) = temp_dir_path {
                let mut result = actual.replace(tmp_path, "$TMP");
                if let Some(dir_name) = std::path::Path::new(tmp_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                {
                    result = result.replace(dir_name, "$TMP");
                }
                result
            } else {
                actual.clone()
            };
            expect.value = ensure_trailing_newline(actual_to_save);
            mutated = true;
            status = CaseStatus::Updated;
        } else {
            status = CaseStatus::Failed;
            failures.push(format_expectation_diff(
                &expect.kind,
                &expected_norm,
                &actual_norm,
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
