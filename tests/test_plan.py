"""Tests for plan loading and management."""

import tempfile
from datetime import date
from pathlib import Path

import pytest

from hive.core.exceptions import PlanError
from hive.core.plan import (
    create_plan,
    generate_task_id,
    load_plan,
    slugify,
    validate_plan,
)


class TestSlugify:
    """Tests for slugify function."""

    def test_simple_text(self):
        assert slugify("hello") == "hello"

    def test_spaces(self):
        assert slugify("hello world") == "hello-world"

    def test_uppercase(self):
        assert slugify("Hello World") == "hello-world"

    def test_special_chars(self):
        assert slugify("Hello, World!") == "hello-world"

    def test_underscores(self):
        assert slugify("hello_world") == "hello-world"

    def test_multiple_spaces(self):
        assert slugify("hello   world") == "hello-world"

    def test_leading_trailing(self):
        assert slugify("  hello world  ") == "hello-world"

    def test_long_text(self):
        long_text = "a" * 100
        result = slugify(long_text)
        assert len(result) <= 50


class TestGenerateTaskId:
    """Tests for generate_task_id function."""

    def test_with_date(self):
        result = generate_task_id("My Task", task_date=date(2026, 2, 12))
        assert result == "2026-02-12-my-task"

    def test_complex_title(self):
        result = generate_task_id("Add POST /v1/attribution", task_date=date(2026, 1, 15))
        # Slashes are removed, resulting in "v1attribution"
        assert result == "2026-01-15-add-post-v1attribution"


class TestValidatePlan:
    """Tests for validate_plan function."""

    def test_valid_minimal_plan(self):
        config = {
            "schema_version": 1,
            "id": "2026-02-12-test",
            "title": "Test Task",
            "branch": "feat/test",
        }
        errors = validate_plan(config)
        assert errors == []

    def test_valid_full_plan(self):
        config = {
            "schema_version": 1,
            "id": "2026-02-12-test",
            "title": "Test Task",
            "branch": "feat/test",
            "repos": {
                "backend": {
                    "base": "main",
                    "tasks": ["Add endpoint", "Add tests"],
                },
            },
            "tmux": {"enabled": True},
            "pr": {"enabled": True, "draft": True},
        }
        errors = validate_plan(config)
        assert errors == []

    def test_missing_schema_version(self):
        config = {"id": "test", "title": "Test", "branch": "feat/test"}
        errors = validate_plan(config)
        assert "Missing required field: schema_version" in errors

    def test_missing_id(self):
        config = {"schema_version": 1, "title": "Test", "branch": "feat/test"}
        errors = validate_plan(config)
        assert "Missing required field: id" in errors

    def test_missing_title(self):
        config = {"schema_version": 1, "id": "test", "branch": "feat/test"}
        errors = validate_plan(config)
        assert "Missing required field: title" in errors

    def test_missing_branch(self):
        config = {"schema_version": 1, "id": "test", "title": "Test"}
        errors = validate_plan(config)
        assert "Missing required field: branch" in errors

    def test_invalid_repos_tasks(self):
        config = {
            "schema_version": 1,
            "id": "test",
            "title": "Test",
            "branch": "feat/test",
            "repos": {
                "backend": {
                    "tasks": "not a list",
                },
            },
        }
        errors = validate_plan(config)
        assert any("must be a list" in e for e in errors)


class TestCreatePlan:
    """Tests for create_plan function."""

    def test_creates_plan_file(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tasks_dir = Path(tmpdir)
            plan_path = create_plan(
                tasks_dir=tasks_dir,
                task_id="2026-02-12-test",
                title="Test Task",
            )

            assert plan_path.exists()
            assert plan_path.name == "plan.yaml"

    def test_creates_task_directory(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tasks_dir = Path(tmpdir)
            create_plan(
                tasks_dir=tasks_dir,
                task_id="2026-02-12-test",
                title="Test Task",
            )

            task_dir = tasks_dir / "2026-02-12-test"
            assert task_dir.exists()
            assert task_dir.is_dir()

    def test_includes_repos(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tasks_dir = Path(tmpdir)
            plan_path = create_plan(
                tasks_dir=tasks_dir,
                task_id="2026-02-12-test",
                title="Test Task",
                repos=["backend", "sdk_ios"],
            )

            plan = load_plan(plan_path)
            assert "repos" in plan
            assert "backend" in plan["repos"]
            assert "sdk_ios" in plan["repos"]

    def test_rejects_duplicate(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tasks_dir = Path(tmpdir)
            create_plan(
                tasks_dir=tasks_dir,
                task_id="2026-02-12-test",
                title="Test Task",
            )

            with pytest.raises(PlanError) as exc_info:
                create_plan(
                    tasks_dir=tasks_dir,
                    task_id="2026-02-12-test",
                    title="Test Task 2",
                )
            assert "already exists" in str(exc_info.value)


class TestLoadPlan:
    """Tests for load_plan function."""

    def test_loads_valid_plan(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tasks_dir = Path(tmpdir)
            plan_path = create_plan(
                tasks_dir=tasks_dir,
                task_id="2026-02-12-test",
                title="Test Task",
            )

            plan = load_plan(plan_path)
            assert plan["id"] == "2026-02-12-test"
            assert plan["title"] == "Test Task"

    def test_missing_file(self):
        with pytest.raises(PlanError) as exc_info:
            load_plan(Path("/nonexistent/plan.yaml"))
        assert "not found" in str(exc_info.value)
