"""hive start - Start work on an umbrella issue."""

import fnmatch
import json
import os
import subprocess
from pathlib import Path
from typing import Annotated, Any

import typer

from hive.core.context import get_context
from hive.core.exceptions import GitError, TmuxError, WorkspaceError
from hive.core.git import create_worktree, fetch_origin, worktree_exists
from hive.core.tmux import (
    create_session,
    create_window,
    is_inside_tmux,
    run_tmux,
    send_keys,
    session_exists,
    switch_client,
    window_exists,
)

SESSION_NAME = "hive-planner"


def start(
    ctx: typer.Context,
    umbrella_id: Annotated[
        str,
        typer.Argument(help="Umbrella issue number to start work on."),
    ],
    repos: Annotated[
        str | None,
        typer.Option("--repos", "-r", help="Comma-separated list of repos to work on (default: all)."),
    ] = None,
    no_tmux: Annotated[
        bool,
        typer.Option("--no-tmux", help="Don't create tmux windows."),
    ] = False,
    with_claude: Annotated[
        bool,
        typer.Option("--with-claude", help="Start Claude in each repo window."),
    ] = False,
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """Start work on an umbrella issue.

    Creates worktrees for each repo and opens tmux windows.

    \b
    What this does:
    1. Loads issues.json from .hive/tasks/<umbrella_id>/
    2. Creates worktrees in .hive/wt/<umbrella_id>/<repo>/
    3. Creates branch feat/<umbrella_id>-<repo>
    4. Writes TASK.md with issue links in each worktree
    5. Opens tmux windows per repo (in hive-planner session)
    """
    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    # Load workspace
    if not context.has_workspace():
        _error("workspace.yaml not found", json_output, exit_code=3)

    try:
        workspace = context.load_workspace()
    except WorkspaceError as e:
        _error(str(e), json_output, exit_code=3)

    # Load issues.json
    task_dir = context.task_dir(umbrella_id)
    issues_path = task_dir / "issues.json"

    if not issues_path.exists():
        _error(f"Task not found: {umbrella_id}. Run 'hive issue new' first.", json_output, exit_code=1)

    try:
        with open(issues_path) as f:
            issues_data = json.load(f)
    except (json.JSONDecodeError, OSError) as e:
        _error(f"Failed to load issues.json: {e}", json_output, exit_code=1)

    umbrella = issues_data.get("umbrella", {})
    repo_issues = issues_data.get("repos", {})

    if not repo_issues:
        _error("No repo issues found in issues.json", json_output, exit_code=1)

    # Filter repos if --repos specified
    if repos:
        selected_repos = [r.strip() for r in repos.split(",") if r.strip()]
        # Validate selected repos exist in issues.json
        for repo_key in selected_repos:
            if repo_key not in repo_issues:
                _error(f"Repo '{repo_key}' not found in issues.json", json_output, exit_code=1)
    else:
        selected_repos = list(repo_issues.keys())

    if not json_output:
        typer.echo(f"Starting work on #{umbrella_id}")
        typer.echo(f"  Repos: {', '.join(selected_repos)}")
        typer.echo()

    # Ensure directories exist
    context.ensure_dirs()

    # Process each repo
    results: list[dict[str, Any]] = []

    for repo_key in selected_repos:
        result = _setup_repo_worktree(
            context=context,
            workspace=workspace,
            umbrella_id=umbrella_id,
            umbrella=umbrella,
            repo_key=repo_key,
            repo_issue=repo_issues.get(repo_key, {}),
            json_output=json_output,
        )
        results.append(result)

    # Create tmux windows if requested
    if not no_tmux:
        _setup_tmux_windows(
            context=context,
            umbrella_id=umbrella_id,
            results=results,
            json_output=json_output,
            with_claude=with_claude,
            claude_yolo=workspace.defaults.claude_yolo,
        )

    # Output results
    if json_output:
        output = {
            "umbrella_id": umbrella_id,
            "umbrella_url": umbrella.get("url"),
            "repos": {r["repo_key"]: r for r in results},
        }
        typer.echo(json.dumps(output, indent=2))
    else:
        typer.echo()
        _print_summary(results)
        typer.echo()
        typer.echo("Worktrees ready. Switch to hive-planner session to begin work.")


def _setup_repo_worktree(
    context,
    workspace,
    umbrella_id: str,
    umbrella: dict[str, Any],
    repo_key: str,
    repo_issue: dict[str, Any],
    json_output: bool,
) -> dict[str, Any]:
    """Set up worktree for a single repo."""
    result: dict[str, Any] = {
        "repo_key": repo_key,
        "status": "unknown",
    }

    # Get repo path from workspace
    repo_config = workspace.get_repo(repo_key)
    if repo_config is None:
        result["status"] = "error"
        result["error"] = "repo not in workspace.yaml"
        if not json_output:
            typer.secho(f"  {repo_key}: error (not in workspace.yaml)", fg="red")
        return result

    repo_path = repo_config.path
    if not repo_path.exists():
        result["status"] = "error"
        result["error"] = f"repo path does not exist: {repo_path}"
        if not json_output:
            typer.secho(f"  {repo_key}: error (repo path missing)", fg="red")
        return result

    # Determine worktree path and branch
    worktree_path = context.worktree_dir(umbrella_id, repo_key)
    branch_name = f"feat/{umbrella_id}-{repo_key}"
    base_branch = workspace.defaults.base_branch

    # Check if worktree already exists
    if worktree_path.exists():
        result["status"] = "exists"
        result["worktree_path"] = str(worktree_path)
        result["branch"] = branch_name
        if not json_output:
            typer.echo(f"  {repo_key}: exists at {worktree_path}")
        return result

    if not json_output:
        typer.echo(f"  {repo_key}: creating worktree...")

    # Fetch origin first to ensure we have latest refs
    try:
        fetch_origin(repo_path)
    except GitError as e:
        # Non-fatal, continue with what we have
        if not json_output:
            typer.secho(f"    warning: fetch failed - {e}", fg="yellow")

    # Create worktree
    try:
        worktree_path.parent.mkdir(parents=True, exist_ok=True)
        create_worktree(
            repo_path=repo_path,
            worktree_path=worktree_path,
            branch=branch_name,
            base_ref=f"origin/{base_branch}",
        )
    except GitError as e:
        result["status"] = "error"
        result["error"] = f"worktree creation failed: {e}"
        if not json_output:
            typer.secho(f"  {repo_key}: failed - {e}", fg="red")
        return result

    # Symlink important files from main repo
    symlink_patterns = workspace.defaults.symlink_patterns
    symlinked = _symlink_files(repo_path, worktree_path, symlink_patterns)
    if symlinked and not json_output:
        typer.echo(f"    symlinked: {', '.join(symlinked)}")

    # Run setup commands in worktree
    setup_commands = workspace.defaults.worktree_setup_commands
    _run_setup_commands(worktree_path, setup_commands, json_output)

    # Write TASK.md
    task_md = _generate_task_md(
        umbrella_id=umbrella_id,
        umbrella_url=umbrella.get("url", ""),
        repo_key=repo_key,
        repo_issue=repo_issue,
    )
    task_md_path = worktree_path / "TASK.md"
    try:
        with open(task_md_path, "w") as f:
            f.write(task_md)
    except OSError as e:
        # Non-fatal
        if not json_output:
            typer.secho(f"    warning: failed to write TASK.md - {e}", fg="yellow")

    # Write CLAUDE.local.md (worker context)
    claude_local = _generate_claude_local_md(umbrella_id)
    claude_local_path = worktree_path / "CLAUDE.local.md"
    try:
        with open(claude_local_path, "w") as f:
            f.write(claude_local)
    except OSError as e:
        # Non-fatal
        if not json_output:
            typer.secho(f"    warning: failed to write CLAUDE.local.md - {e}", fg="yellow")

    result["status"] = "created"
    result["symlinked"] = symlinked
    result["worktree_path"] = str(worktree_path)
    result["branch"] = branch_name

    if not json_output:
        typer.secho(f"  {repo_key}: created at {worktree_path}", fg="green")

    return result


def _generate_task_md(
    umbrella_id: str,
    umbrella_url: str,
    repo_key: str,
    repo_issue: dict[str, Any],
) -> str:
    """Generate TASK.md content for a worktree."""
    repo_url = repo_issue.get("url", "")
    branch_name = f"feat/{umbrella_id}-{repo_key}"

    lines = [
        f"# Task #{umbrella_id}",
        "",
        f"**Repo:** {repo_key}",
        f"**Branch:** `{branch_name}` (already checked out)",
        "",
        "## Context",
        "",
        f"This is part of a multi-repo task coordinated by umbrella issue #{umbrella_id}.",
        f"The umbrella issue tracks the overall progress: {umbrella_url}",
        "",
    ]

    if repo_url:
        lines.extend([
            f"This repo's specific issue: {repo_url}",
            "",
        ])

    lines.append("## Instructions")
    lines.append("")
    lines.append("1. Review the umbrella issue and this repo's issue (if any) for requirements")
    lines.append("2. Implement the required changes in this repo")
    lines.append("3. Commit with clear, descriptive messages")
    lines.append("4. Push and create a PR with:")
    lines.append("   - Title: A clear description of what this PR does")
    lines.append(f"   - Body: Reference the umbrella issue with `Part of #{umbrella_id}`")

    # Add instruction to close repo-specific issue if it exists
    repo_number = repo_issue.get("number")
    if repo_number:
        lines.append(f"   - Body: Close this repo's issue with `Closes #{repo_number}`")

    lines.extend([
        "",
        "---",
        "*Generated by hive*",
    ])

    return "\n".join(lines)


def _generate_claude_local_md(umbrella_id: str) -> str:
    """Generate CLAUDE.local.md content for worker context."""
    return f"""# Worker Agent

You are a worker agent in a multi-repo task system (hive). Your job is to implement changes in THIS repo only.

## Your Role

- Focus on implementing the task described in TASK.md
- Work only within this repository
- Write clean, well-tested code following this repo's conventions
- Create a PR when your implementation is complete

## Guidelines

- Read TASK.md first for your specific task and PR instructions
- Check the linked issues for full context and requirements
- If requirements are unclear, make reasonable assumptions and note them in the PR
- Keep commits focused and well-described

## Important

- Do NOT commit CLAUDE.local.md or TASK.md (add to .gitignore if needed)
- These files are generated by hive for your context only

---
*Generated by hive for task #{umbrella_id}*
"""


def _symlink_files(
    source_dir: Path,
    target_dir: Path,
    patterns: list[str],
) -> list[str]:
    """Symlink files matching patterns from source to target directory.

    Args:
        source_dir: Source directory (main repo).
        target_dir: Target directory (worktree).
        patterns: List of glob patterns to match.

    Returns:
        List of filenames that were symlinked.
    """
    symlinked = []

    for pattern in patterns:
        # Find matching files in source directory
        for source_file in source_dir.iterdir():
            if not source_file.is_file():
                continue

            # Check if filename matches pattern
            if fnmatch.fnmatch(source_file.name, pattern):
                target_file = target_dir / source_file.name

                # Skip if already exists (don't overwrite)
                if target_file.exists() or target_file.is_symlink():
                    continue

                try:
                    # Create relative symlink
                    relative_source = os.path.relpath(source_file, target_dir)
                    target_file.symlink_to(relative_source)
                    symlinked.append(source_file.name)
                except OSError:
                    # Non-fatal, skip this file
                    pass

    return symlinked


def _run_setup_commands(
    worktree_path: Path,
    commands: list[str],
    json_output: bool,
) -> list[str]:
    """Run setup commands in the worktree directory.

    Args:
        worktree_path: Path to the worktree.
        commands: List of shell commands to run.
        json_output: Whether to suppress output.

    Returns:
        List of commands that succeeded.
    """
    succeeded = []
    for cmd in commands:
        try:
            result = subprocess.run(
                cmd,
                shell=True,
                cwd=worktree_path,
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                if not json_output:
                    typer.echo(f"    {cmd}: ok")
                succeeded.append(cmd)
            # Non-zero return is non-fatal, just skip
        except OSError:
            # Command failed to run - non-fatal
            pass
    return succeeded


def _setup_tmux_windows(
    context,
    umbrella_id: str,
    results: list[dict[str, Any]],
    json_output: bool,
    with_claude: bool = False,
    claude_yolo: bool = False,
) -> None:
    """Create tmux windows for repos."""
    successful_repos = [r for r in results if r["status"] in ("created", "exists")]

    if not successful_repos:
        return

    if not json_output:
        typer.echo("Setting up tmux windows...")

    try:
        # Create session if it doesn't exist
        if not session_exists(SESSION_NAME):
            if not json_output:
                typer.echo(f"  Creating {SESSION_NAME} session...")
            create_session(
                session_name=SESSION_NAME,
                start_dir=context.root,
                window_name="planner",
                detached=True,
            )

        # Create window for each repo
        windows_created = []
        for repo_result in successful_repos:
            repo_key = repo_result["repo_key"]
            worktree_path = Path(repo_result["worktree_path"])
            window_name = f"{umbrella_id}-{repo_key}"

            if window_exists(SESSION_NAME, window_name):
                if not json_output:
                    typer.echo(f"  {window_name}: window exists")
                # Still track for claude startup
                if with_claude:
                    windows_created.append(window_name)
                continue

            create_window(
                session_name=SESSION_NAME,
                window_name=window_name,
                start_dir=worktree_path,
            )
            windows_created.append(window_name)

            if not json_output:
                typer.secho(f"  {window_name}: window created", fg="green")

        # Split each window: Claude on left, shell on right
        if with_claude and windows_created:
            if not json_output:
                typer.echo("Setting up worker panes...")
            claude_cmd = "claude --dangerously-skip-permissions" if claude_yolo else "claude"
            for window_name in windows_created:
                target = f"{SESSION_NAME}:{window_name}"
                # Get worktree path from results (window_name is {umbrella_id}-{repo_key})
                worktree_path = None
                repo_key = window_name.split("-", 1)[1] if "-" in window_name else None
                for r in successful_repos:
                    if r.get("repo_key") == repo_key:
                        worktree_path = r.get("worktree_path")
                        break

                # Split window horizontally - shell on right (35%)
                if worktree_path:
                    run_tmux([
                        "split-window", "-t", target,
                        "-h",  # horizontal split
                        "-p", "35",  # shell pane is 35%
                        "-c", str(worktree_path),
                    ])
                    # Select back to left pane for Claude
                    run_tmux(["select-pane", "-t", target, "-L"])

                # Start Claude in left pane
                send_keys(SESSION_NAME, window_name, claude_cmd)
                if not json_output:
                    mode = " (yolo)" if claude_yolo else ""
                    typer.secho(f"  {window_name}: Claude + shell{mode}", fg="green")

    except TmuxError as e:
        if not json_output:
            typer.secho(f"  tmux error: {e}", fg="yellow")


def _print_summary(results: list[dict[str, Any]]) -> None:
    """Print summary of worktree setup."""
    created = sum(1 for r in results if r["status"] == "created")
    exists = sum(1 for r in results if r["status"] == "exists")
    errors = sum(1 for r in results if r["status"] == "error")

    typer.echo("Summary:")
    if created:
        typer.secho(f"  Created: {created}", fg="green")
    if exists:
        typer.echo(f"  Already existed: {exists}")
    if errors:
        typer.secho(f"  Errors: {errors}", fg="red")

    # Show worktree paths
    worktrees = [r for r in results if r.get("worktree_path")]
    if worktrees:
        typer.echo()
        typer.echo("Worktrees:")
        for r in worktrees:
            typer.echo(f"  {r['repo_key']}: {r['worktree_path']}")


def _error(message: str, json_output: bool, exit_code: int = 1) -> None:
    """Output error and exit."""
    if json_output:
        typer.echo(json.dumps({"error": message}))
    else:
        typer.secho(f"Error: {message}", fg="red", err=True)
    raise typer.Exit(exit_code)
