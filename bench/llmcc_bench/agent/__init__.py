"""
Agent benchmark system for measuring llmcc effectiveness.

This module provides tools to run A/B comparisons of AI agent performance
with and without llmcc-generated architecture graphs.
"""

from .config import ExperimentConfig, GraphConfig, RunLimits
from .metrics import MetricsCollector, TaskMetrics, ToolCall
from .report import ExperimentSummary, TaskSummary, generate_report
from .runner import AgentRunner, MockAgentRunner, ClaudeAgentRunner, CodexAgentRunner, RunContext
from .tasks import Task, TaskCategory, TaskDifficulty, load_tasks

__all__ = [
    # Config
    "ExperimentConfig",
    "GraphConfig",
    "RunLimits",
    # Tasks
    "Task",
    "TaskCategory",
    "TaskDifficulty",
    "load_tasks",
    # Metrics
    "MetricsCollector",
    "TaskMetrics",
    "ToolCall",
    # Runner
    "AgentRunner",
    "MockAgentRunner",
    "ClaudeAgentRunner",
    "CodexAgentRunner",
    "RunContext",
    # Report
    "ExperimentSummary",
    "TaskSummary",
    "generate_report",
]
