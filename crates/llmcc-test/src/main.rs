use std::path::PathBuf;

use clap::{Parser, Subcommand};
use llmcc_error::{Error, ErrorKind, Result};
use llmcc_test::{CaseOutcome, CaseStatus, RunOptions, load_suite_files, run_path};
use strum_macros::{Display, IntoStaticStr};

#[derive(Parser, Debug)]
#[command(
    name = "llmcc-test",
    about = "JSON-backed corpus runner for llmcc",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug, Display, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
enum Command {
    /// Run one JSON suite file or every JSON suite under a directory.
    Run {
        #[arg(value_name = "PATH", default_value = "tests/json")]
        path: PathBuf,
        /// Only run cases whose id contains this substring.
        #[arg(long)]
        filter: Option<String>,
        /// Bless expected graph documents with current output.
        #[arg(long)]
        update: bool,
        /// Keep materialized temp projects on disk.
        #[arg(long = "keep-temps")]
        keep_temps: bool,
        /// Build HIR and graphs in parallel.
        #[arg(long)]
        parallel: bool,
        /// Print IR during symbol resolution.
        #[arg(long = "print-ir")]
        print_ir: bool,
    },
    /// List cases in one JSON suite file or directory.
    List {
        #[arg(value_name = "PATH", default_value = "tests/json")]
        path: PathBuf,
        #[arg(long)]
        filter: Option<String>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Run {
            path,
            filter,
            update,
            keep_temps,
            parallel,
            print_ir,
        } => run_command(
            path,
            RunOptions {
                filter,
                update,
                keep_temps,
                parallel,
                print_ir,
            },
        ),
        Command::List { path, filter } => list_command(path, filter),
    }
}

fn run_command(path: PathBuf, options: RunOptions) -> Result<()> {
    let report = run_path(&path, options)?;

    for outcome in &report.outcomes {
        print_outcome(outcome);
    }

    println!(
        "\nSummary: {} passed, {} updated, {} failed",
        report.passed(),
        report.updated(),
        report.failed()
    );

    if report.failed() > 0 {
        return Err(Error::new(
            ErrorKind::AssertionFailed,
            format!("{} llmcc JSON case(s) failed", report.failed()),
        ));
    }

    Ok(())
}

fn print_outcome(outcome: &CaseOutcome) {
    match outcome.status {
        CaseStatus::Passed => println!("  {} ... ok", outcome.id),
        CaseStatus::Updated => println!("  {} ... updated", outcome.id),
        CaseStatus::Failed => {
            println!("  {} ... FAILED", outcome.id);
            if let Some(message) = &outcome.message {
                for line in message.lines() {
                    println!("        {line}");
                }
            }
        }
    }
}

fn list_command(path: PathBuf, filter: Option<String>) -> Result<()> {
    let suites = load_suite_files(&path)?;
    let mut count = 0usize;

    for suite in suites {
        for case in &suite.suite.cases {
            if let Some(term) = &filter
                && !case.id.contains(term)
            {
                continue;
            }

            count += 1;
            println!("{}", case.id);
        }
    }

    if count == 0 {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "no llmcc JSON cases matched the requested path/filter",
        ));
    }

    Ok(())
}
