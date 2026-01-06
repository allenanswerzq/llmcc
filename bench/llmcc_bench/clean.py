"""
Clean up generated files in sample directory.
"""

import shutil
from pathlib import Path
from typing import List

from .core import PROJECTS, Config


def clean_sample_dir(
    config: Config,
    remove_all: bool = False,
    dry_run: bool = False,
    verbose: bool = True,
) -> int:
    """
    Clean up generated files in the sample directory.

    Args:
        config: Configuration
        remove_all: Remove entire sample directory and recreate it
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

    # If --all, delete entire sample directory and recreate
    if remove_all:
        if verbose:
            print(f"Removing entire sample directory: {sample_dir}")
        if not dry_run:
            if sample_dir.exists():
                shutil.rmtree(sample_dir)
            sample_dir.mkdir(parents=True, exist_ok=True)
        if verbose:
            print("Recreated empty sample directory")
        return 1

    removed = 0
    keep_dirs = {"repos", "benchmark_logs", "__pycache__"}

    for item in sample_dir.iterdir():
        # Handle files - skip all files in normal clean
        if item.is_file():
            continue

        # Skip preserved directories
        if item.name in keep_dirs:
            continue

        # Remove project output directories (rust/, typescript/, etc.)
        if item.is_dir():
            if verbose:
                print(f"Removing: {item.name}/")
            if not dry_run:
                shutil.rmtree(item)
            removed += 1

    if verbose:
        print()
        print(f"Removed {removed} items")
        print()
        print("Kept: repos/, benchmark_logs/")
        print("Use '--all' to delete entire sample directory")

    return removed
