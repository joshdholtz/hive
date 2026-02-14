"""Git operations."""

import subprocess
from pathlib import Path
from typing import Any

from hive.core.exceptions import GitError


def run_git(
    args: list[str],
    cwd: Path | None = None,
    check: bool = True,
    capture_output: bool = True,
) -> subprocess.CompletedProcess[str]:
    """Run a git command safely.

    Args:
        args: Git command arguments (without 'git' prefix).
        cwd: Working directory.
        check: Raise exception on non-zero exit.
        capture_output: Capture stdout/stderr.

    Returns:
        Completed process result.

    Raises:
        GitError: If git command fails and check=True.
    """
    cmd = ["git"] + args
    try:
        result = subprocess.run(
            cmd,
            cwd=cwd,
            capture_output=capture_output,
            text=True,
            timeout=300,  # 5 minute timeout for long operations
        )
        if check and result.returncode != 0:
            error_msg = result.stderr.strip() or result.stdout.strip()
            raise GitError(f"git {' '.join(args)} failed: {error_msg}")
        return result
    except subprocess.TimeoutExpired as e:
        raise GitError(f"git {' '.join(args)} timed out") from e
    except FileNotFoundError as e:
        raise GitError("git not found in PATH") from e


def is_git_repo(path: Path) -> bool:
    """Check if path is a git repository.

    Args:
        path: Path to check.

    Returns:
        True if path is a git repo.
    """
    if not path.exists():
        return False
    try:
        result = run_git(
            ["rev-parse", "--git-dir"],
            cwd=path,
            check=False,
        )
        return result.returncode == 0
    except GitError:
        return False


def fetch_origin(repo_path: Path, prune: bool = True) -> None:
    """Fetch from origin.

    Args:
        repo_path: Path to git repository.
        prune: Whether to prune deleted remote branches.
    """
    args = ["fetch", "origin"]
    if prune:
        args.append("--prune")
    run_git(args, cwd=repo_path)


def get_current_branch(repo_path: Path) -> str | None:
    """Get the current branch name.

    Args:
        repo_path: Path to git repository.

    Returns:
        Branch name or None if detached HEAD.
    """
    try:
        result = run_git(
            ["rev-parse", "--abbrev-ref", "HEAD"],
            cwd=repo_path,
        )
        branch = result.stdout.strip()
        return None if branch == "HEAD" else branch
    except GitError:
        return None


def is_dirty(repo_path: Path) -> bool:
    """Check if working tree has uncommitted changes.

    Args:
        repo_path: Path to git repository.

    Returns:
        True if there are uncommitted changes.
    """
    try:
        result = run_git(
            ["status", "--porcelain"],
            cwd=repo_path,
        )
        return bool(result.stdout.strip())
    except GitError:
        return False


def get_ahead_behind(repo_path: Path, branch: str | None = None) -> tuple[int, int]:
    """Get commits ahead/behind upstream.

    Args:
        repo_path: Path to git repository.
        branch: Branch name (default: current branch).

    Returns:
        Tuple of (ahead, behind) counts.
    """
    if branch is None:
        branch = get_current_branch(repo_path)
    if not branch:
        return (0, 0)

    try:
        result = run_git(
            ["rev-list", "--left-right", "--count", f"{branch}...origin/{branch}"],
            cwd=repo_path,
            check=False,
        )
        if result.returncode != 0:
            return (0, 0)
        parts = result.stdout.strip().split()
        if len(parts) == 2:
            return (int(parts[0]), int(parts[1]))
        return (0, 0)
    except (GitError, ValueError):
        return (0, 0)


def get_last_commit(repo_path: Path) -> dict[str, str] | None:
    """Get info about the last commit.

    Args:
        repo_path: Path to git repository.

    Returns:
        Dict with sha and subject, or None.
    """
    try:
        result = run_git(
            ["log", "-1", "--format=%H%n%s"],
            cwd=repo_path,
        )
        lines = result.stdout.strip().split("\n")
        if len(lines) >= 2:
            return {
                "sha": lines[0][:12],  # Short SHA
                "subject": lines[1],
            }
        return None
    except GitError:
        return None


def get_git_status(repo_path: Path) -> dict[str, Any]:
    """Get git status information.

    Args:
        repo_path: Path to git repository.

    Returns:
        Dict with branch, dirty, ahead, behind, last_commit info.
    """
    branch = get_current_branch(repo_path)
    ahead, behind = get_ahead_behind(repo_path, branch)
    last_commit = get_last_commit(repo_path)

    return {
        "branch": branch,
        "dirty": is_dirty(repo_path),
        "ahead": ahead,
        "behind": behind,
        "last_commit": last_commit,
    }


def create_worktree(
    repo_path: Path,
    worktree_path: Path,
    branch: str,
    base_ref: str,
) -> None:
    """Create a git worktree.

    Args:
        repo_path: Path to main git repository.
        worktree_path: Path for new worktree.
        branch: Branch name to create.
        base_ref: Base ref to branch from (e.g., origin/main).
    """
    run_git(
        ["worktree", "add", "-b", branch, str(worktree_path), base_ref],
        cwd=repo_path,
    )


def remove_worktree(repo_path: Path, worktree_path: Path, force: bool = False) -> None:
    """Remove a git worktree.

    Args:
        repo_path: Path to main git repository.
        worktree_path: Path to worktree to remove.
        force: Force removal even if dirty.
    """
    args = ["worktree", "remove", str(worktree_path)]
    if force:
        args.append("--force")
    run_git(args, cwd=repo_path)


def clone_repo(url: str, dest: Path) -> None:
    """Clone a repository.

    Args:
        url: Repository URL (SSH or HTTPS).
        dest: Destination path.
    """
    run_git(["clone", url, str(dest)])


def worktree_exists(repo_path: Path, worktree_path: Path) -> bool:
    """Check if a worktree exists.

    Args:
        repo_path: Path to main git repository.
        worktree_path: Path to check for worktree.

    Returns:
        True if worktree exists.
    """
    try:
        result = run_git(["worktree", "list", "--porcelain"], cwd=repo_path)
        # Check if worktree_path appears in the list
        for line in result.stdout.split("\n"):
            if line.startswith("worktree "):
                wt_path = Path(line.replace("worktree ", ""))
                if wt_path.resolve() == worktree_path.resolve():
                    return True
        return False
    except GitError:
        return False
