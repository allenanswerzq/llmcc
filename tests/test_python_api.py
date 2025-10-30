"""Ensure Python package exposes expected public API."""

import pytest

try:
    import llmcc
except ImportError as exc:  # pragma: no cover - handled by pytest skip
    pytest.skip(f"llmcc bindings not available: {exc}")


def test_llmcc_python_api_exposes_run():
    assert hasattr(llmcc, "run"), "llmcc.run should be exposed via __init__"
    assert callable(llmcc.run), "llmcc.run should be callable"
