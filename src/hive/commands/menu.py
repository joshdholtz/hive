"""hive menu - Interactive TUI dashboard for planner pane."""

import json
import subprocess
import sys
import termios
import tty
from dataclasses import dataclass
from typing import Any

import typer

from hive.core.context import get_context
from hive.core.tmux import list_windows, run_tmux, session_exists

menu_app = typer.Typer(
    name="menu",
    help="Interactive menu for planner session.",
    no_args_is_help=False,
)

SESSION_NAME = "hive-planner"


@dataclass
class Worker:
    """Active worker window."""
    window_name: str
    task_id: str
    repo: str


@dataclass
class Issue:
    """Open issue from GitHub."""
    number: int
    title: str
    labels: list[str]


@dataclass
class PullRequest:
    """Open pull request."""
    number: int
    title: str
    repo: str
    branch: str
    url: str
    draft: bool = False


# ANSI color codes
class Colors:
    RESET = "\033[0m"
    BOLD = "\033[1m"
    DIM = "\033[2m"
    GREEN = "\033[32m"
    YELLOW = "\033[33m"
    BLUE = "\033[34m"
    CYAN = "\033[36m"
    WHITE = "\033[37m"
    REVERSE = "\033[7m"  # Swap foreground/background


def _clear_screen():
    """Clear terminal screen."""
    print("\033[2J\033[H", end="", flush=True)


def _hide_cursor():
    """Hide terminal cursor."""
    print("\033[?25l", end="", flush=True)


def _show_cursor():
    """Show terminal cursor."""
    print("\033[?25h", end="", flush=True)


def _get_key() -> str:
    """Read a single keypress."""
    fd = sys.stdin.fileno()
    old_settings = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        ch = sys.stdin.read(1)
        # Handle arrow keys (escape sequences)
        if ch == '\x1b':
            ch2 = sys.stdin.read(1)
            if ch2 == '[':
                ch3 = sys.stdin.read(1)
                if ch3 == 'A':
                    return 'up'
                elif ch3 == 'B':
                    return 'down'
        return ch
    finally:
        termios.tcsetattr(fd, termios.TCSADRAIN, old_settings)


def _confirm(prompt: str) -> bool:
    """Show inline confirmation prompt and wait for y/n.

    Args:
        prompt: The confirmation message to display.

    Returns:
        True if user pressed y, False otherwise.
    """
    print(f"\n  {Colors.YELLOW}{prompt} (y/n){Colors.RESET}", end="", flush=True)
    key = _get_key()
    return key.lower() == 'y'


def _get_workers() -> list[Worker]:
    """Get active worker windows from tmux."""
    workers = []

    if not session_exists(SESSION_NAME):
        return workers

    windows = list_windows(SESSION_NAME)
    for window in windows:
        # Worker windows are named <task_id>-<repo>
        if '-' in window and window not in ('planner', 'shell'):
            parts = window.split('-', 1)
            if len(parts) == 2:
                task_id, repo = parts
                workers.append(Worker(
                    window_name=window,
                    task_id=task_id,
                    repo=repo,
                ))

    return workers


def _check_and_close_completed_umbrellas(context) -> list[int]:
    """Check for umbrella issues where all sub-issues are closed, and close them.

    Returns list of umbrella issue numbers that were closed.
    """
    closed = []

    if not context.has_workspace():
        return closed

    tasks_dir = context.tasks_dir
    if not tasks_dir.exists():
        return closed

    try:
        for task_dir in tasks_dir.iterdir():
            if not task_dir.is_dir():
                continue

            issues_path = task_dir / "issues.json"
            if not issues_path.exists():
                continue

            try:
                with open(issues_path) as f:
                    issues_data = json.load(f)
            except (json.JSONDecodeError, OSError):
                continue

            umbrella = issues_data.get("umbrella", {})
            repos = issues_data.get("repos", {})

            if not umbrella or not repos:
                continue

            umbrella_repo = umbrella.get("repo")
            umbrella_number = umbrella.get("number")

            if not umbrella_repo or not umbrella_number:
                continue

            # Check if all sub-issues are closed
            all_closed = True
            for repo_key, repo_data in repos.items():
                sub_repo = repo_data.get("repo")
                sub_number = repo_data.get("number")
                if not sub_repo or not sub_number:
                    continue

                try:
                    result = subprocess.run(
                        ["gh", "issue", "view", str(sub_number), "--repo", sub_repo, "--json", "state"],
                        capture_output=True,
                        text=True,
                        timeout=5,
                    )
                    if result.returncode == 0:
                        state_data = json.loads(result.stdout)
                        if state_data.get("state") != "CLOSED":
                            all_closed = False
                            break
                except (subprocess.TimeoutExpired, json.JSONDecodeError):
                    all_closed = False
                    break

            if all_closed:
                # Check if umbrella is still open before closing
                try:
                    result = subprocess.run(
                        ["gh", "issue", "view", str(umbrella_number), "--repo", umbrella_repo, "--json", "state"],
                        capture_output=True,
                        text=True,
                        timeout=5,
                    )
                    if result.returncode == 0:
                        state_data = json.loads(result.stdout)
                        if state_data.get("state") == "OPEN":
                            # Close the umbrella
                            close_result = subprocess.run(
                                ["gh", "issue", "close", str(umbrella_number), "--repo", umbrella_repo],
                                capture_output=True,
                                timeout=10,
                            )
                            if close_result.returncode == 0:
                                closed.append(umbrella_number)
                except (subprocess.TimeoutExpired, json.JSONDecodeError):
                    pass
    except Exception:
        pass

    return closed


def _get_issues(context) -> list[Issue]:
    """Fetch open issues from the configured provider."""
    issues = []

    if not context.has_workspace():
        return issues

    try:
        workspace = context.load_workspace()

        # Use provider abstraction
        from hive.providers import get_provider
        from hive.providers.base import ProviderConfig as ProviderCfg

        # Build provider config
        provider_cfg = ProviderCfg(
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

        provider = get_provider(provider_cfg)
        provider_issues = provider.list_issues(state="open", limit=20)

        for pi in provider_issues:
            issues.append(Issue(
                number=pi.number,
                title=pi.title,
                labels=pi.labels,
            ))
    except Exception:
        pass

    return issues


def _get_prs(context) -> list[PullRequest]:
    """Fetch open PRs from all repos in workspace."""
    prs = []

    if not context.has_workspace():
        return prs

    try:
        workspace = context.load_workspace()

        # Fetch PRs in parallel using ThreadPoolExecutor
        from concurrent.futures import ThreadPoolExecutor, as_completed

        def fetch_repo_prs(repo_key: str, repo_path) -> list[PullRequest]:
            result_prs = []
            if not repo_path.exists():
                return result_prs
            try:
                result = subprocess.run(
                    ["gh", "pr", "list", "--json", "number,title,headRefName,url,isDraft", "--limit", "5"],
                    cwd=repo_path,
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                if result.returncode == 0 and result.stdout.strip():
                    pr_data = json.loads(result.stdout)
                    for pr in pr_data:
                        result_prs.append(PullRequest(
                            number=pr["number"],
                            title=pr["title"],
                            repo=repo_key,
                            branch=pr["headRefName"],
                            url=pr["url"],
                            draft=pr.get("isDraft", False),
                        ))
            except (subprocess.TimeoutExpired, json.JSONDecodeError, FileNotFoundError):
                pass
            return result_prs

        with ThreadPoolExecutor(max_workers=8) as executor:
            futures = {
                executor.submit(fetch_repo_prs, key, cfg.path): key
                for key, cfg in workspace.repos.items()
            }
            for future in as_completed(futures, timeout=10):
                try:
                    prs.extend(future.result())
                except Exception:
                    pass

    except Exception:
        pass

    return prs


def _render_dashboard(
    workers: list[Worker],
    issues: list[Issue],
    selected_idx: int,
    section: str,  # 'workers' or 'issues'
    message: str = "",
):
    """Render the dashboard (workers + issues only)."""
    _clear_screen()

    width = 60

    # Header
    print(f"{Colors.BOLD}{'═' * width}{Colors.RESET}")
    print(f"{Colors.BOLD}  HIVE{Colors.RESET}")
    print(f"{Colors.BOLD}{'═' * width}{Colors.RESET}")
    print()

    # Workers section
    section_marker = f"{Colors.REVERSE} " if section == 'workers' else "  "
    print(f"{section_marker}{Colors.BOLD}{Colors.CYAN}Active Workers ({len(workers)}){Colors.RESET}")
    if workers:
        for i, worker in enumerate(workers):
            if section == 'workers' and i == selected_idx:
                print(f"  {Colors.GREEN}{Colors.BOLD}> {worker.window_name:<25}{Colors.RESET} {Colors.DIM}feat/{worker.task_id}-{worker.repo}{Colors.RESET}")
            else:
                print(f"    {worker.window_name:<25} {Colors.DIM}feat/{worker.task_id}-{worker.repo}{Colors.RESET}")
    else:
        print(f"    {Colors.DIM}No active workers{Colors.RESET}")
    print()

    # Issues section
    section_marker = f"{Colors.REVERSE} " if section == 'issues' else "  "
    print(f"{section_marker}{Colors.BOLD}{Colors.CYAN}Open Issues ({len(issues)}){Colors.RESET}")
    if issues:
        for i, issue in enumerate(issues):
            # Truncate title if needed
            title = issue.title[:40] + "..." if len(issue.title) > 40 else issue.title
            if section == 'issues' and i == selected_idx:
                print(f"  {Colors.GREEN}{Colors.BOLD}> #{issue.number:<5} {title}{Colors.RESET}")
            else:
                print(f"    #{issue.number:<5} {title}")
    else:
        print(f"    {Colors.DIM}No open issues{Colors.RESET}")
    print()

    # Help bar
    print(f"{'─' * width}")
    if section == 'workers' and workers:
        print(f"  {Colors.DIM}j/k{Colors.RESET} navigate  {Colors.DIM}Enter{Colors.RESET} jump  {Colors.DIM}x{Colors.RESET} close  {Colors.DIM}X{Colors.RESET} close+clean  {Colors.DIM}Tab{Colors.RESET} issues  {Colors.DIM}r{Colors.RESET} refresh  {Colors.DIM}q{Colors.RESET} quit")
    elif section == 'issues' and issues:
        print(f"  {Colors.DIM}j/k{Colors.RESET} navigate  {Colors.DIM}Enter{Colors.RESET} start+claude  {Colors.DIM}Tab{Colors.RESET} workers  {Colors.DIM}r{Colors.RESET} refresh  {Colors.DIM}q{Colors.RESET} quit")
    else:
        print(f"  {Colors.DIM}Tab{Colors.RESET} switch section  {Colors.DIM}s{Colors.RESET} start issue  {Colors.DIM}n{Colors.RESET} new issue  {Colors.DIM}r{Colors.RESET} refresh  {Colors.DIM}q{Colors.RESET} quit")

    # Message
    if message:
        print()
        print(f"  {Colors.YELLOW}{message}{Colors.RESET}")


def _jump_to_worker(worker: Worker):
    """Switch to a worker's tmux window."""
    try:
        run_tmux(["select-window", "-t", f"{SESSION_NAME}:{worker.window_name}"])
    except Exception:
        pass


def _kill_worker(worker: Worker, clean: bool = False, context=None) -> str:
    """Kill a worker window and optionally clean up worktree.

    Args:
        worker: Worker to kill.
        clean: If True, also remove worktree.
        context: Workspace context for cleaning.
    """
    try:
        run_tmux(["kill-window", "-t", f"{SESSION_NAME}:{worker.window_name}"])

        if clean and context:
            # Clean up worktree directly
            import shutil
            worktree_path = context.worktrees_dir / worker.task_id / worker.repo

            if worktree_path.exists():
                # Try git worktree remove first
                try:
                    workspace = context.load_workspace() if context.has_workspace() else None
                    if workspace:
                        repo_config = workspace.get_repo(worker.repo)
                        if repo_config and repo_config.path.exists():
                            from hive.core.git import remove_worktree
                            remove_worktree(repo_config.path, worktree_path, force=True)
                except Exception:
                    # Fallback to manual removal
                    shutil.rmtree(worktree_path, ignore_errors=True)

                # Prune and delete branch
                if workspace:
                    repo_config = workspace.get_repo(worker.repo)
                    if repo_config and repo_config.path.exists():
                        branch_name = f"feat/{worker.task_id}-{worker.repo}"
                        subprocess.run(
                            ["git", "worktree", "prune"],
                            cwd=repo_config.path,
                            capture_output=True,
                        )
                        subprocess.run(
                            ["git", "branch", "-D", branch_name],
                            cwd=repo_config.path,
                            capture_output=True,
                        )

                return f"Closed and cleaned {worker.window_name}"
            else:
                return f"Closed {worker.window_name} (no worktree found)"

        return f"Closed {worker.window_name} (worktree kept)"
    except Exception as e:
        return f"Failed: {e}"


def _open_url(url: str) -> None:
    """Open URL in default browser."""
    try:
        subprocess.run(["open", url], capture_output=True)
    except Exception:
        pass


def _start_issue(issue: Issue, with_claude: bool = False) -> str:
    """Start working on an issue."""
    _show_cursor()
    _clear_screen()

    cmd = ["hive", "start", str(issue.number)]
    if with_claude:
        cmd.append("--with-claude")

    print(f"\n> {' '.join(cmd)}\n")
    try:
        subprocess.run(cmd)
        return f"Started #{issue.number}"
    except Exception as e:
        return f"Failed: {e}"
    finally:
        input("\nPress Enter to continue...")
        _hide_cursor()


def _run_interactive_command(cmd: list[str], prompt: str = "") -> str:
    """Run an interactive command."""
    _show_cursor()
    _clear_screen()

    if prompt:
        print(f"\n{prompt}")

    print(f"\n> {' '.join(cmd)}\n")
    try:
        subprocess.run(cmd)
        return "Done"
    except Exception as e:
        return f"Failed: {e}"
    finally:
        input("\nPress Enter to continue...")
        _hide_cursor()


@menu_app.callback(invoke_without_command=True)
def menu_loop(
    ctx: typer.Context,
) -> None:
    """Run interactive TUI dashboard.

    This is designed to run in the planner's second pane.
    """
    if ctx.invoked_subcommand is not None:
        return

    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    # Initial data load
    workers = _get_workers()
    issues = _get_issues(context)

    # State
    section = 'workers' if workers else 'issues'
    selected_idx = 0
    message = ""

    _hide_cursor()

    try:
        while True:
            _render_dashboard(workers, issues, selected_idx, section, message)
            message = ""

            key = _get_key()

            # Get current list based on section
            current_list = workers if section == 'workers' else issues

            if key == 'q':
                break

            elif key in ('up', 'k'):
                # Up arrow or k = move up
                if current_list and selected_idx > 0:
                    selected_idx -= 1

            elif key in ('down', 'j'):
                # Down arrow or j = move down
                if current_list and selected_idx < len(current_list) - 1:
                    selected_idx += 1

            elif key == 'x' and section == 'workers':
                # x = close window (keep worktree)
                if workers and 0 <= selected_idx < len(workers):
                    worker = workers[selected_idx]
                    if _confirm(f"Close {worker.window_name}?"):
                        message = _kill_worker(worker, clean=False, context=context)
                        workers = _get_workers()
                        if selected_idx >= len(workers):
                            selected_idx = max(0, len(workers) - 1)
                    else:
                        message = "Cancelled"

            elif key == 'X' and section == 'workers':
                # X = close window AND clean worktree
                if workers and 0 <= selected_idx < len(workers):
                    worker = workers[selected_idx]
                    if _confirm(f"Close + clean {worker.window_name}?"):
                        message = _kill_worker(worker, clean=True, context=context)
                        workers = _get_workers()
                        if selected_idx >= len(workers):
                            selected_idx = max(0, len(workers) - 1)
                    else:
                        message = "Cancelled"

            elif key == '\t':  # Tab - switch section
                if section == 'workers':
                    section = 'issues'
                else:
                    section = 'workers'
                selected_idx = 0

            elif key == '\r':  # Enter
                if section == 'workers' and workers:
                    worker = workers[selected_idx]
                    _jump_to_worker(worker)
                    message = f"Jumped to {worker.window_name}"
                elif section == 'issues' and issues:
                    issue = issues[selected_idx]
                    message = _start_issue(issue, with_claude=True)  # Always start with Claude
                    workers = _get_workers()
                    issues = _get_issues(context)

            elif key == 's':
                # Quick start - pick issue
                _run_interactive_command(["hive", "pick", "start"], "Pick an issue to start:")
                workers = _get_workers()
                issues = _get_issues(context)

            elif key == 'S':
                # Quick start with Claude
                _run_interactive_command(["hive", "pick", "start", "--with-claude"], "Pick an issue to start with Claude:")
                workers = _get_workers()
                issues = _get_issues(context)

            elif key == 'r':
                # Refresh - also check for completed umbrellas to auto-close
                closed_umbrellas = _check_and_close_completed_umbrellas(context)
                workers = _get_workers()
                issues = _get_issues(context)
                if closed_umbrellas:
                    message = f"Refreshed. Auto-closed umbrella(s): #{', #'.join(map(str, closed_umbrellas))}"
                else:
                    message = "Refreshed"

            elif key == 'n':
                # New issue
                _show_cursor()
                _clear_screen()
                print("\nCreate new issue:")
                try:
                    title = input("Title: ").strip()
                    if title:
                        repos = input("Repos (comma-separated): ").strip()
                        if repos:
                            cmd = ["hive", "issue", "new", title, "--repos", repos]
                            subprocess.run(cmd)
                except (KeyboardInterrupt, EOFError):
                    pass
                input("\nPress Enter to continue...")
                _hide_cursor()
                issues = _get_issues(context)

    finally:
        _show_cursor()
        _clear_screen()


def _get_terminal_height() -> int:
    """Get terminal height."""
    try:
        import shutil
        return shutil.get_terminal_size().lines
    except Exception:
        return 24  # fallback


def _render_prs_panel(
    prs: list[PullRequest],
    selected_idx: int,
    scroll_offset: int = 0,
    message: str = "",
):
    """Render PRs-only panel with scrolling."""
    _clear_screen()

    width = 50
    term_height = _get_terminal_height()
    # Reserve lines for header (4), footer (3), and message (2)
    visible_rows = max(3, term_height - 9)

    # Header
    print(f"{Colors.BOLD}{'═' * width}{Colors.RESET}")
    print(f"{Colors.BOLD}  PULL REQUESTS ({len(prs)}){Colors.RESET}")
    print(f"{Colors.BOLD}{'═' * width}{Colors.RESET}")
    print()

    if prs:
        # Show scroll indicator if needed
        if scroll_offset > 0:
            print(f"    {Colors.DIM}↑ {scroll_offset} more{Colors.RESET}")

        # Render visible PRs
        end_idx = min(scroll_offset + visible_rows, len(prs))
        for i in range(scroll_offset, end_idx):
            pr = prs[i]
            title = pr.title[:30] + "..." if len(pr.title) > 30 else pr.title
            draft = f"{Colors.DIM}[draft]{Colors.RESET} " if pr.draft else ""
            if i == selected_idx:
                print(f"  {Colors.GREEN}{Colors.BOLD}>{Colors.RESET} {Colors.GREEN}{pr.repo:<10}{Colors.RESET} #{pr.number:<4} {draft}{title}")
            else:
                print(f"    {pr.repo:<10} #{pr.number:<4} {draft}{title}")

        # Show scroll indicator if more below
        remaining = len(prs) - end_idx
        if remaining > 0:
            print(f"    {Colors.DIM}↓ {remaining} more{Colors.RESET}")
    else:
        print(f"    {Colors.DIM}No open PRs{Colors.RESET}")

    print()
    print(f"{'─' * width}")
    print(f"  {Colors.DIM}j/k{Colors.RESET} navigate  {Colors.DIM}Enter{Colors.RESET} open  {Colors.DIM}u{Colors.RESET} show URL  {Colors.DIM}r{Colors.RESET} refresh  {Colors.DIM}q{Colors.RESET} quit")

    if message:
        print()
        print(f"  {Colors.YELLOW}{message}{Colors.RESET}")


def _adjust_scroll(selected_idx: int, scroll_offset: int, visible_rows: int) -> int:
    """Adjust scroll offset to keep selection visible."""
    if selected_idx < scroll_offset:
        return selected_idx
    elif selected_idx >= scroll_offset + visible_rows:
        return selected_idx - visible_rows + 1
    return scroll_offset


@menu_app.command("prs")
def prs_panel(
    ctx: typer.Context,
) -> None:
    """Show PRs panel (for split pane)."""
    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    prs = _get_prs(context)
    selected_idx = 0
    scroll_offset = 0
    message = ""

    _hide_cursor()

    try:
        while True:
            # Calculate visible rows for scrolling
            term_height = _get_terminal_height()
            visible_rows = max(3, term_height - 9)

            # Adjust scroll to keep selection visible
            scroll_offset = _adjust_scroll(selected_idx, scroll_offset, visible_rows)

            _render_prs_panel(prs, selected_idx, scroll_offset, message)
            message = ""

            key = _get_key()

            if key == 'q':
                break

            elif key in ('up', 'k'):
                if prs and selected_idx > 0:
                    selected_idx -= 1

            elif key in ('down', 'j'):
                if prs and selected_idx < len(prs) - 1:
                    selected_idx += 1

            elif key == '\r':  # Enter
                if prs and 0 <= selected_idx < len(prs):
                    pr = prs[selected_idx]
                    _open_url(pr.url)
                    message = f"Opened PR #{pr.number}"

            elif key == 'u':  # Show URL
                if prs and 0 <= selected_idx < len(prs):
                    pr = prs[selected_idx]
                    _show_cursor()
                    _clear_screen()
                    print(f"\n  PR #{pr.number}: {pr.title}\n")
                    print(f"  {Colors.CYAN}{pr.url}{Colors.RESET}\n")
                    print(f"  {Colors.DIM}(Select and copy the URL above){Colors.RESET}\n")
                    input("  Press Enter to continue...")
                    _hide_cursor()

            elif key == 'r':
                prs = _get_prs(context)
                message = "Refreshed"
                if selected_idx >= len(prs):
                    selected_idx = max(0, len(prs) - 1)
                scroll_offset = 0

    finally:
        _show_cursor()
        _clear_screen()


if __name__ == "__main__":
    menu_app()
