"""Issue providers for hive."""

from hive.providers.base import Issue, IssueProvider, ProviderConfig
from hive.providers.github import GitHubProvider
from hive.providers.linear import LinearProvider

__all__ = [
    "Issue",
    "IssueProvider",
    "ProviderConfig",
    "GitHubProvider",
    "LinearProvider",
    "get_provider",
]


def get_provider(config: ProviderConfig) -> IssueProvider:
    """Get an issue provider based on configuration.

    Args:
        config: Provider configuration from workspace.yaml

    Returns:
        Configured issue provider.

    Raises:
        ValueError: If provider type is not supported.
    """
    if config.type == "github":
        return GitHubProvider(config)
    elif config.type == "linear":
        return LinearProvider(config)
    else:
        raise ValueError(f"Unknown provider type: {config.type}")
