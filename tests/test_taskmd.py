"""Tests for TASK.md generation."""

import tempfile
from pathlib import Path

from hive.core.taskmd import generate_task_md, write_task_md


class TestGenerateTaskMd:
    """Tests for generate_task_md function."""

    def test_basic_generation(self):
        content = generate_task_md(
            repo_key="backend",
            title="Add attribution endpoint",
            tasks=["Add POST /v1/attribution", "Add unit tests"],
        )

        assert "# Hive Task: backend" in content
        assert "Add attribution endpoint" in content
        assert "- Add POST /v1/attribution" in content
        assert "- Add unit tests" in content

    def test_with_test_command(self):
        content = generate_task_md(
            repo_key="backend",
            title="Test task",
            tasks=["Do something"],
            test_command="make test",
        )

        assert "`make test`" in content

    def test_empty_tasks(self):
        content = generate_task_md(
            repo_key="backend",
            title="Test task",
            tasks=[],
        )

        assert "TODO: Define tasks" in content

    def test_constraints_section(self):
        content = generate_task_md(
            repo_key="backend",
            title="Test task",
            tasks=["Task 1"],
        )

        assert "## Constraints" in content
        assert "Scope to this repo only" in content
        assert "backward compatibility" in content


class TestWriteTaskMd:
    """Tests for write_task_md function."""

    def test_writes_file(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            worktree_path = Path(tmpdir)
            task_md_path = write_task_md(
                worktree_path=worktree_path,
                repo_key="backend",
                title="Test task",
                tasks=["Task 1"],
            )

            assert task_md_path.exists()
            assert task_md_path.name == "TASK.md"

            content = task_md_path.read_text()
            assert "# Hive Task: backend" in content
