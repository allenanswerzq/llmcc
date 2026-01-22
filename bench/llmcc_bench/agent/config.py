"""
Configuration for agent benchmark experiments.
"""

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import List, Literal, Optional


class Condition(str, Enum):
    """Experiment condition."""
    BASELINE = "baseline"
    WITH_LLMCC = "with_llmcc"


@dataclass
class RunLimits:
    """Limits for a single agent run."""

    max_tool_calls: int = 100
    """Maximum number of tool invocations before termination."""

    max_tokens: int = 200_000
    """Maximum total tokens (input + output) before termination."""

    max_wall_time_seconds: float = 600.0
    """Maximum wall clock time in seconds (default: 10 minutes)."""

    max_llm_calls: int = 50
    """Maximum number of LLM API calls."""

    def should_terminate(
        self,
        tool_calls: int,
        tokens: int,
        wall_time: float,
        llm_calls: int,
    ) -> bool:
        """Check if any limit has been exceeded."""
        return (
            tool_calls >= self.max_tool_calls
            or tokens >= self.max_tokens
            or wall_time >= self.max_wall_time_seconds
            or llm_calls >= self.max_llm_calls
        )


@dataclass
class GraphConfig:
    """Configuration for llmcc graph generation."""

    depth: int = 2
    """Graph depth: 0=project, 1=crate, 2=module, 3=file+symbol."""

    pagerank_top_k: Optional[int] = 100
    """Limit nodes to top-k by PageRank. None = all nodes."""

    cluster_by_crate: bool = True
    """Group modules by parent crate in visualization."""

    short_labels: bool = False
    """Use shortened labels for nodes."""

    def to_cli_args(self) -> List[str]:
        """Convert to llmcc CLI arguments."""
        args = ["--graph", "--depth", str(self.depth)]

        if self.pagerank_top_k is not None:
            args.extend(["--pagerank-top-k", str(self.pagerank_top_k)])

        if self.cluster_by_crate:
            args.append("--cluster-by-crate")

        if self.short_labels:
            args.append("--short-labels")

        return args

    @property
    def name(self) -> str:
        """Human-readable name for this config."""
        if self.pagerank_top_k:
            return f"depth{self.depth}_top{self.pagerank_top_k}"
        return f"depth{self.depth}_full"


# Preset configurations
GRAPH_CONFIGS = {
    "minimal": GraphConfig(depth=1, pagerank_top_k=20),
    "compact": GraphConfig(depth=2, pagerank_top_k=50),
    "standard": GraphConfig(depth=2, pagerank_top_k=100),
    "detailed": GraphConfig(depth=3, pagerank_top_k=200),
    "full": GraphConfig(depth=3, pagerank_top_k=None),
}


@dataclass
class ExperimentConfig:
    """Configuration for a complete benchmark experiment."""

    # Repo settings
    repo: str
    """Repository identifier (e.g., 'tokio-rs/tokio')."""

    repo_path: Optional[Path] = None
    """Local path to repository. If None, will be cloned."""

    commit: Optional[str] = None
    """Specific commit to checkout. If None, uses HEAD."""

    # Experiment settings
    runs_per_condition: int = 3
    """Number of runs per (task, condition) pair for statistical validity."""

    conditions: List[Condition] = field(
        default_factory=lambda: [Condition.BASELINE, Condition.WITH_LLMCC]
    )
    """Conditions to compare."""

    graph_config: GraphConfig = field(default_factory=lambda: GRAPH_CONFIGS["detailed"])
    """Graph configuration for WITH_LLMCC condition."""

    run_limits: RunLimits = field(default_factory=RunLimits)
    """Limits for each agent run."""

    # Output settings
    output_dir: Optional[Path] = None
    """Directory for results. If None, uses bench/results/<timestamp>_<repo>/."""

    # LLM settings
    model: str = "claude-opus-4-5-20251101"
    """LLM model to use."""

    temperature: float = 0.0
    """LLM temperature (0 for more deterministic results)."""

    # Task filtering
    task_ids: Optional[List[str]] = None
    """Specific task IDs to run. If None, runs all tasks for the repo."""

    task_categories: Optional[List[str]] = None
    """Filter tasks by category (exploration, small, medium, large)."""

    task_difficulties: Optional[List[str]] = None
    """Filter tasks by difficulty (small, medium, large)."""

    parallel: int = 1
    """Number of tasks to run in parallel. Default 1 (sequential)."""

    sample: Optional[int] = None
    """Randomly sample this many tasks. If None, run all tasks."""

    def validate(self) -> None:
        """Validate configuration."""
        if self.runs_per_condition < 1:
            raise ValueError("runs_per_condition must be at least 1")

        if self.graph_config.depth < 0 or self.graph_config.depth > 3:
            raise ValueError("graph depth must be 0-3")

        if not self.conditions:
            raise ValueError("at least one condition must be specified")


@dataclass
class ComparisonConfig:
    """Configuration for comparing multiple graph configurations."""

    repo: str
    """Repository to benchmark."""

    graph_configs: List[GraphConfig] = field(
        default_factory=lambda: list(GRAPH_CONFIGS.values())
    )
    """Graph configurations to compare."""

    runs_per_config: int = 3
    """Number of runs per configuration."""

    task_ids: Optional[List[str]] = None
    """Specific tasks to run. If None, runs all."""
