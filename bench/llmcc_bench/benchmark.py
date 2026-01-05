"""
Benchmark llmcc performance on sample projects.
"""

import os
import re
import shutil
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Tuple

from .core import (
    PROJECTS,
    Config,
    count_files,
    count_graph_stats,
    count_loc,
    count_rust_files,
    format_loc,
    format_time,
    run_command,
)


@dataclass
class TimingResult:
    """Timing data from a single benchmark run."""
    files: int = 0
    parse: float = 0.0
    ir_symbols: float = 0.0
    binding: float = 0.0
    graph: float = 0.0
    link: float = 0.0
    total: float = 0.0

    @classmethod
    def from_log(cls, log_file: Path) -> "TimingResult":
        """Parse timing values from a log file."""
        result = cls()

        if not log_file.exists():
            return result

        try:
            content = log_file.read_text(encoding='utf-8', errors='ignore')

            # Extract timing values using regex
            patterns = {
                'files': r'Parsing total (\d+)',
                'parse': r'Parsing & tree-sitter: ([0-9.]+)s',
                'ir_symbols': r'IR build \+ Symbol collection: ([0-9.]+)s',
                'binding': r'Symbol binding: ([0-9.]+)s',
                'graph': r'Graph building: ([0-9.]+)s',
                'link': r'Linking units: ([0-9.]+)s',
                'total': r'Total time: ([0-9.]+)s',
            }

            for key, pattern in patterns.items():
                if match := re.search(pattern, content):
                    value = match.group(1)
                    if key == 'files':
                        result.files = int(value)
                    else:
                        setattr(result, key, float(value))

            # Try legacy format for ir_symbols
            if result.ir_symbols == 0.0:
                ir_match = re.search(r'IR building: ([0-9.]+)', content)
                sym_match = re.search(r'Symbol collection: ([0-9.]+)', content)
                if ir_match and sym_match:
                    result.ir_symbols = float(ir_match.group(1)) + float(sym_match.group(1))

        except Exception:
            pass

        return result


@dataclass
class BenchmarkResult:
    """Complete benchmark result for a project."""
    name: str
    src_dir: Path
    loc: int = 0
    file_count: int = 0
    full_timing: Optional[TimingResult] = None
    pagerank_timing: Optional[TimingResult] = None
    full_nodes: int = 0
    full_edges: int = 0
    pr_nodes: int = 0
    pr_edges: int = 0

    @property
    def node_reduction(self) -> float:
        """Calculate node reduction percentage."""
        if self.full_nodes > 0:
            return (1 - self.pr_nodes / self.full_nodes) * 100
        return 0.0

    @property
    def edge_reduction(self) -> float:
        """Calculate edge reduction percentage."""
        if self.full_edges > 0:
            return (1 - self.pr_edges / self.full_edges) * 100
        return 0.0


@dataclass
class ScalingResult:
    """Thread scaling benchmark result."""
    threads: int
    timing: TimingResult
    speedup: float = 1.0


def run_llmcc(
    config: Config,
    src_dir: Path,
    output_file: Path,
    depth: int = 3,
    pagerank_top_k: Optional[int] = None,
    threads: Optional[int] = None,
    language: str = "rust",
) -> Tuple[Path, float]:
    """
    Run llmcc and capture output.

    Returns: (log_file_path, elapsed_time)
    """
    if not config.llmcc_path:
        raise RuntimeError("llmcc binary not found")

    log_file = output_file.with_suffix('.log')

    cmd = [
        str(config.llmcc_path),
        "-d", str(src_dir),
        "--graph",
        "--depth", str(depth),
        "-o", str(output_file),
        "--lang", language,
    ]

    if pagerank_top_k:
        cmd.extend(["--pagerank-top-k", str(pagerank_top_k)])

    env = {"RUST_LOG": "info,llmcc_resolver=error"}
    if threads:
        env["RAYON_NUM_THREADS"] = str(threads)

    start = time.perf_counter()
    result = run_command(cmd, env=env, capture=True)
    elapsed = time.perf_counter() - start

    # Write stdout/stderr to log file
    with open(log_file, 'w', encoding='utf-8') as f:
        f.write(result.stdout)
        f.write(result.stderr)

    return log_file, elapsed


def benchmark_project(
    name: str,
    config: Config,
    verbose: bool = True,
) -> Optional[BenchmarkResult]:
    """
    Run full and PageRank benchmarks on a project.

    Returns: BenchmarkResult or None if project not found
    """
    if name not in PROJECTS:
        if verbose:
            print(f"  Skipping {name} (unknown project)")
        return None

    project = PROJECTS[name]
    src_dir = config.project_repo_path(project)

    if not src_dir.exists():
        if verbose:
            print(f"  Skipping {name} (not found)")
        return None

    result = BenchmarkResult(name=name, src_dir=src_dir)

    # Count files and LoC
    result.file_count = count_files(src_dir, project.language)
    result.loc = count_loc(src_dir)

    config.benchmark_logs_dir.mkdir(parents=True, exist_ok=True)

    # Run full graph benchmark
    if verbose:
        print(f"  Running depth={config.depth} benchmark...")

    full_dot = config.benchmark_logs_dir / f"{name}_depth{config.depth}.dot"
    log_file, _ = run_llmcc(config, src_dir, full_dot, depth=config.depth, language=project.language)
    result.full_timing = TimingResult.from_log(log_file)

    if verbose and result.full_timing:
        print(f"    Files: {result.full_timing.files}, Total: {format_time(result.full_timing.total)}")

    # Get graph stats
    result.full_nodes, result.full_edges = count_graph_stats(full_dot)

    # Run PageRank benchmark
    if verbose:
        print(f"  Running depth={config.depth} benchmark with PageRank top-{config.top_k}...")

    pr_dot = config.benchmark_logs_dir / f"{name}_pagerank_depth{config.depth}.dot"
    log_file, _ = run_llmcc(config, src_dir, pr_dot, depth=config.depth, pagerank_top_k=config.top_k, language=project.language)
    result.pagerank_timing = TimingResult.from_log(log_file)

    if verbose and result.pagerank_timing:
        print(f"    Files: {result.pagerank_timing.files}, Total: {format_time(result.pagerank_timing.total)}")

    # Get PageRank graph stats
    result.pr_nodes, result.pr_edges = count_graph_stats(pr_dot)

    return result


def benchmark_all(
    config: Config,
    projects: Optional[List[str]] = None,
    verbose: bool = True,
) -> List[BenchmarkResult]:
    """
    Benchmark all or specified projects.

    Returns: List of BenchmarkResults sorted by file count (descending)
    """
    # Clean benchmark_logs for fresh start
    if config.benchmark_logs_dir.exists():
        if verbose:
            print(f"Cleaning {config.benchmark_logs_dir}...")
        shutil.rmtree(config.benchmark_logs_dir)
    config.benchmark_logs_dir.mkdir(parents=True, exist_ok=True)

    to_benchmark = projects if projects else list(PROJECTS.keys())

    # First, count files to sort by size
    if verbose:
        print("Counting files in projects...")

    file_counts: Dict[str, int] = {}
    for name in to_benchmark:
        if name not in PROJECTS:
            file_counts[name] = 0
            continue
        project = PROJECTS[name]
        src_dir = config.project_repo_path(project)
        if src_dir.exists():
            count = count_files(src_dir, project.language)
            file_counts[name] = count
            if verbose:
                print(f"  {name}: {count} files")
        else:
            file_counts[name] = 0

    # Sort by file count descending
    sorted_projects = sorted(to_benchmark, key=lambda n: file_counts.get(n, 0), reverse=True)

    results = []
    for name in sorted_projects:
        if verbose:
            print()
            print(f"=== Benchmarking {name} ===")

        result = benchmark_project(name, config, verbose=verbose)
        if result:
            results.append(result)
        elif verbose and name in PROJECTS:
            # Create placeholder for missing projects
            project = PROJECTS[name]
            results.append(BenchmarkResult(name=name, src_dir=config.project_repo_path(project)))

    return results


def run_scaling_benchmark(
    config: Config,
    project: str = "databend",
    thread_counts: Optional[List[int]] = None,
    verbose: bool = True,
) -> List[ScalingResult]:
    """
    Run thread scaling benchmark on a project.

    Returns: List of ScalingResults for each thread count
    """
    from .core import get_cpu_info

    if project not in PROJECTS:
        if verbose:
            print(f"Unknown project: {project}")
        return []

    proj = PROJECTS[project]
    src_dir = config.project_repo_path(proj)
    if not src_dir.exists():
        if verbose:
            print(f"Project {project} not found")
        return []

    _, physical_cores, logical_cores = get_cpu_info()

    # Build thread counts dynamically based on CPU
    if thread_counts is None:
        thread_counts = [1]
        for t in [2, 4, 8, 16, 24, 32, 48, 64]:
            if t <= logical_cores:
                thread_counts.append(t)

    if verbose:
        print(f"=== Thread Scaling Benchmark ({project}) ===")

    config.benchmark_logs_dir.mkdir(parents=True, exist_ok=True)

    results = []
    baseline_time = 0.0

    for threads in thread_counts:
        if verbose:
            print(f"  Running with {threads} thread(s)...", end=" ", flush=True)

        dot_file = config.benchmark_logs_dir / f"{project}_scaling_{threads}t.dot"
        log_file, _ = run_llmcc(
            config, src_dir, dot_file,
            depth=config.depth,
            pagerank_top_k=config.top_k,
            threads=threads
        )

        timing = TimingResult.from_log(log_file)

        # Calculate speedup
        if threads == 1:
            baseline_time = timing.total
            speedup = 1.0
        elif baseline_time > 0 and timing.total > 0:
            speedup = baseline_time / timing.total
        else:
            speedup = 0.0

        result = ScalingResult(threads=threads, timing=timing, speedup=speedup)
        results.append(result)

        if verbose:
            speedup_str = f"{speedup:.2f}x" if threads > 1 else "-"
            print(f"{format_time(timing.total)} ({speedup_str})")

    return results
