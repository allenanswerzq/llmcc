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

    trace_file: Optional[Path] = None
    """Path to write conversation trace (prompts and responses)."""

    debug: bool = False
    """Enable debug mode to show model reasoning for each step."""

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


def build_graph_context(graph: Optional[str] = None) -> Optional[str]:
    """
    Build the graph context to append to the system prompt.

    Args:
        graph: Optional llmcc graph to include.

    Returns:
        Graph context string or None.

    Note: We no longer include the graph in the prompt since the llmcc skill
    should guide Claude to run llmcc on its own. This function is kept for
    potential future use but currently returns None.
    """
    # The llmcc skill (~/.claude/skills/llmcc/SKILL.md) should guide Claude
    # to run llmcc on its own. We don't pre-feed the graph anymore.
    return None


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

        # Build graph context if present
        graph_context = build_graph_context(context.graph_context)

        # Build the completion instruction - make it very clear
        debug_instruction = """

⚠️ STEP-BY-STEP MODE ENABLED:
You MUST think out loud before EVERY action. Before calling ANY tool, first write a brief paragraph explaining:
"I will use [TOOL NAME] because [REASON]. I'm looking for [WHAT] and expect to find [EXPECTED RESULT]."

DO NOT call tools silently. Always explain your thinking first, then call the tool.""" if context.debug else ""

        completion_instruction = f"""{debug_instruction}

IMPORTANT: You MUST MUST end your response with one of these exact phrases:
- "TASK COMPLETE" followed by a brief summary if successful
- "TASK FAILED" followed by explanation if unsuccessful

Do NOT end with tool calls or partial work. Always provide a final text response with TASK COMPLETE or TASK FAILED."""

        # Build the prompt - include graph context in the user prompt for reliability
        # Include explicit repo path note to prevent llmcc path mistakes
        repo_path_note = f"""
⚠️ IMPORTANT: The repository path is: {context.workspace_path} """

        if graph_context:
            prompt = f"""{graph_context}

Task: {context.task.description}

Expected files to modify/create: {', '.join(context.task.expected_files) if context.task.expected_files else 'As needed'}

Workspace: {context.workspace_path}
{repo_path_note}
{completion_instruction}"""
        else:
            prompt = f"""Task: {context.task.description}

Expected files to modify/create: {', '.join(context.task.expected_files) if context.task.expected_files else 'As needed'}

Workspace: {context.workspace_path}
{repo_path_note}
{completion_instruction}"""

        # Set up environment
        env = os.environ.copy()
        env.update(self.env_overrides)

        # Use localhost for the bridge - it should be accessible in both WSL and native Linux
        env.setdefault("ANTHROPIC_BASE_URL", "http://localhost:5168")
        env.setdefault("ANTHROPIC_AUTH_TOKEN", "sk-copilot-bridge")
        env.setdefault("ANTHROPIC_API_KEY", "sk-copilot-bridge")

        # System prompt to ensure proper completion
        debug_system_note = """

⚠️ STEP-BY-STEP MODE: Before EVERY tool call, you MUST first write out your reasoning explaining what you're about to do and why.""" if context.debug else ""

        # Simple system prompt - llmcc skill handles exploration guidance
        system_prompt = f"""You are a coding assistant completing benchmark tasks.{debug_system_note}
CRITICAL: Every task response MUST end with either "TASK COMPLETE" or "TASK FAILED" in your final message.
Never end a task with just tool calls - always provide a final summary with the completion status."""

        # Manage llmcc skill symlink based on condition
        # The skill is at ~/.claude/skills/llmcc/SKILL.md -> <project>/doc/claude-skill-llmcc.md
        skill_link = Path.home() / ".claude" / "skills" / "llmcc" / "SKILL.md"
        # Get project root: runner.py is at bench/llmcc_bench/agent/runner.py
        project_root = Path(__file__).parent.parent.parent.parent
        skill_target = project_root / "doc" / "claude-skill-llmcc.md"

        if context.condition == Condition.BASELINE:
            # Remove symlink for baseline so Claude can't use the skill
            if skill_link.is_symlink():
                skill_link.unlink()
        else:
            # Ensure symlink exists for with_llmcc condition
            if not skill_link.exists():
                skill_link.parent.mkdir(parents=True, exist_ok=True)
                skill_link.symlink_to(skill_target)

        # Build command
        cmd = [
            "claude",
            "--print",
            "--verbose",
            "--output-format", "stream-json",
            "--dangerously-skip-permissions",
            "--model", self.model,
            "--system-prompt", system_prompt,
        ]

        cmd.append(prompt)

        # Record graph info if present
        if context.graph_context:
            nodes = context.graph_context.count("[label=")
            edges = context.graph_context.count("->")
            tokens = count_tokens(context.graph_context)
            collector.set_graph_info(tokens, nodes, edges)

        task_completed = False
        last_error = ""

        # Open trace file if specified
        trace_file = None
        if context.trace_file:
            trace_file = open(context.trace_file, "a")
            # Write initial context
            trace_file.write(json.dumps({
                "type": "init",
                "task_id": context.task.id,
                "run_id": context.run_id,
                "condition": context.condition.value,
                "graph_context": graph_context,
                "user_prompt": prompt,
                "timestamp": time.time(),
            }) + "\n")
            trace_file.flush()

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
                # Track cumulative usage for computing deltas
                last_usage: Dict[str, int] = {"input_tokens": 0, "output_tokens": 0}
                final_answer = ""
                debug_mode = context.debug

                while True:
                    line = await process.stdout.readline()
                    if not line:
                        break

                    try:
                        event = json.loads(line.decode().strip())

                        # Write event to trace file
                        if trace_file:
                            trace_file.write(json.dumps(event) + "\n")
                            trace_file.flush()

                        last_usage = await self._process_event(event, collector, last_usage=last_usage, debug=debug_mode)

                        # Check for completion and capture final answer
                        if event.get("type") == "assistant":
                            content = event.get("message", {}).get("content", [])
                            for block in content:
                                if block.get("type") == "text":
                                    text = block.get("text", "")
                                    if "TASK COMPLETE" in text.upper():
                                        task_completed = True
                                        final_answer = text
                                        collector.set_answer(text)
                                        # Print the final answer
                                        print(f"\n      === ANSWER ===")
                                        for line_text in text.strip().split("\n"):
                                            print(f"      {line_text}")
                                        print(f"      ===============\n")
                                    elif "TASK FAILED" in text.upper():
                                        task_completed = False
                                        final_answer = text
                                        collector.set_answer(text)
                                        print(f"\n      === ANSWER (FAILED) ===")
                                        for line_text in text.strip().split("\n"):
                                            print(f"      {line_text}")
                                        print(f"      ==========================\n")

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
        finally:
            if trace_file:
                trace_file.close()

        collector.set_task_completed(task_completed)
        collector.end("completed" if task_completed else "failed")

        return collector.get_metrics()

    async def _process_event(
        self,
        event: Dict[str, Any],
        collector: MetricsCollector,
        verbose: bool = True,
        last_usage: Optional[Dict[str, int]] = None,
        debug: bool = False,
    ) -> Dict[str, int]:
        """Process a streaming event from Claude.

        Returns the current cumulative usage for tracking deltas.
        """
        event_type = event.get("type", "")
        current_usage = last_usage or {"input_tokens": 0, "output_tokens": 0}

        if event_type == "assistant":
            # Assistant message - may contain tool_use blocks
            message = event.get("message", {})
            content = message.get("content", [])

            # In debug mode, print any text output from the model
            if debug:
                for block in content:
                    if block.get("type") == "text":
                        text = block.get("text", "").strip()
                        if text:
                            print(f"      💭 {text[:200]}")

            for block in content:
                if block.get("type") == "tool_use":
                    tool_name = block.get("name", "unknown")
                    tool_input = block.get("input", {})
                    tool_call = ToolCall(
                        timestamp=monotonic_time(),
                        tool_name=tool_name,
                        parameters=tool_input,
                        result_preview="",
                        duration_seconds=0,
                        tokens_in=0,
                        tokens_out=0,
                        success=True,
                    )
                    collector.record_tool_call(tool_call)
                    # Print tool call details
                    if verbose:
                        call_num = collector.tool_calls_total
                        # Summarize input
                        input_summary = self._summarize_tool_input(tool_name, tool_input)
                        print(f"      [{call_num}] {tool_name}: {input_summary}")
                        # Print description/explanation if present
                        description = tool_input.get("description") or tool_input.get("explanation") or tool_input.get("reason")
                        if description:
                            print(f"          → {description[:150]}")
                        # For Bash, always print full command
                        if tool_name == "Bash":
                            cmd = tool_input.get("command", "")
                            if cmd:
                                print(f"          $ {cmd}")
            # Record usage from assistant message if present
            # Note: Claude CLI reports CUMULATIVE usage, so compute deltas
            usage = message.get("usage", {})
            if usage.get("input_tokens") is not None or usage.get("output_tokens") is not None:
                new_in = usage.get("input_tokens", 0)
                new_out = usage.get("output_tokens", 0)
                # Compute delta from last seen usage
                delta_in = new_in - current_usage["input_tokens"]
                delta_out = new_out - current_usage["output_tokens"]
                # Only record if there's a positive delta
                if delta_in > 0 or delta_out > 0:
                    collector.record_llm_call(tokens_in=delta_in, tokens_out=delta_out)
                    if verbose:
                        print(f"        → tokens: +{delta_in} in, +{delta_out} out (total: {collector.total_tokens})")
                # Update current usage
                current_usage = {"input_tokens": new_in, "output_tokens": new_out}

        elif event_type == "result":
            # Final result - contains total usage
            usage = event.get("usage", {})
            # Note: total usage already includes all turns, don't double count
            # Just record it for reference
            pass

        elif event_type == "user":
            # Tool result - could check for errors
            tool_result = event.get("tool_use_result", {})
            # Could track tool success/failure here if needed
            pass

        return current_usage

    def _summarize_tool_input(self, tool_name: str, tool_input: Dict[str, Any]) -> str:
        """Summarize tool input for display."""
        # File-related tools
        if tool_name in ("Read", "read_file"):
            path = tool_input.get("file_path") or tool_input.get("path", "")
            start_line = tool_input.get("start_line") or tool_input.get("startLine")
            end_line = tool_input.get("end_line") or tool_input.get("endLine")
            short_path = self._short_path(path)
            if start_line and end_line:
                return f"{short_path}:{start_line}-{end_line}"
            elif start_line:
                return f"{short_path}:{start_line}-"
            return short_path
        if tool_name in ("Glob", "file_search"):
            pattern = tool_input.get("pattern", "")
            return f"'{pattern}'"
        if tool_name in ("Grep", "grep_search"):
            pattern = tool_input.get("pattern") or tool_input.get("query", "")
            path = tool_input.get("path", "")
            if path:
                return f"'{pattern}' in {self._short_path(path)}"
            return f"'{pattern}'"
        if tool_name in ("Write", "Edit", "create_file", "edit_file"):
            path = tool_input.get("file_path") or tool_input.get("path", "")
            return self._short_path(path)
        if tool_name in ("Bash", "run_in_terminal"):
            cmd = tool_input.get("command", "")
            if len(cmd) > 60:
                cmd = cmd[:57] + "..."
            return f"$ {cmd}"
        # Default: show first key=value
        if tool_input:
            first_key = next(iter(tool_input.keys()))
            first_val = str(tool_input[first_key])
            if len(first_val) > 50:
                first_val = first_val[:47] + "..."
            return f"{first_key}={first_val}"
        return ""

    def _short_path(self, path: str) -> str:
        """Shorten a file path for display."""
        if not path:
            return ""
        # Remove common workspace prefixes
        for prefix in ("/home/", "/Users/", "/tmp/"):
            if path.startswith(prefix):
                parts = path.split("/")
                if len(parts) > 4:
                    return "/".join(["..."] + parts[-3:])
        if len(path) > 60:
            return "..." + path[-57:]
        return path


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
IMPORTANT: You MUST end your response with one of these exact phrases:
- "TASK COMPLETE" followed by a brief summary if successful
- "TASK FAILED" followed by explanation if unsuccessful

Do NOT end with tool calls or partial work. Always provide a final text response."""

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


class LlcraftAgentRunner(AgentRunner):
    """
    Runner for llcraft - custom agent with llmcc tool support.

    Uses llcraft with --llmcc flag to enable the llmcc tool.
    For baseline condition, runs WITHOUT --llmcc so the tool isn't available.
    """

    def __init__(
        self,
        model: str = "claude-opus-4-5-20251101",
        timeout: float = 600,
        env_overrides: Optional[Dict[str, str]] = None,
    ):
        self.model = model
        self.timeout = timeout
        self.env_overrides = env_overrides or {}

    def get_name(self) -> str:
        return "llcraft"

    async def run(self, context: RunContext) -> TaskMetrics:
        """Execute llcraft on the task."""
        import json

        collector = MetricsCollector(
            task_id=context.task.id,
            condition=context.condition.value,
            run_id=context.run_id,
        )

        collector.start()

        # Build the prompt - for llcraft we don't pre-load graph, llmcc is a tool
        debug_instruction = """

⚠️ STEP-BY-STEP MODE ENABLED:
Before EVERY tool call, explain your reasoning briefly.""" if context.debug else ""

        # System prompt varies by condition
        if context.condition == Condition.WITH_LLMCC:
            system_prompt = f"""You are a coding assistant with access to the llmcc code architecture tool.{debug_instruction}

For complex codebase exploration, consider using the llmcc tool to understand architecture:
- llmcc(dirs=["/path"], depth=2) - shows module-level structure
- llmcc(dirs=["/path"], depth=3, pagerank_top_k=100) - detailed view with top 100 important nodes

WORKFLOW:
1. Use llmcc to understand architecture when exploring a new codebase
2. Use grep/read_file for specific code details
3. Always end with "TASK COMPLETE" or "TASK FAILED"

CRITICAL: End every response with "TASK COMPLETE" or "TASK FAILED"."""
        else:
            system_prompt = f"""You are a coding assistant completing benchmark tasks.{debug_instruction}

Use available tools (read_file, search_files, list_dir, run_command) to explore the codebase.

CRITICAL: End every response with "TASK COMPLETE" or "TASK FAILED"."""

        # Format prompt as single line to work with readline REPL
        task_desc = context.task.description.replace('\n', ' ').replace('  ', ' ')
        expected = ', '.join(context.task.expected_files) if context.task.expected_files else 'As needed'
        prompt = f"Task: {task_desc} | Workspace: {context.workspace_path} | Expected files: {expected} | IMPORTANT: End with TASK COMPLETE followed by your answer, or TASK FAILED with explanation."

        # Set up environment
        env = os.environ.copy()
        env.update(self.env_overrides)
        env.setdefault("ANTHROPIC_BASE_URL", "http://localhost:5168")
        env.setdefault("ANTHROPIC_API_KEY", "sk-copilot-bridge")

        # Build command - conditionally add --llmcc flag
        llcraft_path = Path(__file__).parent.parent.parent.parent / "agent" / "llcraft" / "dist" / "index.js"

        cmd = [
            "node", str(llcraft_path),
            "--new",  # Fresh session
        ]

        # Only add --llmcc for WITH_LLMCC condition
        if context.condition == Condition.WITH_LLMCC:
            cmd.append("--llmcc")

        # Debug: print the command being run
        print(f"      Running: {' '.join(cmd)}", flush=True)

        task_completed = False
        last_error = ""
        final_answer = ""

        # Open trace file if specified
        trace_file = None
        if context.trace_file:
            trace_file = open(context.trace_file, "a")
            trace_file.write(json.dumps({
                "type": "init",
                "task_id": context.task.id,
                "run_id": context.run_id,
                "condition": context.condition.value,
                "with_llmcc_tool": context.condition == Condition.WITH_LLMCC,
                "prompt": prompt,
                "timestamp": time.time(),
            }) + "\n")
            trace_file.flush()

        try:
            # Start llcraft process
            # Force Node.js to unbuffer stdout
            env["FORCE_COLOR"] = "1"  # Often helps with output buffering
            process = await asyncio.create_subprocess_exec(
                *cmd,
                stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.STDOUT,  # Merge stderr to stdout
                cwd=str(context.workspace_path),
                env=env,
            )

            # Send the prompt first, wait for response, then exit
            prompt_input = f"{prompt}\n"

            async def read_output():
                nonlocal task_completed, final_answer
                output_lines = []
                waiting_for_prompt = True
                response_started = False
                idle_count = 0
                exit_sent = False
                lines_after_complete = 0

                while True:
                    try:
                        # Use a longer timeout to allow for API calls
                        line = await asyncio.wait_for(
                            process.stdout.readline(),
                            timeout=5.0
                        )
                        idle_count = 0  # Reset idle counter on data

                        # Track lines after TASK COMPLETE
                        if task_completed and not exit_sent:
                            lines_after_complete += 1
                            # After 150 lines post-completion, force exit
                            if lines_after_complete >= 150:
                                try:
                                    process.stdin.write(b"/exit\n")
                                    await process.stdin.drain()
                                    exit_sent = True
                                except:
                                    pass
                    except asyncio.TimeoutError:
                        idle_count += 1
                        # If we've already sent exit and been idle, break
                        if exit_sent and idle_count >= 2:
                            print(f"      >>> Breaking after /exit (idle={idle_count})", flush=True)
                            break
                        # If we've seen a response and been idle, send exit
                        if response_started and idle_count >= 5 and not exit_sent:
                            print(f"      >>> Sending /exit (idle={idle_count}, response_started={response_started})", flush=True)
                            try:
                                process.stdin.write(b"/exit\n")
                                await process.stdin.drain()
                                exit_sent = True
                            except:
                                pass
                            # Continue reading to get "Session saved" etc
                            continue
                        elif idle_count >= 30:  # 60 second total timeout for response
                            break
                        continue

                    if not line:
                        break

                    decoded = line.decode().rstrip()
                    output_lines.append(decoded)

                    if trace_file:
                        trace_file.write(json.dumps({"type": "output", "line": decoded}) + "\n")
                        trace_file.flush()

                    # Strip ANSI codes for detection
                    stripped = re.sub(r'\x1b\[[0-9;]*m', '', decoded)

                    # Detect when llcraft starts responding (after the prompt echo)
                    if "llcraft:" in stripped and not response_started:
                        response_started = True

                    # Detect when llcraft exits (after /exit command)
                    if "Goodbye!" in stripped or "Session saved" in stripped:
                        # Wait a tiny bit for any final output then break
                        await asyncio.sleep(0.1)
                        break

                    # Parse tool calls from output (llcraft shows them as [tool_name: args] with ANSI colors)
                    # Strip ANSI codes first
                    stripped = re.sub(r'\x1b\[[0-9;]*m', '', decoded)
                    # Find all tool calls in the line (may have multiple)
                    tool_matches = re.findall(r'\[(\w+):\s*([^\]]+)\]', stripped)
                    for tool_name, tool_args in tool_matches:
                        tool_call = ToolCall(
                            timestamp=monotonic_time(),
                            tool_name=tool_name,
                            parameters={"summary": tool_args},
                            result_preview="",
                            duration_seconds=0,
                            tokens_in=0,
                            tokens_out=0,
                            success=True,
                        )
                        collector.record_tool_call(tool_call)
                        print(f"      [{collector.tool_calls_total}] {tool_name}: {tool_args[:60]}", flush=True)

                    # Check for completion
                    if "TASK COMPLETE" in decoded.upper():
                        task_completed = True
                        print(f"      >>> TASK COMPLETE detected", flush=True)
                    elif "TASK FAILED" in decoded.upper():
                        task_completed = False
                        print(f"      >>> TASK FAILED detected", flush=True)

                # Capture final output as answer
                final_answer = "\n".join(output_lines[-20:])  # Last 20 lines as answer

            try:
                # Write prompt first
                process.stdin.write(prompt_input.encode())
                await process.stdin.drain()

                # Read output (will send /exit when response is complete)
                await asyncio.wait_for(read_output(), timeout=self.timeout)
            except asyncio.TimeoutError:
                process.kill()
                last_error = f"Timeout after {self.timeout}s"
                collector.record_error(last_error)

            await process.wait()

            if process.returncode != 0:
                stderr = await process.stderr.read()
                last_error = stderr.decode()[:500]
                if last_error:
                    collector.record_error(last_error)

        except FileNotFoundError:
            last_error = f"llcraft not found at {llcraft_path}. Run 'npm run build' in agent/llcraft first."
            collector.record_error(last_error)
        except Exception as e:
            last_error = str(e)
            collector.record_error(last_error)
        finally:
            if trace_file:
                trace_file.close()

        collector.set_answer(final_answer)
        collector.set_task_completed(task_completed)
        collector.end("completed" if task_completed else "failed")

        # Print answer summary
        if final_answer:
            print(f"\n      === ANSWER ===")
            for line in final_answer.strip().split("\n")[-10:]:
                print(f"      {line}")
            print(f"      ===============\n")

        return collector.get_metrics()


# Export for external agent implementations
__all__ = [
    "AgentRunner",
    "MockAgentRunner",
    "ClaudeAgentRunner",
    "CodexAgentRunner",
    "LlcraftAgentRunner",
    "RunContext",
    "generate_graph",
    "build_system_prompt",
    "run_validation",
    "reset_workspace",
    "count_tokens",
]
