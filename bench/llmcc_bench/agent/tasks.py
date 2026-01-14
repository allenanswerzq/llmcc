"""
Task definitions and loading for agent benchmarks.
"""

from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
from typing import Dict, List, Optional

try:
    import tomllib  # Python 3.11+
except ImportError:
    import tomli as tomllib  # type: ignore


class TaskCategory(str, Enum):
    """Category of task based on what it involves."""

    EXPLORATION = "exploration"
    """Finding information, no code changes."""

    SMALL = "small"
    """Single file, < 50 lines changed."""

    MEDIUM = "medium"
    """2-5 files, < 200 lines changed."""

    LARGE = "large"
    """5+ files, architectural changes."""


class TaskDifficulty(str, Enum):
    """Difficulty level of the task."""

    EASY = "easy"
    """Straightforward, clear path to solution."""

    MEDIUM = "medium"
    """Requires some exploration and understanding."""

    HARD = "hard"
    """Complex, requires deep understanding of codebase."""


@dataclass
class Task:
    """A benchmark task definition."""

    # Identification
    id: str
    """Unique task identifier."""

    repo: str
    """Repository this task applies to (e.g., 'tokio-rs/tokio')."""

    # Description
    description: str
    """Full task description given to the agent."""

    category: TaskCategory = TaskCategory.SMALL
    """Task category."""

    difficulty: TaskDifficulty = TaskDifficulty.MEDIUM
    """Task difficulty."""

    # Validation
    expected_files: List[str] = field(default_factory=list)
    """Files expected to be modified (for non-exploration tasks)."""

    expected_answer: List[str] = field(default_factory=list)
    """Expected answer content (for exploration tasks)."""

    validation_command: Optional[str] = None
    """Shell command to validate success (exit 0 = success)."""

    # Limits (override defaults)
    max_tool_calls: Optional[int] = None
    """Max tool calls for this task (overrides experiment config)."""

    max_tokens: Optional[int] = None
    """Max tokens for this task (overrides experiment config)."""

    # Metadata
    tags: List[str] = field(default_factory=list)
    """Optional tags for filtering."""

    notes: Optional[str] = None
    """Notes about this task (not shown to agent)."""

    @classmethod
    def from_dict(cls, task_id: str, data: Dict) -> "Task":
        """Create a Task from a dictionary (parsed from TOML)."""
        return cls(
            id=data.get("id", task_id),
            repo=data["repo"],
            description=data["description"],
            category=TaskCategory(data.get("category", "small")),
            difficulty=TaskDifficulty(data.get("difficulty", "medium")),
            expected_files=data.get("expected_files", []),
            expected_answer=data.get("expected_answer", []),
            validation_command=data.get("validation_command"),
            max_tool_calls=data.get("max_tool_calls"),
            max_tokens=data.get("max_tokens"),
            tags=data.get("tags", []),
            notes=data.get("notes"),
        )

    def to_dict(self) -> Dict:
        """Convert to dictionary for serialization."""
        result = {
            "id": self.id,
            "repo": self.repo,
            "description": self.description,
            "category": self.category.value,
            "difficulty": self.difficulty.value,
        }

        if self.expected_files:
            result["expected_files"] = self.expected_files
        if self.expected_answer:
            result["expected_answer"] = self.expected_answer
        if self.validation_command:
            result["validation_command"] = self.validation_command
        if self.max_tool_calls:
            result["max_tool_calls"] = self.max_tool_calls
        if self.max_tokens:
            result["max_tokens"] = self.max_tokens
        if self.tags:
            result["tags"] = self.tags
        if self.notes:
            result["notes"] = self.notes

        return result

    @property
    def prompt(self) -> str:
        """Get the prompt to give to the agent."""
        return self.description


def load_tasks_from_file(path: Path) -> List[Task]:
    """Load tasks from a single TOML file."""
    if not path.exists():
        return []

    try:
        with open(path, "rb") as f:
            data = tomllib.load(f)

        tasks = []
        for task_data in data.get("tasks", []):
            task_id = task_data.get("id", f"task_{len(tasks)}")
            tasks.append(Task.from_dict(task_id, task_data))

        return tasks
    except Exception as e:
        print(f"Warning: Failed to load tasks from {path}: {e}")
        return []


def load_tasks(
    tasks_dir: Optional[Path] = None,
    repo: Optional[str] = None,
    category: Optional[TaskCategory] = None,
    difficulty: Optional[TaskDifficulty] = None,
    tags: Optional[List[str]] = None,
) -> List[Task]:
    """
    Load tasks from the tasks directory.

    Args:
        tasks_dir: Directory containing task TOML files.
                   Defaults to bench/tasks/.
        repo: Filter by repository (e.g., 'tokio-rs/tokio').
        category: Filter by task category.
        difficulty: Filter by difficulty level.
        tags: Filter by tags (task must have all specified tags).

    Returns:
        List of matching tasks.
    """
    if tasks_dir is None:
        # Default to bench/tasks/ relative to this file
        tasks_dir = Path(__file__).parent.parent.parent / "tasks"

    if not tasks_dir.exists():
        return []

    # Load all TOML files in the directory
    all_tasks: List[Task] = []
    for toml_file in tasks_dir.glob("*.toml"):
        all_tasks.extend(load_tasks_from_file(toml_file))

    # Apply filters
    filtered = all_tasks

    if repo:
        filtered = [t for t in filtered if t.repo == repo]

    if category:
        filtered = [t for t in filtered if t.category == category]

    if difficulty:
        filtered = [t for t in filtered if t.difficulty == difficulty]

    if tags:
        filtered = [t for t in filtered if all(tag in t.tags for tag in tags)]

    return filtered


def get_task_by_id(task_id: str, tasks_dir: Optional[Path] = None) -> Optional[Task]:
    """Get a specific task by ID."""
    all_tasks = load_tasks(tasks_dir)
    for task in all_tasks:
        if task.id == task_id:
            return task
    return None


def list_repos(tasks_dir: Optional[Path] = None) -> List[str]:
    """Get list of all repos that have tasks defined."""
    all_tasks = load_tasks(tasks_dir)
    repos = set(t.repo for t in all_tasks)
    return sorted(repos)
