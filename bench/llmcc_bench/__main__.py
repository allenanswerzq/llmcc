"""
CLI entry point for llmcc-bench.

Usage:
    python -m llmcc_bench <command> [options]

Commands:
    fetch       Fetch sample repositories
    benchmark   Run benchmarks on sample projects
    generate    Generate architecture graphs
    clean       Clean up generated files
    info        Show system and configuration info
    compare     Run A/B comparison benchmarks (agent benchmark)
    report      Generate reports from benchmark results
"""

import argparse
import sys
from pathlib import Path
from typing import Dict, List

from . import __version__
from .core import Config, PROJECTS, find_llmcc, get_system_info


def cmd_fetch(args, config: Config) -> int:
    """Fetch sample repositories."""
    from .fetch import fetch_all, list_repos

    if args.list:
        list_repos(config)
        return 0

    repos = args.repos if args.repos else None
    failed = fetch_all(config, force=args.force, repos=repos)
    return 1 if failed > 0 else 0


def cmd_benchmark(args, config: Config) -> int:
    """Run benchmarks on sample projects."""
    from .benchmark import benchmark_all, run_scaling_benchmark
    from .report import generate_report

    # Filter projects by language if specified
    if args.language:
        filtered_projects = [
            name for name, p in PROJECTS.items()
            if p.language == args.language
        ]
        if not filtered_projects:
            print(f"Error: No projects found for language '{args.language}'")
            return 1
    else:
        filtered_projects = None

    # Check if any repos exist
    projects_to_check = filtered_projects if filtered_projects else list(PROJECTS.keys())
    has_repos = any(
        config.project_repo_path(PROJECTS[name]).exists()
        for name in projects_to_check
    )
    if not has_repos:
        print("Error: No repositories found. Run 'llmcc-bench fetch' first.")
        return 1

    # Update config from args
    if args.top_k:
        config.top_k = args.top_k
    if args.depth:
        config.depth = args.depth

    # Use explicit projects if given, otherwise use language-filtered projects
    projects = args.projects if args.projects else filtered_projects

    # Default to verbose output unless explicitly disabled
    verbose = not getattr(args, 'quiet', False)

    print("=== LLMCC Benchmark ===")
    print(f"Binary: {config.llmcc_path}")
    print(f"Results: {config.benchmark_file(language=args.language or '')}")
    if args.language:
        print(f"Language: {args.language}")
    print()

    # Run benchmarks
    results = benchmark_all(config, projects=projects, verbose=verbose)

    # Run scaling benchmark if not skipped (only for Rust projects)
    scaling_results = None
    scaling_project = args.scaling_project or "databend"

    # Skip scaling for non-rust language filter
    skip_scaling = args.skip_scaling or (args.language and args.language != "rust")
    if not skip_scaling:
        print()
        scaling_results = run_scaling_benchmark(config, project=scaling_project)

    # Generate report
    output_file = config.benchmark_file(language=args.language or '')
    report = generate_report(
        config, results,
        scaling_results=scaling_results,
        scaling_project=scaling_project,
        output_file=output_file,
    )

    print()
    print("=== Benchmark Complete ===")
    print()
    print(report)

    return 0


def cmd_generate(args, config: Config) -> int:
    """Generate architecture graphs."""
    from .generate import generate_all

    # Filter projects by language if specified
    if args.lang:
        filtered_projects = [
            name for name, p in PROJECTS.items()
            if p.language == args.lang
        ]
        if not filtered_projects:
            print(f"Error: No projects found for language '{args.lang}'")
            return 1
    else:
        filtered_projects = None

    # Check if any repos exist
    projects_to_check = filtered_projects if filtered_projects else list(PROJECTS.keys())
    has_repos = any(
        config.project_repo_path(PROJECTS[name]).exists()
        for name in projects_to_check
    )
    if not has_repos:
        print("Error: No repositories found. Run 'llmcc-bench fetch' first.")
        return 1

    # Use explicit projects if given, otherwise use language-filtered projects
    projects = args.projects if args.projects else filtered_projects
    failed = generate_all(config, projects=projects, skip_svg=not args.svg)

    return 1 if failed > 0 else 0


def cmd_clean(args, config: Config) -> int:
    """Clean up generated files."""
    from .clean import clean_sample_dir

    clean_sample_dir(
        config,
        remove_all=args.all,
        dry_run=args.dry_run,
    )
    return 0


def cmd_info(args, config: Config) -> int:
    """Show system and configuration info."""
    info = get_system_info()

    print("=== System Information ===")
    print()
    print(f"CPU: {info.cpu_model}")
    print(f"Cores: {info.cpu_physical_cores} physical, {info.cpu_logical_cores} logical")
    print(f"Memory: {info.memory_total} total, {info.memory_available} available")
    print(f"OS: {info.os_distribution}")
    print(f"Kernel: {info.os_kernel}")

    print()
    print("=== Configuration ===")
    print()
    print(f"Project root: {config.project_root}")
    print(f"Sample dir: {config.sample_dir}")
    print(f"LLMCC binary: {config.llmcc_path or 'Not found'}")
    print(f"Top-K: {config.top_k}")
    print(f"Depth: {config.depth}")

    print()
    print("=== Projects ===")
    print()
    # Group by language
    by_language: Dict[str, List] = {}
    for name, project in PROJECTS.items():
        by_language.setdefault(project.language, []).append((name, project))

    for lang in sorted(by_language.keys()):
        print(f"  [{lang}]")
        for name, project in sorted(by_language[lang]):
            exists = "✓" if config.project_repo_path(project).exists() else "✗"
            print(f"    {exists} {name}: {project.github_path}")

    return 0


def cmd_compare(args, config: Config) -> int:
    """Run A/B comparison benchmarks."""
    import asyncio
    from datetime import datetime
    from .agent.config import (
        GRAPH_CONFIGS,
        Condition,
        ExperimentConfig,
        RunLimits,
    )
    from .agent.runner import MockAgentRunner
    from .agent.tasks import get_task_by_id, load_tasks
    from .compare import run_comparison

    # Determine conditions
    conditions = [Condition.BASELINE, Condition.WITH_LLMCC]
    if args.baseline_only:
        conditions = [Condition.BASELINE]
    elif args.llmcc_only:
        conditions = [Condition.WITH_LLMCC]

    # Determine repo and tasks
    if args.task:
        task = get_task_by_id(args.task)
        if task is None:
            print(f"Task not found: {args.task}")
            return 1
        repo = task.repo
        task_ids = [args.task]
    else:
        repo = args.repo
        task_ids = None

    # Create config
    exp_config = ExperimentConfig(
        repo=repo,
        repo_path=args.repo_path,
        runs_per_condition=args.runs,
        conditions=conditions,
        graph_config=GRAPH_CONFIGS[args.graph_config],
        run_limits=RunLimits(
            max_tool_calls=args.max_tool_calls,
        ),
        task_ids=task_ids,
        parallel=args.parallel,
        sample=args.sample,
        debug=getattr(args, 'debug', False),
    )

    # Create output directory
    if args.output:
        output_dir = args.output
    else:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        repo_name = repo.split("/")[-1]
        output_dir = config.sample_dir.parent / "bench" / "results" / f"{timestamp}_{repo_name}"

    # Create runner
    if args.runner == "mock":
        runner = MockAgentRunner(seed=args.seed)
    elif args.runner == "claude":
        from .agent.runner import ClaudeAgentRunner
        runner = ClaudeAgentRunner(
            model=args.model or "opus",
            timeout=args.timeout or 600,
        )
    elif args.runner == "codex":
        from .agent.runner import CodexAgentRunner
        runner = CodexAgentRunner(
            model=args.model or "o3",
            timeout=args.timeout or 600,
        )
    elif args.runner == "llcraft":
        from .agent.runner import LlcraftAgentRunner
        runner = LlcraftAgentRunner(
            model=args.model or "claude-opus-4-5-20251101",
            timeout=args.timeout or 600,
        )
    else:
        print(f"Unknown runner: {args.runner}")
        return 1

    print(f"llmcc Agent Benchmark")
    print(f"=" * 50)
    print(f"Repository: {repo}")
    print(f"Conditions: {[c.value for c in conditions]}")
    print(f"Runs per condition: {args.runs}")
    gc = exp_config.graph_config
    print(f"Graph: depth={gc.depth}, top_k={gc.pagerank_top_k}")
    print(f"Runner: {args.runner}")
    print(f"Evaluation: {'enabled (' + args.eval_model + ')' if args.eval else 'disabled'}")
    print(f"Parallel: {args.parallel}")
    if exp_config.debug:
        print(f"Debug: enabled (model will explain reasoning)")
    print(f"Output: {output_dir}")
    print()

    # Create evaluator if requested
    evaluator = None
    if args.eval:
        from .eval import Evaluator
        evaluator = Evaluator(model=args.eval_model)

    # Run benchmark
    asyncio.run(run_comparison(exp_config, runner, output_dir, evaluator))

    return 0


def cmd_report(args, config: Config) -> int:
    """Generate reports from benchmark results."""
    from .agent.report import generate_report

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


def cmd_eval(args, config: Config) -> int:
    """Evaluate benchmark results - deprecated, use --eval with compare command."""
    print("The separate 'eval' command is deprecated.")
    print("Use 'compare --eval' to enable inline evaluation during benchmark runs.")
    return 1


def main() -> int:
    parser = argparse.ArgumentParser(
        prog="llmcc-bench",
        description="Cross-platform benchmarking and graph generation for llmcc",
    )
    parser.add_argument(
        "-V", "--version",
        action="version",
        version=f"llmcc-bench {__version__}",
    )
    parser.add_argument(
        "--project-root",
        type=Path,
        help="Path to llmcc project root",
    )
    parser.add_argument(
        "--llmcc",
        type=Path,
        help="Path to llmcc binary",
    )

    subparsers = parser.add_subparsers(dest="command", help="Commands")

    # fetch command
    fetch_parser = subparsers.add_parser("fetch", help="Fetch sample repositories")
    fetch_parser.add_argument(
        "-f", "--force",
        action="store_true",
        help="Remove existing repos and re-clone",
    )
    fetch_parser.add_argument(
        "-l", "--list",
        action="store_true",
        help="List available repositories",
    )
    fetch_parser.add_argument(
        "repos",
        nargs="*",
        help="Specific repos to fetch (default: all)",
    )

    # benchmark command
    bench_parser = subparsers.add_parser("benchmark", help="Run benchmarks")
    bench_parser.add_argument(
        "--top-k",
        type=int,
        default=200,
        help="PageRank top-K value (default: 200)",
    )
    bench_parser.add_argument(
        "--depth",
        type=int,
        default=3,
        help="Graph depth level (default: 3)",
    )
    bench_parser.add_argument(
        "--skip-scaling",
        action="store_true",
        help="Skip thread scaling benchmark",
    )
    bench_parser.add_argument(
        "--scaling-project",
        type=str,
        default="databend",
        help="Project for scaling benchmark (default: databend)",
    )
    bench_parser.add_argument(
        "--language",
        type=str,
        default=None,
        help="Filter projects by language (rust, typescript)",
    )
    bench_parser.add_argument(
        "-q", "--quiet",
        action="store_true",
        help="Suppress progress output",
    )
    bench_parser.add_argument(
        "projects",
        nargs="*",
        help="Specific projects to benchmark (default: all)",
    )

    # generate command
    gen_parser = subparsers.add_parser("generate", help="Generate architecture graphs")
    gen_parser.add_argument(
        "--svg",
        action="store_true",
        help="Also generate SVG files (requires Graphviz)",
    )
    gen_parser.add_argument(
        "--lang",
        type=str,
        default=None,
        help="Filter projects by language (rust, typescript)",
    )
    gen_parser.add_argument(
        "projects",
        nargs="*",
        help="Specific projects to process (default: all)",
    )

    # clean command
    clean_parser = subparsers.add_parser("clean", help="Clean up generated files")
    clean_parser.add_argument(
        "-a", "--all",
        action="store_true",
        help="Remove everything including repos/, benchmark_logs/, and results",
    )
    clean_parser.add_argument(
        "-n", "--dry-run",
        action="store_true",
        help="Print what would be removed without removing",
    )

    # info command
    subparsers.add_parser("info", help="Show system and configuration info")

    # compare command (agent benchmark)
    compare_parser = subparsers.add_parser(
        "compare",
        help="Run A/B comparison benchmarks for llmcc effectiveness"
    )
    compare_group = compare_parser.add_mutually_exclusive_group(required=True)
    compare_group.add_argument(
        "--task",
        help="Specific task ID to run",
    )
    compare_group.add_argument(
        "--repo",
        help="Repository to run all tasks for (e.g., 'tokio-rs/tokio')",
    )
    compare_parser.add_argument(
        "--runs",
        type=int,
        default=3,
        help="Number of runs per condition (default: 3)",
    )
    compare_parser.add_argument(
        "--graph-config",
        choices=["minimal", "compact", "standard", "detailed", "full"],
        default="detailed",
        help="Graph configuration preset (default: detailed)",
    )
    compare_parser.add_argument(
        "--baseline-only",
        action="store_true",
        help="Only run baseline condition (no graph)",
    )
    compare_parser.add_argument(
        "--llmcc-only",
        action="store_true",
        help="Only run with_llmcc condition",
    )
    compare_parser.add_argument(
        "--runner",
        choices=["mock", "claude", "codex", "llcraft"],
        default="mock",
        help="Agent runner to use (default: mock)",
    )
    compare_parser.add_argument(
        "--model",
        type=str,
        default=None,
        help="Model to use (claude: opus/sonnet, codex: o3/o4-mini)",
    )
    compare_parser.add_argument(
        "--timeout",
        type=float,
        default=600,
        help="Timeout per run in seconds (default: 600)",
    )
    compare_parser.add_argument(
        "--seed",
        type=int,
        default=None,
        help="Random seed for reproducibility (mock runner only)",
    )
    compare_parser.add_argument(
        "--max-tool-calls",
        type=int,
        default=100,
        help="Maximum tool calls per run (default: 100)",
    )
    compare_parser.add_argument(
        "-o", "--output",
        type=Path,
        help="Output directory for results",
    )
    compare_parser.add_argument(
        "--repo-path",
        type=Path,
        help="Local path to repository (skip automatic lookup)",
    )
    compare_parser.add_argument(
        "--eval",
        action="store_true",
        help="Enable LLM-as-judge evaluation of answer quality",
    )
    compare_parser.add_argument(
        "--eval-model",
        default="claude-opus-4-20250514",
        help="Model to use for evaluation (default: claude-opus-4-20250514)",
    )
    compare_parser.add_argument(
        "--parallel", "-j",
        type=int,
        default=1,
        help="Number of tasks to run in parallel (default: 1)",
    )
    compare_parser.add_argument(
        "--sample",
        type=int,
        default=None,
        help="Randomly sample N tasks (for quick testing)",
    )
    compare_parser.add_argument(
        "--debug",
        action="store_true",
        help="Enable debug mode: model explains reasoning for each tool call",
    )

    # report command
    report_parser = subparsers.add_parser(
        "report",
        help="Generate reports from benchmark results"
    )
    report_parser.add_argument(
        "-i", "--input",
        type=Path,
        required=True,
        help="Results directory to summarize",
    )
    report_parser.add_argument(
        "-f", "--format",
        choices=["markdown", "json"],
        default="markdown",
        help="Output format (default: markdown)",
    )
    report_parser.add_argument(
        "-o", "--output",
        type=Path,
        help="Output file (default: stdout)",
    )

    args = parser.parse_args()

    if not args.command:
        parser.print_help()
        return 0

    # Build configuration
    config = Config()

    if args.project_root:
        config.project_root = args.project_root
        config.sample_dir = args.project_root / "sample"

    if args.llmcc:
        config.llmcc_path = args.llmcc
    elif config.llmcc_path is None:
        config.llmcc_path = find_llmcc(config.project_root)

    # Dispatch command
    commands = {
        "fetch": cmd_fetch,
        "benchmark": cmd_benchmark,
        "generate": cmd_generate,
        "clean": cmd_clean,
        "info": cmd_info,
        "compare": cmd_compare,
        "report": cmd_report,
        "eval": cmd_eval,
    }

    handler = commands.get(args.command)
    if handler:
        return handler(args, config)
    else:
        parser.print_help()
        return 1


if __name__ == "__main__":
    sys.exit(main())
