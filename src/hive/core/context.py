"""Workspace context and configuration loading."""

from dataclasses import dataclass, field
from pathlib import Path

from hive.core.exceptions import WorkspaceError
from hive.models.workspace import WorkspaceConfig


@dataclass
class WorkspaceContext:
    """Runtime context for hive commands."""

    root: Path
    no_color: bool = False
    verbose: bool = False
    _workspace_config: WorkspaceConfig | None = field(default=None, repr=False)

    @property
    def hive_dir(self) -> Path:
        """Return path to .hive directory."""
        return self.root / ".hive"

    @property
    def tasks_dir(self) -> Path:
        """Return path to .hive/tasks directory."""
        return self.hive_dir / "tasks"

    @property
    def worktrees_dir(self) -> Path:
        """Return path to .hive/wt directory."""
        return self.hive_dir / "wt"

    @property
    def workspace_yaml(self) -> Path:
        """Return path to workspace.yaml."""
        return self.root / "workspace.yaml"

    @property
    def workspace(self) -> WorkspaceConfig:
        """Return loaded workspace configuration.

        Raises:
            WorkspaceError: If workspace.yaml is not loaded.
        """
        if self._workspace_config is None:
            raise WorkspaceError("Workspace configuration not loaded")
        return self._workspace_config

    def has_workspace(self) -> bool:
        """Check if workspace.yaml exists."""
        return self.workspace_yaml.exists()

    def load_workspace(self) -> WorkspaceConfig:
        """Load workspace configuration from workspace.yaml.

        Returns:
            Loaded workspace configuration.

        Raises:
            WorkspaceError: If loading fails.
        """
        from hive.core.workspace import load_workspace

        self._workspace_config = load_workspace(self.workspace_yaml)
        return self._workspace_config

    def ensure_dirs(self) -> None:
        """Ensure .hive/tasks and .hive/wt directories exist."""
        self.tasks_dir.mkdir(parents=True, exist_ok=True)
        self.worktrees_dir.mkdir(parents=True, exist_ok=True)

    def task_dir(self, task_id: str) -> Path:
        """Return path to a specific task directory."""
        return self.tasks_dir / task_id

    def worktree_dir(self, task_id: str, repo_key: str) -> Path:
        """Return path to a specific worktree directory."""
        return self.worktrees_dir / task_id / repo_key


def get_context(
    root: str | Path | None = None,
    no_color: bool = False,
    verbose: bool = False,
) -> WorkspaceContext:
    """Create a workspace context.

    Args:
        root: Workspace root directory. Defaults to current directory.
        no_color: Disable colored output.
        verbose: Enable verbose output.

    Returns:
        Configured workspace context.
    """
    if root is None:
        resolved_root = Path.cwd()
    else:
        resolved_root = Path(root).resolve()

    return WorkspaceContext(
        root=resolved_root,
        no_color=no_color,
        verbose=verbose,
    )
