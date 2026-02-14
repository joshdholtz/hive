"""Tests for hive issue v2 commands."""

import json
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from typer.testing import CliRunner

from hive.cli import app


runner = CliRunner()


class TestIssueNew:
    """Tests for hive issue new command."""

    def test_requires_repos(self, tmp_path):
        """issue new returns error when no repos specified."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("schema_version: 1\nrepos:\n  test: ./test\n")

        with patch("hive.commands.issue_v2.check_gh_auth", return_value=True):
            result = runner.invoke(
                app, ["--root", str(tmp_path), "issue", "new", "Test Issue", "--repos", ""]
            )

        assert result.exit_code == 1
        assert "No repos specified" in result.output

    def test_validates_repos_exist(self, tmp_path):
        """issue new validates repos exist in workspace."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("schema_version: 1\nrepos:\n  test: ./test\n")

        with patch("hive.commands.issue_v2.check_gh_auth", return_value=True):
            result = runner.invoke(
                app, ["--root", str(tmp_path), "issue", "new", "Test Issue", "--repos", "nonexistent"]
            )

        assert result.exit_code == 1
        assert "not found in workspace" in result.output

    def test_creates_issues_successfully(self, tmp_path):
        """issue new creates umbrella and repo issues."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("schema_version: 1\nrepos:\n  test: ./test\n  hive: ./hive\n")
        # Create repo dirs for git remote check
        (tmp_path / "test").mkdir()
        (tmp_path / "hive").mkdir()

        with patch("hive.commands.issue_v2.check_gh_auth", return_value=True), \
             patch("hive.commands.issue_v2.create_issue") as mock_create, \
             patch("hive.commands.issue_v2.update_issue_body") as mock_update, \
             patch("hive.commands.issue_v2._get_repo_github_path") as mock_path:
            mock_create.side_effect = [
                {"number": 42, "url": "https://github.com/org/hive/issues/42"},  # Umbrella
                {"number": 10, "url": "https://github.com/org/test/issues/10"},  # Repo
            ]
            mock_path.side_effect = ["org/hive", "org/test"]

            result = runner.invoke(
                app, ["--root", str(tmp_path), "issue", "new", "Test Issue", "--repos", "test"]
            )

        assert result.exit_code == 0
        assert "Created task #42" in result.output
        assert mock_create.call_count == 2

        # Check issues.json was created
        issues_path = tmp_path / ".hive" / "tasks" / "42" / "issues.json"
        assert issues_path.exists()
        issues_data = json.loads(issues_path.read_text())
        assert issues_data["umbrella"]["number"] == 42
        assert "test" in issues_data["repos"]

    def test_json_output(self, tmp_path):
        """issue new returns JSON output when requested."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("schema_version: 1\nrepos:\n  test: ./test\n  hive: ./hive\n")
        (tmp_path / "test").mkdir()
        (tmp_path / "hive").mkdir()

        with patch("hive.commands.issue_v2.check_gh_auth", return_value=True), \
             patch("hive.commands.issue_v2.create_issue") as mock_create, \
             patch("hive.commands.issue_v2.update_issue_body"), \
             patch("hive.commands.issue_v2._get_repo_github_path") as mock_path:
            mock_create.side_effect = [
                {"number": 42, "url": "https://github.com/org/hive/issues/42"},
                {"number": 10, "url": "https://github.com/org/test/issues/10"},
            ]
            mock_path.side_effect = ["org/hive", "org/test"]

            result = runner.invoke(
                app, ["--root", str(tmp_path), "issue", "new", "Test", "--repos", "test", "--json"]
            )

        assert result.exit_code == 0
        output = json.loads(result.output)
        assert output["umbrella"]["number"] == 42


class TestIssueList:
    """Tests for hive issue list command."""

    def test_empty_list(self, tmp_path):
        """issue list shows message when no tasks."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("schema_version: 1\nrepos:\n  test: ./test\n")

        result = runner.invoke(app, ["--root", str(tmp_path), "issue", "list"])

        assert result.exit_code == 0
        assert "No tasks found" in result.output

    def test_lists_tasks(self, tmp_path):
        """issue list shows existing tasks."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("schema_version: 1\nrepos:\n  test: ./test\n")

        # Create task with issues.json
        task_dir = tmp_path / ".hive" / "tasks" / "42"
        task_dir.mkdir(parents=True)
        issues_path = task_dir / "issues.json"
        issues_path.write_text(json.dumps({
            "umbrella": {
                "repo": "org/hive",
                "number": 42,
                "url": "https://github.com/org/hive/issues/42",
            },
            "repos": {
                "test": {
                    "repo": "org/test",
                    "number": 10,
                    "url": "https://github.com/org/test/issues/10",
                }
            }
        }))

        result = runner.invoke(app, ["--root", str(tmp_path), "issue", "list"])

        assert result.exit_code == 0
        assert "#42" in result.output
        assert "test" in result.output

    def test_json_output(self, tmp_path):
        """issue list returns JSON when requested."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("schema_version: 1\nrepos:\n  test: ./test\n")

        task_dir = tmp_path / ".hive" / "tasks" / "42"
        task_dir.mkdir(parents=True)
        issues_path = task_dir / "issues.json"
        issues_path.write_text(json.dumps({
            "umbrella": {"number": 42, "url": "https://example.com"},
            "repos": {"test": {}},
        }))

        result = runner.invoke(app, ["--root", str(tmp_path), "issue", "list", "--json"])

        assert result.exit_code == 0
        output = json.loads(result.output)
        assert len(output["tasks"]) == 1
        assert output["tasks"][0]["number"] == 42


class TestIssueSync:
    """Tests for hive issue sync command."""

    def test_task_not_found(self, tmp_path):
        """issue sync returns error when task not found."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("schema_version: 1\nrepos:\n  test: ./test\n")

        with patch("hive.commands.issue_v2.check_gh_auth", return_value=True):
            result = runner.invoke(
                app, ["--root", str(tmp_path), "issue", "sync", "999"]
            )

        assert result.exit_code == 1
        assert "Task not found" in result.output

    def test_syncs_successfully(self, tmp_path):
        """issue sync updates umbrella issue."""
        workspace_yaml = tmp_path / "workspace.yaml"
        workspace_yaml.write_text("schema_version: 1\nrepos:\n  test: ./test\n")

        task_dir = tmp_path / ".hive" / "tasks" / "42"
        task_dir.mkdir(parents=True)
        issues_path = task_dir / "issues.json"
        issues_path.write_text(json.dumps({
            "umbrella": {
                "repo": "org/hive",
                "number": 42,
                "url": "https://github.com/org/hive/issues/42",
            },
            "repos": {
                "test": {
                    "repo": "org/test",
                    "number": 10,
                    "url": "https://github.com/org/test/issues/10",
                }
            }
        }))

        with patch("hive.commands.issue_v2.check_gh_auth", return_value=True), \
             patch("hive.commands.issue_v2.get_issue_info") as mock_info, \
             patch("hive.commands.issue_v2.update_issue_body") as mock_update:
            mock_info.return_value = {"state": "OPEN"}

            result = runner.invoke(
                app, ["--root", str(tmp_path), "issue", "sync", "42"]
            )

        assert result.exit_code == 0
        assert "Synced successfully" in result.output
        mock_update.assert_called_once()
