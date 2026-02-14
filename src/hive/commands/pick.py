"""hive pick - Interactive issue picker."""

import json
import shutil
import subprocess
import sys
from typing import Annotated, Any

import typer

from hive.core.context import get_context
from hive.core.exceptions import GhError, WorkspaceError
from hive.core.gh import check_gh_auth
from hive.providers import IssueProvider, get_provider
from hive.providers.base import ProviderConfig

pick_app = typer.Typer(
    name="pick",
    help="Interactive issue picker.",
)


def _get_provider(context) -> IssueProvider | None:
    """Get the configured issue provider from workspace."""
    if not context.has_workspace():
        return None
    try:
        workspace = context.load_workspace()

        # Build provider config
        provider_cfg = ProviderConfig(
            type=workspace.provider.type,
            repo=workspace.provider.repo,
            project=workspace.provider.project,
            api_key_env=workspace.provider.api_key_env,
        )

        # For GitHub, if no repo specified, use issues_repo from workspace
        if provider_cfg.type == "github" and not provider_cfg.repo:
            from hive.commands.issue_v2 import _get_repo_github_path
            issues_repo_key = workspace.defaults.issues_repo
            issues_repo = workspace.get_repo(issues_repo_key)
            if issues_repo:
                provider_cfg.repo = _get_repo_github_path(issues_repo.path)

        return get_provider(provider_cfg)
    except (WorkspaceError, ValueError):
        return None


def _fetch_umbrella_issues(provider: IssueProvider, limit: int = 50) -> list[dict[str, Any]]:
    """Fetch open issues from provider."""
    try:
        issues = provider.list_issues(state="open", limit=limit)
        return [
            {
                "number": issue.number,
                "title": issue.title,
                "state": issue.state,
                "url": issue.url,
            }
            for issue in issues
        ]
    except Exception:
        return []


def _has_fzf() -> bool:
    """Check if fzf is available."""
    return shutil.which("fzf") is not None


def _pick_with_fzf(issues: list[dict[str, Any]]) -> int | None:
    """Use fzf for interactive selection."""
    # Format issues for fzf
    lines = []
    for issue in issues:
        lines.append(f"#{issue['number']}: {issue['title']}")

    input_text = "\n".join(lines)

    try:
        result = subprocess.run(
            ["fzf", "--height=50%", "--reverse", "--prompt=Select issue> "],
            input=input_text,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            return None

        # Parse selected line: "#123: Title"
        selected = result.stdout.strip()
        if selected.startswith("#"):
            num_str = selected.split(":")[0][1:]
            return int(num_str)
    except (subprocess.TimeoutExpired, FileNotFoundError, ValueError):
        pass
    return None


def _pick_with_prompt(issues: list[dict[str, Any]]) -> int | None:
    """Fallback to simple numbered prompt."""
    if not issues:
        return None

    typer.echo("Select an issue:\n")
    for i, issue in enumerate(issues[:20], 1):  # Show max 20
        typer.echo(f"  {i}) #{issue['number']}: {issue['title'][:60]}")

    typer.echo()
    try:
        choice = typer.prompt("Enter number (or 'q' to quit)", default="q")
        if choice.lower() == 'q':
            return None
        idx = int(choice) - 1
        if 0 <= idx < len(issues):
            return issues[idx]["number"]
    except (ValueError, KeyboardInterrupt):
        pass
    return None


def _pick_repos_with_fzf(repo_keys: list[str]) -> list[str]:
    """Use fzf for multi-select repos."""
    input_text = "\n".join(repo_keys)

    try:
        result = subprocess.run(
            ["fzf", "--multi", "--height=50%", "--reverse", "--prompt=Select repos (TAB to multi-select)> "],
            input=input_text,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            return []

        return [r.strip() for r in result.stdout.strip().split("\n") if r.strip()]
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return []


def _pick_repos_with_prompt(repo_keys: list[str]) -> list[str]:
    """Fallback to simple prompt for repo selection."""
    if not repo_keys:
        return []

    typer.echo("Select repos (comma-separated numbers, or 'a' for all):\n")
    for i, key in enumerate(repo_keys, 1):
        typer.echo(f"  {i}) {key}")

    typer.echo()
    try:
        choice = typer.prompt("Enter numbers (e.g., 1,2,3) or 'a' for all", default="a")
        if choice.lower() == 'a':
            return repo_keys

        selected = []
        for num_str in choice.split(","):
            idx = int(num_str.strip()) - 1
            if 0 <= idx < len(repo_keys):
                selected.append(repo_keys[idx])
        return selected
    except (ValueError, KeyboardInterrupt):
        return []


@pick_app.callback(invoke_without_command=True)
def pick_umbrella(
    ctx: typer.Context,
    repo: Annotated[
        str | None,
        typer.Option("--repo", "-r", help="Repo to fetch issues from (GitHub: owner/name, Linear: team key)."),
    ] = None,
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """Interactively select an umbrella issue.

    Outputs the selected issue number to stdout.
    Use with: hive start $(hive pick)
    """
    # If subcommand invoked, skip
    if ctx.invoked_subcommand is not None:
        return

    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    # Get provider
    provider = _get_provider(context)
    if not provider:
        typer.secho("Error: No provider configured. Check workspace.yaml.", fg="red", err=True)
        raise typer.Exit(2)

    # Check auth
    if not provider.is_authenticated():
        typer.secho(f"Error: {provider.name} not authenticated", fg="red", err=True)
        raise typer.Exit(2)

    # Fetch issues
    issues = _fetch_umbrella_issues(provider)
    if not issues:
        typer.secho("No open issues found", fg="yellow", err=True)
        raise typer.Exit(1)

    # Interactive selection
    if _has_fzf():
        selected = _pick_with_fzf(issues)
    else:
        selected = _pick_with_prompt(issues)

    if selected is None:
        raise typer.Exit(1)

    # Output just the number
    if json_output:
        typer.echo(json.dumps({"number": selected}))
    else:
        typer.echo(selected)


@pick_app.command("start")
def pick_start(
    ctx: typer.Context,
    repo: Annotated[
        str | None,
        typer.Option("--repo", "-r", help="Repo to fetch issues from."),
    ] = None,
    repos: Annotated[
        str | None,
        typer.Option("--repos", help="Comma-separated repos to start (skips repo picker)."),
    ] = None,
    pick_repos: Annotated[
        bool,
        typer.Option("--pick-repos", help="Interactively select repos after picking issue."),
    ] = False,
    with_claude: Annotated[
        bool,
        typer.Option("--with-claude", help="Start Claude in each repo window."),
    ] = False,
) -> None:
    """Pick an issue and start working on it.

    One-step interactive start - no copy/paste needed.
    """
    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    # Get provider
    provider = _get_provider(context)
    if not provider:
        typer.secho("Error: No provider configured. Check workspace.yaml.", fg="red", err=True)
        raise typer.Exit(2)

    # Check auth
    if not provider.is_authenticated():
        typer.secho(f"Error: {provider.name} not authenticated", fg="red", err=True)
        raise typer.Exit(2)

    # Fetch and pick issue
    issues = _fetch_umbrella_issues(provider)
    if not issues:
        typer.secho("No open issues found", fg="yellow", err=True)
        raise typer.Exit(1)

    if _has_fzf():
        selected = _pick_with_fzf(issues)
    else:
        selected = _pick_with_prompt(issues)

    if selected is None:
        typer.secho("Cancelled", fg="yellow", err=True)
        raise typer.Exit(1)

    umbrella_id = str(selected)
    typer.echo(f"\nSelected issue #{umbrella_id}")

    # Optionally pick repos
    final_repos = repos
    if pick_repos and not repos:
        # Load issues.json to get repo keys
        task_dir = context.task_dir(umbrella_id)
        issues_path = task_dir / "issues.json"

        if issues_path.exists():
            try:
                with open(issues_path) as f:
                    issues_data = json.load(f)
                repo_keys = list(issues_data.get("repos", {}).keys())

                if repo_keys:
                    typer.echo()
                    if _has_fzf():
                        selected_repos = _pick_repos_with_fzf(repo_keys)
                    else:
                        selected_repos = _pick_repos_with_prompt(repo_keys)

                    if selected_repos:
                        final_repos = ",".join(selected_repos)
            except (json.JSONDecodeError, OSError):
                pass

    # Run hive start via subprocess (cleaner than trying to invoke typer context)
    import subprocess

    typer.echo()
    cmd = ["hive", "start", umbrella_id]
    if final_repos:
        cmd.extend(["--repos", final_repos])
    if with_claude:
        cmd.append("--with-claude")

    subprocess.run(cmd)


@pick_app.command("repos")
def pick_repos_cmd(
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
    """Interactively select repos for an umbrella issue.

    Outputs comma-separated repo keys.
    Use with: hive start <id> --repos $(hive pick repos <id>)
    """
    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    # Load issues.json
    task_dir = context.task_dir(umbrella_id)
    issues_path = task_dir / "issues.json"

    if not issues_path.exists():
        typer.secho(f"Error: Task not found: {umbrella_id}", fg="red", err=True)
        raise typer.Exit(1)

    try:
        with open(issues_path) as f:
            issues_data = json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        typer.secho(f"Error: Failed to load issues.json: {e}", fg="red", err=True)
        raise typer.Exit(1)

    repo_keys = list(issues_data.get("repos", {}).keys())
    if not repo_keys:
        typer.secho("No repos found in issues.json", fg="yellow", err=True)
        raise typer.Exit(1)

    # Interactive selection
    if _has_fzf():
        selected = _pick_repos_with_fzf(repo_keys)
    else:
        selected = _pick_repos_with_prompt(repo_keys)

    if not selected:
        raise typer.Exit(1)

    # Output
    if json_output:
        typer.echo(json.dumps({"repos": selected}))
    else:
        typer.echo(",".join(selected))
