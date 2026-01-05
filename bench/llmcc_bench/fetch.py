"""
Fetch sample repositories for benchmarking.
"""

import shutil
import subprocess
from pathlib import Path
from typing import List, Optional

from .core import PROJECTS, Config


def fetch_repo(name: str, config: Config, force: bool = False) -> bool:
    """
    Clone a repository if it doesn't exist.

    Args:
        name: Project name
        config: Configuration
        force: If True, remove existing and re-clone

    Returns:
        True if successful or already exists, False on error
    """
    if name not in PROJECTS:
        print(f"Unknown project: {name}")
        return False

    project = PROJECTS[name]
    repo_dir = config.project_repo_path(project)

    if repo_dir.exists():
        if force:
            print(f"  Removing existing {name}...")
            shutil.rmtree(repo_dir)
        else:
            print(f"  Skipping {name} (already exists)")
            return True

    # Ensure repos directory exists
    config.repos_dir.mkdir(parents=True, exist_ok=True)

    print(f"  Cloning {project.github_path}...")
    try:
        result = subprocess.run(
            ["git", "clone", "--depth", "1", project.url, str(repo_dir)],
            capture_output=True,
            text=True
        )
        if result.returncode != 0:
            print(f"    Error: {result.stderr.strip()}")
            return False
        return True
    except FileNotFoundError:
        print("Error: git not found. Please install git.")
        return False
    except Exception as e:
        print(f"    Error: {e}")
        return False


def fetch_all(
    config: Config,
    force: bool = False,
    repos: Optional[List[str]] = None,
    verbose: bool = True,
) -> int:
    """
    Fetch all or specified repositories.

    Args:
        config: Configuration
        force: If True, remove existing and re-clone
        repos: Optional list of repo names to fetch (None = all)
        verbose: Print progress

    Returns:
        Number of failed fetches
    """
    if verbose:
        print("=== Fetching sample repositories ===")
        print(f"Sample directory: {config.sample_dir}")
        print()

    failed = 0
    to_fetch = repos if repos else list(PROJECTS.keys())

    for name in to_fetch:
        if not fetch_repo(name, config, force=force):
            failed += 1

    if verbose:
        print()
        print(f"Total: {len(to_fetch)}, Success: {len(to_fetch) - failed}, Failed: {failed}")

    return failed


def list_repos(config: Config) -> None:
    """List available repositories and their status."""
    print("Available repositories:")

    # Group by language
    by_language: dict = {}
    for name, project in PROJECTS.items():
        by_language.setdefault(project.language, []).append((name, project))

    for lang in sorted(by_language.keys()):
        print(f"  [{lang}]")
        for name, project in sorted(by_language[lang]):
            exists = "✓" if config.project_repo_path(project).exists() else "✗"
            print(f"    {exists} {name}: github.com/{project.github_path}")
