"""Main CLI entry point for hive."""

from typing import Annotated, Optional

import typer

from hive import __version__
from hive.commands import clean, context, doctor, init, planner, show, status

app = typer.Typer(
    name="hive",
    help="Terminal-first polyrepo coordinator. GitHub Issues are the source of truth.",
    rich_markup_mode="rich",
)

# Global options callback
@app.callback(invoke_without_command=True)
def main(
    ctx: typer.Context,
    root: Annotated[
        Optional[str],
        typer.Option("--root", help="Workspace root directory."),
    ] = None,
    no_color: Annotated[
        bool,
        typer.Option("--no-color", help="Disable colored output."),
    ] = False,
    verbose: Annotated[
        bool,
        typer.Option("--verbose", "-v", help="Enable verbose output."),
    ] = False,
    version: Annotated[
        bool,
        typer.Option("--version", help="Show version and exit."),
    ] = False,
) -> None:
    """Hive: Terminal-first polyrepo coordinator."""
    if version:
        typer.echo(f"hive {__version__}")
        raise typer.Exit()

    # If no command specified and not --version, show help
    if ctx.invoked_subcommand is None:
        typer.echo(ctx.get_help())
        raise typer.Exit()

    # Store global options in context for commands to access
    ctx.ensure_object(dict)
    ctx.obj["root"] = root
    ctx.obj["no_color"] = no_color
    ctx.obj["verbose"] = verbose


# Register commands
app.command(name="doctor")(doctor.doctor)
app.command(name="init")(init.init)
app.command(name="planner")(planner.planner)
app.command(name="status")(status.status)
app.command(name="clean")(clean.clean)
app.command(name="show")(show.show)
app.command(name="context")(context.context)

# Import and register issue subcommand group (v2)
from hive.commands.issue_v2 import issue_app
app.add_typer(issue_app, name="issue")

# Import and register start command
from hive.commands.start import start
app.command(name="start")(start)

# Import and register pick subcommand group
from hive.commands.pick import pick_app
app.add_typer(pick_app, name="pick")

# Import and register menu command
from hive.commands.menu import menu_app
app.add_typer(menu_app, name="menu")


if __name__ == "__main__":
    app()
