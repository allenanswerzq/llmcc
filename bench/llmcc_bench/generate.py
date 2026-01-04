"""
Generate architecture graphs for sample projects.
"""

import shutil
import subprocess
from pathlib import Path
from typing import List, Optional

from .core import (
    PROJECTS,
    Config,
    count_loc,
    format_loc,
    run_command,
)


# Depth level names for output files
DEPTH_NAMES = {
    0: "depth_0_project",
    1: "depth_1_crate",
    2: "depth_2_module",
    3: "depth_3_file",
}


def compute_top_k(loc: int, depth: int) -> int:
    """
    Compute top-K values based on LoC and depth.
    Larger codebases get larger top-K to keep graphs readable.
    """
    if depth == 1:  # Crate level
        if loc > 400000:
            return 25
        elif loc > 200000:
            return 20
        elif loc > 50000:
            return 15
        else:
            return 10
    elif depth == 2:  # Module level
        if loc > 400000:
            return 50
        elif loc > 200000:
            return 40
        elif loc > 50000:
            return 30
        else:
            return 20
    elif depth == 3:  # File level
        if loc > 400000:
            return 300
        elif loc > 200000:
            return 250
        elif loc > 50000:
            return 200
        else:
            return 150
    return 0


def generate_svg(
    dot_file: Path,
    svg_file: Path,
    timeout: int = 20,
    size_threshold: int = 500000,
) -> bool:
    """
    Generate SVG from DOT file using Graphviz.

    Returns: True if successful
    """
    if not shutil.which("dot"):
        return False

    # Check file size
    if dot_file.stat().st_size > size_threshold:
        svg_file.write_text(f"<!-- SVG skipped: {dot_file.stat().st_size} bytes -->")
        return False

    try:
        result = subprocess.run(
            ["dot", "-Tsvg", str(dot_file), "-o", str(svg_file)],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return result.returncode == 0
    except subprocess.TimeoutExpired:
        svg_file.write_text(f"<!-- SVG timeout: {timeout}s -->")
        return False
    except Exception:
        return False


def generate_graphs(
    name: str,
    config: Config,
    output_dir: Path,
    use_pagerank: bool = False,
    loc: int = 0,
    skip_svg: bool = True,
    verbose: bool = True,
) -> bool:
    """
    Generate graphs for a project at all depth levels.

    Returns: True if successful
    """
    if name not in PROJECTS:
        if verbose:
            print(f"  Skipping {name} (unknown project)")
        return False

    project = PROJECTS[name]
    src_dir = config.project_repo_path(project)

    if not src_dir.exists():
        if verbose:
            print(f"  Skipping {name} (not found)")
        return False

    if not config.llmcc_path:
        if verbose:
            print("Error: llmcc binary not found")
        return False

    output_dir.mkdir(parents=True, exist_ok=True)

    for depth in range(4):
        depth_name = DEPTH_NAMES[depth]
        dot_file = output_dir / f"{depth_name}.dot"

        cmd = [
            str(config.llmcc_path),
            "-d", str(src_dir),
            "--graph",
            "--depth", str(depth),
            "-o", str(dot_file),
        ]

        # Add PageRank filtering for pagerank mode
        if use_pagerank:
            top_k = compute_top_k(loc, depth)
            if top_k > 0:
                cmd.extend(["--pagerank-top-k", str(top_k)])
                if verbose:
                    print(f"    {depth_name} (top-{top_k})...")
            else:
                if verbose:
                    print(f"    {depth_name}...")
        else:
            if verbose:
                print(f"    {depth_name}...")

        # Add layout flags for large projects at module level
        if depth == 2 and loc > 50000:
            cmd.extend(["--cluster-by-crate", "--short-labels"])

        result = run_command(cmd, capture=True)
        if result.returncode != 0:
            if verbose:
                print(f"      Error: {result.stderr}")
            return False

    # Generate SVGs if not skipped
    if not skip_svg and shutil.which("dot"):
        if verbose:
            print("    Generating SVG files...")
        for dot_file in output_dir.glob("*.dot"):
            svg_file = dot_file.with_suffix(".svg")
            if generate_svg(dot_file, svg_file):
                if verbose:
                    print(f"      ✓ {dot_file.name}")
            else:
                if verbose:
                    print(f"      ✗ {dot_file.name}")

    return True


def generate_all(
    config: Config,
    projects: Optional[List[str]] = None,
    skip_svg: bool = True,
    verbose: bool = True,
) -> int:
    """
    Generate graphs for all or specified projects.

    Returns: Number of failed projects
    """
    to_generate = projects if projects else list(PROJECTS.keys())

    # Calculate LoC for all projects (use fast estimate)
    if verbose:
        print("=== Calculating LoC for all projects ===")

    project_loc = {}
    for name in to_generate:
        if name not in PROJECTS:
            project_loc[name] = 0
            continue
        project = PROJECTS[name]
        src_dir = config.project_repo_path(project)
        if src_dir.exists():
            # Use estimate for speed (file_count * 200)
            loc = count_loc(src_dir, use_estimate=True)
            project_loc[name] = loc
            if verbose:
                print(f"  {name}: {format_loc(loc)}")
        else:
            project_loc[name] = 0

    # Sort by LoC descending
    sorted_projects = sorted(to_generate, key=lambda n: project_loc.get(n, 0), reverse=True)

    if verbose:
        print()
        print("=== Generating graphs ===")

    failed = 0
    for name in sorted_projects:
        loc = project_loc.get(name, 0)

        if loc == 0:
            if verbose:
                print(f"Skipping {name} (not found)")
            continue

        if verbose:
            print()
            print(f"=== {name} ({format_loc(loc)}) ===")

        project = PROJECTS[name]

        # Full graphs (no PageRank filtering)
        if verbose:
            print("  [Full graphs]")
        full_dir = config.project_output_dir(project)
        if not generate_graphs(name, config, full_dir, use_pagerank=False, loc=loc, skip_svg=skip_svg, verbose=verbose):
            failed += 1

        # PageRank filtered
        if verbose:
            print("  [PageRank filtered]")
        pr_dir = config.project_output_dir(project, suffix="-pagerank")
        if not generate_graphs(name, config, pr_dir, use_pagerank=True, loc=loc, skip_svg=skip_svg, verbose=verbose):
            failed += 1

    return failed
