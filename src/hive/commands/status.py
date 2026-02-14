"""hive status - Show task status."""

import json
from pathlib import Path
from typing import Annotated, Any

import typer

from hive.core.context import get_context
from hive.core.exceptions import PlanError, WorkspaceError
from hive.core.gh import get_pr_info
from hive.core.git import get_git_status
from hive.core.plan import get_issue_config, get_plan_branch, get_plan_repos, load_plan
from hive.core.state import get_issue_state, load_prs, load_state


def status(
    ctx: typer.Context,
    task_id: Annotated[
        str,
        typer.Argument(help="Task ID to check status for."),
    ],
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """Show status for a task.

    Reports per-repo:
    - Worktree existence
    - Git branch, dirty state, ahead/behind
    - Last commit SHA and subject
    - PR information (URL, state, number)

    JSON output includes full structured data for LLM consumption.
    """
    # Get context from global options
    root = ctx.obj.get("root") if ctx.obj else None
    no_color = ctx.obj.get("no_color", False) if ctx.obj else False

    context = get_context(root=root, no_color=no_color)

    # Load workspace (optional - needed for repo paths)
    workspace = None
    if context.has_workspace():
        try:
            workspace = context.load_workspace()
        except WorkspaceError:
            pass

    # Load plan
    task_dir = context.task_dir(task_id)
    plan_path = task_dir / "plan.yaml"

    if not plan_path.exists():
        if json_output:
            typer.echo(json.dumps({"error": f"Task not found: {task_id}"}))
        else:
            typer.secho(f"Error: Task not found: {task_id}", fg="red", err=True)
        raise typer.Exit(1)

    try:
        plan = load_plan(plan_path)
    except PlanError as e:
        if json_output:
            typer.echo(json.dumps({"error": str(e)}))
        else:
            typer.secho(f"Error: {e}", fg="red", err=True)
        raise typer.Exit(1)

    # Get plan info
    branch = get_plan_branch(plan)
    plan_repos = get_plan_repos(plan)

    # Load state and PR info
    state = load_state(task_dir)
    prs_data = load_prs(task_dir)

    # Get issue info
    issue_config = get_issue_config(plan)
    issue_state = get_issue_state(state)

    issue_info = None
    if issue_config.get("number") or (issue_state and issue_state.get("number")):
        issue_info = {
            "repo": issue_config.get("repo") or (issue_state.get("repo") if issue_state else None),
            "number": issue_config.get("number") or (issue_state.get("number") if issue_state else None),
            "sync_mode": issue_config.get("sync_mode", "comment"),
        }
        # Build URL if we have repo and number
        if issue_info["repo"] and issue_info["number"]:
            issue_info["url"] = f"https://github.com/{issue_info['repo']}/issues/{issue_info['number']}"

    # Build status report
    report: dict[str, Any] = {
        "task_id": task_id,
        "branch": branch,
        "repos": {},
    }

    if issue_info:
        report["issue"] = issue_info

    for repo_key in plan_repos:
        repo_status = _get_repo_status(
            context=context,
            workspace=workspace,
            state=state,
            prs_data=prs_data,
            task_id=task_id,
            repo_key=repo_key,
            branch=branch,
        )
        report["repos"][repo_key] = repo_status

    # Output
    if json_output:
        typer.echo(json.dumps(report, indent=2))
    else:
        _print_status(report, no_color)


def _get_repo_status(
    context,
    workspace,
    state: dict[str, Any] | None,
    prs_data: dict[str, Any] | None,
    task_id: str,
    repo_key: str,
    branch: str,
) -> dict[str, Any]:
    """Get status for a single repo."""
    result: dict[str, Any] = {
        "worktree_exists": False,
        "repo_path": None,
        "worktree_path": None,
        "git": None,
        "pr": None,
    }

    # Get paths from state or workspace
    worktree_path = context.worktree_dir(task_id, repo_key)
    result["worktree_path"] = str(worktree_path)

    # Get repo path from state or workspace
    if state and "repos" in state and repo_key in state["repos"]:
        repo_info = state["repos"][repo_key]
        result["repo_path"] = repo_info.get("repo_path")
    elif workspace:
        repo_config = workspace.get_repo(repo_key)
        if repo_config:
            result["repo_path"] = str(repo_config.path)

    # Check if worktree exists
    if worktree_path.exists():
        result["worktree_exists"] = True

        # Get git status
        try:
            git_status = get_git_status(worktree_path)
            result["git"] = git_status
        except Exception:
            result["git"] = {"error": "Failed to get git status"}

    # Get PR info from cached data or live query
    if prs_data and "repos" in prs_data and repo_key in prs_data["repos"]:
        pr_info = prs_data["repos"][repo_key]
        result["pr"] = {
            "url": pr_info.get("pr_url"),
            "number": pr_info.get("pr_number"),
            "state": pr_info.get("pr_state"),
        }
    elif result["worktree_exists"] and result["repo_path"]:
        # Try to get live PR info
        try:
            pr_info = get_pr_info(Path(result["repo_path"]), branch)
            if pr_info:
                result["pr"] = pr_info
        except Exception:
            pass

    return result


def _print_status(report: dict[str, Any], no_color: bool) -> None:
    """Print status in human-readable format."""
    typer.echo(f"Task: {report['task_id']}")
    typer.echo(f"Branch: {report['branch']}")

    # Issue info
    issue_info = report.get("issue")
    if issue_info:
        issue_url = issue_info.get("url", "")
        issue_number = issue_info.get("number", "?")
        typer.echo(f"Issue: #{issue_number} {issue_url}")

    typer.echo()

    for repo_key, repo_status in report["repos"].items():
        _print_repo_status(repo_key, repo_status, no_color)
        typer.echo()


def _print_repo_status(repo_key: str, status: dict[str, Any], no_color: bool) -> None:
    """Print status for a single repo."""
    # Repo header
    if status["worktree_exists"]:
        typer.secho(f"  {repo_key}:", fg="green" if not no_color else None, bold=True)
    else:
        typer.secho(f"  {repo_key}: (no worktree)", fg="yellow" if not no_color else None)
        return

    # Git info
    git_info = status.get("git")
    if git_info and "error" not in git_info:
        branch = git_info.get("branch", "unknown")
        dirty = git_info.get("dirty", False)
        ahead = git_info.get("ahead", 0)
        behind = git_info.get("behind", 0)

        # Branch and dirty status
        dirty_marker = " *" if dirty else ""
        if dirty:
            typer.secho(f"    Branch: {branch}{dirty_marker}", fg="yellow" if not no_color else None)
        else:
            typer.echo(f"    Branch: {branch}")

        # Ahead/behind
        if ahead or behind:
            sync_status = []
            if ahead:
                sync_status.append(f"↑{ahead}")
            if behind:
                sync_status.append(f"↓{behind}")
            typer.echo(f"    Sync: {' '.join(sync_status)}")

        # Last commit
        last_commit = git_info.get("last_commit")
        if last_commit:
            sha = last_commit.get("sha", "")[:7]
            subject = last_commit.get("subject", "")
            # Truncate long subjects
            if len(subject) > 50:
                subject = subject[:47] + "..."
            typer.echo(f"    Commit: {sha} {subject}")

    elif git_info and "error" in git_info:
        typer.secho(f"    Git: {git_info['error']}", fg="red" if not no_color else None)

    # PR info
    pr_info = status.get("pr")
    if pr_info:
        pr_state = pr_info.get("state", "UNKNOWN")
        pr_number = pr_info.get("number", "?")
        pr_url = pr_info.get("url", "")

        # Color based on state
        state_colors = {
            "OPEN": "green",
            "CLOSED": "red",
            "MERGED": "magenta",
        }
        color = state_colors.get(pr_state) if not no_color else None

        typer.secho(f"    PR: #{pr_number} ({pr_state})", fg=color)
        if pr_url:
            typer.echo(f"        {pr_url}")
    else:
        typer.echo("    PR: none")
