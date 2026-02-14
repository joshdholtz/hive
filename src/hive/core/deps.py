"""External dependency checks."""

import shutil
import subprocess
from dataclasses import dataclass


@dataclass
class DepCheck:
    """Result of a dependency check."""

    name: str
    available: bool
    version: str | None = None
    error: str | None = None


def check_git() -> DepCheck:
    """Check if git is installed and get version."""
    path = shutil.which("git")
    if not path:
        return DepCheck(name="git", available=False, error="git not found in PATH")

    try:
        result = subprocess.run(
            ["git", "--version"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode == 0:
            # Output is like "git version 2.39.0"
            version = result.stdout.strip().replace("git version ", "")
            return DepCheck(name="git", available=True, version=version)
        return DepCheck(
            name="git",
            available=False,
            error=f"git --version failed: {result.stderr.strip()}",
        )
    except subprocess.TimeoutExpired:
        return DepCheck(name="git", available=False, error="git --version timed out")
    except Exception as e:
        return DepCheck(name="git", available=False, error=str(e))


def check_tmux() -> DepCheck:
    """Check if tmux is installed and get version."""
    path = shutil.which("tmux")
    if not path:
        return DepCheck(name="tmux", available=False, error="tmux not found in PATH")

    try:
        result = subprocess.run(
            ["tmux", "-V"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode == 0:
            # Output is like "tmux 3.3a"
            version = result.stdout.strip().replace("tmux ", "")
            return DepCheck(name="tmux", available=True, version=version)
        return DepCheck(
            name="tmux",
            available=False,
            error=f"tmux -V failed: {result.stderr.strip()}",
        )
    except subprocess.TimeoutExpired:
        return DepCheck(name="tmux", available=False, error="tmux -V timed out")
    except Exception as e:
        return DepCheck(name="tmux", available=False, error=str(e))


def check_gh() -> DepCheck:
    """Check if gh CLI is installed and get version."""
    path = shutil.which("gh")
    if not path:
        return DepCheck(name="gh", available=False, error="gh not found in PATH")

    try:
        result = subprocess.run(
            ["gh", "--version"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode == 0:
            # Output is like "gh version 2.40.0 (2023-12-13)"
            first_line = result.stdout.strip().split("\n")[0]
            version = first_line.replace("gh version ", "").split(" ")[0]
            return DepCheck(name="gh", available=True, version=version)
        return DepCheck(
            name="gh",
            available=False,
            error=f"gh --version failed: {result.stderr.strip()}",
        )
    except subprocess.TimeoutExpired:
        return DepCheck(name="gh", available=False, error="gh --version timed out")
    except Exception as e:
        return DepCheck(name="gh", available=False, error=str(e))


def check_gh_auth() -> DepCheck:
    """Check if gh CLI is authenticated."""
    try:
        result = subprocess.run(
            ["gh", "auth", "status"],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode == 0:
            return DepCheck(name="gh auth", available=True)
        # gh auth status returns non-zero if not logged in
        error_msg = result.stderr.strip() or result.stdout.strip()
        return DepCheck(
            name="gh auth",
            available=False,
            error=error_msg or "Not authenticated. Run: gh auth login",
        )
    except subprocess.TimeoutExpired:
        return DepCheck(name="gh auth", available=False, error="gh auth status timed out")
    except FileNotFoundError:
        return DepCheck(name="gh auth", available=False, error="gh not found")
    except Exception as e:
        return DepCheck(name="gh auth", available=False, error=str(e))


def check_all_deps() -> list[DepCheck]:
    """Check all external dependencies."""
    return [
        check_git(),
        check_tmux(),
        check_gh(),
        check_gh_auth(),
    ]
