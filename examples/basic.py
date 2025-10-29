#!/usr/bin/env python3
"""Example usage of the simplified llmcc Python API."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

import llmcc


def example_run_with_files() -> None:
    project_root = Path(__file__).parent.parent
    rust_files = [
        project_root / "crates/llmcc/src/main.rs",
        project_root / "crates/llmcc/src/lib.rs",
    ]
    print("Running llmcc over multiple Rust files...")
    result = llmcc.run(files=rust_files, lang="rust")
    print(f"Result: {result}")


def example_run_with_directory() -> None:
    project_root = Path(__file__).parent.parent
    rust_dir = project_root / "crates/llmcc-core/src"
    print("Running llmcc over a Rust directory (no query)...")
    llmcc.run(dirs=[rust_dir], lang="rust")
    print("Completed without returning output (no query specified).")


def example_run_with_query() -> None:
    project_root = Path(__file__).parent.parent
    rust_dir = project_root / "crates/llmcc-core/src"
    print("Running llmcc with a dependency query for 'caller'...")
    output = llmcc.run(dirs=[rust_dir], lang="rust", query="CompileUnit", recursive=True)
    print("Query output:\n")
    print(output or "<no results>")


def example_run_print_modes() -> None:
    project_root = Path(__file__).parent.parent
    rust_file = project_root / "crates/llmcc-core/src/context.rs"
    print("Running llmcc with IR and block printing enabled...")
    llmcc.run(files=[rust_file], lang="rust", print_ir=True, print_block=True)
    print("Printed IR and block graph to stdout.")


def example_run_design_graph() -> None:
    project_root = Path(__file__).parent.parent
    rust_dirs = [
        project_root / "crates/llmcc-core/src",
        project_root / "crates/llmcc-rust/src",
    ]
    print("Rendering compact project graph with PageRank filter...")
    graph = llmcc.run(
        dirs=rust_dirs,
        lang="rust",
        design_graph=True,
        pagerank=True,
        top_k=5,
    )
    print("Graph DOT output (first 400 chars):")
    print((graph or "<no output>")[:400])


def example_run_dependents_query() -> None:
    project_root = Path(__file__).parent.parent
    rust_dir = project_root / "crates/llmcc-core/src"
    print("Running llmcc for dependents of 'CompileCtxt' (non-recursive)...")
    output = llmcc.run(dirs=[rust_dir], lang="rust", query="CompileCtxt", dependents=True)
    print("Dependents output:\n")
    print(output or "<no results>")


def example_run_summary_query() -> None:
    project_root = Path(__file__).parent.parent
    rust_dir = project_root / "crates/llmcc-core/src"
    print("Running llmcc summary output for 'CompileCtxt'...")
    output = llmcc.run(
        dirs=[rust_dir],
        lang="rust",
        query="CompileCtxt",
        depends=True,
        summary=True,
    )
    print("Summary output:\n")
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
    example_run_print_modes()
    print("\n" + "-" * 40 + "\n")
    example_run_design_graph()
    print("\n" + "-" * 40 + "\n")
    example_run_dependents_query()
    print("\n" + "-" * 40 + "\n")
    example_run_summary_query()
    print("\n" + "-" * 40 + "\n")
    example_run_python_with_query()
