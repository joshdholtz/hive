"""Tests for hive init command."""

import pytest

from hive.commands.init import _filter_repos, _repo_name_to_key


class TestFilterRepos:
    """Tests for repo filtering."""

    def test_filters_archived(self):
        repos = [
            {"name": "active", "isArchived": False, "isFork": False},
            {"name": "archived", "isArchived": True, "isFork": False},
        ]
        result = _filter_repos(repos)
        assert len(result) == 1
        assert result[0]["name"] == "active"

    def test_filters_forks(self):
        repos = [
            {"name": "original", "isArchived": False, "isFork": False},
            {"name": "forked", "isArchived": False, "isFork": True},
        ]
        result = _filter_repos(repos)
        assert len(result) == 1
        assert result[0]["name"] == "original"

    def test_match_filter(self):
        repos = [
            {"name": "sdk-ios", "isArchived": False, "isFork": False},
            {"name": "sdk-android", "isArchived": False, "isFork": False},
            {"name": "backend", "isArchived": False, "isFork": False},
        ]
        result = _filter_repos(repos, match="sdk")
        assert len(result) == 2
        names = [r["name"] for r in result]
        assert "sdk-ios" in names
        assert "sdk-android" in names

    def test_exclude_filter(self):
        repos = [
            {"name": "sdk-ios", "isArchived": False, "isFork": False},
            {"name": "sdk-android", "isArchived": False, "isFork": False},
            {"name": "backend", "isArchived": False, "isFork": False},
        ]
        result = _filter_repos(repos, exclude="android")
        assert len(result) == 2
        names = [r["name"] for r in result]
        assert "sdk-ios" in names
        assert "backend" in names

    def test_match_and_exclude(self):
        repos = [
            {"name": "sdk-ios", "isArchived": False, "isFork": False},
            {"name": "sdk-android", "isArchived": False, "isFork": False},
            {"name": "backend", "isArchived": False, "isFork": False},
        ]
        result = _filter_repos(repos, match="sdk", exclude="android")
        assert len(result) == 1
        assert result[0]["name"] == "sdk-ios"

    def test_regex_match(self):
        repos = [
            {"name": "api-v1", "isArchived": False, "isFork": False},
            {"name": "api-v2", "isArchived": False, "isFork": False},
            {"name": "web-v1", "isArchived": False, "isFork": False},
        ]
        result = _filter_repos(repos, match=r"api-v\d+")
        assert len(result) == 2


class TestRepoNameToKey:
    """Tests for repo name to key conversion."""

    def test_simple_name(self):
        assert _repo_name_to_key("backend") == "backend"

    def test_hyphenated_name(self):
        assert _repo_name_to_key("sdk-ios") == "sdk_ios"

    def test_multiple_hyphens(self):
        assert _repo_name_to_key("my-cool-repo") == "my_cool_repo"
