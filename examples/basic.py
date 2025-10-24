#!/usr/bin/env python3
"""Example usage of the simplified llmcc Python API."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

import llmcc


def example_run_with_files() -> None:
    project_root = Path(__file__).parent.parent
    rust_file = project_root / "crates/llmcc/src/main.rs"
    print("Running llmcc over a single Rust file...")
    result = llmcc.run(files=[rust_file], lang="rust")
    print(f"Result: {result}")


def example_run_with_directory() -> None:
    project_root = Path(__file__).parent.parent
    rust_dir = project_root / "crates/llmcc-core/src"
    print("Running llmcc over a Rust directory (no query)...")
    llmcc.run(directory=rust_dir, lang="rust")
    print("Completed without returning output (no query specified).")


def example_run_with_query() -> None:
    project_root = Path(__file__).parent.parent
    rust_dir = project_root / "crates/llmcc-core/src"
    print("Running llmcc with a dependency query for 'caller'...")
    output = llmcc.run(directory=rust_dir, lang="rust", query="CompileUnit", recursive=True)
    print("Query output:\n")
    print(output or "<no results>")

def example_run_python_with_query() -> None:
    project_root = Path(__file__).parent.parent
    rust_file = project_root / "llmcc/api.py"
    print("Running llmcc with a dependency query for 'caller' using Python API...")
    output = llmcc.run(files=[rust_file], lang="python", query="run", recursive=True)
    print("Query output:\n")
    print(output or "<no results>")


if __name__ == "__main__":
    example_run_with_files()
    print("\n" + "-" * 40 + "\n")
    example_run_with_directory()
    print("\n" + "-" * 40 + "\n")
    example_run_with_query()
    print("\n" + "-" * 40 + "\n")
    example_run_python_with_query()
