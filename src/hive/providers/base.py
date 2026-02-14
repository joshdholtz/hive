"""Base classes for issue providers."""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any


@dataclass
class ProviderConfig:
    """Configuration for an issue provider."""

    type: str  # 'github' or 'linear'
    repo: str | None = None  # GitHub: owner/repo, Linear: team key
    project: str | None = None  # Linear: project name/id
    api_key_env: str | None = None  # Env var name for API key

    @classmethod
    def from_dict(cls, data: dict[str, Any]) -> "ProviderConfig":
        """Create config from dictionary."""
        return cls(
            type=data.get("type", "github"),
            repo=data.get("repo"),
            project=data.get("project"),
            api_key_env=data.get("api_key_env"),
        )

    @classmethod
    def default_github(cls) -> "ProviderConfig":
        """Create default GitHub config."""
        return cls(type="github")


@dataclass
class Issue:
    """An issue from any provider."""

    id: str  # Provider-specific ID
    number: int | str  # Display number/identifier
    title: str
    body: str = ""
    url: str = ""
    state: str = "open"  # open, closed, etc.
    labels: list[str] = field(default_factory=list)
    repo: str | None = None  # For multi-repo issues

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary."""
        return {
            "id": self.id,
            "number": self.number,
            "title": self.title,
            "body": self.body,
            "url": self.url,
            "state": self.state,
            "labels": self.labels,
            "repo": self.repo,
        }


class IssueProvider(ABC):
    """Abstract base class for issue providers."""

    def __init__(self, config: ProviderConfig):
        """Initialize the provider.

        Args:
            config: Provider configuration.
        """
        self.config = config

    @property
    @abstractmethod
    def name(self) -> str:
        """Return the provider name."""
        ...

    @abstractmethod
    def list_issues(
        self,
        state: str = "open",
        limit: int = 50,
        labels: list[str] | None = None,
    ) -> list[Issue]:
        """List issues.

        Args:
            state: Issue state filter ('open', 'closed', 'all').
            limit: Maximum number of issues to return.
            labels: Filter by labels.

        Returns:
            List of issues.
        """
        ...

    @abstractmethod
    def get_issue(self, issue_id: str | int) -> Issue | None:
        """Get a single issue by ID or number.

        Args:
            issue_id: Issue ID or number.

        Returns:
            Issue if found, None otherwise.
        """
        ...

    @abstractmethod
    def create_issue(
        self,
        title: str,
        body: str = "",
        labels: list[str] | None = None,
    ) -> Issue:
        """Create a new issue.

        Args:
            title: Issue title.
            body: Issue body/description.
            labels: Labels to apply.

        Returns:
            Created issue.
        """
        ...

    @abstractmethod
    def update_issue(
        self,
        issue_id: str | int,
        title: str | None = None,
        body: str | None = None,
        state: str | None = None,
        labels: list[str] | None = None,
    ) -> Issue:
        """Update an existing issue.

        Args:
            issue_id: Issue ID or number.
            title: New title (optional).
            body: New body (optional).
            state: New state (optional).
            labels: New labels (optional).

        Returns:
            Updated issue.
        """
        ...

    @abstractmethod
    def add_comment(self, issue_id: str | int, body: str) -> None:
        """Add a comment to an issue.

        Args:
            issue_id: Issue ID or number.
            body: Comment body.
        """
        ...

    def is_authenticated(self) -> bool:
        """Check if the provider is authenticated.

        Returns:
            True if authenticated, False otherwise.
        """
        return True  # Override in subclasses
