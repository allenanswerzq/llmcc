"""Python facade for the Rust-backed llmcc workflow."""

from __future__ import annotations

from pathlib import Path
from typing import Iterable, Optional


def _normalize_files(files: Iterable[str]) -> list[str]:
    return [str(Path(path)) for path in files]


def _normalize_dir(directory: str) -> str:
    return str(Path(directory))


def run(
    files: Optional[Iterable[str]] = None,
    *,
    dirs: Optional[Iterable[str]] = None,
    lang: str = "rust",
    print_ir: bool = False,
    print_block: bool = False,
    design_graph: bool = False,
    pagerank: bool = False,
    top_k: Optional[int] = None,
    query: Optional[str] = None,
    recursive: bool = False,
    depends: bool = False,
    dependents: bool = False,
    summary: bool = False,
) -> Optional[str]:
    """Execute the core llmcc workflow from Python.

    Analyzes source code and generates context information for LLM processing.

    **Input** (required, one of):
        files: Individual source files to analyze (repeatable list).
        dirs: Directories to scan recursively (repeatable list).

    **Language** (optional):
        lang: Programming language - 'rust' or 'python' [default: 'rust'].

    **Analysis** (optional):
        design_graph: Generate high-level design graph [default: False].
        pagerank: Rank by importance using PageRank [default: False].
        top_k: Limit results to top K items [default: None (no limit)].
        query: Symbol/function name to analyze [default: None].
        depends: Show what the symbol depends on [default: False].
        dependents: Show what depends on the symbol [default: False].
        recursive: Include transitive dependencies (vs. direct only) [default: False].

    **Output format** (optional):
        summary: Show file paths and line ranges (vs. full code texts) [default: False].
        print_ir: Internal: print intermediate representation [default: False].
        print_block: Internal: print basic block graph [default: False].

    Returns:
        Analysis result as string, or None if no output generated.

    Raises:
        ValueError: If neither files nor dirs provided, both provided, or
                   conflicting options (e.g., pagerank without design_graph,
                   or both depends and dependents).

    Examples:
        Design graph with PageRank ranking:
        >>> result = llmcc.run(dirs=['src'], lang='rust',
        ...                    design_graph=True, pagerank=True, top_k=100)

        Dependencies of a symbol:
        >>> result = llmcc.run(dirs=['src'], lang='rust',
        ...                    query='MyFunction', depends=True, summary=True)

        Dependents with recursive analysis:
        >>> result = llmcc.run(dirs=['src'], lang='rust',
        ...                    query='MyFunction', dependents=True, recursive=True)

        Multiple files:
        >>> result = llmcc.run(files=['src/main.rs', 'src/lib.rs'],
        ...                    lang='rust', query='run_main')
    """

    if files is None and dirs is None:
        raise ValueError("Either 'files' or 'dirs' must be provided")
    if files is not None and dirs is not None:
        raise ValueError("Provide either 'files' or 'dirs', not both")
    if pagerank and not design_graph:
        raise ValueError("'pagerank' requires 'design_graph=True'")
    if depends and dependents:
        raise ValueError("'depends' and 'dependents' are mutually exclusive")

    file_list = _normalize_files(files) if files is not None else None
    dir_list = [_normalize_dir(path) for path in dirs] if dirs is not None else None

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
        depends=bool(depends),
        dependents=bool(dependents),
        summary=bool(summary),
    )


__all__ = ["run"]
