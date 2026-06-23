//! llmcc-bench: measure how llmcc architecture graphs reduce agent effort.

mod report;
mod runner;
mod task;

use std::fs;
use std::path::{Path, PathBuf};

use clap::Parser;

use runner::Mode;

#[derive(Parser)]
#[command(
    name = "llmcc-bench",
    about = "Benchmark llmcc architecture context vs baseline"
)]
struct Cli {
    /// Path to a tasks TOML file.
    #[arg(long, short)]
    tasks: PathBuf,

    /// Run mode: baseline, llmcc, or both.
    #[arg(long, default_value = "both")]
    mode: String,

    /// Run only one task by id.
    #[arg(long = "task-id")]
    task_id: Option<String>,

    /// Output CSV file path.
    #[arg(long, short)]
    output: Option<PathBuf>,

    /// Directory where prompts, graphs, Codex output, and metadata are stored.
    #[arg(long)]
    artifacts: Option<PathBuf>,

    /// Directory where benchmark repositories are cloned and reused.
    #[arg(long = "repo-root")]
    repo_root: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    let mut tasks = task::load(&cli.tasks);

    if let Some(task_id) = &cli.task_id {
        tasks.retain(|task| task.id == *task_id);
        if tasks.is_empty() {
            eprintln!("task id not found: {task_id}");
            std::process::exit(1);
        }
    }

    let modes: Vec<Mode> = match cli.mode.as_str() {
        "baseline" => vec![Mode::Baseline],
        "llmcc" => vec![Mode::WithLlmcc],
        _ => vec![Mode::Baseline, Mode::WithLlmcc],
    };

    let artifact_root = match cli.artifacts {
        Some(path) => path,
        None => {
            let path = default_artifact_root(&cli.tasks);
            if path.exists() {
                fs::remove_dir_all(&path).unwrap();
            }
            path
        }
    };
    fs::create_dir_all(&artifact_root).unwrap();
    println!("Artifacts: {}", artifact_root.display());

    let repo = tasks.first().unwrap().repo.clone();
    assert!(
        tasks.iter().all(|task| task.repo == repo),
        "all tasks in one task file must use the same repo"
    );
    let repo_root = cli.repo_root.unwrap_or_else(default_repo_root);
    println!("Repo root: {}", repo_root.display());
    let checkout = runner::checkout_repo(&repo, &repo_root);
    println!("Checkout: {}", checkout.path().display());
    println!("Checking Codex tool execution...");
    runner::verify_codex_tool_execution(checkout.path());

    let mut results = Vec::new();

    for task in &tasks {
        for &mode in &modes {
            println!("▶ {} [{}]", task.id, mode);
            let result = runner::run_task(task, mode, checkout.path(), &artifact_root);
            println!(
                "  in={:.1}k cached={:.1}k out={:.1}k tools={} time={:.1}s",
                result.input_tokens as f64 / 1000.0,
                result.cached_input_tokens as f64 / 1000.0,
                result.output_tokens as f64 / 1000.0,
                result.tool_calls,
                result.wall_time_s,
            );
            results.push(result);
        }
    }

    println!();
    report::print_detail(&results);
    report::print_summary(&results);

    if let Some(csv_path) = cli.output {
        report::write_csv(&results, &csv_path);
    }
}

fn default_artifact_root(tasks_path: &Path) -> PathBuf {
    let task_file = tasks_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("tasks");
    workspace_root()
        .join("target")
        .join("llmcc-bench-artifacts")
        .join(task_file)
}

fn default_repo_root() -> PathBuf {
    workspace_root().join("target").join("llmcc-bench-repos")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}
