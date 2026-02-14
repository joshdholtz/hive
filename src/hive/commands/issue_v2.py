"""hive issue - GitHub issue management (v2).

In v2, GitHub Issues are the canonical source of truth.
"""

import json
from pathlib import Path
from typing import Annotated, Any

import typer

from hive.core.context import get_context
from hive.core.exceptions import GhError, WorkspaceError
from hive.core.gh import (
    add_issue_comment,
    check_gh_auth,
    create_issue,
    get_issue_info,
    list_issue_comments,
    update_issue_body,
    update_issue_comment,
)

# Marker for hive-managed status comments
HIVE_STATUS_MARKER = "<!-- hive-status -->"

issue_app = typer.Typer(
    name="issue",
    help="GitHub issue management. Issues are the source of truth.",
)


@issue_app.command("new")
def issue_new(
    ctx: typer.Context,
    title: Annotated[
        str,
        typer.Argument(help="Issue title."),
    ],
    repos: Annotated[
        str,
        typer.Option("--repos", "-r", help="Comma-separated list of repo keys."),
    ],
    umbrella_repo: Annotated[
        str | None,
        typer.Option("--umbrella-repo", "-u", help="Repo for umbrella issue (default: hive repo)."),
    ] = None,
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """Create umbrella issue + per-repo issues.

    Creates:
    - 1 umbrella issue in the hive repo (or --umbrella-repo)
    - 1 issue per repo listed in --repos
    - Links repo issues in umbrella issue body

    Stores mapping in .hive/tasks/<umbrella_id>/issues.json
    """
    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    # Check gh auth
    if not check_gh_auth():
        _error("gh CLI not authenticated. Run: gh auth login", json_output, exit_code=2)

    # Load workspace
    if not context.has_workspace():
        _error("workspace.yaml not found", json_output, exit_code=3)

    try:
        workspace = context.load_workspace()
    except WorkspaceError as e:
        _error(str(e), json_output, exit_code=3)

    # Parse repo list
    repo_keys = [r.strip() for r in repos.split(",") if r.strip()]
    if not repo_keys:
        _error("No repos specified", json_output, exit_code=1)

    # Validate repos exist in workspace
    for repo_key in repo_keys:
        if workspace.get_repo(repo_key) is None:
            _error(f"Repo '{repo_key}' not found in workspace.yaml", json_output, exit_code=1)

    # Determine umbrella repo
    if umbrella_repo is None:
        # Default to 'hive' repo if it exists, otherwise error
        if workspace.get_repo("hive"):
            umbrella_repo = _get_repo_github_path(workspace.get_repo("hive").path)
        else:
            _error("No --umbrella-repo specified and 'hive' repo not found in workspace", json_output, exit_code=1)

    if not json_output:
        typer.echo(f"Creating issues for: {title}")
        typer.echo(f"  Umbrella repo: {umbrella_repo}")
        typer.echo(f"  Repos: {', '.join(repo_keys)}")
        typer.echo()

    # Create umbrella issue first
    if not json_output:
        typer.echo("Creating umbrella issue...")

    try:
        umbrella_body = _generate_umbrella_body(title, repo_keys)
        umbrella_result = create_issue(umbrella_repo, title, umbrella_body)
        umbrella_number = umbrella_result["number"]
        umbrella_url = umbrella_result["url"]
    except GhError as e:
        _error(f"Failed to create umbrella issue: {e}", json_output, exit_code=1)

    if not json_output:
        typer.secho(f"  Created umbrella: #{umbrella_number}", fg="green")

    # Create repo issues
    repo_issues: dict[str, dict[str, Any]] = {}

    for repo_key in repo_keys:
        repo_config = workspace.get_repo(repo_key)
        repo_github_path = _get_repo_github_path(repo_config.path)

        if not json_output:
            typer.echo(f"Creating issue for {repo_key}...")

        try:
            repo_title = f"#{umbrella_number}: {title} ({repo_key})"
            repo_body = _generate_repo_issue_body(title, umbrella_url, repo_key)
            repo_result = create_issue(repo_github_path, repo_title, repo_body)

            repo_issues[repo_key] = {
                "repo": repo_github_path,
                "number": repo_result["number"],
                "url": repo_result["url"],
            }

            if not json_output:
                typer.secho(f"  Created: #{repo_result['number']}", fg="green")

        except GhError as e:
            if not json_output:
                typer.secho(f"  Failed: {e}", fg="red")
            repo_issues[repo_key] = {"error": str(e)}

    # Update umbrella issue with repo issue links
    try:
        updated_body = _generate_umbrella_body(title, repo_keys, repo_issues, umbrella_url)
        update_issue_body(umbrella_repo, umbrella_number, updated_body)
    except GhError:
        pass  # Non-fatal

    # Save issues.json
    task_id = str(umbrella_number)
    task_dir = context.task_dir(task_id)
    task_dir.mkdir(parents=True, exist_ok=True)

    issues_data = {
        "umbrella": {
            "repo": umbrella_repo,
            "number": umbrella_number,
            "url": umbrella_url,
        },
        "repos": repo_issues,
    }

    issues_path = task_dir / "issues.json"
    with open(issues_path, "w") as f:
        json.dump(issues_data, f, indent=2)

    # Output
    if json_output:
        typer.echo(json.dumps(issues_data, indent=2))
    else:
        typer.echo()
        typer.secho(f"Created task #{umbrella_number}", fg="green")
        typer.echo(f"  Umbrella: {umbrella_url}")
        typer.echo(f"  Local: {issues_path}")
        typer.echo()
        typer.echo(f"Next: hive start {umbrella_number}")


@issue_app.command("list")
def issue_list(
    ctx: typer.Context,
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """List umbrella issues from local .hive/tasks/."""
    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    tasks_dir = context.tasks_dir
    if not tasks_dir.exists():
        if json_output:
            typer.echo(json.dumps({"tasks": []}))
        else:
            typer.echo("No tasks found.")
        return

    tasks = []
    for task_dir in sorted(tasks_dir.iterdir(), reverse=True):
        if not task_dir.is_dir():
            continue

        issues_path = task_dir / "issues.json"
        if not issues_path.exists():
            continue

        try:
            with open(issues_path) as f:
                issues_data = json.load(f)

            umbrella = issues_data.get("umbrella", {})
            tasks.append({
                "id": task_dir.name,
                "number": umbrella.get("number"),
                "url": umbrella.get("url"),
                "repos": list(issues_data.get("repos", {}).keys()),
            })
        except (json.JSONDecodeError, OSError):
            continue

    if json_output:
        typer.echo(json.dumps({"tasks": tasks}, indent=2))
    else:
        if not tasks:
            typer.echo("No tasks found.")
            return

        typer.echo("Tasks:")
        for task in tasks:
            repos_str = ", ".join(task["repos"][:3])
            if len(task["repos"]) > 3:
                repos_str += f", +{len(task['repos']) - 3} more"
            typer.echo(f"  #{task['number']}: {repos_str}")
            typer.echo(f"    {task['url']}")


@issue_app.command("sync")
def issue_sync(
    ctx: typer.Context,
    umbrella_id: Annotated[
        str,
        typer.Argument(help="Umbrella issue number."),
    ],
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """Sync umbrella issue with current state.

    Updates umbrella issue body/comment with:
    - Repo issue links and states
    - PR links if detected
    """
    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    # Check gh auth
    if not check_gh_auth():
        _error("gh CLI not authenticated", json_output, exit_code=2)

    # Load issues.json
    task_dir = context.task_dir(umbrella_id)
    issues_path = task_dir / "issues.json"

    if not issues_path.exists():
        _error(f"Task not found: {umbrella_id}", json_output, exit_code=1)

    try:
        with open(issues_path) as f:
            issues_data = json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        _error(f"Failed to load issues.json: {e}", json_output, exit_code=1)

    umbrella = issues_data.get("umbrella", {})
    umbrella_repo = umbrella.get("repo")
    umbrella_number = umbrella.get("number")
    umbrella_url = umbrella.get("url")

    if not umbrella_repo or not umbrella_number:
        _error("Invalid issues.json: missing umbrella info", json_output, exit_code=1)

    if not json_output:
        typer.echo(f"Syncing #{umbrella_number}...")

    # Get current state of repo issues
    repo_issues = issues_data.get("repos", {})
    for repo_key, repo_info in repo_issues.items():
        if "error" in repo_info:
            continue

        repo = repo_info.get("repo")
        number = repo_info.get("number")

        if repo and number:
            try:
                info = get_issue_info(repo, number)
                if info:
                    repo_info["state"] = info.get("state", "OPEN")
            except GhError:
                pass

    # Generate updated body
    updated_body = _generate_umbrella_body(
        title="",  # Will be ignored since we're updating
        repo_keys=list(repo_issues.keys()),
        repo_issues=repo_issues,
        umbrella_url=umbrella_url,
    )

    # Update umbrella issue body
    try:
        update_issue_body(umbrella_repo, umbrella_number, updated_body)
        if not json_output:
            typer.secho("Synced successfully", fg="green")
    except GhError as e:
        _error(f"Failed to update umbrella issue: {e}", json_output, exit_code=1)

    if json_output:
        typer.echo(json.dumps({"synced": True, "umbrella": umbrella}, indent=2))


def _generate_umbrella_body(
    title: str,
    repo_keys: list[str],
    repo_issues: dict[str, dict[str, Any]] | None = None,
    umbrella_url: str | None = None,
) -> str:
    """Generate umbrella issue body."""
    lines = [
        "## Goal",
        "",
        title if title else "(See issue title)",
        "",
        "## Repo Issues",
        "",
    ]

    for repo_key in repo_keys:
        if repo_issues and repo_key in repo_issues:
            repo_info = repo_issues[repo_key]
            if "error" in repo_info:
                lines.append(f"- [ ] **{repo_key}**: (failed to create)")
            else:
                url = repo_info.get("url", "")
                state = repo_info.get("state", "OPEN")
                checkbox = "[x]" if state == "CLOSED" else "[ ]"
                lines.append(f"- {checkbox} **{repo_key}**: {url}")
        else:
            lines.append(f"- [ ] **{repo_key}**: (pending)")

    lines.extend([
        "",
        "---",
        "*Managed by hive*",
    ])

    return "\n".join(lines)


def _generate_repo_issue_body(title: str, umbrella_url: str, repo_key: str) -> str:
    """Generate repo issue body."""
    return f"""## Goal

{title}

## Context

This is the **{repo_key}** portion of umbrella issue:
{umbrella_url}

## Instructions

Implement the required changes for this repo.

---
*Managed by hive*
"""


def _get_repo_github_path(repo_path: Path) -> str:
    """Get GitHub owner/repo path from local repo."""
    # Try to get from git remote
    import subprocess

    try:
        result = subprocess.run(
            ["git", "remote", "get-url", "origin"],
            cwd=repo_path,
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode == 0:
            url = result.stdout.strip()
            # Parse SSH: git@github.com:owner/repo.git
            # Parse HTTPS: https://github.com/owner/repo.git
            if "github.com" in url:
                if url.startswith("git@"):
                    # git@github.com:owner/repo.git
                    path = url.split(":")[-1]
                else:
                    # https://github.com/owner/repo.git
                    path = url.split("github.com/")[-1]
                return path.removesuffix(".git")
    except (subprocess.TimeoutExpired, OSError):
        pass

    # Fallback: use directory name
    return f"UNKNOWN/{repo_path.name}"


def _error(message: str, json_output: bool, exit_code: int = 1) -> None:
    """Output error and exit."""
    if json_output:
        typer.echo(json.dumps({"error": message}))
    else:
        typer.secho(f"Error: {message}", fg="red", err=True)
    raise typer.Exit(exit_code)
