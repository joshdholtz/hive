"""hive doctor - Check workspace health."""

import json
import os
from dataclasses import dataclass, field
from typing import Annotated

import typer

from hive.core.context import get_context
from hive.core.deps import check_all_deps, DepCheck
from hive.core.exceptions import WorkspaceError
from hive.core.git import is_git_repo


@dataclass
class CheckResult:
    """Result of a single check."""

    name: str
    passed: bool
    message: str | None = None
    details: dict | None = None


@dataclass
class DoctorReport:
    """Full doctor report."""

    checks: list[CheckResult] = field(default_factory=list)

    @property
    def all_passed(self) -> bool:
        return all(c.passed for c in self.checks)

    @property
    def has_dep_failures(self) -> bool:
        dep_checks = ["git", "tmux", "gh", "gh auth"]
        return any(not c.passed for c in self.checks if c.name in dep_checks)

    @property
    def has_workspace_failures(self) -> bool:
        workspace_checks = ["workspace.yaml", "repos"]
        return any(not c.passed for c in self.checks if c.name in workspace_checks)

    def add(self, name: str, passed: bool, message: str | None = None, details: dict | None = None):
        self.checks.append(CheckResult(name=name, passed=passed, message=message, details=details))

    def to_dict(self) -> dict:
        return {
            "passed": self.all_passed,
            "checks": [
                {
                    "name": c.name,
                    "passed": c.passed,
                    "message": c.message,
                    **({"details": c.details} if c.details else {}),
                }
                for c in self.checks
            ],
        }


def doctor(
    ctx: typer.Context,
    json_output: Annotated[
        bool,
        typer.Option("--json", help="Output as JSON."),
    ] = False,
) -> None:
    """Check workspace health and dependencies.

    Verifies:
    - workspace.yaml exists and is valid
    - git, tmux, gh are installed
    - gh is authenticated
    - .tasks and .wt directories are writable
    - All configured repo paths exist and are git repos

    Exit codes:
    - 0: All checks passed
    - 2: Missing dependencies
    - 3: Invalid workspace
    """
    # Get context from global options
    root = ctx.obj.get("root") if ctx.obj else None
    no_color = ctx.obj.get("no_color", False) if ctx.obj else False
    verbose = ctx.obj.get("verbose", False) if ctx.obj else False

    context = get_context(root=root, no_color=no_color, verbose=verbose)
    report = DoctorReport()

    # Check external dependencies
    deps = check_all_deps()
    for dep in deps:
        if dep.available:
            msg = f"v{dep.version}" if dep.version else "available"
            report.add(dep.name, True, msg)
        else:
            report.add(dep.name, False, dep.error)

    # Check workspace.yaml
    if context.has_workspace():
        try:
            workspace = context.load_workspace()
            report.add("workspace.yaml", True, f"{len(workspace.repos)} repos configured")
        except WorkspaceError as e:
            report.add("workspace.yaml", False, str(e))
    else:
        report.add("workspace.yaml", False, f"Not found: {context.workspace_yaml}")

    # Check .tasks directory
    tasks_dir = context.tasks_dir
    if tasks_dir.exists():
        if os.access(tasks_dir, os.W_OK):
            report.add(".tasks", True, "writable")
        else:
            report.add(".tasks", False, "exists but not writable")
    else:
        # Try to create it
        try:
            tasks_dir.mkdir(parents=True, exist_ok=True)
            report.add(".tasks", True, "created")
        except Exception as e:
            report.add(".tasks", False, f"cannot create: {e}")

    # Check .wt directory
    wt_dir = context.worktrees_dir
    if wt_dir.exists():
        if os.access(wt_dir, os.W_OK):
            report.add(".wt", True, "writable")
        else:
            report.add(".wt", False, "exists but not writable")
    else:
        # Try to create it
        try:
            wt_dir.mkdir(parents=True, exist_ok=True)
            report.add(".wt", True, "created")
        except Exception as e:
            report.add(".wt", False, f"cannot create: {e}")

    # Check repo paths (only if workspace loaded successfully)
    if context._workspace_config is not None:
        workspace = context.workspace
        repo_details = {}
        all_repos_ok = True

        for key, repo_config in workspace.repos.items():
            path = repo_config.path
            if not path.exists():
                repo_details[key] = {"exists": False, "is_git": False, "error": "path not found"}
                all_repos_ok = False
            elif not is_git_repo(path):
                repo_details[key] = {"exists": True, "is_git": False, "error": "not a git repo"}
                all_repos_ok = False
            else:
                repo_details[key] = {"exists": True, "is_git": True}

        if all_repos_ok:
            report.add("repos", True, f"all {len(workspace.repos)} repos valid")
        else:
            failed = [k for k, v in repo_details.items() if "error" in v]
            report.add("repos", False, f"{len(failed)} repo(s) invalid", details=repo_details)

    # Output results
    if json_output:
        typer.echo(json.dumps(report.to_dict(), indent=2))
    else:
        _print_report(report, no_color)

    # Determine exit code
    if not report.all_passed:
        if report.has_dep_failures:
            raise typer.Exit(2)
        else:
            raise typer.Exit(3)


def _print_report(report: DoctorReport, no_color: bool = False) -> None:
    """Print report in human-readable format."""
    # Use simple symbols for compatibility
    check_mark = "ok" if no_color else "\u2713"
    cross_mark = "FAIL" if no_color else "\u2717"

    for check in report.checks:
        symbol = check_mark if check.passed else cross_mark
        msg = f" ({check.message})" if check.message else ""

        if check.passed:
            typer.echo(f"  {symbol} {check.name}{msg}")
        else:
            typer.secho(f"  {symbol} {check.name}{msg}", fg="red" if not no_color else None)

            # Print details for failed checks
            if check.details:
                for key, detail in check.details.items():
                    if isinstance(detail, dict) and "error" in detail:
                        typer.echo(f"      {key}: {detail['error']}")

    typer.echo()
    if report.all_passed:
        typer.secho("All checks passed.", fg="green" if not no_color else None)
    else:
        failed_count = sum(1 for c in report.checks if not c.passed)
        typer.secho(f"{failed_count} check(s) failed.", fg="red" if not no_color else None)
