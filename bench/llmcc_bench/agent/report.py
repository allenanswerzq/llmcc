"""
Report generation for agent benchmark results.

Generates markdown reports and JSON summaries from benchmark runs.
"""

import json
import math
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from .metrics import TaskMetrics, load_metrics_jsonl


@dataclass
class MeanStd:
    """Mean and standard deviation."""
    mean: float
    std: float
    n: int

    def __str__(self) -> str:
        if self.std == 0:
            return f"{self.mean:.1f}"
        return f"{self.mean:.1f} ± {self.std:.1f}"

    @classmethod
    def from_values(cls, values: List[float]) -> "MeanStd":
        if not values:
            return cls(0, 0, 0)
        n = len(values)
        mean = sum(values) / n
        if n < 2:
            std = 0.0
        else:
            variance = sum((x - mean) ** 2 for x in values) / (n - 1)
            std = math.sqrt(variance)
        return cls(mean, std, n)


@dataclass
class TaskSummary:
    """Summary statistics for a single task."""

    task_id: str

    # Baseline stats
    baseline_tool_calls: MeanStd
    baseline_tokens: MeanStd
    baseline_time: MeanStd
    baseline_success_rate: float

    # With llmcc stats
    llmcc_tool_calls: MeanStd
    llmcc_tokens: MeanStd
    llmcc_time: MeanStd
    llmcc_success_rate: float

    # Improvements (negative = reduction = good)
    tool_call_change_pct: float
    token_change_pct: float
    time_change_pct: float
    success_rate_change_pct: float

    # Graph info
    graph_tokens: int = 0
    graph_nodes: int = 0
    graph_edges: int = 0


@dataclass
class ExperimentSummary:
    """Summary of a complete benchmark experiment."""

    repo: str
    timestamp: str
    runs_per_condition: int
    graph_config: str

    # Aggregate stats
    total_tasks: int
    tasks_improved: int  # Tasks where llmcc was better

    # Average improvements
    avg_tool_call_change_pct: float
    avg_token_change_pct: float
    avg_time_change_pct: float
    avg_success_rate_change_pct: float

    # Per-task summaries
    task_summaries: List[TaskSummary] = field(default_factory=list)


def calculate_improvement(baseline: float, treatment: float) -> float:
    """
    Calculate percentage change from baseline to treatment.

    Negative = reduction (good for tools/tokens/time)
    Positive = increase
    """
    if baseline == 0:
        return 0.0
    return ((treatment - baseline) / baseline) * 100


def t_test(group1: List[float], group2: List[float]) -> float:
    """
    Perform independent samples t-test.

    Returns p-value (approximate).
    """
    if len(group1) < 2 or len(group2) < 2:
        return 1.0  # Can't compute

    n1, n2 = len(group1), len(group2)
    mean1 = sum(group1) / n1
    mean2 = sum(group2) / n2

    var1 = sum((x - mean1) ** 2 for x in group1) / (n1 - 1)
    var2 = sum((x - mean2) ** 2 for x in group2) / (n2 - 1)

    # Pooled standard error
    se = math.sqrt(var1 / n1 + var2 / n2)
    if se == 0:
        return 1.0

    t = (mean1 - mean2) / se
    df = n1 + n2 - 2

    # Approximate p-value using normal distribution for large df
    # This is a simplification; for proper stats use scipy
    p = 2 * (1 - _normal_cdf(abs(t)))
    return p


def _normal_cdf(x: float) -> float:
    """Approximate normal CDF using error function approximation."""
    return 0.5 * (1 + math.erf(x / math.sqrt(2)))


def summarize_task(
    task_id: str,
    metrics: List[TaskMetrics],
) -> Optional[TaskSummary]:
    """
    Summarize results for a single task.

    Args:
        task_id: The task identifier.
        metrics: All metrics for this task (both conditions).

    Returns:
        TaskSummary or None if insufficient data.
    """
    # Filter out invalid metrics (negative times, etc.)
    valid_metrics = [m for m in metrics if m.wall_time_seconds >= 0]
    if len(valid_metrics) < len(metrics):
        print(f"  Warning: Filtered out {len(metrics) - len(valid_metrics)} invalid metrics for {task_id}")

    baseline_metrics = [m for m in valid_metrics if m.condition == "baseline"]
    llmcc_metrics = [m for m in valid_metrics if m.condition == "with_llmcc"]

    if not baseline_metrics or not llmcc_metrics:
        return None

    # Extract values
    baseline_tools = [m.tool_calls_total for m in baseline_metrics]
    baseline_tokens = [m.tokens_input + m.tokens_output for m in baseline_metrics]
    baseline_times = [m.wall_time_seconds for m in baseline_metrics]
    baseline_success = sum(1 for m in baseline_metrics if m.task_completed) / len(baseline_metrics)

    llmcc_tools = [m.tool_calls_total for m in llmcc_metrics]
    llmcc_tokens = [m.tokens_input + m.tokens_output for m in llmcc_metrics]
    llmcc_times = [m.wall_time_seconds for m in llmcc_metrics]
    llmcc_success = sum(1 for m in llmcc_metrics if m.task_completed) / len(llmcc_metrics)

    # Calculate means
    baseline_tool_mean = MeanStd.from_values(baseline_tools)
    baseline_token_mean = MeanStd.from_values(baseline_tokens)
    baseline_time_mean = MeanStd.from_values(baseline_times)

    llmcc_tool_mean = MeanStd.from_values(llmcc_tools)
    llmcc_token_mean = MeanStd.from_values(llmcc_tokens)
    llmcc_time_mean = MeanStd.from_values(llmcc_times)

    # Calculate improvements
    tool_change = calculate_improvement(baseline_tool_mean.mean, llmcc_tool_mean.mean)
    token_change = calculate_improvement(baseline_token_mean.mean, llmcc_token_mean.mean)
    time_change = calculate_improvement(baseline_time_mean.mean, llmcc_time_mean.mean)
    success_change = (llmcc_success - baseline_success) * 100

    # Get graph info from first llmcc run
    graph_tokens = llmcc_metrics[0].graph_tokens if llmcc_metrics else 0
    graph_nodes = llmcc_metrics[0].graph_nodes if llmcc_metrics else 0
    graph_edges = llmcc_metrics[0].graph_edges if llmcc_metrics else 0

    return TaskSummary(
        task_id=task_id,
        baseline_tool_calls=baseline_tool_mean,
        baseline_tokens=baseline_token_mean,
        baseline_time=baseline_time_mean,
        baseline_success_rate=baseline_success * 100,
        llmcc_tool_calls=llmcc_tool_mean,
        llmcc_tokens=llmcc_token_mean,
        llmcc_time=llmcc_time_mean,
        llmcc_success_rate=llmcc_success * 100,
        tool_call_change_pct=tool_change,
        token_change_pct=token_change,
        time_change_pct=time_change,
        success_rate_change_pct=success_change,
        graph_tokens=graph_tokens,
        graph_nodes=graph_nodes,
        graph_edges=graph_edges,
    )


def summarize_experiment(
    results_dir: Path,
) -> Optional[ExperimentSummary]:
    """
    Generate summary from benchmark results directory.

    Args:
        results_dir: Directory containing config.json and raw_metrics.jsonl.

    Returns:
        ExperimentSummary or None if data is missing.
    """
    config_path = results_dir / "config.json"
    metrics_path = results_dir / "raw_metrics.jsonl"

    if not config_path.exists() or not metrics_path.exists():
        return None

    # Load config
    with open(config_path) as f:
        config = json.load(f)

    # Load metrics
    all_metrics = load_metrics_jsonl(metrics_path)

    if not all_metrics:
        return None

    # Group by task
    task_metrics: Dict[str, List[TaskMetrics]] = {}
    for m in all_metrics:
        if m.task_id not in task_metrics:
            task_metrics[m.task_id] = []
        task_metrics[m.task_id].append(m)

    # Summarize each task
    task_summaries: List[TaskSummary] = []
    for task_id, metrics in task_metrics.items():
        summary = summarize_task(task_id, metrics)
        if summary:
            task_summaries.append(summary)

    if not task_summaries:
        return None

    # Calculate aggregate stats
    tasks_improved = sum(1 for s in task_summaries if s.tool_call_change_pct < 0)

    avg_tool_change = sum(s.tool_call_change_pct for s in task_summaries) / len(task_summaries)
    avg_token_change = sum(s.token_change_pct for s in task_summaries) / len(task_summaries)
    avg_time_change = sum(s.time_change_pct for s in task_summaries) / len(task_summaries)
    avg_success_change = sum(s.success_rate_change_pct for s in task_summaries) / len(task_summaries)

    return ExperimentSummary(
        repo=config.get("repo", "unknown"),
        timestamp=config.get("timestamp", "unknown"),
        runs_per_condition=config.get("runs_per_condition", 0),
        graph_config=config.get("graph_config", "unknown"),
        total_tasks=len(task_summaries),
        tasks_improved=tasks_improved,
        avg_tool_call_change_pct=avg_tool_change,
        avg_token_change_pct=avg_token_change,
        avg_time_change_pct=avg_time_change,
        avg_success_rate_change_pct=avg_success_change,
        task_summaries=task_summaries,
    )


def format_change(value: float) -> str:
    """Format a percentage change with color indicator."""
    if value < -10:
        return f"**{value:+.1f}%**"  # Bold for significant improvement
    elif value < 0:
        return f"{value:+.1f}%"
    elif value > 10:
        return f"*{value:+.1f}%*"  # Italic for significant regression
    else:
        return f"{value:+.1f}%"


def generate_markdown_report(summary: ExperimentSummary) -> str:
    """Generate a markdown report from experiment summary."""
    lines = [
        f"# llmcc Agent Benchmark Results",
        "",
        f"**Repository:** {summary.repo}  ",
        f"**Date:** {summary.timestamp}  ",
        f"**Runs per condition:** {summary.runs_per_condition}  ",
        f"**Graph config:** {summary.graph_config}",
        "",
        "## Executive Summary",
        "",
        f"- **Tasks evaluated:** {summary.total_tasks}",
        f"- **Tasks improved with llmcc:** {summary.tasks_improved}/{summary.total_tasks} "
        f"({summary.tasks_improved / summary.total_tasks * 100:.0f}%)",
        "",
        "### Average Improvements",
        "",
        "| Metric | Change |",
        "|--------|--------|",
        f"| Tool Calls | {format_change(summary.avg_tool_call_change_pct)} |",
        f"| Tokens | {format_change(summary.avg_token_change_pct)} |",
        f"| Time | {format_change(summary.avg_time_change_pct)} |",
        f"| Success Rate | {format_change(summary.avg_success_rate_change_pct)} |",
        "",
        "> Negative percentages indicate improvement (reduction in tool calls/tokens/time).",
        "",
        "## Per-Task Results",
        "",
        "| Task | Baseline Tools | llmcc Tools | Tool Δ | Baseline Success | llmcc Success |",
        "|------|---------------|-------------|--------|-----------------|---------------|",
    ]

    for task in summary.task_summaries:
        lines.append(
            f"| {task.task_id} | {task.baseline_tool_calls} | {task.llmcc_tool_calls} | "
            f"{format_change(task.tool_call_change_pct)} | {task.baseline_success_rate:.0f}% | "
            f"{task.llmcc_success_rate:.0f}% |"
        )

    lines.extend([
        "",
        "## Detailed Results",
        "",
    ])

    for task in summary.task_summaries:
        lines.extend([
            f"### {task.task_id}",
            "",
            "| Metric | Baseline | With llmcc | Change |",
            "|--------|----------|------------|--------|",
            f"| Tool Calls | {task.baseline_tool_calls} | {task.llmcc_tool_calls} | "
            f"{format_change(task.tool_call_change_pct)} |",
            f"| Tokens | {task.baseline_tokens} | {task.llmcc_tokens} | "
            f"{format_change(task.token_change_pct)} |",
            f"| Time (s) | {task.baseline_time} | {task.llmcc_time} | "
            f"{format_change(task.time_change_pct)} |",
            f"| Success Rate | {task.baseline_success_rate:.0f}% | {task.llmcc_success_rate:.0f}% | "
            f"{format_change(task.success_rate_change_pct)} |",
            "",
            f"Graph: {task.graph_nodes} nodes, {task.graph_edges} edges, ~{task.graph_tokens} tokens",
            "",
        ])

    lines.extend([
        "---",
        "",
        f"*Generated by llmcc-bench on {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}*",
    ])

    return "\n".join(lines)


def generate_json_report(summary: ExperimentSummary) -> str:
    """Generate a JSON report from experiment summary."""
    data = {
        "meta": {
            "repo": summary.repo,
            "timestamp": summary.timestamp,
            "runs_per_condition": summary.runs_per_condition,
            "graph_config": summary.graph_config,
        },
        "summary": {
            "total_tasks": summary.total_tasks,
            "tasks_improved": summary.tasks_improved,
            "avg_tool_call_change_pct": round(summary.avg_tool_call_change_pct, 1),
            "avg_token_change_pct": round(summary.avg_token_change_pct, 1),
            "avg_time_change_pct": round(summary.avg_time_change_pct, 1),
            "avg_success_rate_change_pct": round(summary.avg_success_rate_change_pct, 1),
        },
        "tasks": [
            {
                "task_id": t.task_id,
                "baseline": {
                    "tool_calls": {"mean": t.baseline_tool_calls.mean, "std": t.baseline_tool_calls.std},
                    "tokens": {"mean": t.baseline_tokens.mean, "std": t.baseline_tokens.std},
                    "time_seconds": {"mean": t.baseline_time.mean, "std": t.baseline_time.std},
                    "success_rate": t.baseline_success_rate,
                },
                "with_llmcc": {
                    "tool_calls": {"mean": t.llmcc_tool_calls.mean, "std": t.llmcc_tool_calls.std},
                    "tokens": {"mean": t.llmcc_tokens.mean, "std": t.llmcc_tokens.std},
                    "time_seconds": {"mean": t.llmcc_time.mean, "std": t.llmcc_time.std},
                    "success_rate": t.llmcc_success_rate,
                },
                "change": {
                    "tool_calls_pct": round(t.tool_call_change_pct, 1),
                    "tokens_pct": round(t.token_change_pct, 1),
                    "time_pct": round(t.time_change_pct, 1),
                    "success_rate_pct": round(t.success_rate_change_pct, 1),
                },
                "graph": {
                    "nodes": t.graph_nodes,
                    "edges": t.graph_edges,
                    "tokens": t.graph_tokens,
                },
            }
            for t in summary.task_summaries
        ],
    }
    return json.dumps(data, indent=2)


def generate_report(
    results_dir: Path,
    output_format: str = "markdown",
) -> Optional[str]:
    """
    Generate a report from benchmark results.

    Args:
        results_dir: Directory containing benchmark results.
        output_format: "markdown" or "json".

    Returns:
        Report content or None if data is missing.
    """
    summary = summarize_experiment(results_dir)
    if summary is None:
        return None

    if output_format == "json":
        return generate_json_report(summary)
    else:
        return generate_markdown_report(summary)


def main():
    """CLI for generating reports."""
    import argparse

    parser = argparse.ArgumentParser(description="Generate benchmark reports")
    parser.add_argument(
        "--input", "-i",
        type=Path,
        required=True,
        help="Results directory to summarize",
    )
    parser.add_argument(
        "--format", "-f",
        choices=["markdown", "json"],
        default="markdown",
        help="Output format (default: markdown)",
    )
    parser.add_argument(
        "--output", "-o",
        type=Path,
        help="Output file (default: stdout)",
    )

    args = parser.parse_args()

    report = generate_report(args.input, args.format)

    if report is None:
        print("Error: Could not generate report. Check that results directory exists.")
        return 1

    if args.output:
        args.output.write_text(report)
        print(f"Report written to: {args.output}")
    else:
        print(report)

    return 0


if __name__ == "__main__":
    exit(main())
