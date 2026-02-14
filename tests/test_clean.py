"""Tests for hive clean command."""

from pathlib import Path

import pytest
from typer.testing import CliRunner

from hive.cli import app


runner = CliRunner()


@pytest.fixture
def workspace_with_worktrees(tmp_path):
    """Create a workspace with task and worktrees."""
    # Create workspace.yaml
    workspace_yaml = tmp_path / "workspace.yaml"
    workspace_yaml.write_text("""
schema_version: 1
repos:
  frontend: ./repos/frontend
  backend: ./repos/backend
""")

    # Create task directory with plan
    task_dir = tmp_path / ".hive/tasks" / "test-123"
    task_dir.mkdir(parents=True)

    plan_yaml = task_dir / "plan.yaml"
    plan_yaml.write_text("""
schema_version: "1"
id: test-123
title: Test Task
branch: feature/test-123
repos:
  frontend:
    tasks:
      - Frontend task
  backend:
    tasks:
      - Backend task
""")

    # Create worktree directories (simulating applied task)
    wt_base = tmp_path / ".hive/wt" / "test-123"
    (wt_base / "frontend").mkdir(parents=True)
    (wt_base / "backend").mkdir(parents=True)

    # Add some files to worktrees
    (wt_base / "frontend" / "index.js").write_text("// frontend")
    (wt_base / "backend" / "app.py").write_text("# backend")

    return tmp_path


class TestCleanCommand:
    """Tests for hive clean command."""

    def test_requires_yes_flag(self, workspace_with_worktrees):
        """clean requires --yes flag to confirm deletion."""
        result = runner.invoke(
            app,
            ["--root", str(workspace_with_worktrees), "clean", "test-123"]
        )
        assert result.exit_code == 1
        assert "--yes flag required" in result.output

    def test_task_not_found(self, tmp_path):
        """clean returns error when task not found."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("""
schema_version: 1
repos:
  test: ./test
""")

        result = runner.invoke(
            app,
            ["--root", str(tmp_path), "clean", "nonexistent", "--yes"]
        )
        assert result.exit_code == 1
        assert "Task not found" in result.output

    def test_no_worktrees_to_clean(self, tmp_path):
        """clean handles case with no worktrees gracefully."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("""
schema_version: 1
repos:
  test: ./test
""")

        task_dir = tmp_path / ".hive/tasks" / "test-123"
        task_dir.mkdir(parents=True)
        plan_yaml = task_dir / "plan.yaml"
        plan_yaml.write_text("""
schema_version: "1"
id: test-123
title: Test
branch: feature/test
repos:
  test:
    tasks:
      - Test
""")

        result = runner.invoke(
            app,
            ["--root", str(tmp_path), "clean", "test-123", "--yes"]
        )
        assert result.exit_code == 0
        assert "No worktrees to clean" in result.output

    def test_removes_worktrees(self, workspace_with_worktrees):
        """clean removes worktree directories."""
        wt_base = workspace_with_worktrees / ".hive/wt" / "test-123"
        assert wt_base.exists()
        assert (wt_base / "frontend").exists()
        assert (wt_base / "backend").exists()

        result = runner.invoke(
            app,
            ["--root", str(workspace_with_worktrees), "clean", "test-123", "--yes"]
        )

        assert result.exit_code == 0
        assert "removed" in result.output
        assert not (wt_base / "frontend").exists()
        assert not (wt_base / "backend").exists()

    def test_preserves_task_directory(self, workspace_with_worktrees):
        """clean preserves the .hive/tasks/<task_id> directory."""
        task_dir = workspace_with_worktrees / ".hive/tasks" / "test-123"
        plan_path = task_dir / "plan.yaml"

        result = runner.invoke(
            app,
            ["--root", str(workspace_with_worktrees), "clean", "test-123", "--yes"]
        )

        assert result.exit_code == 0
        assert task_dir.exists()
        assert plan_path.exists()

    def test_json_output(self, workspace_with_worktrees):
        """clean supports --json output format."""
        result = runner.invoke(
            app,
            ["--root", str(workspace_with_worktrees), "clean", "test-123", "--yes", "--json"]
        )

        assert result.exit_code == 0
        import json
        output = json.loads(result.output)
        assert output["task_id"] == "test-123"
        assert "frontend" in output["removed"]
        assert "backend" in output["removed"]

    def test_json_error_without_yes(self, workspace_with_worktrees):
        """clean returns JSON error when --yes not provided."""
        result = runner.invoke(
            app,
            ["--root", str(workspace_with_worktrees), "clean", "test-123", "--json"]
        )

        assert result.exit_code == 1
        import json
        output = json.loads(result.output)
        assert "error" in output
        assert "--yes flag required" in output["error"]
