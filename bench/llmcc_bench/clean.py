"""
Clean up generated files in sample directory.
"""

import shutil
from pathlib import Path
from typing import List

from .core import PROJECTS, Config


def clean_sample_dir(
    config: Config,
    remove_logs: bool = False,
    remove_results: bool = False,
    dry_run: bool = False,
    verbose: bool = True,
) -> int:
    """
    Clean up generated files in the sample directory.

    Removes project output directories (e.g., databend/, databend-pagerank/).
    Keeps: repos/, scripts, and optionally benchmark_logs and results.

    Args:
        config: Configuration
        remove_logs: Also remove benchmark_logs/
        remove_results: Also remove benchmark_results*.md
        dry_run: Print what would be removed without removing
        verbose: Print progress

    Returns:
        Number of items removed
    """
    sample_dir = config.sample_dir

    if verbose:
        print("=== Cleaning sample directory ===")
        print(f"Directory: {sample_dir}")
        if dry_run:
            print("(dry run - no files will be removed)")
        print()

    removed = 0
    keep_dirs = {"repos", "__pycache__"}

    if not remove_logs:
        keep_dirs.add("benchmark_logs")

    for item in sample_dir.iterdir():
        # Handle files
        if item.is_file():
            # Only remove benchmark results if requested
            if remove_results and item.name.startswith("benchmark_results") and item.suffix == ".md":
                if verbose:
                    print(f"Removing: {item.name}")
                if not dry_run:
                    item.unlink()
                removed += 1
            continue

        # Skip preserved directories
        if item.name in keep_dirs:
            continue

        # Remove project output directories
        if item.is_dir():
            base_name = item.name.replace("-pagerank", "")
            if base_name in PROJECTS or item.name.endswith("-pagerank"):
                if verbose:
                    print(f"Removing: {item.name}/")
                if not dry_run:
                    shutil.rmtree(item)
                removed += 1

    # Remove benchmark_logs if requested
    if remove_logs:
        logs_dir = config.benchmark_logs_dir
        if logs_dir.exists():
            if verbose:
                print(f"Removing: benchmark_logs/")
            if not dry_run:
                shutil.rmtree(logs_dir)
            removed += 1

    if verbose:
        print()
        print(f"Removed {removed} items")
        if not remove_logs:
            print()
            print("Kept: repos/, benchmark_logs/, scripts")
            print("Use '--all' to also remove benchmark_logs/ and benchmark_results*.md")

    return removed
