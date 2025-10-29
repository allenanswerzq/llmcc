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
    directories: Optional[Iterable[str]] = None,
    lang: str = "rust",
    print_ir: bool = False,
    print_block: bool = False,
    design_graph: bool = False,
    pagerank: bool = False,
    top_k: Optional[int] = None,
    query: Optional[str] = None,
    recursive: bool = False,
    dependents: bool = False,
    summary: bool = False,
) -> Optional[str]:
    """Execute the core llmcc workflow from Python.
    """

    if directory is None and files is None:
        if directories is None:
            raise ValueError("Either 'files', 'directory', or 'directories' must be provided")
    if files is not None and (directory is not None or directories is not None):
        raise ValueError("Provide either 'files' or directories, not both")
    if directory is not None and directories is not None:
        raise ValueError("Use either 'directory' or 'directories', not both")
    if pagerank and not design_graph:
        raise ValueError("'pagerank' requires 'design_graph=True'")

    file_list = _normalize_files(files) if files is not None else None
    dir_list: Optional[list[str]]
    if directory is not None:
        dir_list = [_normalize_directory(directory)]
    elif directories is not None:
        dir_list = [_normalize_directory(path) for path in directories]
    else:
        dir_list = None

    import llmcc_bindings

    query_value = str(query) if query is not None else None

    return llmcc_bindings.run_llmcc(
        lang=lang,
        files=file_list,
        dirs=dir_list,
        print_ir=bool(print_ir),
        print_block=bool(print_block),
        print_design_graph=bool(design_graph),
        pagerank=bool(pagerank),
        top_k=top_k,
        query=query_value,
        recursive=bool(recursive),
        dependents=bool(dependents),
        summary=bool(summary),
    )


__all__ = ["run"]
