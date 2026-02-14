"""GitHub issue provider using gh CLI."""

import json
import subprocess
from pathlib import Path

from hive.providers.base import Issue, IssueProvider, ProviderConfig


class GitHubProvider(IssueProvider):
    """GitHub issue provider using gh CLI."""

    @property
    def name(self) -> str:
        return "github"

    def _get_repo(self) -> str | None:
        """Get the GitHub repo (owner/repo format)."""
        if self.config.repo:
            return self.config.repo

        # Try to detect from git remote
        try:
            result = subprocess.run(
                ["git", "remote", "get-url", "origin"],
                capture_output=True,
                text=True,
                timeout=10,
            )
            if result.returncode == 0:
                url = result.stdout.strip()
                return self._parse_github_url(url)
        except Exception:
            pass

        return None

    def _parse_github_url(self, url: str) -> str | None:
        """Parse GitHub URL to owner/repo format."""
        # SSH: git@github.com:owner/repo.git
        if url.startswith("git@github.com:"):
            path = url[15:]
            if path.endswith(".git"):
                path = path[:-4]
            return path

        # HTTPS: https://github.com/owner/repo.git
        if "github.com/" in url:
            parts = url.split("github.com/")
            if len(parts) > 1:
                path = parts[1]
                if path.endswith(".git"):
                    path = path[:-4]
                return path

        return None

    def _run_gh(self, args: list[str]) -> subprocess.CompletedProcess:
        """Run a gh CLI command."""
        cmd = ["gh"] + args
        return subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=30,
        )

    def is_authenticated(self) -> bool:
        """Check if gh CLI is authenticated."""
        try:
            result = self._run_gh(["auth", "status"])
            return result.returncode == 0
        except Exception:
            return False

    def list_issues(
        self,
        state: str = "open",
        limit: int = 50,
        labels: list[str] | None = None,
    ) -> list[Issue]:
        """List issues from GitHub."""
        repo = self._get_repo()
        if not repo:
            return []

        args = [
            "issue", "list",
            "--repo", repo,
            "--state", state,
            "--limit", str(limit),
            "--json", "number,title,body,url,state,labels",
        ]

        if labels:
            for label in labels:
                args.extend(["--label", label])

        try:
            result = self._run_gh(args)
            if result.returncode != 0:
                return []

            data = json.loads(result.stdout)
            issues = []
            for item in data:
                label_names = [l.get("name", "") for l in item.get("labels", [])]
                issues.append(Issue(
                    id=str(item["number"]),
                    number=item["number"],
                    title=item["title"],
                    body=item.get("body", ""),
                    url=item.get("url", ""),
                    state=item.get("state", "open").lower(),
                    labels=label_names,
                ))
            return issues
        except Exception:
            return []

    def get_issue(self, issue_id: str | int) -> Issue | None:
        """Get a single issue by number."""
        repo = self._get_repo()
        if not repo:
            return None

        args = [
            "issue", "view",
            str(issue_id),
            "--repo", repo,
            "--json", "number,title,body,url,state,labels",
        ]

        try:
            result = self._run_gh(args)
            if result.returncode != 0:
                return None

            item = json.loads(result.stdout)
            label_names = [l.get("name", "") for l in item.get("labels", [])]
            return Issue(
                id=str(item["number"]),
                number=item["number"],
                title=item["title"],
                body=item.get("body", ""),
                url=item.get("url", ""),
                state=item.get("state", "open").lower(),
                labels=label_names,
            )
        except Exception:
            return None

    def create_issue(
        self,
        title: str,
        body: str = "",
        labels: list[str] | None = None,
    ) -> Issue:
        """Create a new GitHub issue."""
        repo = self._get_repo()
        if not repo:
            raise RuntimeError("No GitHub repo configured")

        args = [
            "issue", "create",
            "--repo", repo,
            "--title", title,
        ]

        if body:
            args.extend(["--body", body])

        if labels:
            for label in labels:
                args.extend(["--label", label])

        result = self._run_gh(args)
        if result.returncode != 0:
            raise RuntimeError(f"Failed to create issue: {result.stderr}")

        # Parse the URL from output to get issue number
        # Output is like: https://github.com/owner/repo/issues/123
        url = result.stdout.strip()
        number = int(url.split("/")[-1])

        return Issue(
            id=str(number),
            number=number,
            title=title,
            body=body,
            url=url,
            state="open",
            labels=labels or [],
        )

    def update_issue(
        self,
        issue_id: str | int,
        title: str | None = None,
        body: str | None = None,
        state: str | None = None,
        labels: list[str] | None = None,
    ) -> Issue:
        """Update an existing GitHub issue."""
        repo = self._get_repo()
        if not repo:
            raise RuntimeError("No GitHub repo configured")

        args = [
            "issue", "edit",
            str(issue_id),
            "--repo", repo,
        ]

        if title:
            args.extend(["--title", title])

        if body:
            args.extend(["--body", body])

        result = self._run_gh(args)
        if result.returncode != 0:
            raise RuntimeError(f"Failed to update issue: {result.stderr}")

        # Handle state change separately
        if state:
            if state == "closed":
                self._run_gh(["issue", "close", str(issue_id), "--repo", repo])
            elif state == "open":
                self._run_gh(["issue", "reopen", str(issue_id), "--repo", repo])

        # Fetch and return updated issue
        updated = self.get_issue(issue_id)
        if not updated:
            raise RuntimeError("Failed to fetch updated issue")
        return updated

    def add_comment(self, issue_id: str | int, body: str) -> None:
        """Add a comment to a GitHub issue."""
        repo = self._get_repo()
        if not repo:
            raise RuntimeError("No GitHub repo configured")

        args = [
            "issue", "comment",
            str(issue_id),
            "--repo", repo,
            "--body", body,
        ]

        result = self._run_gh(args)
        if result.returncode != 0:
            raise RuntimeError(f"Failed to add comment: {result.stderr}")
