"""
CLI for running A/B comparison benchmarks.

Usage:
    python -m llmcc_bench compare --task <task_id> [options]
    python -m llmcc_bench compare --repo <repo> [options]
"""

import argparse
import asyncio
import json
import os
import sys
from datetime import datetime
from pathlib import Path
from typing import List, Optional

from .agent.config import (
    GRAPH_CONFIGS,
    Condition,
    ExperimentConfig,
    GraphConfig,
    RunLimits,
)
from .agent.metrics import TaskMetrics, append_metrics_jsonl
from .agent.runner import (
    AgentRunner,
    MockAgentRunner,
    RunContext,
    generate_graph,
    reset_workspace,
    run_validation,
)
from .agent.tasks import Task, get_task_by_id, load_tasks
from .core import PROJECTS, load_projects


def get_repo_path(repo: str, sample_dir: Optional[Path] = None) -> Path:
    """Get the local path to a repository."""
    if sample_dir is None:
        sample_dir = Path(__file__).parent.parent.parent / "sample" / "repos"

    # Convert owner/repo to just repo name
    repo_name = repo.split("/")[-1]
    repo_path = sample_dir / repo_name

    if not repo_path.exists():
        raise FileNotFoundError(
            f"Repository not found at {repo_path}. "
            f"Run 'python -m llmcc_bench fetch {repo_name}' first."
        )

    return repo_path


async def run_single_task(
    task: Task,
    runner: AgentRunner,
    repo_path: Path,
    config: ExperimentConfig,
    graph_content: Optional[str] = None,
) -> List[TaskMetrics]:
    """
    Run a single task under all conditions.

    Returns:
        List of TaskMetrics for each run.
    """
    results: List[TaskMetrics] = []

    for condition in config.conditions:
        for run_num in range(config.runs_per_condition):
            run_id = f"{condition.value}_{run_num}"

            print(f"  Running {task.id} [{condition.value}] run {run_num + 1}/{config.runs_per_condition}...")

            # Reset workspace before each run
            try:
                reset_workspace(repo_path)
            except Exception as e:
                print(f"    Warning: Failed to reset workspace: {e}")

            # Create run context
            context = RunContext(
                task=task,
                condition=condition,
                workspace_path=repo_path,
                graph_context=graph_content if condition == Condition.WITH_LLMCC else None,
                run_id=run_id,
                limits=config.run_limits,
            )

            # Run the agent
            metrics = await runner.run(context)

            # Run validation if specified
            if task.validation_command:
                passed, output = await run_validation(task, repo_path)
                metrics.validation_passed = passed
                metrics.validation_output = output

            results.append(metrics)

            # Print summary
            status = "✓" if metrics.task_completed else "✗"
            print(f"    {status} {metrics.tool_calls_total} tools, "
                  f"{metrics.tokens_input + metrics.tokens_output} tokens, "
                  f"{metrics.wall_time_seconds:.1f}s")

    return results


async def run_comparison(
    config: ExperimentConfig,
    runner: AgentRunner,
    output_dir: Path,
) -> List[TaskMetrics]:
    """
    Run the full comparison benchmark.

    Args:
        config: Experiment configuration.
        runner: Agent runner to use.
        output_dir: Directory for results.

    Returns:
        List of all TaskMetrics.
    """
    # Ensure output directory exists
    output_dir.mkdir(parents=True, exist_ok=True)

    # Save config
    config_path = output_dir / "config.json"
    with open(config_path, "w") as f:
        json.dump({
            "repo": config.repo,
            "runs_per_condition": config.runs_per_condition,
            "graph_config": config.graph_config.name,
            "model": config.model,
            "timestamp": datetime.now().isoformat(),
        }, f, indent=2)

    # Get repository path
    repo_path = config.repo_path or get_repo_path(config.repo)
    print(f"Using repository at: {repo_path}")

    # Load tasks
    tasks = load_tasks(repo=config.repo)
    if config.task_ids:
        tasks = [t for t in tasks if t.id in config.task_ids]

    if not tasks:
        print(f"No tasks found for repo: {config.repo}")
        return []

    print(f"Found {len(tasks)} tasks")

    # Generate graph once (if any condition uses it)
    graph_content: Optional[str] = None
    if Condition.WITH_LLMCC in config.conditions:
        print("Generating llmcc graph...")
        try:
            # Detect language from project config
            projects = load_projects()
            language = "rust"
            for name, proj in projects.items():
                if proj.github_path == config.repo:
                    language = proj.language
                    break

            graph_content, nodes, edges = generate_graph(
                repo_path,
                config.graph_config,
                language=language,
            )
            print(f"  Generated graph: {nodes} nodes, {edges} edges")

            # Warn if graph is empty
            if nodes == 0:
                print("  ⚠️  WARNING: Graph is empty! Results may not be meaningful.")
                print("     This usually means the repository doesn't exist or llmcc failed.")
        except Exception as e:
            print(f"  Warning: Failed to generate graph: {e}")
            print("  Continuing without graph...")

    # Run all tasks
    all_results: List[TaskMetrics] = []
    results_path = output_dir / "raw_metrics.jsonl"

    for i, task in enumerate(tasks):
        print(f"\n[{i + 1}/{len(tasks)}] Task: {task.id}")
        print(f"  Category: {task.category.value}, Difficulty: {task.difficulty.value}")

        task_results = await run_single_task(
            task=task,
            runner=runner,
            repo_path=repo_path,
            config=config,
            graph_content=graph_content,
        )

        # Save results incrementally
        for metrics in task_results:
            append_metrics_jsonl(metrics, results_path)

        all_results.extend(task_results)

    print(f"\nResults saved to: {output_dir}")
    return all_results


def main():
    parser = argparse.ArgumentParser(
        description="Run A/B comparison benchmarks for llmcc"
    )

    # Task/repo selection
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument(
        "--task",
        help="Specific task ID to run",
    )
    group.add_argument(
        "--repo",
        help="Repository to run all tasks for (e.g., 'tokio-rs/tokio')",
    )

    # Experiment settings
    parser.add_argument(
        "--runs",
        type=int,
        default=3,
        help="Number of runs per condition (default: 3)",
    )
    parser.add_argument(
        "--graph-config",
        choices=list(GRAPH_CONFIGS.keys()),
        default="standard",
        help="Graph configuration preset (default: standard)",
    )
    parser.add_argument(
        "--baseline-only",
        action="store_true",
        help="Only run baseline condition (no graph)",
    )
    parser.add_argument(
        "--llmcc-only",
        action="store_true",
        help="Only run with_llmcc condition",
    )

    # Runner settings
    parser.add_argument(
        "--runner",
        choices=["mock", "claude"],
        default="mock",
        help="Agent runner to use (default: mock)",
    )
    parser.add_argument(
        "--model",
        default="claude-sonnet-4-20250514",
        help="LLM model to use",
    )
    parser.add_argument(
        "--seed",
        type=int,
        default=None,
        help="Random seed for reproducibility (mock runner only)",
    )

    # Limits
    parser.add_argument(
        "--max-tool-calls",
        type=int,
        default=100,
        help="Maximum tool calls per run (default: 100)",
    )
    parser.add_argument(
        "--max-tokens",
        type=int,
        default=200000,
        help="Maximum tokens per run (default: 200000)",
    )
    parser.add_argument(
        "--max-time",
        type=float,
        default=600,
        help="Maximum wall time in seconds (default: 600)",
    )

    # Output
    parser.add_argument(
        "--output",
        "-o",
        type=Path,
        help="Output directory for results",
    )
    parser.add_argument(
        "--repo-path",
        type=Path,
        help="Local path to repository (skip automatic lookup)",
    )

    args = parser.parse_args()

    # Determine conditions
    conditions = [Condition.BASELINE, Condition.WITH_LLMCC]
    if args.baseline_only:
        conditions = [Condition.BASELINE]
    elif args.llmcc_only:
        conditions = [Condition.WITH_LLMCC]

    # Determine repo
    if args.task:
        task = get_task_by_id(args.task)
        if task is None:
            print(f"Task not found: {args.task}")
            sys.exit(1)
        repo = task.repo
        task_ids = [args.task]
    else:
        repo = args.repo
        task_ids = None

    # Create config
    config = ExperimentConfig(
        repo=repo,
        repo_path=args.repo_path,
        runs_per_condition=args.runs,
        conditions=conditions,
        graph_config=GRAPH_CONFIGS[args.graph_config],
        run_limits=RunLimits(
            max_tool_calls=args.max_tool_calls,
            max_tokens=args.max_tokens,
            max_wall_time_seconds=args.max_time,
        ),
        model=args.model,
        task_ids=task_ids,
    )

    # Create output directory
    if args.output:
        output_dir = args.output
    else:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        repo_name = repo.split("/")[-1]
        output_dir = Path(__file__).parent.parent / "results" / f"{timestamp}_{repo_name}"

    # Create runner
    if args.runner == "mock":
        runner = MockAgentRunner(seed=args.seed)
    elif args.runner == "claude":
        from .agent.runner import ClaudeAgentRunner
        runner = ClaudeAgentRunner(
            model=getattr(args, 'model', None) or 'opus',
            timeout=getattr(args, 'max_time', 600),
        )
    elif args.runner == "codex":
        from .agent.runner import CodexAgentRunner
        runner = CodexAgentRunner(
            model=getattr(args, 'model', None) or 'o3',
            timeout=getattr(args, 'max_time', 600),
        )
    else:
        print(f"Unknown runner: {args.runner}")
        sys.exit(1)

    print(f"llmcc Agent Benchmark")
    print(f"=" * 50)
    print(f"Repository: {repo}")
    print(f"Conditions: {[c.value for c in conditions]}")
    print(f"Runs per condition: {args.runs}")
    print(f"Graph config: {args.graph_config}")
    print(f"Runner: {args.runner}")
    print(f"Output: {output_dir}")
    print()

    # Run benchmark
    asyncio.run(run_comparison(config, runner, output_dir))


if __name__ == "__main__":
    main()
