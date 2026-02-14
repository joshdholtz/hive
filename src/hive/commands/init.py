"""hive init - Initialize workspace from GitHub org."""

import json
import re
from pathlib import Path
from typing import Annotated, Any, Optional

import typer

from hive.core.context import get_context
from hive.core.exceptions import GhError, GitError
from hive.core.gh import check_gh_auth, list_org_repos
from hive.core.git import clone_repo, fetch_origin, is_git_repo
from hive.core.workspace import create_workspace_yaml, update_workspace_repos


def init(
    ctx: typer.Context,
    org: Annotated[
        str,
        typer.Option("--org", help="GitHub organization to clone repos from."),
    ],
    name: Annotated[
        Optional[str],
        typer.Option("--name", help="Project name for CLAUDE.md."),
    ] = None,
    description: Annotated[
        Optional[str],
        typer.Option("--description", help="Project description for CLAUDE.md."),
    ] = None,
    dest: Annotated[
        Optional[str],
        typer.Option("--dest", help="Destination directory for cloned repos."),
    ] = None,
    match: Annotated[
        Optional[str],
        typer.Option("--match", help="Regex pattern to match repo names."),
    ] = None,
    exclude: Annotated[
        Optional[str],
        typer.Option("--exclude", help="Regex pattern to exclude repo names."),
    ] = None,
    visibility: Annotated[
        str,
        typer.Option(
            "--visibility",
            help="Filter by visibility: all, public, private, internal.",
        ),
    ] = "all",
    fetch: Annotated[
        bool,
        typer.Option("--fetch", help="Fetch existing repos instead of skipping."),
    ] = False,
    ssh: Annotated[
        bool,
        typer.Option("--ssh", help="Clone using SSH URLs."),
    ] = True,
    https: Annotated[
        bool,
        typer.Option("--https", help="Clone using HTTPS URLs."),
    ] = False,
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """Clone repositories from a GitHub organization and generate workspace.yaml.

    If run from inside a git repo, clones sibling repos to the parent directory
    and includes the current repo in workspace.yaml.

    Creates .tasks/ and .wt/ directories if missing.
    Filters out archived repos and forks by default.
    Updates workspace.yaml with cloned repo mappings.

    Exit codes:
    - 0: Success
    - 2: gh CLI missing or not authenticated
    - 3: Invalid workspace
    """
    # Get context from global options
    root = ctx.obj.get("root") if ctx.obj else None
    no_color = ctx.obj.get("no_color", False) if ctx.obj else False
    verbose = ctx.obj.get("verbose", False) if ctx.obj else False

    # Check if we're inside a git repo - if so, clone siblings to parent
    cwd = Path.cwd()
    current_repo_name: str | None = None
    clone_to_parent = False

    if is_git_repo(cwd) and root is None and dest is None:
        # We're in a git repo - this repo is the "command center"
        # Clone siblings to parent, but workspace.yaml stays here
        current_repo_name = cwd.name
        clone_to_parent = True
        if not json_output:
            typer.echo(f"Detected git repo: {current_repo_name}")
            typer.echo(f"Cloning sibling repos to: {cwd.parent}")
            typer.echo(f"Workspace config will be in: {cwd}")

    context = get_context(root=root, no_color=no_color, verbose=verbose)

    # Determine clone URL type (--https overrides --ssh)
    use_ssh = ssh and not https

    # Determine destination directory for cloning
    if clone_to_parent:
        dest_path = cwd.parent
    elif dest:
        dest_path = Path(dest)
        if not dest_path.is_absolute():
            dest_path = (context.root / dest_path).resolve()
    else:
        dest_path = context.root

    # Check gh auth
    if not check_gh_auth():
        _error("gh CLI not authenticated. Run: gh auth login", json_output, exit_code=2)
        return

    # Create required directories
    context.ensure_dirs()
    dest_path.mkdir(parents=True, exist_ok=True)

    # Fetch repo list from GitHub
    if not json_output:
        typer.echo(f"Fetching repos from {org}...")

    try:
        repos = list_org_repos(org, visibility=visibility)
    except GhError as e:
        _error(str(e), json_output, exit_code=2)
        return

    # Filter repos
    filtered_repos = _filter_repos(repos, match=match, exclude=exclude)

    if not filtered_repos:
        _error(f"No repos found matching filters in {org}", json_output, exit_code=3)
        return

    if not json_output:
        typer.echo(f"Found {len(filtered_repos)} repos to process")

    # Process each repo
    results: list[dict[str, Any]] = []
    workspace_repos: dict[str, str] = {}

    # Include current repo if we're inside one (as "." since workspace.yaml is here)
    if current_repo_name:
        repo_key = _repo_name_to_key(current_repo_name)
        workspace_repos[repo_key] = "."
        if not json_output:
            typer.echo(f"  Including current repo: {current_repo_name} (path: .)")

    for repo in filtered_repos:
        result = _process_repo(
            repo=repo,
            dest_path=dest_path,
            use_ssh=use_ssh,
            do_fetch=fetch,
            verbose=verbose and not json_output,
        )
        results.append(result)

        if result["status"] in ("cloned", "exists", "fetched"):
            # Add to workspace mapping
            repo_key = _repo_name_to_key(repo["name"])
            # Use ../ paths when cloning to parent (sibling repos)
            if clone_to_parent:
                rel_path = f"../{repo['name']}"
            else:
                rel_path = f"./{repo['name']}"
            workspace_repos[repo_key] = rel_path

    # Update workspace.yaml
    if workspace_repos:
        workspace_path = context.workspace_yaml
        if workspace_path.exists():
            update_workspace_repos(workspace_path, workspace_repos)
        else:
            create_workspace_yaml(workspace_path, workspace_repos)

    # Generate CLAUDE.md
    claude_md_path = context.root / "CLAUDE.md"
    if not claude_md_path.exists():
        project_name = name or org
        project_desc = description or f"A multi-repo project managed with hive."
        _generate_claude_md(claude_md_path, project_name, project_desc, workspace_repos, org)
        if not json_output:
            typer.echo(f"Generated: {claude_md_path}")
            typer.echo("  Edit CLAUDE.md to add repo descriptions and project details.")

    # Output results
    if json_output:
        output = {
            "org": org,
            "destination": str(dest_path),
            "repos_processed": len(results),
            "repos_cloned": sum(1 for r in results if r["status"] == "cloned"),
            "repos_skipped": sum(1 for r in results if r["status"] == "exists"),
            "repos_fetched": sum(1 for r in results if r["status"] == "fetched"),
            "repos_failed": sum(1 for r in results if r["status"] == "error"),
            "workspace_yaml": str(context.workspace_yaml),
            "results": results,
        }
        typer.echo(json.dumps(output, indent=2))
    else:
        _print_summary(results, context.workspace_yaml)


def _filter_repos(
    repos: list[dict[str, Any]],
    match: str | None = None,
    exclude: str | None = None,
) -> list[dict[str, Any]]:
    """Filter repos by name patterns, excluding archived and forks."""
    filtered = []

    # Compile regex patterns
    match_re = re.compile(match) if match else None
    exclude_re = re.compile(exclude) if exclude else None

    for repo in repos:
        name = repo["name"]

        # Skip archived repos
        if repo.get("isArchived", False):
            continue

        # Skip forks
        if repo.get("isFork", False):
            continue

        # Apply match filter
        if match_re and not match_re.search(name):
            continue

        # Apply exclude filter
        if exclude_re and exclude_re.search(name):
            continue

        filtered.append(repo)

    return filtered


def _process_repo(
    repo: dict[str, Any],
    dest_path: Path,
    use_ssh: bool,
    do_fetch: bool,
    verbose: bool,
) -> dict[str, Any]:
    """Process a single repo (clone or fetch)."""
    name = repo["name"]
    repo_path = dest_path / name

    result: dict[str, Any] = {
        "name": name,
        "path": str(repo_path),
    }

    # Get clone URL
    clone_url = repo.get("sshUrl") if use_ssh else repo.get("url")
    if not clone_url:
        result["status"] = "error"
        result["error"] = "No clone URL available"
        return result

    if repo_path.exists():
        if is_git_repo(repo_path):
            if do_fetch:
                # Fetch existing repo
                if verbose:
                    typer.echo(f"  Fetching {name}...")
                try:
                    fetch_origin(repo_path, prune=True)
                    result["status"] = "fetched"
                except GitError as e:
                    result["status"] = "error"
                    result["error"] = str(e)
            else:
                if verbose:
                    typer.echo(f"  Skipping {name} (exists)")
                result["status"] = "exists"
        else:
            result["status"] = "error"
            result["error"] = "Path exists but is not a git repo"
    else:
        # Clone repo
        if verbose:
            typer.echo(f"  Cloning {name}...")
        try:
            clone_repo(clone_url, repo_path)
            result["status"] = "cloned"
        except GitError as e:
            result["status"] = "error"
            result["error"] = str(e)

    return result


def _repo_name_to_key(name: str) -> str:
    """Convert repo name to a valid workspace key.

    Converts hyphens to underscores for Python-friendly keys.
    """
    return name.replace("-", "_")


def _print_summary(results: list[dict[str, Any]], workspace_path: Path) -> None:
    """Print human-readable summary."""
    cloned = sum(1 for r in results if r["status"] == "cloned")
    skipped = sum(1 for r in results if r["status"] == "exists")
    fetched = sum(1 for r in results if r["status"] == "fetched")
    failed = sum(1 for r in results if r["status"] == "error")

    typer.echo()
    typer.echo("Summary:")
    if cloned:
        typer.secho(f"  Cloned: {cloned}", fg="green")
    if fetched:
        typer.secho(f"  Fetched: {fetched}", fg="cyan")
    if skipped:
        typer.echo(f"  Skipped (existing): {skipped}")
    if failed:
        typer.secho(f"  Failed: {failed}", fg="red")
        for r in results:
            if r["status"] == "error":
                typer.echo(f"    {r['name']}: {r.get('error', 'unknown error')}")

    typer.echo()
    typer.echo(f"Workspace updated: {workspace_path}")


def _error(message: str, json_output: bool, exit_code: int) -> None:
    """Output error and exit."""
    if json_output:
        typer.echo(json.dumps({"error": message}))
    else:
        typer.secho(f"Error: {message}", fg="red", err=True)
    raise typer.Exit(exit_code)


def _generate_claude_md(
    path: Path,
    project_name: str,
    project_description: str,
    repos: dict[str, str],
    org: str,
) -> None:
    """Generate CLAUDE.md with project context for the planner agent."""
    repo_list = "\n".join(f"- **{key}**: `{path}` - [describe what this repo does]" for key, path in repos.items())

    content = f'''# {project_name}

{project_description}

## Repositories

This workspace contains the following repositories:

{repo_list}

## Hive Workflow

This project uses **hive** for multi-repo coordination. GitHub Issues are the source of truth.

### Key Commands

```bash
# Planning & Coordination
hive planner              # Open planner session (Claude + interactive menu)
hive pick start           # Interactively select an issue and start working

# Issue Management
hive issue new "title" --repos repo1,repo2   # Create umbrella + repo issues
hive issue list           # List local tasks
hive issue sync <id>      # Sync umbrella issue status

# Working on Tasks
hive start <id>           # Create worktrees and tmux windows for a task
hive start <id> --repos x # Start only specific repos
hive status <id>          # Show task status

# Cleanup
hive clean <id> --yes     # Remove worktrees for a task
```

### Workflow

1. **Plan**: Use `hive planner` to discuss and plan work
2. **Create Issues**: `hive issue new "Feature X" --repos api,web` creates GitHub issues
3. **Start Work**: `hive start <issue_number>` creates worktrees and branches
4. **Implement**: Work in each repo's tmux window, commit and push
5. **Sync**: `hive issue sync <id>` updates the umbrella issue with progress

### Architecture Notes

- **Umbrella Issue**: Created in the hive repo, links to all repo-specific issues
- **Repo Issues**: One per repo, titled "#<umbrella_number>: <title> (<repo>)"
- **Worktrees**: Created in `.hive/wt/<task_id>/<repo>/`
- **Branches**: Named `feat/<task_id>-<repo>`

## Project-Specific Notes

[Add any project-specific context, conventions, or guidelines here]

---
*Generated by hive init. Edit this file to add project details.*
'''

    with open(path, "w") as f:
        f.write(content)
