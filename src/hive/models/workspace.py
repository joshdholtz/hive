"""Workspace configuration models."""

from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


DEFAULT_SYMLINK_PATTERNS = [
    ".env",
    ".env.*",
    ".envrc",
    ".tool-versions",
    ".node-version",
    ".python-version",
    ".ruby-version",
]

DEFAULT_SETUP_COMMANDS = [
    "mise trust",
]


@dataclass
class ProviderConfig:
    """Configuration for issue provider."""

    type: str = "github"  # 'github' or 'linear'
    repo: str | None = None  # GitHub: owner/repo, Linear: team key
    project: str | None = None  # Linear: project name
    api_key_env: str | None = None  # Env var for API key

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "ProviderConfig":
        """Create from dictionary."""
        if not data:
            return cls()
        return cls(
            type=data.get("type", "github"),
            repo=data.get("repo"),
            project=data.get("project"),
            api_key_env=data.get("api_key_env"),
        )


@dataclass
class WorkspaceDefaults:
    """Default settings for workspace."""

    base_branch: str = "main"
    tmux: bool = True
    pr_create: bool = True
    symlink_files: list[str] | None = None
    setup_commands: list[str] | None = None
    issues_repo: str = "hive"  # Repo key for umbrella issues (GitHub provider)
    claude_yolo: bool = False  # Run Claude with --dangerously-skip-permissions

    @property
    def symlink_patterns(self) -> list[str]:
        """Return symlink patterns, using defaults if not specified."""
        if self.symlink_files is not None:
            return self.symlink_files
        return DEFAULT_SYMLINK_PATTERNS

    @property
    def worktree_setup_commands(self) -> list[str]:
        """Return setup commands to run after creating worktrees."""
        if self.setup_commands is not None:
            return self.setup_commands
        return DEFAULT_SETUP_COMMANDS


@dataclass
class RepoConfig:
    """Configuration for a single repository."""

    key: str
    path: Path

    @property
    def name(self) -> str:
        """Return the repository directory name."""
        return self.path.name


@dataclass
class WorkspaceConfig:
    """Parsed workspace.yaml configuration."""

    schema_version: int
    repos: dict[str, RepoConfig] = field(default_factory=dict)
    defaults: WorkspaceDefaults = field(default_factory=WorkspaceDefaults)
    provider: ProviderConfig = field(default_factory=ProviderConfig)

    def get_repo(self, key: str) -> RepoConfig | None:
        """Get repo config by key."""
        return self.repos.get(key)

    def get_repo_path(self, key: str) -> Path | None:
        """Get repo path by key."""
        repo = self.repos.get(key)
        return repo.path if repo else None

    @property
    def repo_keys(self) -> list[str]:
        """Return list of all repo keys."""
        return list(self.repos.keys())
