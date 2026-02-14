"""Custom exceptions for hive."""


class HiveError(Exception):
    """Base exception for hive errors."""

    exit_code: int = 1


class WorkspaceError(HiveError):
    """Invalid workspace configuration."""

    exit_code: int = 3


class PlanError(HiveError):
    """Invalid plan configuration."""

    exit_code: int = 1


class DependencyError(HiveError):
    """Missing external dependency."""

    exit_code: int = 2


class GitError(HiveError):
    """Git operation failed."""

    exit_code: int = 1


class GhError(HiveError):
    """GitHub CLI operation failed."""

    exit_code: int = 2


class TmuxError(HiveError):
    """Tmux operation failed."""

    exit_code: int = 1
