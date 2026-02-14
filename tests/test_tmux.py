"""Tests for tmux operations."""

import os
from unittest.mock import MagicMock, patch

import pytest

from hive.core.exceptions import TmuxError
from hive.core.tmux import (
    is_inside_tmux,
    list_windows,
    run_tmux,
    session_exists,
    window_exists,
)


class TestRunTmux:
    """Tests for run_tmux function."""

    def test_returns_completed_process(self):
        """run_tmux returns a completed process object."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(
                returncode=0,
                stdout="output",
                stderr="",
            )
            result = run_tmux(["list-sessions"], check=False)
            assert result.returncode == 0

    def test_raises_on_failure_when_check_true(self):
        """run_tmux raises TmuxError when command fails and check=True."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(
                returncode=1,
                stdout="",
                stderr="session not found",
            )
            with pytest.raises(TmuxError, match="session not found"):
                run_tmux(["has-session", "-t", "nonexistent"])

    def test_does_not_raise_on_failure_when_check_false(self):
        """run_tmux does not raise when command fails and check=False."""
        with patch("subprocess.run") as mock_run:
            mock_run.return_value = MagicMock(
                returncode=1,
                stdout="",
                stderr="error",
            )
            result = run_tmux(["has-session", "-t", "test"], check=False)
            assert result.returncode == 1

    def test_raises_on_timeout(self):
        """run_tmux raises TmuxError on timeout."""
        import subprocess

        with patch("subprocess.run") as mock_run:
            mock_run.side_effect = subprocess.TimeoutExpired(cmd="tmux", timeout=30)
            with pytest.raises(TmuxError, match="timed out"):
                run_tmux(["list-sessions"])

    def test_raises_when_tmux_not_found(self):
        """run_tmux raises TmuxError when tmux is not installed."""
        with patch("subprocess.run") as mock_run:
            mock_run.side_effect = FileNotFoundError()
            with pytest.raises(TmuxError, match="not found in PATH"):
                run_tmux(["list-sessions"])


class TestSessionExists:
    """Tests for session_exists function."""

    def test_returns_true_when_session_exists(self):
        """session_exists returns True when session exists."""
        with patch("hive.core.tmux.run_tmux") as mock_run:
            mock_run.return_value = MagicMock(returncode=0)
            assert session_exists("test-session") is True

    def test_returns_false_when_session_missing(self):
        """session_exists returns False when session doesn't exist."""
        with patch("hive.core.tmux.run_tmux") as mock_run:
            mock_run.return_value = MagicMock(returncode=1)
            assert session_exists("nonexistent") is False

    def test_returns_false_on_error(self):
        """session_exists returns False on TmuxError."""
        with patch("hive.core.tmux.run_tmux") as mock_run:
            mock_run.side_effect = TmuxError("tmux not found")
            assert session_exists("any") is False


class TestListWindows:
    """Tests for list_windows function."""

    def test_returns_window_names(self):
        """list_windows returns list of window names."""
        with patch("hive.core.tmux.run_tmux") as mock_run:
            mock_run.return_value = MagicMock(
                stdout="frontend\nbackend\ndashboard\n",
            )
            windows = list_windows("test-session")
            assert windows == ["frontend", "backend", "dashboard"]

    def test_returns_empty_list_on_error(self):
        """list_windows returns empty list on error."""
        with patch("hive.core.tmux.run_tmux") as mock_run:
            mock_run.side_effect = TmuxError("session not found")
            assert list_windows("nonexistent") == []


class TestWindowExists:
    """Tests for window_exists function."""

    def test_returns_true_when_window_exists(self):
        """window_exists returns True when window exists."""
        with patch("hive.core.tmux.list_windows") as mock_list:
            mock_list.return_value = ["frontend", "backend", "dashboard"]
            assert window_exists("test", "backend") is True

    def test_returns_false_when_window_missing(self):
        """window_exists returns False when window doesn't exist."""
        with patch("hive.core.tmux.list_windows") as mock_list:
            mock_list.return_value = ["frontend", "backend"]
            assert window_exists("test", "concierge") is False


class TestIsInsideTmux:
    """Tests for is_inside_tmux function."""

    def test_returns_true_when_tmux_env_set(self):
        """is_inside_tmux returns True when TMUX env is set."""
        with patch.dict(os.environ, {"TMUX": "/tmp/tmux-1000/default,12345,0"}):
            assert is_inside_tmux() is True

    def test_returns_false_when_tmux_env_not_set(self):
        """is_inside_tmux returns False when TMUX env is not set."""
        env = os.environ.copy()
        env.pop("TMUX", None)
        with patch.dict(os.environ, env, clear=True):
            assert is_inside_tmux() is False
