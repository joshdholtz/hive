"""TASK.md template generation."""

from pathlib import Path
from typing import Any


def generate_task_md(
    repo_key: str,
    title: str,
    tasks: list[str],
    test_command: str | None = None,
) -> str:
    """Generate TASK.md content for a repo.

    Args:
        repo_key: Repository key.
        title: Task/plan title.
        tasks: List of task descriptions.
        test_command: Optional test command.

    Returns:
        TASK.md content string.
    """
    lines = [
        f"# Hive Task: {repo_key}",
        "",
        "## Goal",
        "",
        title,
        "",
        "## Repo tasks",
        "",
    ]

    # Add task bullets
    if tasks:
        for task in tasks:
            lines.append(f"- {task}")
    else:
        lines.append("- TODO: Define tasks for this repo")

    lines.extend([
        "",
        "## Definition of done",
        "",
        "- [ ] Implement repo slice",
    ])

    if test_command:
        lines.append(f"- [ ] Run test command: `{test_command}`")
    else:
        lines.append("- [ ] Run tests")

    lines.extend([
        "- [ ] Summarize changes and risks",
        "",
        "## Constraints",
        "",
        "- Scope to this repo only",
        "- Maintain backward compatibility",
        "- Follow existing code style and patterns",
        "",
    ])

    return "\n".join(lines)


def write_task_md(
    worktree_path: Path,
    repo_key: str,
    title: str,
    tasks: list[str],
    test_command: str | None = None,
) -> Path:
    """Write TASK.md to a worktree.

    Args:
        worktree_path: Path to worktree directory.
        repo_key: Repository key.
        title: Task/plan title.
        tasks: List of task descriptions.
        test_command: Optional test command.

    Returns:
        Path to written TASK.md.
    """
    content = generate_task_md(
        repo_key=repo_key,
        title=title,
        tasks=tasks,
        test_command=test_command,
    )

    task_md_path = worktree_path / "TASK.md"
    with open(task_md_path, "w") as f:
        f.write(content)

    return task_md_path
