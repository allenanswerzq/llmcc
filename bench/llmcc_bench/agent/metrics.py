"""
Metrics collection for agent benchmark runs.
"""

import json
import time
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List, Literal, Optional

# Use monotonic clock for timing to avoid issues with system time adjustments
_monotonic_offset = time.time() - time.monotonic()


def monotonic_time() -> float:
    """Get current time using monotonic clock, offset to match wall time."""
    return time.monotonic() + _monotonic_offset


@dataclass
class ToolCall:
    """Record of a single tool invocation."""

    timestamp: float
    """Unix timestamp when the tool was called."""

    tool_name: str
    """Name of the tool (e.g., 'read_file', 'grep_search')."""

    parameters: Dict[str, Any]
    """Parameters passed to the tool."""

    result_preview: str = ""
    """Truncated preview of the result (for logging)."""

    duration_seconds: float = 0.0
    """Time taken to execute the tool."""

    tokens_in: int = 0
    """Tokens in the tool call parameters."""

    tokens_out: int = 0
    """Tokens in the tool result."""

    success: bool = True
    """Whether the tool call succeeded."""

    error: Optional[str] = None
    """Error message if the tool call failed."""

    def to_dict(self) -> Dict:
        """Convert to dictionary for JSON serialization."""
        return {
            "timestamp": self.timestamp,
            "tool_name": self.tool_name,
            "parameters": self.parameters,
            "result_preview": self.result_preview,
            "duration_seconds": self.duration_seconds,
            "tokens_in": self.tokens_in,
            "tokens_out": self.tokens_out,
            "success": self.success,
            "error": self.error,
        }

    @classmethod
    def from_dict(cls, data: Dict) -> "ToolCall":
        """Create from dictionary."""
        return cls(
            timestamp=data["timestamp"],
            tool_name=data["tool_name"],
            parameters=data.get("parameters", {}),
            result_preview=data.get("result_preview", ""),
            duration_seconds=data.get("duration_seconds", 0.0),
            tokens_in=data.get("tokens_in", 0),
            tokens_out=data.get("tokens_out", 0),
            success=data.get("success", True),
            error=data.get("error"),
        )


@dataclass
class TaskMetrics:
    """Complete metrics for a single task run."""

    # Identification
    task_id: str
    """Task identifier."""

    condition: Literal["baseline", "with_llmcc"]
    """Experiment condition."""

    run_id: str
    """Unique run identifier."""

    # Timing
    start_time: float = 0.0
    """Unix timestamp when run started."""

    end_time: float = 0.0
    """Unix timestamp when run ended."""

    wall_time_seconds: float = 0.0
    """Total wall clock time."""

    # Tool usage
    tool_calls: List[ToolCall] = field(default_factory=list)
    """All tool calls made during the run."""

    tool_calls_total: int = 0
    """Total number of tool calls."""

    tool_calls_by_type: Dict[str, int] = field(default_factory=dict)
    """Count of tool calls by type."""

    # Token usage
    tokens_input: int = 0
    """Total input tokens to LLM."""

    tokens_output: int = 0
    """Total output tokens from LLM."""

    llm_calls: int = 0
    """Number of LLM API calls."""

    # Quality metrics
    task_completed: bool = False
    """Whether the agent reported task completion."""

    files_modified: List[str] = field(default_factory=list)
    """Files that were modified."""

    validation_passed: bool = False
    """Whether validation command passed."""

    validation_output: str = ""
    """Output from validation command."""

    error_count: int = 0
    """Number of errors encountered."""

    errors: List[str] = field(default_factory=list)
    """Error messages."""

    # Termination
    termination_reason: str = ""
    """Why the run ended (completed, max_tools, max_tokens, max_time, error)."""

    # Graph info (for with_llmcc condition)
    graph_tokens: int = 0
    """Tokens in the graph context."""

    graph_nodes: int = 0
    """Number of nodes in the graph."""

    graph_edges: int = 0
    """Number of edges in the graph."""

    def to_dict(self) -> Dict:
        """Convert to dictionary for JSON serialization."""
        return {
            "task_id": self.task_id,
            "condition": self.condition,
            "run_id": self.run_id,
            "start_time": self.start_time,
            "end_time": self.end_time,
            "wall_time_seconds": self.wall_time_seconds,
            "tool_calls": [tc.to_dict() for tc in self.tool_calls],
            "tool_calls_total": self.tool_calls_total,
            "tool_calls_by_type": self.tool_calls_by_type,
            "tokens_input": self.tokens_input,
            "tokens_output": self.tokens_output,
            "llm_calls": self.llm_calls,
            "task_completed": self.task_completed,
            "files_modified": self.files_modified,
            "validation_passed": self.validation_passed,
            "validation_output": self.validation_output,
            "error_count": self.error_count,
            "errors": self.errors,
            "termination_reason": self.termination_reason,
            "graph_tokens": self.graph_tokens,
            "graph_nodes": self.graph_nodes,
            "graph_edges": self.graph_edges,
        }

    @classmethod
    def from_dict(cls, data: Dict) -> "TaskMetrics":
        """Create from dictionary."""
        metrics = cls(
            task_id=data["task_id"],
            condition=data["condition"],
            run_id=data["run_id"],
        )
        metrics.start_time = data.get("start_time", 0.0)
        metrics.end_time = data.get("end_time", 0.0)
        metrics.wall_time_seconds = data.get("wall_time_seconds", 0.0)
        metrics.tool_calls = [ToolCall.from_dict(tc) for tc in data.get("tool_calls", [])]
        metrics.tool_calls_total = data.get("tool_calls_total", 0)
        metrics.tool_calls_by_type = data.get("tool_calls_by_type", {})
        metrics.tokens_input = data.get("tokens_input", 0)
        metrics.tokens_output = data.get("tokens_output", 0)
        metrics.llm_calls = data.get("llm_calls", 0)
        metrics.task_completed = data.get("task_completed", False)
        metrics.files_modified = data.get("files_modified", [])
        metrics.validation_passed = data.get("validation_passed", False)
        metrics.validation_output = data.get("validation_output", "")
        metrics.error_count = data.get("error_count", 0)
        metrics.errors = data.get("errors", [])
        metrics.termination_reason = data.get("termination_reason", "")
        metrics.graph_tokens = data.get("graph_tokens", 0)
        metrics.graph_nodes = data.get("graph_nodes", 0)
        metrics.graph_edges = data.get("graph_edges", 0)
        return metrics

    def to_json(self) -> str:
        """Convert to JSON string."""
        return json.dumps(self.to_dict(), indent=2)

    @classmethod
    def from_json(cls, json_str: str) -> "TaskMetrics":
        """Create from JSON string."""
        return cls.from_dict(json.loads(json_str))


class MetricsCollector:
    """Collects metrics during an agent run."""

    def __init__(
        self,
        task_id: str,
        condition: Literal["baseline", "with_llmcc"],
        run_id: Optional[str] = None,
    ):
        self.task_id = task_id
        self.condition = condition
        self.run_id = run_id or f"{condition}_{datetime.now().strftime('%Y%m%d_%H%M%S')}"

        self._tool_calls: List[ToolCall] = []
        self._start_time: Optional[float] = None
        self._end_time: Optional[float] = None
        self._tokens_input = 0
        self._tokens_output = 0
        self._llm_calls = 0
        self._files_modified: List[str] = []
        self._errors: List[str] = []
        self._task_completed = False
        self._validation_passed = False
        self._validation_output = ""
        self._termination_reason = ""
        self._graph_tokens = 0
        self._graph_nodes = 0
        self._graph_edges = 0

    def start(self) -> None:
        """Mark the start of the run."""
        self._start_time = monotonic_time()

    def end(self, reason: str = "completed") -> None:
        """Mark the end of the run."""
        self._end_time = monotonic_time()
        self._termination_reason = reason

    def record_tool_call(self, tool_call: ToolCall) -> None:
        """Record a tool call."""
        self._tool_calls.append(tool_call)

    def record_llm_call(self, tokens_in: int, tokens_out: int) -> None:
        """Record an LLM API call."""
        self._llm_calls += 1
        self._tokens_input += tokens_in
        self._tokens_output += tokens_out

    def record_file_modified(self, path: str) -> None:
        """Record that a file was modified."""
        if path not in self._files_modified:
            self._files_modified.append(path)

    def record_error(self, error: str) -> None:
        """Record an error."""
        self._errors.append(error)

    def set_task_completed(self, completed: bool = True) -> None:
        """Set whether the task was completed."""
        self._task_completed = completed

    def set_validation_result(self, passed: bool, output: str = "") -> None:
        """Set validation result."""
        self._validation_passed = passed
        self._validation_output = output

    def set_graph_info(self, tokens: int, nodes: int, edges: int) -> None:
        """Set graph information for with_llmcc condition."""
        self._graph_tokens = tokens
        self._graph_nodes = nodes
        self._graph_edges = edges

    @property
    def tool_calls_total(self) -> int:
        """Get total tool call count."""
        return len(self._tool_calls)

    @property
    def total_tokens(self) -> int:
        """Get total token count."""
        return self._tokens_input + self._tokens_output

    @property
    def wall_time(self) -> float:
        """Get current wall time."""
        if self._start_time is None:
            return 0.0
        end = self._end_time or monotonic_time()
        return end - self._start_time

    def get_metrics(self) -> TaskMetrics:
        """Get the final metrics object."""
        # Calculate tool calls by type
        tool_calls_by_type: Dict[str, int] = {}
        for tc in self._tool_calls:
            tool_calls_by_type[tc.tool_name] = tool_calls_by_type.get(tc.tool_name, 0) + 1

        return TaskMetrics(
            task_id=self.task_id,
            condition=self.condition,
            run_id=self.run_id,
            start_time=self._start_time or 0.0,
            end_time=self._end_time or monotonic_time(),
            wall_time_seconds=self.wall_time,
            tool_calls=self._tool_calls.copy(),
            tool_calls_total=len(self._tool_calls),
            tool_calls_by_type=tool_calls_by_type,
            tokens_input=self._tokens_input,
            tokens_output=self._tokens_output,
            llm_calls=self._llm_calls,
            task_completed=self._task_completed,
            files_modified=self._files_modified.copy(),
            validation_passed=self._validation_passed,
            validation_output=self._validation_output,
            error_count=len(self._errors),
            errors=self._errors.copy(),
            termination_reason=self._termination_reason,
            graph_tokens=self._graph_tokens,
            graph_nodes=self._graph_nodes,
            graph_edges=self._graph_edges,
        )


def save_metrics(metrics: TaskMetrics, output_path: Path) -> None:
    """Save metrics to a JSON file."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "w") as f:
        f.write(metrics.to_json())


def load_metrics(path: Path) -> TaskMetrics:
    """Load metrics from a JSON file."""
    with open(path) as f:
        return TaskMetrics.from_json(f.read())


def append_metrics_jsonl(metrics: TaskMetrics, output_path: Path) -> None:
    """Append metrics to a JSONL file (one JSON object per line)."""
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with open(output_path, "a") as f:
        f.write(json.dumps(metrics.to_dict()) + "\n")


def load_metrics_jsonl(path: Path) -> List[TaskMetrics]:
    """Load all metrics from a JSONL file."""
    metrics = []
    with open(path) as f:
        for line in f:
            if line.strip():
                metrics.append(TaskMetrics.from_dict(json.loads(line)))
    return metrics
