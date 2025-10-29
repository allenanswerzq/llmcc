"""Regression tests for the public llmcc Python API."""

from __future__ import annotations

from pathlib import Path
import sys

PROJECT_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PROJECT_ROOT))

import llmcc  # noqa: E402


def test_run_directory_without_query_returns_none() -> None:
    rust_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    output = llmcc.run(dirs=[rust_dir], lang="rust")
    assert output is None


def test_run_dependency_query_returns_output() -> None:
    rust_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    output = llmcc.run(
        dirs=[rust_dir],
        lang="rust",
        query="CompileCtxt",
        depends=True,
        recursive=True,
    )
    assert output is not None
    assert "CompileCtxt" in output


def test_run_design_graph_returns_dot() -> None:
    rust_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    graph = llmcc.run(
        dirs=[rust_dir],
        lang="rust",
        design_graph=True,
        pagerank=True,
        top_k=5,
    )
    assert graph is not None
    assert "digraph" in graph


def test_run_python_source_dependency_query() -> None:
    python_file = PROJECT_ROOT / "llmcc/api.py"
    output = llmcc.run(
        files=[python_file],
        lang="python",
        query="run",
        recursive=True,
    )
    assert output is not None
    assert "run" in output


def test_high_level_design_graph_with_pagerank() -> None:
    """High level design graph with PageRank."""
    rust_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    graph = llmcc.run(
        dirs=[rust_dir],
        lang="rust",
        design_graph=True,
        pagerank=True,
        top_k=100,
    )
    assert graph is not None
    assert "digraph CompactProject" in graph


def test_direct_dependencies_of_symbol() -> None:
    """Direct dependencies of a symbol."""
    rust_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    output = llmcc.run(
        dirs=[rust_dir],
        lang="rust",
        query="CompileCtxt",
        depends=True,
    )
    assert output is not None
    assert "DEPENDS ON" in output


def test_transitive_dependency_fan_out() -> None:
    """Transitive dependency fan-out."""
    rust_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    output = llmcc.run(
        dirs=[rust_dir],
        lang="rust",
        query="CompileCtxt",
        depends=True,
        recursive=True,
    )
    assert output is not None
    assert "CompileCtxt" in output


def test_direct_dependents_of_symbol() -> None:
    """Direct dependents of a symbol."""
    rust_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    output = llmcc.run(
        dirs=[rust_dir],
        lang="rust",
        query="CompileCtxt",
        dependents=True,
    )
    assert output is not None
    assert "DEPENDED BY" in output


def test_transitive_dependents_callers_view() -> None:
    """Transitive dependents (callers) view."""
    rust_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    output = llmcc.run(
        dirs=[rust_dir],
        lang="rust",
        query="CompileCtxt",
        dependents=True,
        recursive=True,
    )
    assert output is not None
    assert "CompileCtxt" in output


def test_metadata_only_summary() -> None:
    """Metadata-only summary (file + line ranges), instead of code texts."""
    rust_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    output = llmcc.run(
        dirs=[rust_dir],
        lang="rust",
        query="CompileCtxt",
        depends=True,
        summary=True,
    )
    assert output is not None
    assert "SYMBOL:" in output or "DEPENDS:" in output


def test_multiple_directories_cross_dir_analysis() -> None:
    """Apply to multiple directories, analyze relation not only inside each dir, but also cross dir."""
    rust_core_dir = PROJECT_ROOT / "crates/llmcc-core/src"
    rust_dir = PROJECT_ROOT / "crates/llmcc-rust/src"
    graph = llmcc.run(
        dirs=[rust_core_dir, rust_dir],
        lang="rust",
        design_graph=True,
        pagerank=True,
        top_k=25,
    )
    assert graph is not None
    assert "digraph CompactProject" in graph


def test_analyze_multiple_files_in_one_run() -> None:
    """Analyze multiple files in one run."""
    main_rs = PROJECT_ROOT / "crates/llmcc/src/main.rs"
    lib_rs = PROJECT_ROOT / "crates/llmcc/src/lib.rs"
    output = llmcc.run(
        files=[main_rs, lib_rs],
        lang="rust",
        query="run_main",
    )
    assert output is not None
    assert "run_main" in output
