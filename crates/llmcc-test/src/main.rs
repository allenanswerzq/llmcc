//! Corpus test runner for llmcc.

mod corpus;
mod pipeline;
mod runner;

use std::path::PathBuf;
use std::process;

use clap::Parser;

#[derive(Parser)]
#[command(name = "llmcc-test", about = "Corpus test runner for llmcc")]
struct Cli {
    /// Root directory containing `.llmcc` corpus files.
    #[arg(long, default_value = "tests")]
    root: PathBuf,

    /// Update expectation sections with current output (bless).
    #[arg(long)]
    update: bool,

    /// Only run cases whose id contains this substring.
    #[arg(value_name = "FILTER")]
    filter: Option<String>,
}

fn main() {
    let cli = Cli::parse();

    let mut corpus = match corpus::Corpus::load(&cli.root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    };

    let summary = match runner::run(&mut corpus, cli.filter.as_deref(), cli.update) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
    };

    if cli.update {
        if let Err(e) = corpus.write_updates() {
            eprintln!("error writing updates: {e}");
            process::exit(1);
        }
    }

    println!("\n{summary}");

    if !summary.is_ok() {
        process::exit(1);
    }
}
