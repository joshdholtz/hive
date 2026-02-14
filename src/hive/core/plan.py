"""Plan loading and management."""

import re
from datetime import date
from pathlib import Path
from typing import Any

import yaml

from hive.core.exceptions import PlanError

SUPPORTED_PLAN_VERSIONS = {1}


def load_plan(plan_path: Path) -> dict[str, Any]:
    """Load and validate plan.yaml.

    Args:
        plan_path: Path to plan.yaml file.

    Returns:
        Parsed plan configuration.

    Raises:
        PlanError: If file is missing or invalid.
    """
    if not plan_path.exists():
        raise PlanError(f"Plan file not found: {plan_path}")

    try:
        with open(plan_path) as f:
            config = yaml.safe_load(f)
    except yaml.YAMLError as e:
        raise PlanError(f"Invalid YAML in plan.yaml: {e}") from e

    if config is None:
        raise PlanError("plan.yaml is empty")

    if not isinstance(config, dict):
        raise PlanError("plan.yaml must be a YAML mapping")

    errors = validate_plan(config)
    if errors:
        raise PlanError("Invalid plan.yaml:\n  - " + "\n  - ".join(errors))

    return config


def validate_plan(config: dict[str, Any]) -> list[str]:
    """Validate plan configuration.

    Args:
        config: Parsed plan configuration.

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
        elif version not in SUPPORTED_PLAN_VERSIONS:
            errors.append(
                f"Unsupported schema_version: {version}. "
                f"Supported versions: {sorted(SUPPORTED_PLAN_VERSIONS)}"
            )

    # Check id
    if "id" not in config:
        errors.append("Missing required field: id")
    elif not isinstance(config["id"], str):
        errors.append("id must be a string")

    # Check title
    if "title" not in config:
        errors.append("Missing required field: title")
    elif not isinstance(config["title"], str):
        errors.append("title must be a string")

    # Check branch
    if "branch" not in config:
        errors.append("Missing required field: branch")
    elif not isinstance(config["branch"], str):
        errors.append("branch must be a string")

    # Check repos (optional but must be valid if present)
    if "repos" in config:
        repos = config["repos"]
        if not isinstance(repos, dict):
            errors.append(f"repos must be a mapping, got {type(repos).__name__}")
        else:
            for key, repo_config in repos.items():
                if not isinstance(key, str):
                    errors.append(f"Repo key must be a string, got {type(key).__name__}")
                if not isinstance(repo_config, dict):
                    errors.append(f"Repo config for '{key}' must be a mapping")
                else:
                    # Validate repo config fields
                    if "tasks" in repo_config:
                        tasks = repo_config["tasks"]
                        if not isinstance(tasks, list):
                            errors.append(f"repos.{key}.tasks must be a list")
                        elif not all(isinstance(t, str) for t in tasks):
                            errors.append(f"repos.{key}.tasks must be a list of strings")

    # Check tmux config (optional)
    if "tmux" in config:
        tmux = config["tmux"]
        if not isinstance(tmux, dict):
            errors.append(f"tmux must be a mapping, got {type(tmux).__name__}")

    # Check pr config (optional)
    if "pr" in config:
        pr = config["pr"]
        if not isinstance(pr, dict):
            errors.append(f"pr must be a mapping, got {type(pr).__name__}")

    # Check issue config (optional)
    if "issue" in config:
        issue = config["issue"]
        if not isinstance(issue, dict):
            errors.append(f"issue must be a mapping, got {type(issue).__name__}")
        else:
            # Validate issue fields
            if "sync_mode" in issue:
                sync_mode = issue["sync_mode"]
                if sync_mode not in ("comment", "body"):
                    errors.append(f"issue.sync_mode must be 'comment' or 'body', got '{sync_mode}'")
            if "repo" in issue:
                repo = issue["repo"]
                if not isinstance(repo, str):
                    errors.append(f"issue.repo must be a string, got {type(repo).__name__}")
                elif "/" not in repo:
                    errors.append(f"issue.repo must be in 'owner/name' format, got '{repo}'")

    return errors


def generate_task_id(title: str, task_date: date | None = None) -> str:
    """Generate a task ID from title.

    Format: YYYY-MM-DD-slugified-title

    Args:
        title: Task title.
        task_date: Date to use (default: today).

    Returns:
        Generated task ID.
    """
    if task_date is None:
        task_date = date.today()

    # Slugify the title
    slug = slugify(title)

    return f"{task_date.isoformat()}-{slug}"


def slugify(text: str) -> str:
    """Convert text to a URL-safe slug.

    Args:
        text: Text to slugify.

    Returns:
        Slugified text.
    """
    # Convert to lowercase
    slug = text.lower()

    # Replace spaces and underscores with hyphens
    slug = re.sub(r"[\s_]+", "-", slug)

    # Remove non-alphanumeric characters (except hyphens)
    slug = re.sub(r"[^a-z0-9-]", "", slug)

    # Collapse multiple hyphens
    slug = re.sub(r"-+", "-", slug)

    # Strip leading/trailing hyphens
    slug = slug.strip("-")

    # Limit length
    if len(slug) > 50:
        slug = slug[:50].rstrip("-")

    return slug


def create_plan(
    tasks_dir: Path,
    task_id: str,
    title: str,
    branch: str | None = None,
    repos: list[str] | None = None,
) -> Path:
    """Create a new plan.yaml file.

    Args:
        tasks_dir: Path to .tasks directory.
        task_id: Task identifier.
        title: Task title.
        branch: Branch name (default: feat/<slugified-title>).
        repos: Optional list of repo keys to include.

    Returns:
        Path to created plan.yaml.

    Raises:
        PlanError: If task directory already exists.
    """
    task_dir = tasks_dir / task_id

    if task_dir.exists():
        raise PlanError(f"Task already exists: {task_id}")

    # Create task directory
    task_dir.mkdir(parents=True)

    # Generate branch name if not provided
    if branch is None:
        slug = slugify(title)
        branch = f"feat/{slug}"

    # Build plan config
    plan: dict[str, Any] = {
        "schema_version": 1,
        "id": task_id,
        "title": title,
        "branch": branch,
    }

    # Add repos section if specified
    if repos:
        plan["repos"] = {}
        for repo_key in repos:
            plan["repos"][repo_key] = {
                "base": "main",
                "tasks": [
                    "TODO: Add tasks for this repo",
                ],
            }

    # Add default tmux config
    plan["tmux"] = {
        "enabled": True,
        "start_agent": False,
        "windows": {
            "include_concierge": True,
            "include_dashboard": True,
        },
    }

    # Add default pr config
    plan["pr"] = {
        "enabled": True,
        "create_on_sync": True,
        "draft": True,
    }

    # Write plan.yaml
    plan_path = task_dir / "plan.yaml"
    with open(plan_path, "w") as f:
        yaml.safe_dump(plan, f, default_flow_style=False, sort_keys=False)

    return plan_path


def get_plan_title(plan: dict[str, Any]) -> str:
    """Get title from plan config."""
    return plan.get("title", "Untitled")


def get_plan_branch(plan: dict[str, Any]) -> str:
    """Get branch from plan config."""
    return plan.get("branch", "main")


def get_plan_repos(plan: dict[str, Any]) -> dict[str, Any]:
    """Get repos config from plan."""
    return plan.get("repos", {})


def get_repo_base_branch(plan: dict[str, Any], repo_key: str, default: str = "main") -> str:
    """Get base branch for a repo from plan config."""
    repos = get_plan_repos(plan)
    repo_config = repos.get(repo_key, {})
    return repo_config.get("base", default)


def get_repo_tasks(plan: dict[str, Any], repo_key: str) -> list[str]:
    """Get tasks list for a repo from plan config."""
    repos = get_plan_repos(plan)
    repo_config = repos.get(repo_key, {})
    return repo_config.get("tasks", [])


def is_tmux_enabled(plan: dict[str, Any]) -> bool:
    """Check if tmux is enabled in plan."""
    tmux = plan.get("tmux", {})
    return tmux.get("enabled", True)


def is_pr_enabled(plan: dict[str, Any]) -> bool:
    """Check if PR creation is enabled in plan."""
    pr = plan.get("pr", {})
    return pr.get("enabled", True)


def is_pr_draft(plan: dict[str, Any]) -> bool:
    """Check if PRs should be created as drafts."""
    pr = plan.get("pr", {})
    return pr.get("draft", True)


def is_issue_enabled(plan: dict[str, Any]) -> bool:
    """Check if issue sync is enabled in plan."""
    issue = plan.get("issue", {})
    return issue.get("enabled", False)


def get_issue_config(plan: dict[str, Any]) -> dict[str, Any]:
    """Get issue config from plan.

    Returns:
        Issue config dict with defaults applied.
    """
    issue = plan.get("issue", {})
    return {
        "enabled": issue.get("enabled", False),
        "repo": issue.get("repo"),
        "number": issue.get("number"),
        "sync_mode": issue.get("sync_mode", "comment"),
        "title_template": issue.get("title_template", "{id}: {title}"),
        "include_repo_checklist": issue.get("include_repo_checklist", True),
        "include_pr_links": issue.get("include_pr_links", True),
    }


def update_plan_issue_number(plan_path: Path, issue_number: int) -> None:
    """Update issue.number in plan.yaml.

    Args:
        plan_path: Path to plan.yaml.
        issue_number: Issue number to set.
    """
    with open(plan_path) as f:
        plan = yaml.safe_load(f)

    if "issue" not in plan:
        plan["issue"] = {}

    plan["issue"]["number"] = issue_number

    with open(plan_path, "w") as f:
        yaml.safe_dump(plan, f, default_flow_style=False, sort_keys=False)
