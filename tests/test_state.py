"""Tests for state management."""

import tempfile
from pathlib import Path

from hive.core.state import (
    create_state,
    load_prs,
    load_state,
    save_prs,
    save_state,
    update_state_repo,
)


class TestState:
    """Tests for state management functions."""

    def test_create_state(self):
        state = create_state(
            task_id="2026-02-12-test",
            branch="feat/test",
            workspace_root=Path("/workspace"),
            repos={},
        )

        assert state["task_id"] == "2026-02-12-test"
        assert state["branch"] == "feat/test"
        assert state["workspace_root"] == "/workspace"
        assert state["tmux_session_name"] == "hive-2026-02-12-test"
        assert "created_at" in state

    def test_save_and_load_state(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            task_dir = Path(tmpdir)
            state = create_state(
                task_id="2026-02-12-test",
                branch="feat/test",
                workspace_root=Path("/workspace"),
                repos={},
            )

            save_state(task_dir, state)
            loaded = load_state(task_dir)

            assert loaded is not None
            assert loaded["task_id"] == "2026-02-12-test"

    def test_load_missing_state(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            task_dir = Path(tmpdir)
            loaded = load_state(task_dir)
            assert loaded is None

    def test_update_state_repo(self):
        state = create_state(
            task_id="2026-02-12-test",
            branch="feat/test",
            workspace_root=Path("/workspace"),
            repos={},
        )

        update_state_repo(
            state,
            repo_key="backend",
            repo_path=Path("/workspace/backend"),
            worktree_path=Path("/workspace/.wt/2026-02-12-test/backend"),
            base_branch="main",
        )

        assert "backend" in state["repos"]
        assert state["repos"]["backend"]["repo_key"] == "backend"
        assert state["repos"]["backend"]["base_branch"] == "main"


class TestPrs:
    """Tests for PR state management."""

    def test_save_and_load_prs(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            task_dir = Path(tmpdir)
            prs = {
                "repos": {
                    "backend": {
                        "pr_url": "https://github.com/org/backend/pull/123",
                        "pr_number": 123,
                        "pr_state": "OPEN",
                    }
                }
            }

            save_prs(task_dir, prs)
            loaded = load_prs(task_dir)

            assert loaded is not None
            assert loaded["repos"]["backend"]["pr_number"] == 123

    def test_load_missing_prs(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            task_dir = Path(tmpdir)
            loaded = load_prs(task_dir)
            assert loaded is None
