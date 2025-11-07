use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use llmcc_test::{run_cases, CaseStatus, Corpus, RunnerConfig};

#[derive(Parser, Debug)]
#[command(name = "llmcc-test", about = "Corpus runner for llmcc", version)]
struct Cli {
    /// Root directory containing `.llmcc` corpus files
    #[arg(long, value_name = "DIR", default_value = "tests/corpus")]
    root: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run the corpus expectations
    Run {
        /// Only run cases whose id contains this substring
        #[arg(long)]
        filter: Option<String>,
        /// Update expectation sections with current output (bless)
        #[arg(long)]
        update: bool,
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
        Command::Run { filter, update } => run_command(cli.root, filter, update),
        Command::List { filter } => list_command(cli.root, filter),
    }
}

fn run_command(root: PathBuf, filter: Option<String>, update: bool) -> Result<()> {
    let mut corpus = Corpus::load(&root)?;
    let outcomes = run_cases(
        &mut corpus,
        RunnerConfig {
            filter: filter.clone(),
            update,
        },
    )?;

    let mut passed = 0usize;
    let mut updated_cases = 0usize;
    let mut failed = 0usize;
    let mut no_expect = 0usize;

    for outcome in &outcomes {
        match outcome.status {
            CaseStatus::Passed => {
                passed += 1;
                println!("[PASS] {}", outcome.id);
            }
            CaseStatus::Updated => {
                updated_cases += 1;
                println!("[UPD ] {}", outcome.id);
            }
            CaseStatus::Failed => {
                failed += 1;
                println!("[FAIL] {}", outcome.id);
                if let Some(message) = &outcome.message {
                    for line in message.lines() {
                        println!("        {line}");
                    }
                }
            }
            CaseStatus::NoExpectations => {
                no_expect += 1;
                println!("[SKIP] {} (no expectations)", outcome.id);
            }
        }
    }

    if update {
        corpus.write_updates()?;
    }

    println!(
        "\nSummary: {passed} passed, {updated_cases} updated, {failed} failed, {no_expect} skipped"
    );

    if failed > 0 {
        anyhow::bail!("{} case(s) failed", failed);
    }

    Ok(())
}

fn list_command(root: PathBuf, filter: Option<String>) -> Result<()> {
    let corpus = Corpus::load(&root)?;
    let mut count = 0usize;
    for file in corpus.files() {
        for case in &file.cases {
            let id = case.id();
            if let Some(term) = &filter {
                if !id.contains(term) {
                    continue;
                }
            }
            count += 1;
            println!("{id}");
        }
    }
    if count == 0 {
        anyhow::bail!(
            "no llmcc-test cases found{}",
            filter
                .as_ref()
                .map(|term| format!(" matching '{term}'"))
                .unwrap_or_default()
        );
    }
    Ok(())
}
