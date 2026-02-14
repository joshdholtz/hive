"""Linear issue provider using GraphQL API."""

import json
import os
import urllib.request
import urllib.error
from typing import Any

from hive.providers.base import Issue, IssueProvider, ProviderConfig

LINEAR_API_URL = "https://api.linear.app/graphql"


class LinearProvider(IssueProvider):
    """Linear issue provider using GraphQL API."""

    def __init__(self, config: ProviderConfig):
        super().__init__(config)
        self._api_key: str | None = None

    @property
    def name(self) -> str:
        return "linear"

    def _get_api_key(self) -> str | None:
        """Get the Linear API key from environment."""
        if self._api_key:
            return self._api_key

        # Check configured env var or default
        env_var = self.config.api_key_env or "LINEAR_API_KEY"
        self._api_key = os.environ.get(env_var)
        return self._api_key

    def _graphql(self, query: str, variables: dict[str, Any] | None = None) -> dict[str, Any]:
        """Execute a GraphQL query against Linear API."""
        api_key = self._get_api_key()
        if not api_key:
            raise RuntimeError("Linear API key not found. Set LINEAR_API_KEY environment variable.")

        payload = {"query": query}
        if variables:
            payload["variables"] = variables

        data = json.dumps(payload).encode("utf-8")

        req = urllib.request.Request(
            LINEAR_API_URL,
            data=data,
            headers={
                "Content-Type": "application/json",
                "Authorization": api_key,
            },
        )

        try:
            with urllib.request.urlopen(req, timeout=30) as response:
                result = json.loads(response.read().decode("utf-8"))
                if "errors" in result:
                    raise RuntimeError(f"Linear API error: {result['errors']}")
                return result.get("data", {})
        except urllib.error.HTTPError as e:
            raise RuntimeError(f"Linear API HTTP error: {e.code} {e.reason}")
        except urllib.error.URLError as e:
            raise RuntimeError(f"Linear API connection error: {e.reason}")

    def _get_team_id(self) -> str | None:
        """Get the team ID from config or by querying."""
        if not self.config.repo:
            # Get first team
            query = """
            query {
                teams {
                    nodes {
                        id
                        key
                    }
                }
            }
            """
            try:
                result = self._graphql(query)
                teams = result.get("teams", {}).get("nodes", [])
                if teams:
                    return teams[0]["id"]
            except Exception:
                pass
            return None

        # Look up team by key
        query = """
        query($key: String!) {
            team(key: $key) {
                id
            }
        }
        """
        try:
            result = self._graphql(query, {"key": self.config.repo})
            return result.get("team", {}).get("id")
        except Exception:
            return None

    def _get_project_id(self) -> str | None:
        """Get the project ID if configured."""
        if not self.config.project:
            return None

        query = """
        query($filter: ProjectFilter) {
            projects(filter: $filter) {
                nodes {
                    id
                    name
                }
            }
        }
        """
        try:
            result = self._graphql(query, {
                "filter": {"name": {"eq": self.config.project}}
            })
            projects = result.get("projects", {}).get("nodes", [])
            if projects:
                return projects[0]["id"]
        except Exception:
            pass
        return None

    def is_authenticated(self) -> bool:
        """Check if Linear API key is configured and valid."""
        api_key = self._get_api_key()
        if not api_key:
            return False

        try:
            query = "query { viewer { id } }"
            self._graphql(query)
            return True
        except Exception:
            return False

    def list_issues(
        self,
        state: str = "open",
        limit: int = 50,
        labels: list[str] | None = None,
    ) -> list[Issue]:
        """List issues from Linear."""
        team_id = self._get_team_id()

        # Build filter
        filter_parts = []

        if team_id:
            filter_parts.append(f'team: {{ id: {{ eq: "{team_id}" }} }}')

        if state == "open":
            filter_parts.append('state: { type: { nin: ["completed", "canceled"] } }')
        elif state == "closed":
            filter_parts.append('state: { type: { in: ["completed", "canceled"] } }')

        if labels:
            label_filter = ", ".join([f'"{l}"' for l in labels])
            filter_parts.append(f'labels: {{ name: {{ in: [{label_filter}] }} }}')

        project_id = self._get_project_id()
        if project_id:
            filter_parts.append(f'project: {{ id: {{ eq: "{project_id}" }} }}')

        filter_str = ", ".join(filter_parts)

        query = f"""
        query {{
            issues(first: {limit}, filter: {{ {filter_str} }}) {{
                nodes {{
                    id
                    identifier
                    title
                    description
                    url
                    state {{
                        name
                        type
                    }}
                    labels {{
                        nodes {{
                            name
                        }}
                    }}
                }}
            }}
        }}
        """

        try:
            result = self._graphql(query)
            issues = []
            for item in result.get("issues", {}).get("nodes", []):
                state_type = item.get("state", {}).get("type", "")
                issue_state = "closed" if state_type in ("completed", "canceled") else "open"
                label_names = [l["name"] for l in item.get("labels", {}).get("nodes", [])]

                issues.append(Issue(
                    id=item["id"],
                    number=item["identifier"],  # e.g., "ENG-123"
                    title=item["title"],
                    body=item.get("description", "") or "",
                    url=item.get("url", ""),
                    state=issue_state,
                    labels=label_names,
                ))
            return issues
        except Exception:
            return []

    def get_issue(self, issue_id: str | int) -> Issue | None:
        """Get a single issue by identifier (e.g., 'ENG-123') or ID."""
        # Try by identifier first
        query = """
        query($id: String!) {
            issue(id: $id) {
                id
                identifier
                title
                description
                url
                state {
                    name
                    type
                }
                labels {
                    nodes {
                        name
                    }
                }
            }
        }
        """

        try:
            result = self._graphql(query, {"id": str(issue_id)})
            item = result.get("issue")
            if not item:
                return None

            state_type = item.get("state", {}).get("type", "")
            issue_state = "closed" if state_type in ("completed", "canceled") else "open"
            label_names = [l["name"] for l in item.get("labels", {}).get("nodes", [])]

            return Issue(
                id=item["id"],
                number=item["identifier"],
                title=item["title"],
                body=item.get("description", "") or "",
                url=item.get("url", ""),
                state=issue_state,
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
        """Create a new Linear issue."""
        team_id = self._get_team_id()
        if not team_id:
            raise RuntimeError("No Linear team configured or found")

        # Get label IDs if labels provided
        label_ids = []
        if labels:
            label_query = """
            query($filter: IssueLabelFilter) {
                issueLabels(filter: $filter) {
                    nodes {
                        id
                        name
                    }
                }
            }
            """
            label_filter = ", ".join([f'"{l}"' for l in labels])
            result = self._graphql(label_query, {
                "filter": {"name": {"in": labels}}
            })
            for label in result.get("issueLabels", {}).get("nodes", []):
                label_ids.append(label["id"])

        mutation = """
        mutation($input: IssueCreateInput!) {
            issueCreate(input: $input) {
                success
                issue {
                    id
                    identifier
                    title
                    description
                    url
                    state {
                        name
                        type
                    }
                }
            }
        }
        """

        input_data: dict[str, Any] = {
            "teamId": team_id,
            "title": title,
        }
        if body:
            input_data["description"] = body
        if label_ids:
            input_data["labelIds"] = label_ids

        project_id = self._get_project_id()
        if project_id:
            input_data["projectId"] = project_id

        result = self._graphql(mutation, {"input": input_data})

        if not result.get("issueCreate", {}).get("success"):
            raise RuntimeError("Failed to create Linear issue")

        item = result["issueCreate"]["issue"]
        return Issue(
            id=item["id"],
            number=item["identifier"],
            title=item["title"],
            body=item.get("description", "") or "",
            url=item.get("url", ""),
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
        """Update an existing Linear issue."""
        mutation = """
        mutation($id: String!, $input: IssueUpdateInput!) {
            issueUpdate(id: $id, input: $input) {
                success
                issue {
                    id
                    identifier
                    title
                    description
                    url
                    state {
                        name
                        type
                    }
                    labels {
                        nodes {
                            name
                        }
                    }
                }
            }
        }
        """

        input_data: dict[str, Any] = {}
        if title:
            input_data["title"] = title
        if body:
            input_data["description"] = body

        # Handle state change - need to find appropriate state ID
        if state:
            # This is simplified - in practice you'd query for states
            pass  # TODO: implement state changes

        result = self._graphql(mutation, {
            "id": str(issue_id),
            "input": input_data,
        })

        if not result.get("issueUpdate", {}).get("success"):
            raise RuntimeError("Failed to update Linear issue")

        item = result["issueUpdate"]["issue"]
        state_type = item.get("state", {}).get("type", "")
        issue_state = "closed" if state_type in ("completed", "canceled") else "open"
        label_names = [l["name"] for l in item.get("labels", {}).get("nodes", [])]

        return Issue(
            id=item["id"],
            number=item["identifier"],
            title=item["title"],
            body=item.get("description", "") or "",
            url=item.get("url", ""),
            state=issue_state,
            labels=label_names,
        )

    def add_comment(self, issue_id: str | int, body: str) -> None:
        """Add a comment to a Linear issue."""
        mutation = """
        mutation($input: CommentCreateInput!) {
            commentCreate(input: $input) {
                success
            }
        }
        """

        result = self._graphql(mutation, {
            "input": {
                "issueId": str(issue_id),
                "body": body,
            }
        })

        if not result.get("commentCreate", {}).get("success"):
            raise RuntimeError("Failed to add comment to Linear issue")
