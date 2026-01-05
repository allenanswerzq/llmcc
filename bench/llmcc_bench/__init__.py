"""
llmcc-bench: Cross-platform benchmarking and graph generation for llmcc.

This package provides tools for:
- Fetching sample Rust repositories
- Benchmarking llmcc performance
- Generating architecture graphs at various depths
- Creating markdown reports with timing data

Usage:
    python -m llmcc_bench <command> [options]

Commands:
    fetch       Fetch sample repositories
    benchmark   Run benchmarks on sample projects
    generate    Generate architecture graphs
    clean       Clean up generated files
    info        Show system and configuration info
"""

__version__ = "0.1.0"

from .core import (
    PROJECTS,
    Config,
    find_llmcc,
    get_cpu_info,
    get_memory_info,
    get_os_info,
    get_system_info,
)
from .__main__ import main

__all__ = [
    "PROJECTS",
    "Config",
    "find_llmcc",
    "get_cpu_info",
    "get_memory_info",
    "get_os_info",
    "get_system_info",
    "main",
]
