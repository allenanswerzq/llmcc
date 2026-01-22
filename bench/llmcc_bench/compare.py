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
import random
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
from .eval import Evaluator

# Max retries for tasks that don't complete
MAX_TASK_RETRIES = 3


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
    output_dir: Optional[Path] = None,
    evaluator: Optional[Evaluator] = None,
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

            print(f"  [{condition.value}] {task.id} run {run_num + 1}/{config.runs_per_condition}...")

            # Retry loop for tasks that don't complete
            metrics = None
            for attempt in range(MAX_TASK_RETRIES):
                if attempt > 0:
                    print(f"    Retry {attempt}/{MAX_TASK_RETRIES - 1} (no TASK COMPLETE)...")

                # Reset workspace before each run
                try:
                    reset_workspace(repo_path)
                except Exception as e:
                    print(f"    Warning: Failed to reset workspace: {e}")

                # Set up trace file (append attempt number for retries)
                trace_file = None
                if output_dir:
                    suffix = f"_retry{attempt}" if attempt > 0 else ""
                    trace_file = output_dir / f"trace_{task.id}_{run_id}{suffix}.jsonl"

                # Create run context
                context = RunContext(
                    task=task,
                    condition=condition,
                    workspace_path=repo_path,
                    graph_context=graph_content if condition == Condition.WITH_LLMCC else None,
                    run_id=run_id,
                    limits=config.run_limits,
                    trace_file=trace_file,
                )

                # Run the agent
                metrics = await runner.run(context)

                # Run validation if specified
                if task.validation_command:
                    passed, output = await run_validation(task, repo_path)
                    metrics.validation_passed = passed
                    metrics.validation_output = output

                # If task completed, no need to retry
                if metrics.task_completed:
                    break

            results.append(metrics)

            # Print summary
            status = "✓" if metrics.task_completed else "✗"
            total_tokens = metrics.tokens_input + metrics.tokens_output
            # Show marginal tokens (excluding graph context) for with_llmcc
            if metrics.graph_tokens > 0:
                marginal_tokens = total_tokens - metrics.graph_tokens
                print(f"    {status} {metrics.tool_calls_total} tools, "
                      f"{total_tokens} tokens ({marginal_tokens} marginal), "
                      f"{metrics.wall_time_seconds:.1f}s")
            else:
                print(f"    {status} {metrics.tool_calls_total} tools, "
                      f"{total_tokens} tokens, "
                      f"{metrics.wall_time_seconds:.1f}s")

    # After all runs, compare baseline vs llmcc if evaluator is provided
    if evaluator and len(config.conditions) == 2:
        # Group results by run number
        baseline_results = [r for r in results if r.condition == Condition.BASELINE.value]
        llmcc_results = [r for r in results if r.condition == Condition.WITH_LLMCC.value]

        comparisons = []
        for run_num in range(config.runs_per_condition):
            if run_num < len(baseline_results) and run_num < len(llmcc_results):
                baseline = baseline_results[run_num]
                llmcc = llmcc_results[run_num]

                print(f"  [eval] Comparing run {run_num + 1}...", end="", flush=True)
                comparison = await evaluator.compare_answers(
                    question=task.description,
                    baseline_answer=baseline.answer or "",
                    llmcc_answer=llmcc.answer or "",
                    task_id=task.id,
                    run_id=str(run_num),
                )

                comparisons.append(comparison)

                if comparison.error:
                    print(f" error: {comparison.error}")
                else:
                    # Store comparison results in both metrics
                    baseline.eval_overall = comparison.baseline_score
                    llmcc.eval_overall = comparison.llmcc_score
                    baseline.eval_reasoning = comparison.reasoning
                    llmcc.eval_reasoning = comparison.reasoning

                    winner_str = {
                        "baseline": "baseline wins",
                        "llmcc": "llmcc wins",
                        "tie": "tie",
                    }.get(comparison.winner, "tie")
                    print(f" {winner_str} ({comparison.margin}): "
                          f"baseline={comparison.baseline_score}/10, "
                          f"llmcc={comparison.llmcc_score}/10")

        # Save eval results to file
        if output_dir and comparisons:
            eval_path = output_dir / f"eval_{task.id}.json"
            with open(eval_path, "w") as f:
                json.dump({
                    "task_id": task.id,
                    "question": task.description,
                    "comparisons": [c.to_dict() for c in comparisons],
                }, f, indent=2)

    return results


async def run_comparison(
    config: ExperimentConfig,
    runner: AgentRunner,
    output_dir: Path,
    evaluator: Optional[Evaluator] = None,
) -> List[TaskMetrics]:
    """
    Run the full comparison benchmark.

    Args:
        config: Experiment configuration.
        runner: Agent runner to use.
        output_dir: Directory for results.
        evaluator: Optional evaluator for answer quality.

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

    # Random sampling if requested
    if config.sample and config.sample < len(tasks):
        tasks = random.sample(tasks, config.sample)
        print(f"Randomly sampled {config.sample} tasks")

    print(f"Found {len(tasks)} tasks")

    # Generate graph once (if any condition uses it)
    graph_content: Optional[str] = None
    if Condition.WITH_LLMCC in config.conditions:
        gc = config.graph_config
        print(f"Generating graph (depth={gc.depth}, top_k={gc.pagerank_top_k})...")
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
            print(f"  {nodes} nodes, {edges} edges")

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

    # Check for parallel execution
    parallel = getattr(config, 'parallel', 1)

    if parallel > 1:
        # Run tasks in parallel batches
        print(f"\nRunning {len(tasks)} tasks with parallelism={parallel}")

        async def run_task_wrapper(i: int, task: Task) -> List[TaskMetrics]:
            """Wrapper to run a single task and save results to its own file."""
            print(f"\n[{i + 1}/{len(tasks)}] Task: {task.id}")
            print(f"  Category: {task.category.value}, Difficulty: {task.difficulty.value}")

            task_results = await run_single_task(
                task=task,
                runner=runner,
                repo_path=repo_path,
                config=config,
                graph_content=graph_content,
                output_dir=output_dir,
                evaluator=evaluator,
            )

            # Save to task-specific file
            task_results_path = output_dir / f"metrics_{task.id}.jsonl"
            for metrics in task_results:
                append_metrics_jsonl(metrics, task_results_path)

            return task_results

        # Process in batches
        for batch_start in range(0, len(tasks), parallel):
            batch_end = min(batch_start + parallel, len(tasks))
            batch = tasks[batch_start:batch_end]

            batch_tasks = [
                run_task_wrapper(batch_start + j, task)
                for j, task in enumerate(batch)
            ]

            batch_results = await asyncio.gather(*batch_tasks, return_exceptions=True)

            for result in batch_results:
                if isinstance(result, Exception):
                    print(f"  Error: {result}")
                else:
                    all_results.extend(result)

        # Combine all task-specific files into raw_metrics.jsonl
        for task in tasks:
            task_results_path = output_dir / f"metrics_{task.id}.jsonl"
            if task_results_path.exists():
                with open(task_results_path) as f:
                    with open(results_path, "a") as out:
                        out.write(f.read())
    else:
        # Sequential execution (original behavior)
        for i, task in enumerate(tasks):
            print(f"\n[{i + 1}/{len(tasks)}] Task: {task.id}")
            print(f"  Category: {task.category.value}, Difficulty: {task.difficulty.value}")

            task_results = await run_single_task(
                task=task,
                runner=runner,
                repo_path=repo_path,
                config=config,
                graph_content=graph_content,
                output_dir=output_dir,
                evaluator=evaluator,
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

    # Task sampling
    parser.add_argument(
        "--sample",
        type=int,
        default=None,
        help="Randomly sample N tasks (for quick testing)",
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
        default="detailed",
        help="Graph configuration preset (default: detailed)",
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
        default="claude-opus-4-5-20251101",
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
    parser.add_argument(
        "--eval",
        action="store_true",
        help="Enable LLM-as-judge evaluation of answer quality",
    )
    parser.add_argument(
        "--eval-model",
        default="gpt-4o-mini",
        help="Model to use for evaluation (default: gpt-4o-mini)",
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
        sample=args.sample,
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
    print(f"Evaluation: {'enabled' if args.eval else 'disabled'}")
    print(f"Output: {output_dir}")
    print()

    # Create evaluator if requested
    evaluator = None
    if args.eval:
        evaluator = Evaluator(model=args.eval_model)
        print(f"Evaluator initialized with model: {args.eval_model}")

    # Run benchmark
    asyncio.run(run_comparison(config, runner, output_dir, evaluator))


if __name__ == "__main__":
    main()
