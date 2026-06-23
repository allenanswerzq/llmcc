//! Execute a benchmark task: clone repo, run llmcc, run codex, collect metrics.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::task::Task;

/// Benchmark execution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Baseline,
    WithLlmcc,
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Baseline => write!(f, "baseline"),
            Mode::WithLlmcc => write!(f, "llmcc"),
        }
    }
}

/// Metrics collected from a single task run.
#[derive(Debug, Clone)]
pub struct RunResult {
    pub task_id: String,
    pub mode: Mode,
    pub input_tokens_k: f64,
    pub output_tokens_k: f64,
    pub tool_calls: u32,
    pub wall_time_s: f64,
    pub artifact_dir: PathBuf,
}

/// Repository checkout shared by all task runs for one task file.
pub struct RepoCheckout {
    path: PathBuf,
}

impl RepoCheckout {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Clone a repository once for a benchmark run.
pub fn checkout_repo(repo: &str, repo_root: &Path) -> RepoCheckout {
    fs::create_dir_all(repo_root).unwrap();
    let path = repo_root.join(repo_dir_name(repo));

    if path.join(".git").exists() {
        println!("Reusing repo: {}", path.display());
    } else {
        println!("Cloning repo: {repo}");
        clone_repo(repo, &path);
    }

    RepoCheckout { path }
}

/// Clone a repo into the given directory.
fn clone_repo(repo: &str, dest: &Path) {
    let status = Command::new("git")
        .args(["clone", "--depth", "1", repo])
        .arg(dest)
        .status()
        .unwrap();
    assert!(status.success(), "git clone failed for {repo}");
}

fn repo_dir_name(repo: &str) -> String {
    repo.trim_end_matches(".git")
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .filter(|name| !name.is_empty())
        .map(sanitize_path_component)
        .unwrap_or_else(|| "repo".into())
}

/// Run llmcc against a directory and return the architecture graph output.
fn run_llmcc(dir: &Path) -> String {
    let mut command = llmcc_command();
    let output = command
        .args(["--dir"])
        .arg(dir)
        .args(["--ai", "true"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "llmcc failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn llmcc_command() -> Command {
    Command::new(release_llmcc_binary())
}

fn release_llmcc_binary() -> PathBuf {
    let binary = target_dir(&workspace_root())
        .join("release")
        .join(if cfg!(windows) { "llmcc.exe" } else { "llmcc" });

    assert!(
        binary.exists(),
        "release llmcc binary not found at {}; run `cargo build --release -p llmcc` before benchmarking",
        binary.display()
    );

    binary
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn target_dir(root: &Path) -> PathBuf {
    std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("target"))
}

/// Run codex exec with the given prompt against the repo directory.
fn run_codex(prompt: &str, work_dir: &Path) -> CodexOutput {
    let mut child = Command::new(codex_command())
        .args(["exec", "--json", "--full-auto", "-C"])
        .arg(work_dir)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(prompt.as_bytes())
        .unwrap();

    let output = child.wait_with_output().unwrap();
    CodexOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    }
}

struct CodexOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

fn codex_command() -> &'static str {
    if cfg!(windows) { "codex.cmd" } else { "codex" }
}

/// Parse JSONL codex output to extract token usage and tool call count.
fn parse_metrics(jsonl: &str) -> (u64, u64, u32) {
    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut tool_calls: u32 = 0;

    for line in jsonl.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        // Count tool calls from item.completed events with tool use
        if event.get("type").and_then(|t| t.as_str()) == Some("item.completed") {
            if let Some(item) = event.get("item") {
                if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    tool_calls += 1;
                }
            }
        }

        // Sum token usage from turn.completed events
        if event.get("type").and_then(|t| t.as_str()) == Some("turn.completed") {
            if let Some(usage) = event.get("usage") {
                input_tokens += usage
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                output_tokens += usage
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            }
        }
    }

    (input_tokens, output_tokens, tool_calls)
}

/// Execute a single task in the given mode and return metrics.
pub fn run_task(task: &Task, mode: Mode, repo_dir: &Path, artifact_root: &Path) -> RunResult {
    let artifact_dir = artifact_root
        .join(sanitize_path_component(&task.id))
        .join(mode.to_string());
    fs::create_dir_all(&artifact_dir).unwrap();

    // Build prompt.
    let mut graph = None;
    let prompt = match mode {
        Mode::Baseline => task.description.clone(),
        Mode::WithLlmcc => {
            let rendered = run_llmcc(repo_dir);
            write_artifact(&artifact_dir.join("llmcc.dot"), &rendered);
            graph = Some(rendered.clone());
            format!(
                "Use this architecture graph as navigation context:\n\n{rendered}\n\nTask:\n{}",
                task.description
            )
        }
    };
    write_artifact(&artifact_dir.join("prompt.txt"), &prompt);

    // Run codex and time it.
    let start = Instant::now();
    let codex = run_codex(&prompt, repo_dir);
    let wall_time_s = start.elapsed().as_secs_f64();
    write_artifact(&artifact_dir.join("codex.jsonl"), &codex.stdout);
    write_artifact(&artifact_dir.join("codex.stderr"), &codex.stderr);
    assert!(codex.success, "codex exec failed: {}", codex.stderr);

    // Parse metrics.
    let (input_tokens, output_tokens, tool_calls) = parse_metrics(&codex.stdout);

    write_artifact(
        &artifact_dir.join("metadata.toml"),
        &format!(
            "task_id = {task_id:?}\nrepo = {repo:?}\nmode = {mode:?}\ninput_tokens_k = {input:.1}\noutput_tokens_k = {output:.1}\ntool_calls = {tools}\nwall_time_s = {time:.1}\ngraph_bytes = {graph_bytes}\n",
            task_id = task.id,
            repo = task.repo,
            mode = mode.to_string(),
            input = input_tokens as f64 / 1000.0,
            output = output_tokens as f64 / 1000.0,
            tools = tool_calls,
            time = wall_time_s,
            graph_bytes = graph.as_ref().map_or(0, |graph| graph.len()),
        ),
    );

    RunResult {
        task_id: task.id.clone(),
        mode,
        input_tokens_k: input_tokens as f64 / 1000.0,
        output_tokens_k: output_tokens as f64 / 1000.0,
        tool_calls,
        wall_time_s,
        artifact_dir,
    }
}

fn write_artifact(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
}

fn sanitize_path_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}
