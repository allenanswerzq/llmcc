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
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub tool_calls: u32,
    pub wall_time_s: f64,
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
        .args(["exec", "--json", "--sandbox", "danger-full-access", "-C"])
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

/// Verify that Codex can execute native shell tools before collecting benchmark metrics.
pub fn verify_codex_tool_execution(work_dir: &Path) {
    let marker_name: String = format!(".llmcc-bench-codex-tool-probe-{}.txt", std::process::id());
    let marker_path = work_dir.join(&marker_name);
    let _ = fs::remove_file(&marker_path);

    let command_hint = if cfg!(windows) {
        format!(r#"powershell.exe -Command "Set-Content -Path '{marker_name}' -Value ok""#)
    } else {
        format!("sh -c 'printf ok > {marker_name}'")
    };
    let prompt = format!(
        "This is an llmcc-bench preflight. Use the shell tool to create a file named `{marker_name}` in the current directory. A suitable command is `{command_hint}`. After the command runs, answer briefly."
    );

    let codex = run_codex(&prompt, work_dir);
    let metrics = parse_metrics(&codex.stdout);
    let marker_created = marker_path.exists();
    let _ = fs::remove_file(&marker_path);

    assert!(
        codex.success,
        "Codex tool preflight failed: {}",
        codex.stderr
    );
    assert!(
        marker_created && metrics.tool_calls > 0,
        "Codex tool preflight did not execute a native shell tool. marker_created={marker_created} tool_calls={}\nstdout excerpt:\n{}\nstderr excerpt:\n{}",
        metrics.tool_calls,
        excerpt(&codex.stdout),
        excerpt(&codex.stderr)
    );
}

#[derive(Debug, Clone, Copy, Default)]
struct Metrics {
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    tool_calls: u32,
}

/// Parse JSONL codex output to extract token usage and tool call count.
fn parse_metrics(jsonl: &str) -> Metrics {
    let mut metrics = Metrics::default();

    for line in jsonl.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        // Count tool calls from item.completed events.
        if event.get("type").and_then(|t| t.as_str()) == Some("item.completed") {
            if let Some(item) = event.get("item") {
                if item
                    .get("type")
                    .and_then(|kind| kind.as_str())
                    .is_some_and(is_tool_item)
                {
                    metrics.tool_calls += 1;
                }
            }
        }

        // Sum token usage from turn.completed events
        if event.get("type").and_then(|t| t.as_str()) == Some("turn.completed") {
            if let Some(usage) = event.get("usage") {
                metrics.input_tokens += usage
                    .get("input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                metrics.cached_input_tokens += usage
                    .get("cached_input_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                metrics.output_tokens += usage
                    .get("output_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
            }
        }
    }

    metrics
}

fn is_tool_item(kind: &str) -> bool {
    matches!(kind, "tool_use" | "command_execution") || kind.contains("tool_call")
}

fn excerpt(text: &str) -> String {
    text.chars().take(1200).collect()
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
    write_artifact(
        &artifact_dir.join("codex.json"),
        &pretty_codex_jsonl(&codex.stdout),
    );
    write_artifact(
        &artifact_dir.join("codex.txt"),
        &readable_codex_jsonl(&codex.stdout),
    );
    write_artifact(
        &artifact_dir.join("tools.txt"),
        &tool_transcript(&codex.stdout),
    );
    write_artifact(&artifact_dir.join("codex.stderr"), &codex.stderr);
    assert!(codex.success, "codex exec failed: {}", codex.stderr);

    // Parse metrics.
    let metrics = parse_metrics(&codex.stdout);

    write_artifact(
        &artifact_dir.join("metadata.toml"),
        &format!(
            "task_id = {task_id:?}\nrepo = {repo:?}\nmode = {mode:?}\ninput_tokens = {input}\ncached_input_tokens = {cached_input}\noutput_tokens = {output}\ninput_tokens_k = {input_k:.1}\ncached_input_tokens_k = {cached_input_k:.1}\noutput_tokens_k = {output_k:.1}\ntool_calls = {tools}\nwall_time_s = {time:.1}\ngraph_bytes = {graph_bytes}\n",
            task_id = task.id,
            repo = task.repo,
            mode = mode.to_string(),
            input = metrics.input_tokens,
            cached_input = metrics.cached_input_tokens,
            output = metrics.output_tokens,
            input_k = metrics.input_tokens as f64 / 1000.0,
            cached_input_k = metrics.cached_input_tokens as f64 / 1000.0,
            output_k = metrics.output_tokens as f64 / 1000.0,
            tools = metrics.tool_calls,
            time = wall_time_s,
            graph_bytes = graph.as_ref().map_or(0, |graph| graph.len()),
        ),
    );

    RunResult {
        task_id: task.id.clone(),
        mode,
        input_tokens: metrics.input_tokens,
        cached_input_tokens: metrics.cached_input_tokens,
        output_tokens: metrics.output_tokens,
        tool_calls: metrics.tool_calls,
        wall_time_s,
    }
}

fn write_artifact(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
}

fn tool_transcript(jsonl: &str) -> String {
    let mut out = String::new();
    let mut count = 0;

    for line in jsonl.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if event.get("type").and_then(|t| t.as_str()) != Some("item.completed") {
            continue;
        }
        let Some(item) = event.get("item") else {
            continue;
        };
        let Some(item_type) = item.get("type").and_then(|kind| kind.as_str()) else {
            continue;
        };

        if is_tool_item(item_type) {
            count += 1;
            out.push_str(&format!(
                "## {count}. {item_type}\n\n{}\n\n",
                serde_json::to_string_pretty(item).unwrap()
            ));
        }
    }

    if out.is_empty() {
        out.push_str("No tool calls captured.\n");
    }

    out
}

fn pretty_codex_jsonl(jsonl: &str) -> String {
    let events: Vec<serde_json::Value> = jsonl
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut text = serde_json::to_string_pretty(&events).unwrap();
    text.push('\n');
    text
}

fn readable_codex_jsonl(jsonl: &str) -> String {
    let mut out = String::new();

    for (idx, line) in jsonl.lines().enumerate() {
        let event = serde_json::from_str::<serde_json::Value>(line).unwrap();
        if idx > 0 {
            out.push_str("\n\n");
        }
        write_readable_value(&mut out, "", &event);
    }

    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn write_readable_value(out: &mut String, prefix: &str, value: &serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                let key = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                write_readable_value(out, &key, value);
            }
        }
        serde_json::Value::Array(items) => {
            for (idx, value) in items.iter().enumerate() {
                write_readable_value(out, &format!("{prefix}[{idx}]"), value);
            }
        }
        serde_json::Value::String(text) if text.contains('\n') => {
            out.push_str(prefix);
            out.push_str(":\n");
            out.push_str(text.trim_matches('\n'));
            out.push('\n');
        }
        serde_json::Value::String(text) => {
            out.push_str(prefix);
            out.push_str(": ");
            out.push_str(text);
            out.push('\n');
        }
        other => {
            out.push_str(prefix);
            out.push_str(": ");
            out.push_str(&other.to_string());
            out.push('\n');
        }
    }
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
