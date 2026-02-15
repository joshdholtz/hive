"""hive planner - Start or attach to the hive planner session."""

from typing import Annotated

import typer

from hive.core.context import get_context
from hive.core.exceptions import TmuxError, WorkspaceError
from hive.core.tmux import (
    attach_session,
    create_session,
    create_window,
    is_inside_tmux,
    run_tmux,
    session_exists,
    switch_client,
    window_exists,
)

SESSION_NAME = "hive-planner"


def planner(
    ctx: typer.Context,
    no_agent: Annotated[
        bool,
        typer.Option("--no-agent", help="Don't start Claude in the planner window."),
    ] = False,
    no_menu: Annotated[
        bool,
        typer.Option("--no-menu", help="Don't start the interactive menu pane."),
    ] = False,
    no_web: Annotated[
        bool,
        typer.Option("--no-web", help="Don't start the web UI server."),
    ] = False,
) -> None:
    """Start or attach to the hive planner tmux session.

    Creates a persistent tmux session with:
    - Pane A (left): Claude Code (architect/planner agent)
    - Pane B (right): Interactive issue picker menu
    - A shell window for manual commands

    This is where you plan and coordinate multi-repo work.
    """
    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    # Load workspace for setup commands
    setup_commands: list[str] = []
    if context.has_workspace():
        try:
            workspace = context.load_workspace()
            setup_commands = workspace.defaults.worktree_setup_commands
        except WorkspaceError:
            pass

    try:
        if not session_exists(SESSION_NAME):
            typer.echo("Creating planner session...")
            _create_planner_session(
                context,
                start_agent=not no_agent,
                start_menu=not no_menu,
                start_web=not no_web,
            )
            # Reopen windows for existing worktrees
            reopened = _reopen_task_windows(context, setup_commands)
            if reopened:
                typer.echo(f"  Reopened {len(reopened)} task windows")
        else:
            typer.echo("Attaching to existing planner session...")
            # Check for any missing task windows
            reopened = _reopen_task_windows(context, setup_commands)
            if reopened:
                typer.echo(f"  Reopened {len(reopened)} task windows")

        # Always select planner window before attaching
        run_tmux(["select-window", "-t", f"{SESSION_NAME}:planner"])

        # Attach or switch
        if is_inside_tmux():
            switch_client(SESSION_NAME)
        else:
            attach_session(SESSION_NAME)

    except TmuxError as e:
        typer.secho(f"Error: {e}", fg="red", err=True)
        raise typer.Exit(1)


def _create_planner_session(
    context,
    start_agent: bool = True,
    start_menu: bool = True,
    start_web: bool = True,
) -> None:
    """Create the hive planner session with split panes."""
    target = f"{SESSION_NAME}:planner"

    # Create session with planner window
    create_session(
        session_name=SESSION_NAME,
        start_dir=context.root,
        window_name="planner",
        detached=True,
    )

    # At this point we have one pane (the original)
    # Name it and start Claude
    run_tmux(["select-pane", "-t", target, "-T", "claude"])
    if start_agent:
        run_tmux(["send-keys", "-t", target, "claude", "Enter"])

    if start_menu:
        # Split window horizontally - new pane on right (35%)
        run_tmux([
            "split-window", "-t", target,
            "-h",  # horizontal split (side by side)
            "-p", "35",  # new pane is 35%
            "-c", str(context.root),
            "hive", "menu",  # command to run in new pane
        ])
        run_tmux(["select-pane", "-T", "tasks"])  # Name the new pane

        # Split it vertically for PRs (bottom 40%)
        run_tmux([
            "split-window", "-t", target,
            "-v",  # vertical split (top/bottom)
            "-p", "40",  # new pane is 40% (bottom)
            "-c", str(context.root),
            "hive", "menu", "prs",  # PR panel
        ])
        run_tmux(["select-pane", "-T", "prs"])  # Name the new pane

        # Split again for web server (tiny pane at bottom, 3 lines)
        if start_web:
            run_tmux([
                "split-window", "-t", target,
                "-v",  # vertical split
                "-l", "3",  # just 3 lines tall
                "-c", str(context.root),
                "hive", "web",  # web server
            ])
            run_tmux(["select-pane", "-T", "web"])

        # Select back to Claude pane (leftmost)
        run_tmux(["select-pane", "-t", target, "-L"])
        run_tmux(["select-pane", "-t", target, "-L"])
    elif start_web:
        # No menu but still want web - add small pane on right
        run_tmux([
            "split-window", "-t", target,
            "-h",  # horizontal split
            "-l", "30",  # narrow
            "-c", str(context.root),
            "hive", "web",
        ])
        run_tmux(["select-pane", "-t", target, "-L"])

    # Create a shell window for manual commands
    create_window(
        session_name=SESSION_NAME,
        window_name="shell",
        start_dir=context.root,
    )

    # Go back to planner window
    run_tmux(["select-window", "-t", target])

    typer.secho("Created planner session", fg="green")
    if start_agent:
        typer.echo("  Left pane: Claude (planner agent)")
    if start_menu:
        typer.echo("  Right top: Workers/Issues menu")
        typer.echo("  Right bottom: PRs panel")
    if start_web:
        typer.echo("  Web UI: http://localhost:8080")
    typer.echo("  Shell window: manual commands")


def _reopen_task_windows(context, setup_commands: list[str]) -> list[str]:
    """Reopen tmux windows for existing task worktrees.

    Scans .hive/wt/ for task directories and creates windows
    for any that don't already exist.

    Args:
        context: Hive context.
        setup_commands: Commands to run in each new window (e.g. ["mise trust"]).

    Returns:
        List of window names that were created.
    """
    reopened = []
    worktrees_dir = context.worktrees_dir

    if not worktrees_dir.exists():
        return reopened

    # Scan for task directories
    for task_dir in worktrees_dir.iterdir():
        if not task_dir.is_dir():
            continue

        task_id = task_dir.name

        # Scan for repo worktrees within this task
        for repo_dir in task_dir.iterdir():
            if not repo_dir.is_dir():
                continue

            repo_key = repo_dir.name
            window_name = f"{task_id}-{repo_key}"

            # Skip if window already exists
            if window_exists(SESSION_NAME, window_name):
                continue

            # Create window with setup commands running before shell starts
            try:
                shell_cmd = "; ".join(setup_commands) if setup_commands else None
                create_window(
                    session_name=SESSION_NAME,
                    window_name=window_name,
                    start_dir=repo_dir,
                    shell_command=shell_cmd,
                )

                # Split window: main pane left, shell pane right (35%)
                target = f"{SESSION_NAME}:{window_name}"
                run_tmux([
                    "split-window", "-t", target,
                    "-h",  # horizontal split
                    "-p", "35",  # shell pane is 35%
                    "-c", str(repo_dir),
                ])
                # Name the shell pane (currently selected after split)
                run_tmux(["select-pane", "-T", "shell"])
                # Select back to left pane and name it
                run_tmux(["select-pane", "-t", target, "-L"])
                run_tmux(["select-pane", "-T", "claude"])

                reopened.append(window_name)
            except TmuxError:
                # Non-fatal, continue with other windows
                pass

    return reopened
