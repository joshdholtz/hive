"""GitHub CLI operations."""

import json
import subprocess
from pathlib import Path
from typing import Any

from hive.core.exceptions import GhError


def run_gh(
    args: list[str],
    cwd: Path | None = None,
    check: bool = True,
    capture_output: bool = True,
) -> subprocess.CompletedProcess[str]:
    """Run a gh command safely.

    Args:
        args: gh command arguments (without 'gh' prefix).
        cwd: Working directory.
        check: Raise exception on non-zero exit.
        capture_output: Capture stdout/stderr.

    Returns:
        Completed process result.

    Raises:
        GhError: If gh command fails and check=True.
    """
    cmd = ["gh"] + args
    try:
        result = subprocess.run(
            cmd,
            cwd=cwd,
            capture_output=capture_output,
            text=True,
            timeout=300,  # 5 minute timeout
        )
        if check and result.returncode != 0:
            error_msg = result.stderr.strip() or result.stdout.strip()
            raise GhError(f"gh {' '.join(args[:2])} failed: {error_msg}")
        return result
    except subprocess.TimeoutExpired as e:
        raise GhError(f"gh {' '.join(args[:2])} timed out") from e
    except FileNotFoundError as e:
        raise GhError("gh CLI not found in PATH. Install from: https://cli.github.com") from e


def check_gh_auth() -> bool:
    """Check if gh CLI is authenticated.

    Returns:
        True if authenticated.
    """
    try:
        result = run_gh(["auth", "status"], check=False)
        return result.returncode == 0
    except GhError:
        return False


def list_org_repos(
    org: str,
    visibility: str = "all",
    limit: int = 1000,
) -> list[dict[str, Any]]:
    """List repositories in an organization.

    Args:
        org: GitHub organization name.
        visibility: Filter by visibility (all, public, private, internal).
        limit: Maximum number of repos to fetch.

    Returns:
        List of repo info dicts with name, sshUrl, url, isArchived, isFork.

    Raises:
        GhError: If gh command fails.
    """
    args = [
        "repo", "list", org,
        "--limit", str(limit),
        "--json", "name,sshUrl,url,isArchived,isFork",
    ]

    if visibility != "all":
        args.extend(["--visibility", visibility])

    result = run_gh(args)

    try:
        repos = json.loads(result.stdout)
        return repos
    except json.JSONDecodeError as e:
        raise GhError(f"Failed to parse gh output: {e}") from e


def get_pr_info(repo_path: Path, branch: str) -> dict[str, Any] | None:
    """Get PR information for a branch.

    Args:
        repo_path: Path to git repository.
        branch: Branch name.

    Returns:
        PR info dict or None if no PR exists.
    """
    try:
        result = run_gh(
            ["pr", "view", "--head", branch, "--json", "url,number,state"],
            cwd=repo_path,
            check=False,
        )
        if result.returncode != 0:
            return None

        data = json.loads(result.stdout)
        return {
            "url": data.get("url"),
            "number": data.get("number"),
            "state": data.get("state"),
        }
    except (GhError, json.JSONDecodeError):
        return None


def create_pr(
    repo_path: Path,
    head: str,
    base: str,
    title: str,
    body: str = "",
    draft: bool = True,
) -> dict[str, Any]:
    """Create a pull request.

    Args:
        repo_path: Path to git repository.
        head: Head branch.
        base: Base branch.
        title: PR title.
        body: PR body.
        draft: Create as draft PR.

    Returns:
        Created PR info dict.

    Raises:
        GhError: If PR creation fails.
    """
    args = [
        "pr", "create",
        "--head", head,
        "--base", base,
        "--title", title,
        "--body", body,
    ]
    if draft:
        args.append("--draft")

    result = run_gh(args, cwd=repo_path)

    # gh pr create outputs the PR URL on success
    pr_url = result.stdout.strip()

    # Get PR details
    pr_info = get_pr_info(repo_path, head)
    if pr_info:
        return pr_info

    # Fallback if we can't get details
    return {"url": pr_url, "number": None, "state": "OPEN"}


def push_branch(repo_path: Path, branch: str, set_upstream: bool = True) -> None:
    """Push a branch to origin.

    Args:
        repo_path: Path to git repository.
        branch: Branch name to push.
        set_upstream: Set upstream tracking.
    """
    from hive.core.git import run_git

    args = ["push", "origin", branch]
    if set_upstream:
        args.insert(1, "-u")

    run_git(args, cwd=repo_path)


def create_issue(
    repo: str,
    title: str,
    body: str = "",
) -> dict[str, Any]:
    """Create a GitHub issue.

    Args:
        repo: Repository in owner/name format.
        title: Issue title.
        body: Issue body.

    Returns:
        Created issue info dict with number and url.

    Raises:
        GhError: If issue creation fails.
    """
    args = [
        "issue", "create",
        "-R", repo,
        "--title", title,
        "--body", body,
    ]

    result = run_gh(args)

    # gh issue create outputs the issue URL on success
    issue_url = result.stdout.strip()

    # Extract issue number from URL
    # URL format: https://github.com/owner/repo/issues/123
    try:
        issue_number = int(issue_url.rstrip("/").split("/")[-1])
    except (ValueError, IndexError):
        issue_number = None

    return {
        "url": issue_url,
        "number": issue_number,
        "repo": repo,
    }


def get_issue_info(repo: str, number: int) -> dict[str, Any] | None:
    """Get issue information.

    Args:
        repo: Repository in owner/name format.
        number: Issue number.

    Returns:
        Issue info dict or None if not found.
    """
    try:
        result = run_gh(
            ["issue", "view", str(number), "-R", repo, "--json", "url,number,title,state,body"],
            check=False,
        )
        if result.returncode != 0:
            return None

        return json.loads(result.stdout)
    except (GhError, json.JSONDecodeError):
        return None


def add_issue_comment(repo: str, number: int, body: str) -> dict[str, Any]:
    """Add a comment to an issue.

    Args:
        repo: Repository in owner/name format.
        number: Issue number.
        body: Comment body.

    Returns:
        Comment info dict with url.

    Raises:
        GhError: If comment creation fails.
    """
    args = [
        "issue", "comment", str(number),
        "-R", repo,
        "--body", body,
    ]

    result = run_gh(args)

    # gh issue comment outputs the comment URL on success
    comment_url = result.stdout.strip()

    return {"url": comment_url}


def update_issue_body(repo: str, number: int, body: str) -> None:
    """Update an issue's body.

    Args:
        repo: Repository in owner/name format.
        number: Issue number.
        body: New issue body.

    Raises:
        GhError: If update fails.
    """
    args = [
        "issue", "edit", str(number),
        "-R", repo,
        "--body", body,
    ]

    run_gh(args)


def list_issue_comments(repo: str, number: int) -> list[dict[str, Any]]:
    """List comments on an issue.

    Args:
        repo: Repository in owner/name format.
        number: Issue number.

    Returns:
        List of comment dicts with id, body, url.
    """
    try:
        # Use gh api to get comments with IDs
        result = run_gh(
            ["api", f"repos/{repo}/issues/{number}/comments", "--jq", ".[] | {id, body, url: .html_url}"],
            check=False,
        )
        if result.returncode != 0:
            return []

        # Parse JSONL output (one JSON object per line)
        comments = []
        for line in result.stdout.strip().split("\n"):
            if line.strip():
                try:
                    comments.append(json.loads(line))
                except json.JSONDecodeError:
                    continue
        return comments
    except GhError:
        return []


def update_issue_comment(repo: str, comment_id: int, body: str) -> None:
    """Update a comment on an issue.

    Args:
        repo: Repository in owner/name format.
        comment_id: Comment ID.
        body: New comment body.

    Raises:
        GhError: If update fails.
    """
    args = [
        "api", "-X", "PATCH",
        f"repos/{repo}/issues/comments/{comment_id}",
        "-f", f"body={body}",
    ]

    run_gh(args)
