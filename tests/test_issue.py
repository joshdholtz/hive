"""Tests for issue configuration in plan module."""

import json
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from typer.testing import CliRunner

from hive.core.plan import get_issue_config, validate_plan


class TestIssueValidation:
    """Tests for issue block validation in plan."""

    def test_valid_issue_config(self):
        """Valid issue config passes validation."""
        config = {
            "schema_version": 1,
            "id": "test-123",
            "title": "Test",
            "branch": "feature/test",
            "issue": {
                "enabled": True,
                "repo": "org/repo",
                "sync_mode": "comment",
            },
        }
        errors = validate_plan(config)
        assert errors == []

    def test_invalid_sync_mode(self):
        """Invalid sync_mode is rejected."""
        config = {
            "schema_version": 1,
            "id": "test-123",
            "title": "Test",
            "branch": "feature/test",
            "issue": {
                "sync_mode": "invalid",
            },
        }
        errors = validate_plan(config)
        assert any("sync_mode" in e for e in errors)

    def test_invalid_repo_format(self):
        """Repo without slash is rejected."""
        config = {
            "schema_version": 1,
            "id": "test-123",
            "title": "Test",
            "branch": "feature/test",
            "issue": {
                "repo": "just-repo-name",
            },
        }
        errors = validate_plan(config)
        assert any("owner/name" in e for e in errors)


class TestGetIssueConfig:
    """Tests for get_issue_config helper."""

    def test_returns_defaults_when_missing(self):
        """Returns defaults when issue block is missing."""
        plan = {"schema_version": 1, "id": "test", "title": "Test", "branch": "main"}
        config = get_issue_config(plan)
        assert config["enabled"] is False
        assert config["sync_mode"] == "comment"
        assert config["title_template"] == "{id}: {title}"

    def test_returns_configured_values(self):
        """Returns configured values when present."""
        plan = {
            "schema_version": 1,
            "id": "test",
            "title": "Test",
            "branch": "main",
            "issue": {
                "enabled": True,
                "repo": "org/repo",
                "number": 42,
                "sync_mode": "body",
            },
        }
        config = get_issue_config(plan)
        assert config["enabled"] is True
        assert config["repo"] == "org/repo"
        assert config["number"] == 42
        assert config["sync_mode"] == "body"
