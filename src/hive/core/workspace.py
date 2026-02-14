"""Workspace configuration loading and validation."""

from pathlib import Path
from typing import Any

import yaml

from hive.core.exceptions import WorkspaceError
from hive.models.workspace import ProviderConfig, RepoConfig, WorkspaceConfig, WorkspaceDefaults

SUPPORTED_SCHEMA_VERSIONS = {1}


def load_workspace(workspace_path: Path) -> WorkspaceConfig:
    """Load and validate workspace.yaml.

    Args:
        workspace_path: Path to workspace.yaml file.

    Returns:
        Parsed workspace configuration.

    Raises:
        WorkspaceError: If file is missing or invalid.
    """
    if not workspace_path.exists():
        raise WorkspaceError(f"Workspace file not found: {workspace_path}")

    try:
        with open(workspace_path) as f:
            raw_config = yaml.safe_load(f)
    except yaml.YAMLError as e:
        raise WorkspaceError(f"Invalid YAML in workspace.yaml: {e}") from e

    if raw_config is None:
        raise WorkspaceError("workspace.yaml is empty")

    if not isinstance(raw_config, dict):
        raise WorkspaceError("workspace.yaml must be a YAML mapping")

    # Validate the raw config
    errors = validate_workspace(raw_config)
    if errors:
        raise WorkspaceError("Invalid workspace.yaml:\n  - " + "\n  - ".join(errors))

    # Parse into typed config
    return parse_workspace(raw_config, workspace_path.parent)


def validate_workspace(config: dict[str, Any]) -> list[str]:
    """Validate workspace configuration.

    Args:
        config: Parsed workspace configuration.

    Returns:
        List of validation error messages (empty if valid).
    """
    errors: list[str] = []

    # Check schema_version
    if "schema_version" not in config:
        errors.append("Missing required field: schema_version")
    else:
        version = config["schema_version"]
        if not isinstance(version, int):
            errors.append(f"schema_version must be an integer, got {type(version).__name__}")
        elif version not in SUPPORTED_SCHEMA_VERSIONS:
            errors.append(
                f"Unsupported schema_version: {version}. "
                f"Supported versions: {sorted(SUPPORTED_SCHEMA_VERSIONS)}"
            )

    # Check repos
    if "repos" not in config:
        errors.append("Missing required field: repos")
    else:
        repos = config["repos"]
        if not isinstance(repos, dict):
            errors.append(f"repos must be a mapping, got {type(repos).__name__}")
        elif len(repos) == 0:
            errors.append("repos cannot be empty")
        else:
            for key, value in repos.items():
                if not isinstance(key, str):
                    errors.append(f"Repo key must be a string, got {type(key).__name__}")
                if not isinstance(value, str):
                    errors.append(
                        f"Repo path for '{key}' must be a string, got {type(value).__name__}"
                    )

    # Check defaults (optional)
    if "defaults" in config:
        defaults = config["defaults"]
        if not isinstance(defaults, dict):
            errors.append(f"defaults must be a mapping, got {type(defaults).__name__}")
        else:
            valid_default_keys = {
                "base_branch", "tmux", "pr_create",
                "symlink_files", "setup_commands", "issues_repo",
                "claude_yolo",
            }
            for key in defaults:
                if key not in valid_default_keys:
                    errors.append(f"Unknown defaults key: {key}")

            if "base_branch" in defaults and not isinstance(defaults["base_branch"], str):
                errors.append("defaults.base_branch must be a string")
            if "tmux" in defaults and not isinstance(defaults["tmux"], bool):
                errors.append("defaults.tmux must be a boolean")
            if "pr_create" in defaults and not isinstance(defaults["pr_create"], bool):
                errors.append("defaults.pr_create must be a boolean")
            if "issues_repo" in defaults and not isinstance(defaults["issues_repo"], str):
                errors.append("defaults.issues_repo must be a string")
            if "claude_yolo" in defaults and not isinstance(defaults["claude_yolo"], bool):
                errors.append("defaults.claude_yolo must be a boolean")

    # Check provider (optional)
    if "provider" in config:
        provider = config["provider"]
        if not isinstance(provider, dict):
            errors.append(f"provider must be a mapping, got {type(provider).__name__}")
        else:
            valid_provider_keys = {"type", "repo", "project", "api_key_env"}
            for key in provider:
                if key not in valid_provider_keys:
                    errors.append(f"Unknown provider key: {key}")

            if "type" in provider:
                if provider["type"] not in ("github", "linear"):
                    errors.append(f"provider.type must be 'github' or 'linear', got '{provider['type']}'")

    return errors


def parse_workspace(config: dict[str, Any], root: Path) -> WorkspaceConfig:
    """Parse validated config dict into WorkspaceConfig.

    Args:
        config: Validated workspace configuration dict.
        root: Workspace root directory.

    Returns:
        Typed WorkspaceConfig object.
    """
    # Parse defaults
    defaults_raw = config.get("defaults", {})
    defaults = WorkspaceDefaults(
        base_branch=defaults_raw.get("base_branch", "main"),
        tmux=defaults_raw.get("tmux", True),
        pr_create=defaults_raw.get("pr_create", True),
        symlink_files=defaults_raw.get("symlink_files"),
        setup_commands=defaults_raw.get("setup_commands"),
        issues_repo=defaults_raw.get("issues_repo", "hive"),
        claude_yolo=defaults_raw.get("claude_yolo", False),
    )

    # Parse repos
    repos: dict[str, RepoConfig] = {}
    for key, path_str in config["repos"].items():
        resolved_path = resolve_repo_path(root, path_str)
        repos[key] = RepoConfig(key=key, path=resolved_path)

    # Parse provider
    provider_raw = config.get("provider", {})
    provider = ProviderConfig.from_dict(provider_raw)

    return WorkspaceConfig(
        schema_version=config["schema_version"],
        repos=repos,
        defaults=defaults,
        provider=provider,
    )


def resolve_repo_path(root: Path, repo_path: str) -> Path:
    """Resolve a repo path from workspace config.

    Args:
        root: Workspace root directory.
        repo_path: Repo path string (relative or absolute).

    Returns:
        Resolved absolute path to repo.
    """
    path = Path(repo_path)
    if path.is_absolute():
        return path
    return (root / path).resolve()


def create_workspace_yaml(
    workspace_path: Path,
    repos: dict[str, str],
    defaults: dict[str, Any] | None = None,
) -> None:
    """Create a new workspace.yaml file.

    Args:
        workspace_path: Path to write workspace.yaml.
        repos: Mapping of repo key to path.
        defaults: Optional defaults section.
    """
    config: dict[str, Any] = {
        "schema_version": 1,
        "repos": repos,
    }
    if defaults:
        config["defaults"] = defaults

    with open(workspace_path, "w") as f:
        yaml.safe_dump(config, f, default_flow_style=False, sort_keys=False)


def update_workspace_repos(workspace_path: Path, repos: dict[str, str]) -> None:
    """Update repos section in existing workspace.yaml.

    Args:
        workspace_path: Path to workspace.yaml.
        repos: Mapping of repo key to path to add/update.
    """
    if workspace_path.exists():
        with open(workspace_path) as f:
            config = yaml.safe_load(f) or {}
    else:
        config = {"schema_version": 1, "repos": {}}

    if "repos" not in config:
        config["repos"] = {}

    config["repos"].update(repos)

    with open(workspace_path, "w") as f:
        yaml.safe_dump(config, f, default_flow_style=False, sort_keys=False)
