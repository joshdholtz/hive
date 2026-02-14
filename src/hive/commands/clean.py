"""hive clean - Remove task worktrees and windows."""

import json
import shutil
from pathlib import Path
from typing import Annotated, Any

import typer

from hive.core.context import get_context
from hive.core.exceptions import GitError, TmuxError, WorkspaceError
from hive.core.git import remove_worktree
from hive.core.tmux import run_tmux, session_exists, list_windows

SESSION_NAME = "hive-planner"


def clean(
    ctx: typer.Context,
    task_id: Annotated[
        str,
        typer.Argument(help="Task ID to clean."),
    ],
    yes: Annotated[
        bool,
        typer.Option("--yes", "-y", help="Confirm deletion (required)."),
    ] = False,
    keep_windows: Annotated[
        bool,
        typer.Option("--keep-windows", help="Don't close tmux windows."),
    ] = False,
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """Remove worktrees and tmux windows for a task.

    Removes all git worktrees under .hive/wt/<task_id>/.
    Closes any tmux windows named <task_id>-*.
    Does NOT delete the .hive/tasks/<task_id>/ directory.

    Requires --yes flag to confirm.
    """
    if not yes:
        if json_output:
            typer.echo(json.dumps({"error": "--yes flag required to confirm deletion"}))
        else:
            typer.secho("Error: --yes flag required to confirm deletion.", fg="red", err=True)
            typer.echo()
            typer.echo("This will remove all worktrees and close tmux windows for the task.")
            typer.echo("The task data will be preserved in .hive/tasks/")
            typer.echo()
            typer.echo(f"Run: hive clean {task_id} --yes")
        raise typer.Exit(1)

    # Get context from global options
    root = ctx.obj.get("root") if ctx.obj else None
    no_color = ctx.obj.get("no_color", False) if ctx.obj else False

    context = get_context(root=root, no_color=no_color)

    # Load workspace (optional - needed for main repo paths)
    workspace = None
    if context.has_workspace():
        try:
            workspace = context.load_workspace()
        except WorkspaceError:
            pass

    # Check if task exists (v2: issues.json, v1: plan.yaml)
    task_dir = context.task_dir(task_id)
    issues_path = task_dir / "issues.json"
    plan_path = task_dir / "plan.yaml"

    if not issues_path.exists() and not plan_path.exists():
        if json_output:
            typer.echo(json.dumps({"error": f"Task not found: {task_id}"}))
        else:
            typer.secho(f"Error: Task not found: {task_id}", fg="red", err=True)
        raise typer.Exit(1)

    # Get repo list from issues.json (v2) or plan.yaml (v1)
    repo_keys = _get_task_repos(issues_path, plan_path)

    # Get worktree base directory
    worktree_base = context.worktrees_dir / task_id

    if not json_output:
        typer.echo(f"Cleaning task: {task_id}")
        typer.echo()

    results: list[dict[str, Any]] = []
    windows_closed: list[str] = []

    # Close tmux windows first
    if not keep_windows:
        windows_closed = _close_task_windows(task_id, json_output)

    # Remove worktrees
    if worktree_base.exists():
        # Process known repos
        for repo_key in repo_keys:
            worktree_path = worktree_base / repo_key
            if worktree_path.exists():
                result = _remove_worktree(
                    context=context,
                    workspace=workspace,
                    repo_key=repo_key,
                    worktree_path=worktree_path,
                    json_output=json_output,
                )
                results.append(result)

        # Also check for any other directories in the worktree base
        for entry in worktree_base.iterdir():
            if entry.is_dir() and entry.name not in repo_keys:
                result = _remove_worktree(
                    context=context,
                    workspace=workspace,
                    repo_key=entry.name,
                    worktree_path=entry,
                    json_output=json_output,
                )
                results.append(result)

        # Remove the task's worktree base directory if empty
        try:
            if worktree_base.exists() and not any(worktree_base.iterdir()):
                worktree_base.rmdir()
        except OSError:
            pass
    else:
        if not json_output:
            typer.echo("No worktrees to clean.")

    # Output results
    if json_output:
        output = {
            "task_id": task_id,
            "removed": [r["repo_key"] for r in results if r["status"] == "removed"],
            "failed": [r["repo_key"] for r in results if r["status"] == "error"],
            "windows_closed": windows_closed,
            "results": results,
        }
        typer.echo(json.dumps(output, indent=2))
    else:
        typer.echo()
        _print_summary(results, windows_closed)


def _get_task_repos(issues_path: Path, plan_path: Path) -> list[str]:
    """Get repo keys from task config (v2 or v1)."""
    # Try v2 (issues.json) first
    if issues_path.exists():
        try:
            with open(issues_path) as f:
                data = json.load(f)
            return list(data.get("repos", {}).keys())
        except (json.JSONDecodeError, OSError):
            pass

    # Fall back to v1 (plan.yaml)
    if plan_path.exists():
        try:
            from hive.core.plan import get_plan_repos, load_plan
            plan = load_plan(plan_path)
            return list(get_plan_repos(plan).keys())
        except Exception:
            pass

    return []


def _close_task_windows(task_id: str, json_output: bool) -> list[str]:
    """Close all tmux windows for a task."""
    closed = []

    if not session_exists(SESSION_NAME):
        return closed

    try:
        windows = list_windows(SESSION_NAME)
        prefix = f"{task_id}-"

        for window in windows:
            if window.startswith(prefix):
                try:
                    run_tmux(["kill-window", "-t", f"{SESSION_NAME}:{window}"])
                    closed.append(window)
                    if not json_output:
                        typer.echo(f"  Closed window: {window}")
                except TmuxError:
                    pass
    except TmuxError:
        pass

    return closed


def _remove_worktree(
    context,
    workspace,
    repo_key: str,
    worktree_path: Path,
    json_output: bool,
) -> dict[str, Any]:
    """Remove a single worktree."""
    result: dict[str, Any] = {
        "repo_key": repo_key,
        "worktree_path": str(worktree_path),
        "status": "unknown",
    }

    # Get the main repo path
    main_repo_path = None
    if workspace:
        repo_config = workspace.get_repo(repo_key)
        if repo_config:
            main_repo_path = repo_config.path

    # Try to use git worktree remove first
    if main_repo_path and main_repo_path.exists():
        try:
            remove_worktree(main_repo_path, worktree_path, force=True)
            result["status"] = "removed"
            if not json_output:
                typer.secho(f"  {repo_key}: worktree removed", fg="green")
            return result
        except GitError as e:
            # Git worktree remove failed, try manual removal
            if not json_output:
                typer.echo(f"  {repo_key}: git worktree remove failed, trying manual removal...")

    # Fallback: manual directory removal
    try:
        if worktree_path.exists():
            shutil.rmtree(worktree_path)
        result["status"] = "removed"
        if not json_output:
            typer.secho(f"  {repo_key}: removed (manual)", fg="green")
    except OSError as e:
        result["status"] = "error"
        result["error"] = str(e)
        if not json_output:
            typer.secho(f"  {repo_key}: failed - {e}", fg="red")
        return result

    # Prune orphaned worktree references and delete branch
    if main_repo_path and main_repo_path.exists():
        _cleanup_git_refs(main_repo_path, worktree_path, json_output)

    return result


def _cleanup_git_refs(main_repo_path: Path, worktree_path: Path, json_output: bool) -> None:
    """Prune orphaned worktrees and delete associated branch."""
    import subprocess

    # Prune orphaned worktree references
    try:
        subprocess.run(
            ["git", "worktree", "prune"],
            cwd=main_repo_path,
            capture_output=True,
            timeout=10,
        )
    except Exception:
        pass

    # Try to extract and delete the branch
    # Branch name is typically the last component of worktree path
    branch_name = f"feat/{worktree_path.parent.name}-{worktree_path.name}"
    try:
        result = subprocess.run(
            ["git", "branch", "-D", branch_name],
            cwd=main_repo_path,
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode == 0 and not json_output:
            typer.echo(f"    deleted branch: {branch_name}")
    except Exception:
        pass


def _print_summary(results: list[dict[str, Any]], windows_closed: list[str]) -> None:
    """Print summary of clean operation."""
    removed = sum(1 for r in results if r["status"] == "removed")
    failed = sum(1 for r in results if r["status"] == "error")

    typer.echo("Summary:")
    if windows_closed:
        typer.echo(f"  Windows closed: {len(windows_closed)}")
    if removed:
        typer.secho(f"  Worktrees removed: {removed}", fg="green")
    if failed:
        typer.secho(f"  Failed: {failed}", fg="red")
        for r in results:
            if r["status"] == "error":
                typer.echo(f"    {r['repo_key']}: {r.get('error', 'unknown error')}")

    if (removed or windows_closed) and not failed:
        typer.echo()
        typer.echo("Task cleaned. Data preserved in .hive/tasks/")
