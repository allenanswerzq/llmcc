use std::path::PathBuf;

use clap::{Parser, Subcommand};
use llmcc_error::{Error, ErrorKind, Result};

use llmcc_test::{
    CaseOutcome, CaseStatus, Corpus, GraphOptions, ProcessingOptions, RunnerConfig, run_cases,
    run_cases_for_file_with_parallel,
};

#[derive(Parser, Debug)]
#[command(name = "llmcc-test", about = "Corpus runner for llmcc", version)]
struct Cli {
    /// Root directory containing `.llmcc` corpus files
    #[arg(long, value_name = "DIR", default_value = "tests")]
    root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run every case contained in a single corpus file
    Run {
        /// Path to the `.llmcc` file (relative to --root or absolute)
        #[arg(value_name = "FILE")]
        file: PathBuf,
        /// Update expectation sections with current output (bless)
        #[arg(long)]
        update: bool,
        /// Keep the temporary project directory for inspection
        #[arg(long = "keep-temps")]
        keep_temps: bool,
        #[command(flatten)]
        graph: GraphOptions,
        #[command(flatten)]
        processing: ProcessingOptions,
    },
    /// Run the entire corpus (optionally filtered by case id or directory)
    RunAll {
        /// Only run cases whose id contains this substring
        #[arg(long)]
        filter: Option<String>,
        /// Optional directory or filter string - if a directory, run all tests in it
        #[arg(value_name = "DIR_OR_FILTER", required = false)]
        dir_or_filter: Option<PathBuf>,
        /// Update expectation sections with current output (bless)
        #[arg(
            long,
            value_name = "UPDATE_FILTER",
            num_args = 0..=1,
            default_missing_value = ""
        )]
        update: Option<String>,
        /// Keep the temporary project directory for inspection
        #[arg(long = "keep-temps")]
        keep_temps: bool,
        #[command(flatten)]
        graph: GraphOptions,
        #[command(flatten)]
        processing: ProcessingOptions,
    },
    /// List available cases (optionally filtering by substring)
    List {
        #[arg(long)]
        filter: Option<String>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            file,
            update,
            keep_temps,
            graph,
            processing,
        } => run_single_command(cli.root, file, update, keep_temps, graph, processing),
        Command::RunAll {
            filter,
            dir_or_filter,
            update,
            keep_temps,
            graph,
            processing,
        } => {
            let (should_update, update_filter) = match update {
                Some(value) if value.is_empty() => (true, None),
                Some(value) => (true, Some(value)),
                None => (false, None),
            };

            // Determine if dir_or_filter is a directory or a filter string
            let (effective_root, effective_filter) = if let Some(ref path) = dir_or_filter {
                if path.is_dir() {
                    // It's a directory - use it as root, no filter
                    (path.clone(), filter.or(update_filter))
                } else {
                    // It's a filter string
                    (
                        cli.root,
                        filter
                            .or(path.to_string_lossy().to_string().into())
                            .or(update_filter),
                    )
                }
            } else {
                (cli.root, filter.or(update_filter))
            };

            run_all_command(
                effective_root,
                effective_filter,
                should_update,
                keep_temps,
                graph,
                processing,
            )
        }
        Command::List { filter } => list_command(cli.root, filter),
    }
}

fn run_all_command(
    root: PathBuf,
    filter: Option<String>,
    update: bool,
    keep_temps: bool,
    graph: GraphOptions,
    processing: ProcessingOptions,
) -> Result<()> {
    let mut corpus = Corpus::load(&root)?;
    let outcomes = run_cases(
        &mut corpus,
        RunnerConfig {
            filter: filter.clone(),
            update,
            keep_temps,
            graph,
            processing,
        },
    )?;

    // Results are already printed inline by the runner
    let summary = count_outcomes(&outcomes);

    if update {
        corpus.write_updates()?;
    }

    print_summary(&summary);
    print_failed_tests(&outcomes);

    Ok(())
}

fn run_single_command(
    root: PathBuf,
    file: PathBuf,
    update: bool,
    keep_temps: bool,
    graph: GraphOptions,
    processing: ProcessingOptions,
) -> Result<()> {
    let mut corpus = Corpus::load(&root)?;
    let root_canon = root.canonicalize().map_err(|e| {
        Error::new(
            ErrorKind::FileNotFound,
            format!("failed to resolve root {}: {}", root.display(), e),
        )
    })?;

    let canonical = if file.is_absolute() {
        file.canonicalize().map_err(|e| {
            Error::new(
                ErrorKind::FileNotFound,
                format!("corpus file '{}' not found: {}", file.display(), e),
            )
        })?
    } else {
        match file.canonicalize() {
            Ok(path) => path,
            Err(_) => {
                let joined = root_canon.join(&file);
                joined.canonicalize().map_err(|e| {
                    Error::new(
                        ErrorKind::FileNotFound,
                        format!(
                            "corpus file '{}' (joined with {}) not found: {}",
                            file.display(),
                            root_canon.display(),
                            e
                        ),
                    )
                })?
            }
        }
    };

    let Some(entry) = corpus
        .files_mut()
        .iter_mut()
        .find(|candidate| candidate.path == canonical)
    else {
        return Err(Error::new(
            ErrorKind::FileNotFound,
            format!(
                "file '{}' is not registered under {}",
                file.display(),
                root.display()
            ),
        ));
    };

    let outcomes = run_cases_for_file_with_parallel(
        entry,
        update,
        keep_temps,
        processing.parallel,
        processing.print_ir,
        graph.component_depth(),
        graph.pagerank_top_k,
    )?;
    // Results are already printed inline by the runner
    let summary = count_outcomes(&outcomes);

    if update {
        corpus.write_updates()?;
    }

    print_summary(&summary);
    print_failed_tests(&outcomes);

    Ok(())
}

fn list_command(root: PathBuf, filter: Option<String>) -> Result<()> {
    let corpus = Corpus::load(&root)?;
    let mut count = 0usize;
    for file in corpus.files() {
        for case in &file.cases {
            let id = case.id();
            if let Some(term) = &filter
                && !id.contains(term)
            {
                continue;
            }
            count += 1;
            println!("{id}");
        }
    }
    if count == 0 {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            format!(
                "no llmcc-test cases found{}",
                filter
                    .as_ref()
                    .map(|term| format!(" matching '{term}'"))
                    .unwrap_or_default()
            ),
        ));
    }
    Ok(())
}

#[derive(Default)]
struct OutcomeSummary {
    passed: usize,
    updated: usize,
    failed: usize,
    skipped: usize,
}

fn count_outcomes(outcomes: &[CaseOutcome]) -> OutcomeSummary {
    let mut summary = OutcomeSummary::default();
    for outcome in outcomes {
        match outcome.status {
            CaseStatus::Passed => summary.passed += 1,
            CaseStatus::Updated => summary.updated += 1,
            CaseStatus::Failed => summary.failed += 1,
            CaseStatus::NoExpectations => summary.skipped += 1,
        }
    }
    summary
}

fn print_summary(summary: &OutcomeSummary) {
    println!(
        "\nSummary: {} passed, {} updated, {} failed, {} skipped",
        summary.passed, summary.updated, summary.failed, summary.skipped
    );
}

fn print_failed_tests(outcomes: &[CaseOutcome]) {
    let failed: Vec<_> = outcomes
        .iter()
        .filter(|o| o.status == CaseStatus::Failed)
        .collect();

    if !failed.is_empty() {
        println!("\nFailed tests:");
        for outcome in failed {
            println!("  - {}", outcome.id);
        }
    }
}
