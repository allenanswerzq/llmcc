"""
Agent runner interface and implementations for benchmarking.
"""

import asyncio
import os
import re
import subprocess
import time
from abc import ABC, abstractmethod
from dataclasses import dataclass
from pathlib import Path
from typing import Any, AsyncIterator, Dict, List, Literal, Optional, Tuple

from .config import Condition, GraphConfig, RunLimits
from .metrics import MetricsCollector, TaskMetrics, ToolCall, monotonic_time
from .tasks import Task


@dataclass
class RunContext:
    """Context for a single agent run."""

    task: Task
    """The task being executed."""

    condition: Condition
    """Experiment condition (baseline or with_llmcc)."""

    workspace_path: Path
    """Path to the repository workspace."""

    graph_context: Optional[str] = None
    """DOT graph content (for with_llmcc condition)."""

    run_id: str = ""
    """Unique identifier for this run."""

    limits: RunLimits = None  # type: ignore
    """Run limits."""

    def __post_init__(self):
        if self.limits is None:
            self.limits = RunLimits()


class AgentRunner(ABC):
    """Abstract interface for running an agent on a task."""

    @abstractmethod
    async def run(self, context: RunContext) -> TaskMetrics:
        """
        Execute agent on the task and return metrics.

        Args:
            context: The run context with task, workspace, and graph.

        Returns:
            TaskMetrics with all collected metrics.
        """
        ...

    @abstractmethod
    def get_name(self) -> str:
        """Get the name of this runner implementation."""
        ...


class MockAgentRunner(AgentRunner):
    """
    Simulated agent for testing the benchmark harness.

    Generates realistic-looking metrics without actually calling an LLM.
    Useful for testing the benchmark infrastructure.
    """

    def __init__(
        self,
        avg_tool_calls: int = 30,
        avg_tokens: int = 50000,
        success_rate: float = 0.8,
        graph_improvement: float = 0.5,  # 50% reduction with graph
        seed: Optional[int] = None,  # Random seed for reproducibility
    ):
        self.avg_tool_calls = avg_tool_calls
        self.avg_tokens = avg_tokens
        self.success_rate = success_rate
        self.graph_improvement = graph_improvement
        self.seed = seed

    def get_name(self) -> str:
        return "mock"

    async def run(self, context: RunContext) -> TaskMetrics:
        """Simulate an agent run with realistic metrics."""
        import random
        import hashlib

        # Use seed if provided for reproducibility
        # Use hashlib for consistent hashing across Python runs
        if self.seed is not None:
            run_hash = hashlib.md5(context.run_id.encode()).hexdigest()
            task_hash = hashlib.md5(context.task.id.encode()).hexdigest()
            combined = self.seed + int(run_hash[:8], 16) + int(task_hash[:8], 16)
            random.seed(combined)

        collector = MetricsCollector(
            task_id=context.task.id,
            condition=context.condition.value,
            run_id=context.run_id,
        )

        collector.start()

        # Scale difficulty based on task
        difficulty_multiplier = {
            "easy": 0.6,
            "medium": 1.0,
            "hard": 1.5,
        }.get(context.task.difficulty.value if hasattr(context.task, 'difficulty') else "medium", 1.0)

        # Apply graph improvement factor
        if context.condition == Condition.WITH_LLMCC:
            tool_multiplier = 1 - self.graph_improvement
            token_multiplier = 1 - self.graph_improvement * 0.8
        else:
            tool_multiplier = 1.0
            token_multiplier = 1.0

        # Generate random tool calls (scaled by difficulty)
        base_calls = self.avg_tool_calls * difficulty_multiplier
        num_tool_calls = int(base_calls * tool_multiplier * random.uniform(0.7, 1.3))
        base_tokens = self.avg_tokens * difficulty_multiplier
        total_tokens = int(base_tokens * token_multiplier * random.uniform(0.8, 1.2))

        tool_types = [
            ("read_file", 0.35),
            ("grep_search", 0.20),
            ("semantic_search", 0.10),
            ("list_dir", 0.10),
            ("run_in_terminal", 0.10),
            ("replace_string_in_file", 0.10),
            ("create_file", 0.05),
        ]

        tokens_per_call = total_tokens // max(num_tool_calls, 1)

        for i in range(num_tool_calls):
            # Pick a random tool type based on weights
            r = random.random()
            cumulative = 0.0
            tool_name = "read_file"
            for name, weight in tool_types:
                cumulative += weight
                if r <= cumulative:
                    tool_name = name
                    break

            tool_call = ToolCall(
                timestamp=monotonic_time(),
                tool_name=tool_name,
                parameters={"mock": True, "index": i},
                result_preview=f"Mock result for {tool_name}",
                duration_seconds=random.uniform(0.1, 0.5),
                tokens_in=tokens_per_call // 3,
                tokens_out=tokens_per_call * 2 // 3,
                success=True,
            )
            collector.record_tool_call(tool_call)

            # Record LLM call every few tool calls
            if i % 3 == 0:
                collector.record_llm_call(
                    tokens_in=random.randint(1000, 5000),
                    tokens_out=random.randint(200, 1000),
                )

            # Simulate some time passing (scaled to tool type)
            if tool_name in ("read_file", "grep_search", "semantic_search"):
                await asyncio.sleep(random.uniform(0.02, 0.05))
            else:
                await asyncio.sleep(random.uniform(0.05, 0.15))

        # Determine success
        success = random.random() < self.success_rate
        if context.condition == Condition.WITH_LLMCC:
            # Higher success rate with graph
            success = random.random() < min(self.success_rate + 0.15, 1.0)

        collector.set_task_completed(success)
        collector.set_validation_result(success, "Mock validation")

        if context.task.expected_files:
            for f in context.task.expected_files:
                collector.record_file_modified(f)

        # Set graph info if applicable
        if context.condition == Condition.WITH_LLMCC and context.graph_context:
            nodes = context.graph_context.count("[label=")
            edges = context.graph_context.count("->")
            tokens = len(context.graph_context) // 4  # Rough estimate
            collector.set_graph_info(tokens, nodes, edges)

        collector.end("completed" if success else "failed")

        return collector.get_metrics()


def generate_graph(
    repo_path: Path,
    config: GraphConfig,
    language: str = "rust",
) -> Tuple[str, int, int]:
    """
    Generate an llmcc graph for the repository.

    Args:
        repo_path: Path to the repository.
        config: Graph configuration.
        language: Programming language (rust, typescript).

    Returns:
        Tuple of (graph_content, node_count, edge_count).
    """
    # Build llmcc command
    cmd = ["llmcc", "-d", str(repo_path), "--lang", language]
    cmd.extend(config.to_cli_args())

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=300,  # 5 minute timeout
        )

        if result.returncode != 0:
            raise RuntimeError(f"llmcc failed: {result.stderr}")

        graph_content = result.stdout

        # Count nodes and edges
        node_count = len(re.findall(r'\[label=', graph_content))
        edge_count = len(re.findall(r'->', graph_content))

        return graph_content, node_count, edge_count

    except subprocess.TimeoutExpired:
        raise RuntimeError("llmcc timed out after 5 minutes")
    except FileNotFoundError:
        raise RuntimeError("llmcc not found in PATH")


def build_system_prompt(task: Task, graph: Optional[str] = None) -> str:
    """
    Build the system prompt for the agent.

    Args:
        task: The task to perform.
        graph: Optional llmcc graph to include.

    Returns:
        System prompt string.
    """
    base_prompt = """You are an expert AI coding assistant. Your goal is to complete the given task efficiently and accurately.

You have access to standard coding tools:
- read_file: Read file contents
- grep_search: Search for text patterns
- semantic_search: Search by meaning
- list_dir: List directory contents
- run_in_terminal: Run shell commands
- replace_string_in_file: Edit files
- create_file: Create new files

Complete the task step by step. When done, clearly state that the task is complete."""

    if graph:
        return f"""{base_prompt}

## Architecture Graph

The following is a dependency graph of the codebase generated by llmcc.
Use this to understand the codebase structure before exploring files.
The graph shows:
- Nodes: modules, structs, functions, interfaces
- Edges: dependencies (imports, calls, type references)

```dot
{graph}
```

Use this graph to navigate directly to relevant code instead of searching blindly.
"""

    return base_prompt


async def run_validation(
    task: Task,
    workspace_path: Path,
) -> Tuple[bool, str]:
    """
    Run the validation command for a task.

    Args:
        task: The task with validation_command.
        workspace_path: Path to the workspace.

    Returns:
        Tuple of (passed, output).
    """
    if not task.validation_command:
        return True, "No validation command specified"

    try:
        result = subprocess.run(
            task.validation_command,
            shell=True,
            cwd=workspace_path,
            capture_output=True,
            text=True,
            timeout=60,
        )

        passed = result.returncode == 0
        output = result.stdout + result.stderr

        return passed, output[:1000]  # Truncate long output

    except subprocess.TimeoutExpired:
        return False, "Validation command timed out"
    except Exception as e:
        return False, f"Validation error: {e}"


def reset_workspace(workspace_path: Path) -> None:
    """
    Reset the workspace to a clean state using git.

    Args:
        workspace_path: Path to the git repository.
    """
    try:
        subprocess.run(
            ["git", "checkout", "."],
            cwd=workspace_path,
            capture_output=True,
            check=True,
        )
        subprocess.run(
            ["git", "clean", "-fd"],
            cwd=workspace_path,
            capture_output=True,
            check=True,
        )
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"Failed to reset workspace: {e}")


def count_tokens(text: str) -> int:
    """
    Estimate token count for text.

    This is a rough estimate using the ~4 chars per token heuristic.
    For accurate counts, use a proper tokenizer.
    """
    return len(text) // 4


class ClaudeAgentRunner(AgentRunner):
    """
    Runner for Claude Code CLI.

    Uses the `claude` command with --print mode for non-interactive execution.
    Requires Claude Code to be installed and configured.
    """

    def __init__(
        self,
        model: str = "opus",
        timeout: float = 600,  # 10 minute timeout
        env_overrides: Optional[Dict[str, str]] = None,
    ):
        self.model = model
        self.timeout = timeout
        self.env_overrides = env_overrides or {}

    def get_name(self) -> str:
        return "claude"

    async def run(self, context: RunContext) -> TaskMetrics:
        """Execute Claude Code on the task."""
        import json

        collector = MetricsCollector(
            task_id=context.task.id,
            condition=context.condition.value,
            run_id=context.run_id,
        )

        collector.start()

        # Build the prompt
        system_prompt = build_system_prompt(context.task, context.graph_context)
        prompt = f"""Task: {context.task.description}

Expected files to modify/create: {', '.join(context.task.expected_files) if context.task.expected_files else 'As needed'}

Workspace: {context.workspace_path}

Complete this task. When done, say "TASK COMPLETE" or "TASK FAILED" followed by a brief summary."""

        # Set up environment
        env = os.environ.copy()
        env.update(self.env_overrides)

        # Use localhost for the bridge - it should be accessible in both WSL and native Linux
        env.setdefault("ANTHROPIC_BASE_URL", "http://localhost:5168")
        env.setdefault("ANTHROPIC_AUTH_TOKEN", "sk-copilot-bridge")
        env.setdefault("ANTHROPIC_API_KEY", "sk-copilot-bridge")

        # Build command
        cmd = [
            "claude",
            "--print",
            "--verbose",
            "--output-format", "stream-json",
            "--dangerously-skip-permissions",
            "--model", self.model,
            "--system-prompt", system_prompt,
            prompt,
        ]

        # Record graph info if present
        if context.graph_context:
            nodes = context.graph_context.count("[label=")
            edges = context.graph_context.count("->")
            tokens = count_tokens(context.graph_context)
            collector.set_graph_info(tokens, nodes, edges)

        task_completed = False
        last_error = ""

        try:
            process = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                cwd=str(context.workspace_path),
                env=env,
                limit=10 * 1024 * 1024,  # 10MB buffer limit for long lines
            )

            # Read streaming JSON output
            async def read_stream():
                nonlocal task_completed, last_error

                while True:
                    line = await process.stdout.readline()
                    if not line:
                        break

                    try:
                        event = json.loads(line.decode().strip())
                        await self._process_event(event, collector)

                        # Check for completion
                        if event.get("type") == "assistant":
                            content = event.get("message", {}).get("content", [])
                            for block in content:
                                if block.get("type") == "text":
                                    text = block.get("text", "")
                                    if "TASK COMPLETE" in text.upper():
                                        task_completed = True
                                    elif "TASK FAILED" in text.upper():
                                        task_completed = False

                    except json.JSONDecodeError:
                        pass  # Skip non-JSON lines

            try:
                await asyncio.wait_for(read_stream(), timeout=self.timeout)
            except asyncio.TimeoutError:
                process.kill()
                last_error = f"Timeout after {self.timeout}s"
                collector.record_error(last_error)

            await process.wait()

            if process.returncode != 0:
                stderr = await process.stderr.read()
                last_error = stderr.decode()[:500]
                collector.record_error(last_error)

        except FileNotFoundError:
            last_error = "claude command not found. Install Claude Code first."
            collector.record_error(last_error)
        except Exception as e:
            last_error = str(e)
            collector.record_error(last_error)

        collector.set_task_completed(task_completed)
        collector.end("completed" if task_completed else "failed")

        return collector.get_metrics()

    async def _process_event(self, event: Dict[str, Any], collector: MetricsCollector):
        """Process a streaming event from Claude."""
        event_type = event.get("type", "")

        if event_type == "tool_use":
            # Tool call started
            tool = event.get("tool", {})
            tool_call = ToolCall(
                timestamp=monotonic_time(),
                tool_name=tool.get("name", "unknown"),
                parameters=tool.get("input", {}),
                result_preview="",
                duration_seconds=0,
                tokens_in=0,
                tokens_out=0,
                success=True,
            )
            collector.record_tool_call(tool_call)

        elif event_type == "tool_result":
            # Tool call completed
            # The result is typically too large to store fully
            pass

        elif event_type == "usage":
            # Token usage stats
            usage = event.get("usage", {})
            collector.record_llm_call(
                tokens_in=usage.get("input_tokens", 0),
                tokens_out=usage.get("output_tokens", 0),
            )


class CodexAgentRunner(AgentRunner):
    """
    Runner for OpenAI Codex CLI.

    Uses the `codex exec` command with --json for non-interactive execution.
    """

    def __init__(
        self,
        model: str = "o3",
        timeout: float = 600,
        sandbox_mode: str = "workspace-write",
        env_overrides: Optional[Dict[str, str]] = None,
    ):
        self.model = model
        self.timeout = timeout
        self.sandbox_mode = sandbox_mode
        self.env_overrides = env_overrides or {}

    def get_name(self) -> str:
        return "codex"

    async def run(self, context: RunContext) -> TaskMetrics:
        """Execute Codex on the task."""
        import json

        collector = MetricsCollector(
            task_id=context.task.id,
            condition=context.condition.value,
            run_id=context.run_id,
        )

        collector.start()

        # Build the prompt with optional graph context
        graph_section = ""
        if context.graph_context:
            graph_section = f"""
## Architecture Graph

The following is a dependency graph of the codebase generated by llmcc.
Use this to understand the codebase structure before exploring files.

```dot
{context.graph_context}
```

Use this graph to navigate directly to relevant code instead of searching blindly.
"""
            nodes = context.graph_context.count("[label=")
            edges = context.graph_context.count("->")
            tokens = count_tokens(context.graph_context)
            collector.set_graph_info(tokens, nodes, edges)

        prompt = f"""Task: {context.task.description}

Expected files to modify/create: {', '.join(context.task.expected_files) if context.task.expected_files else 'As needed'}
{graph_section}
Complete this task efficiently. When done, clearly state "TASK COMPLETE" or "TASK FAILED"."""

        # Set up environment
        env = os.environ.copy()
        env.update(self.env_overrides)

        # Use WSL-aware bridge URL if running in WSL
        if "microsoft" in open("/proc/version", "r").read().lower() if Path("/proc/version").exists() else False:
            host_ip = subprocess.run(
                ["ip", "route", "show", "default"],
                capture_output=True, text=True
            ).stdout.split()[2] if subprocess.run(["ip", "route", "show", "default"], capture_output=True, text=True).returncode == 0 else "localhost"
            env.setdefault("OPENAI_BASE_URL", f"http://{host_ip}:5168/v1")
        else:
            env.setdefault("OPENAI_BASE_URL", "http://localhost:5168/v1")

        env.setdefault("OPENAI_API_KEY", "sk-copilot-bridge")

        # Build command
        cmd = [
            "codex", "exec",
            "--json",
            "--model", self.model,
            "--sandbox", self.sandbox_mode,
            "--dangerously-bypass-approvals-and-sandbox",  # For benchmark automation
            "--cd", str(context.workspace_path),
            prompt,
        ]

        task_completed = False
        last_error = ""

        try:
            process = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                env=env,
            )

            # Read JSONL output
            async def read_stream():
                nonlocal task_completed, last_error

                while True:
                    line = await process.stdout.readline()
                    if not line:
                        break

                    try:
                        event = json.loads(line.decode().strip())
                        await self._process_event(event, collector)

                        # Check for completion in message events
                        if event.get("type") == "message":
                            content = event.get("content", "")
                            if isinstance(content, str):
                                if "TASK COMPLETE" in content.upper():
                                    task_completed = True
                                elif "TASK FAILED" in content.upper():
                                    task_completed = False

                    except json.JSONDecodeError:
                        pass

            try:
                await asyncio.wait_for(read_stream(), timeout=self.timeout)
            except asyncio.TimeoutError:
                process.kill()
                last_error = f"Timeout after {self.timeout}s"
                collector.record_error(last_error)

            await process.wait()

            if process.returncode != 0:
                stderr = await process.stderr.read()
                last_error = stderr.decode()[:500]
                collector.record_error(last_error)

        except FileNotFoundError:
            last_error = "codex command not found. Install Codex CLI first."
            collector.record_error(last_error)
        except Exception as e:
            last_error = str(e)
            collector.record_error(last_error)

        collector.set_task_completed(task_completed)
        collector.end("completed" if task_completed else "failed")

        return collector.get_metrics()

    async def _process_event(self, event: Dict[str, Any], collector: MetricsCollector):
        """Process a JSONL event from Codex."""
        event_type = event.get("type", "")

        if event_type == "function_call" or event_type == "tool_call":
            tool_call = ToolCall(
                timestamp=monotonic_time(),
                tool_name=event.get("name", event.get("function", {}).get("name", "unknown")),
                parameters=event.get("arguments", event.get("function", {}).get("arguments", {})),
                result_preview="",
                duration_seconds=0,
                tokens_in=0,
                tokens_out=0,
                success=True,
            )
            collector.record_tool_call(tool_call)

        elif event_type == "exec":
            # Shell command execution
            tool_call = ToolCall(
                timestamp=monotonic_time(),
                tool_name="shell",
                parameters={"command": event.get("command", "")},
                result_preview=event.get("output", "")[:200],
                duration_seconds=0,
                tokens_in=0,
                tokens_out=0,
                success=event.get("exit_code", 0) == 0,
            )
            collector.record_tool_call(tool_call)

        elif event_type == "usage":
            usage = event.get("usage", {})
            collector.record_llm_call(
                tokens_in=usage.get("prompt_tokens", usage.get("input_tokens", 0)),
                tokens_out=usage.get("completion_tokens", usage.get("output_tokens", 0)),
            )


# Export for external agent implementations
__all__ = [
    "AgentRunner",
    "MockAgentRunner",
    "ClaudeAgentRunner",
    "CodexAgentRunner",
    "RunContext",
    "generate_graph",
    "build_system_prompt",
    "run_validation",
    "reset_workspace",
    "count_tokens",
]
