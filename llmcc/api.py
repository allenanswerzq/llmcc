"""Python facade for the Rust-backed llmcc workflow."""

from __future__ import annotations

from pathlib import Path
from typing import Iterable, Optional


def _normalize_files(files: Iterable[str]) -> list[str]:
    return [str(Path(path)) for path in files]


def _normalize_directory(directory: str) -> str:
    return str(Path(directory))


def run(
    files: Optional[Iterable[str]] = None,
    *,
    directory: Optional[str] = None,
    lang: str = "rust",
    print_ir: bool = False,
    print_graph: bool = False,
    compact_graph: bool = False,
    query: Optional[str] = None,
    recursive: bool = False,
    dependents: bool = False,
) -> Optional[str]:
    """Execute the core llmcc workflow from Python.

    This mirrors the behaviour of the Rust CLI in ``crates/llmcc/src/main.rs``.
    Provide either ``files`` or ``directory``. When ``query`` is supplied, the
    formatted dependency report is returned; otherwise ``None`` is returned.
    """

    if directory is None and files is None:
        raise ValueError("Either 'files' or 'directory' must be provided")

    file_list = _normalize_files(files) if files is not None else None
    dir_path = _normalize_directory(directory) if directory is not None else None

    import llmcc_bindings

    query_value = str(query) if query is not None else None

    if recursive and dependents:
        raise ValueError("'recursive' and 'dependents' cannot be used together")

    return llmcc_bindings.run_llmcc(
        lang,
        file_list,
        dir_path,
        bool(print_ir),
        bool(print_graph),
        bool(compact_graph),
        query_value,
        bool(recursive),
        bool(dependents),
    )


__all__ = ["run"]
