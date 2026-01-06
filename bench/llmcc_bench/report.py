"""
Generate markdown reports from benchmark results.
"""

from datetime import datetime
from pathlib import Path
from typing import List, Optional

from .benchmark import BenchmarkResult, ScalingResult
from .core import Config, PROJECTS, SystemInfo, format_loc, format_time, get_system_info


def generate_machine_info_section(info: SystemInfo) -> str:
    """Generate machine info markdown section."""
    lines = [
        "## Machine Info",
        "",
        "### CPU",
        f"- **Model:** {info.cpu_model}",
        f"- **Cores:** {info.cpu_physical_cores} physical, {info.cpu_logical_cores} logical (threads)",
        "",
        "### Memory",
        f"- **Total:** {info.memory_total}",
        f"- **Available:** {info.memory_available}",
        "",
        "### Disk",
        f"- **Write Speed:** {info.disk_speed}",
        "",
        "### OS",
        f"- **Kernel:** {info.os_kernel}",
        f"- **Distribution:** {info.os_distribution}",
        "",
    ]
    return "\n".join(lines)


def generate_timing_table(
    results: List[BenchmarkResult],
    config: Config,
) -> str:
    """Generate PageRank timing table."""
    lines = [
        f"## PageRank Timing (depth={config.depth}, top-{config.top_k})",
        "",
        "| Project | Language | Files | LoC | Parse | IR+Symbols | Binding | Graph | Link | Total |",
        "|---------|----------|-------|-----|-------|------------|---------|-------|------|-------|",
    ]

    for r in results:
        lang = PROJECTS[r.name].language if r.name in PROJECTS else "-"

        if not r.src_dir.exists():
            lines.append(f"| {r.name} | {lang} | (not found) | - | - | - | - | - | - | - |")
            continue

        t = r.pagerank_timing
        if t is None:
            lines.append(f"| {r.name} | {lang} | - | - | - | - | - | - | - | - |")
            continue

        loc_str = format_loc(r.loc) if r.loc > 0 else "-"
        # Always show timing values when we have a valid result (even if 0.00s)
        parse = format_time(t.parse)
        ir_sym = format_time(t.ir_symbols)
        binding = format_time(t.binding)
        graph = format_time(t.graph)
        link = format_time(t.link)
        total = format_time(t.total)

        lines.append(
            f"| {r.name} | {lang} | {t.files} | {loc_str} | {parse} | {ir_sym} | {binding} | {graph} | {link} | {total} |"
        )

    lines.append("")
    return "\n".join(lines)


def generate_summary_section(
    results: List[BenchmarkResult],
    config: Config,
) -> str:
    """Generate summary section with project size distribution."""
    small = medium = large = 0

    for r in results:
        if r.pagerank_timing and r.pagerank_timing.files > 0:
            files = r.pagerank_timing.files
            if files < 50:
                small += 1
            elif files < 500:
                medium += 1
            else:
                large += 1

    lines = [
        "## Summary",
        "",
        f"Binary: {config.llmcc_path}",
        "",
        "### Project Sizes",
        f"- Small (<50 files): {small} projects",
        f"- Medium (50-500 files): {medium} projects",
        f"- Large (>500 files): {large} projects",
        "",
    ]
    return "\n".join(lines)


def generate_reduction_table(results: List[BenchmarkResult], config: Config) -> str:
    """Generate PageRank graph reduction table."""
    lines = [
        f"## PageRank Graph Reduction (depth={config.depth}, top-{config.top_k})",
        "",
        "| Project | Language | Full Nodes | Full Edges | PR Nodes | PR Edges | Node Reduction | Edge Reduction |",
        "|---------|----------|------------|------------|----------|----------|----------------|----------------|",
    ]

    for r in results:
        lang = PROJECTS[r.name].language if r.name in PROJECTS else "-"

        if r.full_nodes > 0 and r.pr_nodes > 0:
            node_red = f"{r.node_reduction:.1f}%"
            edge_red = f"{r.edge_reduction:.1f}%"
            lines.append(
                f"| {r.name} | {lang} | {r.full_nodes} | {r.full_edges} | {r.pr_nodes} | {r.pr_edges} | {node_red} | {edge_red} |"
            )
        else:
            lines.append(f"| {r.name} | {lang} | - | - | - | - | - | - |")

    lines.append("")
    return "\n".join(lines)


def generate_scaling_table(
    results: List[ScalingResult],
    project: str,
    config: Config,
) -> str:
    """Generate thread scaling table."""
    from .core import get_cpu_info
    _, physical_cores, _ = get_cpu_info()

    lines = [
        f"## Thread Scaling ({project}, depth={config.depth}, top-{config.top_k}, {physical_cores} cores)",
        "",
        "| Threads | Parse | IR+Symbols | Binding | Graph | Link | Total | Speedup |",
        "|---------|-------|------------|---------|-------|------|-------|---------|",
    ]

    for r in results:
        t = r.timing
        # Always show timing values when we have a valid result (even if 0.00s)
        parse = format_time(t.parse)
        ir_sym = format_time(t.ir_symbols)
        binding = format_time(t.binding)
        graph = format_time(t.graph)
        link = format_time(t.link)
        total = format_time(t.total)
        speedup = f"{r.speedup:.2f}x" if r.threads > 1 else "-"

        lines.append(
            f"| {r.threads} | {parse} | {ir_sym} | {binding} | {graph} | {link} | {total} | {speedup} |"
        )

    lines.append("")
    return "\n".join(lines)


def generate_report(
    config: Config,
    results: List[BenchmarkResult],
    scaling_results: Optional[List[ScalingResult]] = None,
    scaling_project: str = "databend",
    output_file: Optional[Path] = None,
) -> str:
    """
    Generate complete benchmark report.

    Returns: Report content as string
    """
    info = get_system_info()

    sections = [
        "# LLMCC Benchmark Results",
        "",
        f"Generated on: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}",
        "",
        generate_machine_info_section(info),
        generate_timing_table(results, config),
        generate_summary_section(results, config),
        generate_reduction_table(results, config),
    ]

    if scaling_results:
        sections.append(generate_scaling_table(scaling_results, scaling_project, config))

    content = "\n".join(sections)

    if output_file:
        output_file.write_text(content, encoding='utf-8')

    return content
