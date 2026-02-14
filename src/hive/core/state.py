"""Task state management."""

import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def load_state(task_dir: Path) -> dict[str, Any] | None:
    """Load state.json for a task.

    Args:
        task_dir: Path to task directory.

    Returns:
        Parsed state or None if not found.
    """
    state_path = task_dir / "state.json"
    if not state_path.exists():
        return None

    try:
        with open(state_path) as f:
            return json.load(f)
    except (json.JSONDecodeError, OSError):
        return None


def save_state(task_dir: Path, state: dict[str, Any]) -> None:
    """Save state.json for a task.

    Args:
        task_dir: Path to task directory.
        state: State to save.
    """
    state_path = task_dir / "state.json"
    with open(state_path, "w") as f:
        json.dump(state, f, indent=2)


def create_state(
    task_id: str,
    branch: str,
    workspace_root: Path,
    repos: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    """Create a new state dict.

    Args:
        task_id: Task identifier.
        branch: Branch name.
        workspace_root: Workspace root path.
        repos: Repo configurations with paths.

    Returns:
        State dict.
    """
    return {
        "task_id": task_id,
        "branch": branch,
        "created_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "workspace_root": str(workspace_root),
        "tmux_session_name": f"hive-{task_id}",
        "repos": repos,
    }


def update_state_repo(
    state: dict[str, Any],
    repo_key: str,
    repo_path: Path,
    worktree_path: Path,
    base_branch: str,
) -> None:
    """Update or add repo info in state.

    Args:
        state: State dict to update.
        repo_key: Repo key.
        repo_path: Path to main repo.
        worktree_path: Path to worktree.
        base_branch: Base branch name.
    """
    if "repos" not in state:
        state["repos"] = {}

    state["repos"][repo_key] = {
        "repo_key": repo_key,
        "repo_path": str(repo_path),
        "worktree_path": str(worktree_path),
        "base_branch": base_branch,
    }


def load_prs(task_dir: Path) -> dict[str, Any] | None:
    """Load prs.json for a task.

    Args:
        task_dir: Path to task directory.

    Returns:
        Parsed PR info or None if not found.
    """
    prs_path = task_dir / "prs.json"
    if not prs_path.exists():
        return None

    try:
        with open(prs_path) as f:
            return json.load(f)
    except (json.JSONDecodeError, OSError):
        return None


def save_prs(task_dir: Path, prs: dict[str, Any]) -> None:
    """Save prs.json for a task.

    Args:
        task_dir: Path to task directory.
        prs: PR info to save.
    """
    prs_path = task_dir / "prs.json"
    with open(prs_path, "w") as f:
        json.dump(prs, f, indent=2)


def get_issue_state(state: dict[str, Any] | None) -> dict[str, Any] | None:
    """Get issue state from state dict.

    Args:
        state: State dict.

    Returns:
        Issue state dict or None if not present.
    """
    if state is None:
        return None
    return state.get("issue")


def update_issue_state(
    state: dict[str, Any],
    repo: str,
    number: int,
    url: str,
    sync_mode: str = "comment",
    status_comment_id: int | None = None,
) -> None:
    """Update or create issue state in state dict.

    Args:
        state: State dict to update.
        repo: GitHub repo (owner/name).
        number: Issue number.
        url: Issue URL.
        sync_mode: Sync mode (comment or body).
        status_comment_id: Optional status comment ID.
    """
    state["issue"] = {
        "enabled": True,
        "repo": repo,
        "number": number,
        "url": url,
        "sync_mode": sync_mode,
        "status_comment_id": status_comment_id,
    }


def update_issue_comment_id(state: dict[str, Any], comment_id: int) -> None:
    """Update status_comment_id in issue state.

    Args:
        state: State dict to update.
        comment_id: Comment ID.
    """
    if "issue" in state:
        state["issue"]["status_comment_id"] = comment_id
