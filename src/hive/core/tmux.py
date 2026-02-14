"""Tmux operations."""

import os
import subprocess
from pathlib import Path

from hive.core.exceptions import TmuxError


def run_tmux(
    args: list[str],
    check: bool = True,
    capture_output: bool = True,
) -> subprocess.CompletedProcess[str]:
    """Run a tmux command safely.

    Args:
        args: tmux command arguments (without 'tmux' prefix).
        check: Raise exception on non-zero exit.
        capture_output: Capture stdout/stderr.

    Returns:
        Completed process result.

    Raises:
        TmuxError: If tmux command fails and check=True.
    """
    cmd = ["tmux"] + args
    try:
        result = subprocess.run(
            cmd,
            capture_output=capture_output,
            text=True,
            timeout=30,
        )
        if check and result.returncode != 0:
            error_msg = result.stderr.strip() or result.stdout.strip()
            raise TmuxError(f"tmux {' '.join(args[:2])} failed: {error_msg}")
        return result
    except subprocess.TimeoutExpired as e:
        raise TmuxError(f"tmux {' '.join(args[:2])} timed out") from e
    except FileNotFoundError as e:
        raise TmuxError("tmux not found in PATH") from e


def session_exists(session_name: str) -> bool:
    """Check if a tmux session exists.

    Args:
        session_name: Session name.

    Returns:
        True if session exists.
    """
    try:
        result = run_tmux(["has-session", "-t", session_name], check=False)
        return result.returncode == 0
    except TmuxError:
        return False


def create_session(
    session_name: str,
    start_dir: Path | None = None,
    window_name: str | None = None,
    detached: bool = True,
) -> None:
    """Create a new tmux session.

    Args:
        session_name: Session name.
        start_dir: Starting directory.
        window_name: Name for the first window.
        detached: Create session in detached mode.
    """
    args = ["new-session", "-s", session_name]

    if detached:
        args.append("-d")

    if window_name:
        args.extend(["-n", window_name])

    if start_dir:
        args.extend(["-c", str(start_dir)])

    run_tmux(args)


def create_window(
    session_name: str,
    window_name: str,
    start_dir: Path | None = None,
    command: str | None = None,
    shell_command: str | None = None,
) -> None:
    """Create a new window in a tmux session.

    Args:
        session_name: Session name.
        window_name: Window name.
        start_dir: Starting directory.
        command: Optional command to send after window creation (via send_keys).
        shell_command: Optional shell command to run BEFORE shell starts.
            This runs as the window's initial command, then execs into shell.
    """
    args = ["new-window", "-t", session_name, "-n", window_name]

    if start_dir:
        args.extend(["-c", str(start_dir)])

    # If shell_command specified, run it then exec into shell
    # Use /bin/sh to avoid loading user's rc files before our command runs
    # Pass PATH explicitly so commands like mise can be found
    if shell_command:
        shell = os.environ.get("SHELL", "/bin/bash")
        user_path = os.environ.get("PATH", "/usr/bin:/bin")
        args.extend(["/bin/sh", "-c", f"export PATH='{user_path}'; {shell_command}; exec {shell}"])

    run_tmux(args)

    # Send command if specified (after shell starts)
    if command:
        send_keys(session_name, window_name, command)


def send_keys(
    session_name: str,
    window_name: str,
    keys: str,
    enter: bool = True,
) -> None:
    """Send keys to a tmux window.

    Args:
        session_name: Session name.
        window_name: Window name.
        keys: Keys/command to send.
        enter: Press enter after keys.
    """
    target = f"{session_name}:{window_name}"
    args = ["send-keys", "-t", target, keys]

    if enter:
        args.append("Enter")

    run_tmux(args)


def attach_session(session_name: str) -> None:
    """Attach to a tmux session.

    This replaces the current process with tmux attach.

    Args:
        session_name: Session name.
    """
    # Use os.execvp to replace current process
    os.execvp("tmux", ["tmux", "attach-session", "-t", session_name])


def switch_client(session_name: str) -> None:
    """Switch tmux client to a session.

    Use this when already inside tmux.

    Args:
        session_name: Session name.
    """
    run_tmux(["switch-client", "-t", session_name])


def is_inside_tmux() -> bool:
    """Check if we're running inside a tmux session."""
    return os.environ.get("TMUX") is not None


def kill_session(session_name: str) -> None:
    """Kill a tmux session.

    Args:
        session_name: Session name.
    """
    run_tmux(["kill-session", "-t", session_name])


def list_windows(session_name: str) -> list[str]:
    """List windows in a session.

    Args:
        session_name: Session name.

    Returns:
        List of window names.
    """
    try:
        result = run_tmux([
            "list-windows",
            "-t", session_name,
            "-F", "#{window_name}",
        ])
        return [w.strip() for w in result.stdout.strip().split("\n") if w.strip()]
    except TmuxError:
        return []


def window_exists(session_name: str, window_name: str) -> bool:
    """Check if a window exists in a session.

    Args:
        session_name: Session name.
        window_name: Window name.

    Returns:
        True if window exists.
    """
    windows = list_windows(session_name)
    return window_name in windows


def select_window(session_name: str, window_name: str) -> None:
    """Select a window in a session.

    Args:
        session_name: Session name.
        window_name: Window name.
    """
    target = f"{session_name}:{window_name}"
    run_tmux(["select-window", "-t", target])


def split_window(
    session_name: str,
    window_name: str,
    horizontal: bool = True,
    percentage: int = 50,
    start_dir: Path | None = None,
) -> None:
    """Split a window into panes.

    Args:
        session_name: Session name.
        window_name: Window name.
        horizontal: If True, split horizontally (side by side). If False, split vertically.
        percentage: Size of new pane as percentage.
        start_dir: Starting directory for new pane.
    """
    target = f"{session_name}:{window_name}"
    args = ["split-window", "-t", target]

    if horizontal:
        args.append("-h")
    else:
        args.append("-v")

    args.extend(["-p", str(percentage)])

    if start_dir:
        args.extend(["-c", str(start_dir)])

    run_tmux(args)


def send_keys_to_pane(
    session_name: str,
    window_name: str,
    pane_index: int,
    keys: str,
    enter: bool = True,
) -> None:
    """Send keys to a specific pane in a tmux window.

    Args:
        session_name: Session name.
        window_name: Window name.
        pane_index: Pane index (0-based).
        keys: Keys/command to send.
        enter: Press enter after keys.
    """
    target = f"{session_name}:{window_name}.{pane_index}"
    args = ["send-keys", "-t", target, keys]

    if enter:
        args.append("Enter")

    run_tmux(args)


def select_pane(
    session_name: str,
    window_name: str,
    pane_index: int,
) -> None:
    """Select a specific pane in a window.

    Args:
        session_name: Session name.
        window_name: Window name.
        pane_index: Pane index (0-based).
    """
    target = f"{session_name}:{window_name}.{pane_index}"
    run_tmux(["select-pane", "-t", target])
