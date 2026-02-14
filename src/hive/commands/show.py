"""hive show - Display task details."""

import json
from typing import Annotated, Any

import typer

from hive.core.context import get_context
from hive.core.exceptions import PlanError
from hive.core.plan import load_plan


def show(
    ctx: typer.Context,
    task_id: Annotated[
        str,
        typer.Argument(help="Task ID to display."),
    ],
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """Display parsed plan for a task."""
    # Get context from global options
    root = ctx.obj.get("root") if ctx.obj else None
    no_color = ctx.obj.get("no_color", False) if ctx.obj else False

    context = get_context(root=root, no_color=no_color)

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

    # Add metadata
    output: dict[str, Any] = {
        "task_dir": str(task_dir),
        "plan_path": str(plan_path),
        **plan,
    }

    if json_output:
        typer.echo(json.dumps(output, indent=2))
    else:
        _print_plan(output, no_color)


def _print_plan(plan: dict[str, Any], no_color: bool) -> None:
    """Print plan in human-readable format."""
    typer.secho(f"Task: {plan.get('id', 'unknown')}", bold=True)
    typer.echo(f"Title: {plan.get('title', '')}")
    typer.echo(f"Branch: {plan.get('branch', '')}")
    typer.echo(f"Path: {plan.get('plan_path', '')}")
    typer.echo()

    # Repos section
    repos = plan.get("repos", {})
    if repos:
        typer.secho("Repos:", bold=True)
        for repo_key, repo_config in repos.items():
            base = repo_config.get("base", "main")
            test = repo_config.get("test", "")
            tasks = repo_config.get("tasks", [])

            typer.echo(f"  {repo_key}:")
            typer.echo(f"    Base: {base}")
            if test:
                typer.echo(f"    Test: {test}")
            if tasks:
                typer.echo("    Tasks:")
                for task in tasks:
                    typer.echo(f"      - {task}")
        typer.echo()

    # Tmux config
    tmux = plan.get("tmux", {})
    if tmux:
        enabled = tmux.get("enabled", True)
        typer.echo(f"Tmux: {'enabled' if enabled else 'disabled'}")

    # PR config
    pr = plan.get("pr", {})
    if pr:
        enabled = pr.get("enabled", True)
        draft = pr.get("draft", True)
        typer.echo(f"PR: {'enabled' if enabled else 'disabled'} {'(draft)' if draft else ''}")
