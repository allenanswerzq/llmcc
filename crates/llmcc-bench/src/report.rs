//! Report benchmark results as a table and optional CSV.

use std::fs;
use std::path::Path;

use crate::runner::{Mode, RunResult};

/// Print per-task detail table.
pub fn print_detail(results: &[RunResult]) {
    println!(
        "{:<32} | {:<8} | {:>8} | {:>8} | {:>8} | {:>5} | {:>8}",
        "task_id", "mode", "in (k)", "cached", "out (k)", "tools", "time_s"
    );
    println!("{}", "-".repeat(94));

    for r in results {
        println!(
            "{:<32} | {:<8} | {:>8.1} | {:>8.1} | {:>8.1} | {:>5} | {:>8.1}",
            r.task_id,
            r.mode,
            r.input_tokens as f64 / 1000.0,
            r.cached_input_tokens as f64 / 1000.0,
            r.output_tokens as f64 / 1000.0,
            r.tool_calls,
            r.wall_time_s,
        );
    }
}

/// Print aggregate summary comparing baseline vs llmcc modes.
pub fn print_summary(results: &[RunResult]) {
    let baseline: Vec<_> = results
        .iter()
        .filter(|r| r.mode == Mode::Baseline)
        .collect();
    let llmcc: Vec<_> = results
        .iter()
        .filter(|r| r.mode == Mode::WithLlmcc)
        .collect();

    if baseline.is_empty() || llmcc.is_empty() {
        return;
    }

    let avg = |items: &[&RunResult], f: fn(&RunResult) -> f64| -> f64 {
        items.iter().map(|r| f(r)).sum::<f64>() / items.len() as f64
    };

    let b_in = avg(&baseline, |r| r.input_tokens as f64 / 1000.0);
    let b_cached = avg(&baseline, |r| r.cached_input_tokens as f64 / 1000.0);
    let b_out = avg(&baseline, |r| r.output_tokens as f64 / 1000.0);
    let b_tools = avg(&baseline, |r| r.tool_calls as f64);
    let b_time = avg(&baseline, |r| r.wall_time_s);

    let l_in = avg(&llmcc, |r| r.input_tokens as f64 / 1000.0);
    let l_cached = avg(&llmcc, |r| r.cached_input_tokens as f64 / 1000.0);
    let l_out = avg(&llmcc, |r| r.output_tokens as f64 / 1000.0);
    let l_tools = avg(&llmcc, |r| r.tool_calls as f64);
    let l_time = avg(&llmcc, |r| r.wall_time_s);

    let delta = |base: f64, with: f64| -> String {
        if base == 0.0 {
            return "n/a".into();
        }
        let pct = ((with - base) / base) * 100.0;
        format!("{pct:+.0}%")
    };

    println!();
    println!("Summary ({} tasks)", baseline.len());
    println!("{}", "-".repeat(64));
    println!(
        "{:<16} | {:>12} | {:>12} | {:>8}",
        "metric", "baseline", "llmcc", "delta"
    );
    println!("{}", "-".repeat(64));
    println!(
        "{:<16} | {:>10.1} k | {:>10.1} k | {:>8}",
        "input_tokens",
        b_in,
        l_in,
        delta(b_in, l_in)
    );
    println!(
        "{:<16} | {:>10.1} k | {:>10.1} k | {:>8}",
        "cached_tokens",
        b_cached,
        l_cached,
        delta(b_cached, l_cached)
    );
    println!(
        "{:<16} | {:>10.1} k | {:>10.1} k | {:>8}",
        "output_tokens",
        b_out,
        l_out,
        delta(b_out, l_out)
    );
    println!(
        "{:<16} | {:>12.1} | {:>12.1} | {:>8}",
        "tool_calls",
        b_tools,
        l_tools,
        delta(b_tools, l_tools)
    );
    println!(
        "{:<16} | {:>10.1} s | {:>10.1} s | {:>8}",
        "wall_time",
        b_time,
        l_time,
        delta(b_time, l_time)
    );
}

/// Write results to a CSV file.
pub fn write_csv(results: &[RunResult], path: &Path) {
    let mut lines = Vec::with_capacity(results.len() + 1);
    lines.push(
        "task_id,mode,input_tokens_k,cached_input_tokens_k,output_tokens_k,tool_calls,wall_time_s"
            .into(),
    );
    for r in results {
        lines.push(format!(
            "{},{},{:.1},{:.1},{:.1},{},{:.1}",
            csv_escape(&r.task_id),
            r.mode,
            r.input_tokens as f64 / 1000.0,
            r.cached_input_tokens as f64 / 1000.0,
            r.output_tokens as f64 / 1000.0,
            r.tool_calls,
            r.wall_time_s,
        ));
    }
    fs::write(path, lines.join("\n")).unwrap();
    println!("\nResults written to {}", path.display());
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
