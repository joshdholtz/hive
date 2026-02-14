"""Tests for workspace loading and validation."""

import tempfile
from pathlib import Path

import pytest

from hive.core.workspace import (
    create_workspace_yaml,
    load_workspace,
    resolve_repo_path,
    validate_workspace,
)
from hive.core.exceptions import WorkspaceError


class TestValidateWorkspace:
    """Tests for validate_workspace function."""

    def test_valid_minimal_config(self):
        config = {
            "schema_version": 1,
            "repos": {"backend": "./backend"},
        }
        errors = validate_workspace(config)
        assert errors == []

    def test_valid_full_config(self):
        config = {
            "schema_version": 1,
            "repos": {
                "backend": "./backend",
                "sdk_ios": "./sdk-ios",
            },
            "defaults": {
                "base_branch": "main",
                "tmux": True,
                "pr_create": False,
            },
        }
        errors = validate_workspace(config)
        assert errors == []

    def test_missing_schema_version(self):
        config = {"repos": {"backend": "./backend"}}
        errors = validate_workspace(config)
        assert "Missing required field: schema_version" in errors

    def test_invalid_schema_version_type(self):
        config = {"schema_version": "1", "repos": {"backend": "./backend"}}
        errors = validate_workspace(config)
        assert any("schema_version must be an integer" in e for e in errors)

    def test_unsupported_schema_version(self):
        config = {"schema_version": 99, "repos": {"backend": "./backend"}}
        errors = validate_workspace(config)
        assert any("Unsupported schema_version" in e for e in errors)

    def test_missing_repos(self):
        config = {"schema_version": 1}
        errors = validate_workspace(config)
        assert "Missing required field: repos" in errors

    def test_empty_repos(self):
        config = {"schema_version": 1, "repos": {}}
        errors = validate_workspace(config)
        assert "repos cannot be empty" in errors

    def test_invalid_repo_path_type(self):
        config = {"schema_version": 1, "repos": {"backend": 123}}
        errors = validate_workspace(config)
        assert any("must be a string" in e for e in errors)


class TestLoadWorkspace:
    """Tests for load_workspace function."""

    def test_load_valid_workspace(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_path = Path(tmpdir) / "workspace.yaml"
            create_workspace_yaml(workspace_path, {"backend": "./backend"})

            config = load_workspace(workspace_path)
            assert config.schema_version == 1
            assert "backend" in config.repos
            assert config.repos["backend"].key == "backend"

    def test_load_missing_file(self):
        with pytest.raises(WorkspaceError) as exc_info:
            load_workspace(Path("/nonexistent/workspace.yaml"))
        assert "not found" in str(exc_info.value)

    def test_load_invalid_yaml(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_path = Path(tmpdir) / "workspace.yaml"
            workspace_path.write_text("invalid: yaml: content: [")
            with pytest.raises(WorkspaceError) as exc_info:
                load_workspace(workspace_path)
            assert "Invalid YAML" in str(exc_info.value)

    def test_load_with_defaults(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_path = Path(tmpdir) / "workspace.yaml"
            create_workspace_yaml(
                workspace_path,
                {"backend": "./backend"},
                defaults={"base_branch": "develop", "tmux": False},
            )

            config = load_workspace(workspace_path)
            assert config.defaults.base_branch == "develop"
            assert config.defaults.tmux is False
            assert config.defaults.pr_create is True  # default value


class TestResolveRepoPath:
    """Tests for resolve_repo_path function."""

    def test_relative_path(self):
        root = Path("/workspace")
        result = resolve_repo_path(root, "./backend")
        assert result == Path("/workspace/backend")

    def test_absolute_path(self):
        root = Path("/workspace")
        result = resolve_repo_path(root, "/absolute/path/repo")
        assert result == Path("/absolute/path/repo")

    def test_parent_relative_path(self):
        root = Path("/workspace/sub")
        result = resolve_repo_path(root, "../backend")
        assert result == Path("/workspace/backend")


class TestCreateWorkspaceYaml:
    """Tests for create_workspace_yaml function."""

    def test_create_minimal(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_path = Path(tmpdir) / "workspace.yaml"
            create_workspace_yaml(workspace_path, {"backend": "./backend"})

            assert workspace_path.exists()
            config = load_workspace(workspace_path)
            assert config.schema_version == 1
            assert "backend" in config.repos

    def test_create_with_defaults(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace_path = Path(tmpdir) / "workspace.yaml"
            create_workspace_yaml(
                workspace_path,
                {"backend": "./backend"},
                defaults={"base_branch": "develop"},
            )

            config = load_workspace(workspace_path)
            assert config.defaults.base_branch == "develop"
