"""
llmcc - LLM Context Compiler

A universal context builder for any language and document type,
enabling Python developers to analyze code using Rust-backed compilation.
"""

__version__ = "0.2.49"

try:
    import llmcc_bindings
except ImportError as exc:
    raise ImportError(
        "Failed to import llmcc bindings. "
        "Ensure the Rust extension is built before using the Python API."
    ) from exc

if not hasattr(llmcc_bindings, "run_llmcc"):
    raise ImportError(
        "The llmcc bindings are missing the 'run_llmcc' entry point."
        " Rebuild the project to regenerate the Python extension."
    )

VERSION = getattr(llmcc_bindings, "VERSION", __version__)

from .api import run

run_llmcc = llmcc_bindings.run_llmcc

__all__ = ["run", "run_llmcc", "VERSION", "__version__"]
