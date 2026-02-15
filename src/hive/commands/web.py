"""hive web - Mobile-friendly web UI for tmux navigation."""

import asyncio
import json
import os
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path
from typing import Annotated

import typer

from hive.core.context import get_context

SESSION_NAME = "hive-planner"


# Shared CSS for all pages
SHARED_CSS = """
    * { box-sizing: border-box; margin: 0; padding: 0; }

    :root {
        --bg-primary: #0c0c0c;
        --bg-secondary: #161616;
        --bg-tertiary: #1c1c1c;
        --bg-hover: #252525;
        --border: #2a2a2a;
        --text-primary: #fafafa;
        --text-secondary: #a0a0a0;
        --text-muted: #606060;
        --accent: #f97316;
        --accent-hover: #ea580c;
        --accent-glow: rgba(249, 115, 22, 0.15);
        --green: #22c55e;
        --yellow: #eab308;
        --purple: #a855f7;
    }

    body {
        font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', sans-serif;
        background: var(--bg-primary);
        color: var(--text-primary);
        min-height: 100vh;
        min-height: 100dvh;
        -webkit-font-smoothing: antialiased;
    }

    .container {
        padding: 24px 16px;
        padding-bottom: 100px;
        max-width: 600px;
        margin: 0 auto;
    }

    h1 {
        font-size: 28px;
        font-weight: 700;
        margin-bottom: 8px;
        color: var(--text-primary);
        letter-spacing: -0.5px;
    }

    .subtitle {
        font-size: 14px;
        color: var(--text-muted);
        margin-bottom: 24px;
    }

    /* Navigation tabs */
    .nav-tabs {
        display: flex;
        gap: 8px;
        margin-bottom: 24px;
        border-bottom: 1px solid var(--border);
        padding-bottom: 12px;
    }

    .nav-tab {
        background: transparent;
        color: var(--text-secondary);
        border: none;
        padding: 8px 16px;
        font-size: 14px;
        font-weight: 500;
        cursor: pointer;
        border-radius: 8px;
        transition: all 0.15s ease;
        text-decoration: none;
    }
    .nav-tab:hover {
        background: var(--bg-hover);
        color: var(--text-primary);
    }
    .nav-tab.active {
        background: var(--bg-tertiary);
        color: var(--text-primary);
    }

    /* Cards */
    .card {
        background: var(--bg-secondary);
        border-radius: 16px;
        padding: 16px 20px;
        margin-bottom: 10px;
        border: 1px solid var(--border);
        transition: all 0.2s ease;
    }
    .card:hover {
        background: var(--bg-hover);
        border-color: #3a3a3a;
    }
    .card.clickable {
        cursor: pointer;
    }
    .card.clickable:active {
        transform: scale(0.98);
    }
    a.card {
        text-decoration: none;
        color: inherit;
        display: block;
    }

    .card-title {
        font-size: 16px;
        font-weight: 600;
        color: var(--text-primary);
        margin-bottom: 4px;
    }

    .card-meta {
        font-size: 13px;
        color: var(--text-muted);
        display: flex;
        align-items: center;
        gap: 12px;
    }

    .card-number {
        color: var(--text-secondary);
        font-family: 'SF Mono', Menlo, monospace;
    }

    .branch-name {
        color: var(--text-muted);
        font-family: 'SF Mono', Menlo, monospace;
        font-size: 12px;
    }

    /* Labels/Tags */
    .label {
        display: inline-block;
        padding: 2px 8px;
        border-radius: 12px;
        font-size: 11px;
        font-weight: 500;
        background: var(--bg-tertiary);
        color: var(--text-secondary);
        margin-right: 6px;
    }
    .label.draft {
        background: rgba(234, 179, 8, 0.15);
        color: var(--yellow);
    }
    .label.repo {
        background: rgba(168, 85, 247, 0.15);
        color: var(--purple);
    }

    .section-title {
        font-size: 11px;
        font-weight: 600;
        text-transform: uppercase;
        color: var(--text-secondary);
        margin: 28px 0 12px;
        letter-spacing: 0.5px;
        padding-left: 4px;
    }

    .empty-state {
        color: var(--text-muted);
        padding: 40px 20px;
        text-align: center;
        font-size: 15px;
    }

    /* Buttons */
    .btn {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 10px 16px;
        border-radius: 10px;
        font-size: 14px;
        font-weight: 500;
        cursor: pointer;
        transition: all 0.15s ease;
        border: none;
        text-decoration: none;
    }
    .btn-primary {
        background: var(--accent);
        color: white;
    }
    .btn-primary:hover {
        background: var(--accent-hover);
    }
    .btn-secondary {
        background: var(--bg-tertiary);
        color: var(--text-primary);
        border: 1px solid var(--border);
    }
    .btn-secondary:hover {
        background: var(--bg-hover);
    }

    /* Refresh FAB */
    .fab {
        position: fixed;
        bottom: 24px;
        right: 24px;
        background: var(--bg-secondary);
        color: var(--text-primary);
        border: 1px solid var(--border);
        border-radius: 50%;
        width: 56px;
        height: 56px;
        font-size: 22px;
        cursor: pointer;
        box-shadow: 0 4px 20px rgba(0,0,0,0.4);
        transition: all 0.2s ease;
    }
    .fab:active {
        transform: scale(0.9);
        background: var(--bg-hover);
    }

    .status-msg {
        position: fixed;
        bottom: 100px;
        left: 50%;
        transform: translateX(-50%);
        background: var(--bg-tertiary);
        border: 1px solid var(--border);
        padding: 12px 24px;
        border-radius: 100px;
        font-size: 14px;
        font-weight: 500;
        opacity: 0;
        transition: opacity 0.3s;
        box-shadow: 0 4px 20px rgba(0,0,0,0.3);
    }
    .status-msg.show { opacity: 1; }

    /* Loading state */
    .loading {
        text-align: center;
        padding: 40px;
        color: var(--text-muted);
    }

    /* Scrollbar styling */
    ::-webkit-scrollbar {
        width: 6px;
        height: 6px;
    }
    ::-webkit-scrollbar-track {
        background: transparent;
    }
    ::-webkit-scrollbar-thumb {
        background: var(--border);
        border-radius: 3px;
    }
"""


def _get_issues_data(context) -> list[dict]:
    """Fetch open issues from the configured provider."""
    issues = []

    if not context.has_workspace():
        return issues

    try:
        workspace = context.load_workspace()

        from hive.providers import get_provider
        from hive.providers.base import ProviderConfig as ProviderCfg

        provider_cfg = ProviderCfg(
            type=workspace.provider.type,
            repo=workspace.provider.repo,
            project=workspace.provider.project,
            api_key_env=workspace.provider.api_key_env,
        )

        if provider_cfg.type == "github" and not provider_cfg.repo:
            from hive.commands.issue_v2 import _get_repo_github_path
            issues_repo_key = workspace.defaults.issues_repo
            issues_repo = workspace.get_repo(issues_repo_key)
            if issues_repo:
                provider_cfg.repo = _get_repo_github_path(issues_repo.path)

        provider = get_provider(provider_cfg)
        provider_issues = provider.list_issues(state="open", limit=30)

        for pi in provider_issues:
            issues.append({
                "number": pi.number,
                "title": pi.title,
                "labels": pi.labels,
                "url": getattr(pi, 'url', f"https://github.com/{provider_cfg.repo}/issues/{pi.number}"),
            })
    except Exception:
        pass

    return issues


def _get_prs_data(context) -> list[dict]:
    """Fetch open PRs from all repos in workspace."""
    prs = []

    if not context.has_workspace():
        return prs

    try:
        workspace = context.load_workspace()

        def fetch_repo_prs(repo_key: str, repo_path) -> list[dict]:
            result_prs = []
            if not repo_path.exists():
                return result_prs
            try:
                result = subprocess.run(
                    ["gh", "pr", "list", "--json", "number,title,headRefName,url,isDraft", "--limit", "10"],
                    cwd=repo_path,
                    capture_output=True,
                    text=True,
                    timeout=10,
                )
                if result.returncode == 0 and result.stdout.strip():
                    pr_data = json.loads(result.stdout)
                    for pr in pr_data:
                        result_prs.append({
                            "number": pr["number"],
                            "title": pr["title"],
                            "repo": repo_key,
                            "branch": pr["headRefName"],
                            "url": pr["url"],
                            "draft": pr.get("isDraft", False),
                        })
            except (subprocess.TimeoutExpired, json.JSONDecodeError, FileNotFoundError):
                pass
            return result_prs

        with ThreadPoolExecutor(max_workers=8) as executor:
            futures = {
                executor.submit(fetch_repo_prs, key, cfg.path): key
                for key, cfg in workspace.repos.items()
            }
            for future in as_completed(futures, timeout=15):
                try:
                    prs.extend(future.result())
                except Exception:
                    pass

    except Exception:
        pass

    return prs


def _get_workers_data() -> list[dict]:
    """Get active worker windows from tmux."""
    workers = []
    try:
        result = subprocess.run(
            ["tmux", "list-windows", "-t", SESSION_NAME, "-F", "#{window_name}"],
            capture_output=True,
            text=True,
        )
        if result.returncode == 0:
            for line in result.stdout.strip().split("\n"):
                window = line.strip()
                if window and '-' in window and window not in ('planner', 'shell'):
                    parts = window.split('-', 1)
                    if len(parts) == 2:
                        workers.append({
                            "window": window,
                            "task_id": parts[0],
                            "repo": parts[1],
                        })
    except Exception:
        pass
    return workers


# Tasks page template
TASKS_TEMPLATE = """<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, interactive-widget=resizes-content">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>Tasks - Hive</title>
    <style>
""" + SHARED_CSS + """
        .card-header {
            display: flex;
            justify-content: space-between;
            align-items: flex-start;
            gap: 12px;
        }
        .card-content {
            flex: 1;
            min-width: 0;
        }
        .card-actions {
            display: flex;
            gap: 8px;
            flex-shrink: 0;
        }
        .action-btn {
            background: var(--bg-tertiary);
            color: var(--text-secondary);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 8px 12px;
            font-size: 13px;
            cursor: pointer;
            transition: all 0.15s ease;
            white-space: nowrap;
        }
        .action-btn:active {
            transform: scale(0.95);
        }
        .action-btn.primary {
            background: var(--accent);
            color: white;
            border-color: var(--accent);
        }
        .action-btn.primary:hover {
            background: var(--accent-hover);
        }
        .action-btn.danger {
            color: #ef4444;
            border-color: #ef4444;
        }
        .action-btn.danger:active {
            background: rgba(239, 68, 68, 0.1);
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>Hive</h1>
        <p class="subtitle">Manage your tasks</p>

        <div class="nav-tabs">
            <a href="/" class="nav-tab">Panes</a>
            <a href="/tasks" class="nav-tab active">Tasks</a>
            <a href="/prs" class="nav-tab">PRs</a>
        </div>

        <div id="workers-section"></div>
        <div id="issues-section"></div>

        <button class="fab" onclick="loadData()">↻</button>
        <div id="status-msg" class="status-msg"></div>
    </div>

    <script>
        function showStatus(msg) {
            const el = document.getElementById('status-msg');
            el.textContent = msg;
            el.classList.add('show');
            setTimeout(() => el.classList.remove('show'), 2000);
        }

        async function loadData() {
            try {
                const [workersRes, issuesRes] = await Promise.all([
                    fetch('/api/workers'),
                    fetch('/api/issues')
                ]);
                const workers = await workersRes.json();
                const issues = await issuesRes.json();
                renderWorkers(workers.workers);
                renderIssues(issues.issues);
                showStatus('Refreshed');
            } catch (e) {
                showStatus('Error loading data');
            }
        }

        function renderWorkers(workers) {
            const container = document.getElementById('workers-section');
            if (!workers || workers.length === 0) {
                container.innerHTML = '';
                return;
            }

            let html = '<div class="section-title">Active Workers</div>';
            workers.forEach(w => {
                html += `
                    <div class="card">
                        <div class="card-header">
                            <div class="card-content">
                                <div class="card-title">${w.window}</div>
                                <div class="card-meta">
                                    <span class="label repo">${w.repo}</span>
                                    <span>Task #${w.task_id}</span>
                                </div>
                            </div>
                            <div class="card-actions">
                                <button class="action-btn" onclick="viewWorker('${w.window}')">View</button>
                                <button class="action-btn danger" onclick="closeWorker('${w.window}')">Close</button>
                            </div>
                        </div>
                    </div>
                `;
            });
            container.innerHTML = html;
        }

        function renderIssues(issues) {
            const container = document.getElementById('issues-section');
            if (!issues || issues.length === 0) {
                container.innerHTML = '<div class="empty-state">No open issues</div>';
                return;
            }

            let html = '<div class="section-title">Open Issues</div>';
            issues.forEach(issue => {
                const labels = (issue.labels || []).map(l =>
                    `<span class="label">${l}</span>`
                ).join('');
                html += `
                    <div class="card">
                        <div class="card-header">
                            <div class="card-content">
                                <div class="card-title">${issue.title}</div>
                                <div class="card-meta">
                                    <span class="card-number">#${issue.number}</span>
                                    ${labels}
                                </div>
                            </div>
                            <div class="card-actions">
                                <a href="${issue.url}" target="_blank" class="action-btn">View</a>
                                <button class="action-btn primary" onclick="startIssue(${issue.number})">Start</button>
                            </div>
                        </div>
                    </div>
                `;
            });
            container.innerHTML = html;
        }

        function viewWorker(windowName) {
            // Navigate to panes view with this window selected
            window.location.href = '/?window=' + encodeURIComponent(windowName);
        }

        async function closeWorker(windowName) {
            if (!confirm('Close worker ' + windowName + '?')) return;
            showStatus('Closing ' + windowName + '...');
            try {
                const res = await fetch('/api/worker/' + encodeURIComponent(windowName), { method: 'DELETE' });
                const data = await res.json();
                if (data.error) {
                    showStatus('Error: ' + data.error);
                } else {
                    showStatus('Closed!');
                    setTimeout(() => loadData(), 500);
                }
            } catch (e) {
                showStatus('Error closing worker');
            }
        }

        async function startIssue(number) {
            if (!confirm('Start working on issue #' + number + '?')) return;
            showStatus('Starting issue #' + number + '...');
            try {
                const res = await fetch('/api/start/' + number, { method: 'POST' });
                const data = await res.json();
                if (data.error) {
                    showStatus('Error: ' + data.error);
                } else {
                    showStatus('Started! Refreshing...');
                    setTimeout(() => loadData(), 1000);
                }
            } catch (e) {
                showStatus('Error starting issue');
            }
        }

        loadData();
    </script>
</body>
</html>
"""


# PRs page template
PRS_TEMPLATE = """<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, interactive-widget=resizes-content">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>PRs - Hive</title>
    <style>
""" + SHARED_CSS + """
    </style>
</head>
<body>
    <div class="container">
        <h1>Hive</h1>
        <p class="subtitle">Pull requests across repos</p>

        <div class="nav-tabs">
            <a href="/" class="nav-tab">Panes</a>
            <a href="/tasks" class="nav-tab">Tasks</a>
            <a href="/prs" class="nav-tab active">PRs</a>
        </div>

        <div id="prs-list">
            <div class="loading">Loading PRs...</div>
        </div>

        <button class="fab" onclick="loadPRs()">↻</button>
        <div id="status-msg" class="status-msg"></div>
    </div>

    <script>
        function showStatus(msg) {
            const el = document.getElementById('status-msg');
            el.textContent = msg;
            el.classList.add('show');
            setTimeout(() => el.classList.remove('show'), 2000);
        }

        async function loadPRs() {
            try {
                const res = await fetch('/api/prs');
                const data = await res.json();
                renderPRs(data.prs);
                showStatus('Refreshed');
            } catch (e) {
                showStatus('Error loading PRs');
            }
        }

        function renderPRs(prs) {
            const container = document.getElementById('prs-list');

            if (!prs || prs.length === 0) {
                container.innerHTML = '<div class="empty-state">No open pull requests</div>';
                return;
            }

            // Group by repo
            const byRepo = {};
            prs.forEach(pr => {
                if (!byRepo[pr.repo]) byRepo[pr.repo] = [];
                byRepo[pr.repo].push(pr);
            });

            let html = '';
            for (const [repo, repoPRs] of Object.entries(byRepo)) {
                html += `<div class="section-title">${repo}</div>`;
                repoPRs.forEach(pr => {
                    const draftLabel = pr.draft ? '<span class="label draft">Draft</span>' : '';
                    html += `
                        <a href="${pr.url}" target="_blank" class="card clickable">
                            <div class="card-title">${pr.title}</div>
                            <div class="card-meta">
                                <span class="card-number">#${pr.number}</span>
                                ${draftLabel}
                                <span class="branch-name">${pr.branch}</span>
                            </div>
                        </a>
                    `;
                });
            }
            container.innerHTML = html;
        }

        loadPRs();
    </script>
</body>
</html>
"""


# Panes page template with xterm.js
HTML_TEMPLATE = """<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, interactive-widget=resizes-content">
    <meta name="apple-mobile-web-app-capable" content="yes">
    <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
    <title>Hive</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/xterm@5.3.0/css/xterm.css">
    <style>
        * { box-sizing: border-box; margin: 0; padding: 0; }

        :root {
            --bg-primary: #0c0c0c;
            --bg-secondary: #161616;
            --bg-tertiary: #1c1c1c;
            --bg-hover: #252525;
            --border: #2a2a2a;
            --text-primary: #fafafa;
            --text-secondary: #a0a0a0;
            --text-muted: #606060;
            --accent: #f97316;
            --accent-hover: #ea580c;
            --accent-glow: rgba(249, 115, 22, 0.15);
        }

        body {
            font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', 'Segoe UI', sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            min-height: 100vh;
            min-height: 100dvh;
            -webkit-font-smoothing: antialiased;
        }

        /* List view */
        #list-view {
            padding: 24px 16px;
            padding-bottom: 100px;
            max-width: 500px;
            margin: 0 auto;
        }
        #list-view.hidden { display: none; }

        h1 {
            font-size: 28px;
            font-weight: 700;
            margin-bottom: 8px;
            color: var(--text-primary);
            letter-spacing: -0.5px;
        }

        .subtitle {
            font-size: 14px;
            color: var(--text-muted);
            margin-bottom: 24px;
        }

        .nav-tabs {
            display: flex;
            gap: 8px;
            margin-bottom: 24px;
            border-bottom: 1px solid var(--border);
            padding-bottom: 12px;
        }

        .nav-tab {
            background: transparent;
            color: var(--text-secondary);
            border: none;
            padding: 8px 16px;
            font-size: 14px;
            font-weight: 500;
            cursor: pointer;
            border-radius: 8px;
            transition: all 0.15s ease;
            text-decoration: none;
        }
        .nav-tab:hover {
            background: var(--bg-hover);
            color: var(--text-primary);
        }
        .nav-tab.active {
            background: var(--bg-tertiary);
            color: var(--text-primary);
        }

        .pane-card {
            background: var(--bg-secondary);
            border-radius: 16px;
            padding: 16px 20px;
            margin-bottom: 10px;
            cursor: pointer;
            transition: all 0.2s ease;
            border: 1px solid var(--border);
            display: flex;
            align-items: center;
            justify-content: space-between;
        }
        .pane-card:hover {
            background: var(--bg-hover);
            border-color: #3a3a3a;
        }
        .pane-card:active {
            transform: scale(0.98);
        }
        .pane-name {
            font-size: 16px;
            font-weight: 600;
            color: var(--text-primary);
        }
        .pane-info {
            font-size: 13px;
            color: var(--text-muted);
            font-family: 'SF Mono', Menlo, monospace;
        }

        .section-title {
            font-size: 11px;
            font-weight: 600;
            text-transform: uppercase;
            color: var(--text-secondary);
            margin: 28px 0 12px;
            letter-spacing: 0.5px;
            padding-left: 4px;
        }

        .empty-state {
            color: var(--text-muted);
            padding: 40px 20px;
            text-align: center;
            font-size: 15px;
        }

        /* Terminal view */
        #terminal-view {
            display: none;
            height: 100vh;
            height: 100dvh;
            flex-direction: column;
            background: #000;
        }
        #terminal-view.active {
            display: flex;
        }

        #terminal-header {
            position: absolute;
            top: 0;
            left: 0;
            right: 0;
            background: linear-gradient(to bottom, rgba(0,0,0,0.7) 0%, transparent 100%);
            padding: 8px 12px;
            display: flex;
            align-items: center;
            gap: 8px;
            z-index: 10;
            pointer-events: none;
        }

        #back-btn {
            background: rgba(0,0,0,0.5);
            color: var(--accent);
            border: none;
            padding: 6px 10px;
            font-size: 14px;
            font-weight: 600;
            cursor: pointer;
            display: flex;
            align-items: center;
            gap: 4px;
            border-radius: 6px;
            pointer-events: auto;
        }
        #back-btn:active {
            opacity: 0.7;
        }

        #current-pane {
            font-size: 12px;
            font-weight: 500;
            flex: 1;
            text-align: center;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
            color: rgba(255,255,255,0.6);
        }

        #terminal-container {
            flex: 1;
            background: #000;
            overflow: auto;
            -webkit-overflow-scrolling: touch;
            position: relative;
        }

        #quick-keys {
            background: var(--bg-secondary);
            padding: 6px 8px;
            display: flex;
            gap: 4px;
            overflow-x: auto;
            border-top: 1px solid var(--border);
            flex-shrink: 0;
            -webkit-overflow-scrolling: touch;
        }

        .quick-key {
            background: var(--bg-tertiary);
            color: var(--text-secondary);
            border: 1px solid var(--border);
            border-radius: 6px;
            padding: 6px 10px;
            font-size: 12px;
            font-family: 'SF Mono', Menlo, monospace;
            cursor: pointer;
            white-space: nowrap;
            transition: all 0.15s ease;
        }
        .quick-key:active {
            background: var(--bg-hover);
            transform: scale(0.95);
        }

        #input-bar {
            background: var(--bg-secondary);
            padding: 8px;
            display: flex;
            gap: 8px;
            flex-shrink: 0;
            border-top: 1px solid var(--border);
        }

        #input-text {
            flex: 1;
            background: var(--bg-primary);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 10px 12px;
            color: var(--text-primary);
            font-size: 16px;
            font-family: 'SF Mono', Menlo, monospace;
            outline: none;
            transition: border-color 0.2s;
        }
        #input-text:focus {
            border-color: var(--accent);
            box-shadow: 0 0 0 2px var(--accent-glow);
        }
        #input-text::placeholder {
            color: var(--text-muted);
        }

        #send-btn {
            background: var(--accent);
            color: white;
            border: none;
            border-radius: 8px;
            padding: 10px 16px;
            font-size: 14px;
            font-weight: 600;
            cursor: pointer;
            transition: all 0.15s ease;
        }
        #send-btn:active {
            background: var(--accent-hover);
            transform: scale(0.95);
        }

        /* Refresh FAB */
        #refresh-btn {
            position: fixed;
            bottom: 24px;
            right: 24px;
            background: var(--bg-secondary);
            color: var(--text-primary);
            border: 1px solid var(--border);
            border-radius: 50%;
            width: 56px;
            height: 56px;
            font-size: 22px;
            cursor: pointer;
            box-shadow: 0 4px 20px rgba(0,0,0,0.4);
            transition: all 0.2s ease;
        }
        #refresh-btn:active {
            transform: scale(0.9);
            background: var(--bg-hover);
        }

        .status-msg {
            position: fixed;
            bottom: 100px;
            left: 50%;
            transform: translateX(-50%);
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            padding: 12px 24px;
            border-radius: 100px;
            font-size: 14px;
            font-weight: 500;
            opacity: 0;
            transition: opacity 0.3s;
            box-shadow: 0 4px 20px rgba(0,0,0,0.3);
        }
        .status-msg.show { opacity: 1; }

        /* Scrollbar styling */
        ::-webkit-scrollbar {
            width: 6px;
            height: 6px;
        }
        ::-webkit-scrollbar-track {
            background: transparent;
        }
        ::-webkit-scrollbar-thumb {
            background: var(--border);
            border-radius: 3px;
        }
    </style>
</head>
<body>
    <div id="list-view">
        <h1>Hive</h1>
        <p class="subtitle">Select a pane to connect</p>
        <div class="nav-tabs">
            <a href="/" class="nav-tab active">Panes</a>
            <a href="/tasks" class="nav-tab">Tasks</a>
            <a href="/prs" class="nav-tab">PRs</a>
        </div>
        <div id="panes-list"></div>
        <button id="refresh-btn" onclick="loadPanes()">↻</button>
        <div id="status-msg" class="status-msg"></div>
    </div>

    <div id="terminal-view">
        <div id="terminal-header">
            <button id="back-btn" onclick="goBack()">‹</button>
            <span id="current-pane"></span>
        </div>
        <div id="terminal-container"></div>
        <div id="quick-keys">
            <button class="quick-key" onclick="refreshPane()">↻</button>
            <button class="quick-key" onclick="sendSpecial('Enter')">↵</button>
            <button class="quick-key" onclick="sendSpecial('C-c')">^C</button>
            <button class="quick-key" onclick="sendSpecial('C-d')">^D</button>
            <button class="quick-key" onclick="sendSpecial('Tab')">⇥</button>
            <button class="quick-key" onclick="sendSpecial('Escape')">Esc</button>
            <button class="quick-key" onclick="sendSpecial('Up')">↑</button>
            <button class="quick-key" onclick="sendSpecial('Down')">↓</button>
        </div>
        <div id="input-bar">
            <input type="text" id="input-text" placeholder="Type here..." autocomplete="off" autocapitalize="off" autocorrect="off" spellcheck="false" enterkeyhint="send">
            <button id="send-btn" onclick="sendInput()">Send</button>
        </div>
    </div>

    <script src="https://cdn.jsdelivr.net/npm/xterm@5.3.0/lib/xterm.min.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/xterm-addon-fit@0.8.0/lib/xterm-addon-fit.min.js"></script>
    <script>
        let term = null;
        let ws = null;
        let fitAddon = null;
        let currentPane = null;

        function showStatus(msg) {
            const el = document.getElementById('status-msg');
            el.textContent = msg;
            el.classList.add('show');
            setTimeout(() => el.classList.remove('show'), 2000);
        }

        async function loadPanes() {
            try {
                const resp = await fetch('/api/panes');
                const data = await resp.json();
                renderPanes(data.panes);
                showStatus('Refreshed');
            } catch (e) {
                showStatus('Error loading panes');
            }
        }

        function renderPanes(panes) {
            const container = document.getElementById('panes-list');

            // Sort panes by window_index, then pane index (same order as tmux)
            panes.sort((a, b) => {
                if (a.window_index !== b.window_index) {
                    return a.window_index - b.window_index;
                }
                return a.index - b.index;
            });

            // Group by window (maintaining order)
            const byWindow = [];
            const windowMap = {};
            panes.forEach(p => {
                if (!(p.window in windowMap)) {
                    windowMap[p.window] = byWindow.length;
                    byWindow.push({ name: p.window, panes: [] });
                }
                byWindow[windowMap[p.window]].panes.push(p);
            });

            let html = '';

            for (const win of byWindow) {
                html += `<div class="section-title">${win.name}</div>`;

                win.panes.forEach(p => {
                    const label = p.label || p.cmd || `pane ${p.index}`;
                    html += `
                        <div class="pane-card" onclick="connectTo('${p.id}', '${win.name}', '${label}')">
                            <div class="pane-name">${label}</div>
                            <div class="pane-info">${p.width}x${p.height}</div>
                        </div>
                    `;
                });
            }

            if (panes.length === 0) {
                html = '<div class="empty-state">No panes found. Is hive-planner running?</div>';
            }

            container.innerHTML = html;
        }

        function connectTo(paneId, windowName, label, skipHistory) {
            currentPane = paneId;
            document.getElementById('current-pane').textContent = `${windowName} / ${label}`;
            document.getElementById('list-view').classList.add('hidden');
            document.getElementById('terminal-view').classList.add('active');

            // Update URL for back button support
            if (!skipHistory) {
                history.pushState(
                    { paneId, windowName, label },
                    '',
                    `#${encodeURIComponent(paneId)}`
                );
            }

            // Create terminal
            if (term) {
                term.dispose();
            }

            term = new Terminal({
                fontSize: 14,
                fontFamily: 'Menlo, Monaco, monospace',
                theme: {
                    background: '#000000',
                    foreground: '#ffffff',
                },
                cursorBlink: false,
                disableStdin: true,  // Read-only display
                scrollback: 5000,
                convertEol: true,
            });

            fitAddon = new FitAddon.FitAddon();
            term.loadAddon(fitAddon);

            const container = document.getElementById('terminal-container');
            container.innerHTML = '';
            term.open(container);

            // Fit after a brief delay
            setTimeout(() => {
                fitAddon.fit();
                connectWebSocket(paneId);
            }, 50);

            // Handle resize (including mobile keyboard show/hide)
            const resizeHandler = () => {
                if (fitAddon && term) {
                    fitAddon.fit();
                    sendResize();
                }
            };
            window.addEventListener('resize', resizeHandler);

            // Handle mobile keyboard via visualViewport API
            if (window.visualViewport) {
                window.visualViewport.addEventListener('resize', () => {
                    // Update terminal view height to match visual viewport
                    const vh = window.visualViewport.height;
                    document.getElementById('terminal-view').style.height = vh + 'px';
                    setTimeout(resizeHandler, 50);
                });
                window.visualViewport.addEventListener('scroll', () => {
                    // Prevent iOS from scrolling the page when keyboard appears
                    window.scrollTo(0, 0);
                });
            }

            term._resizeHandler = resizeHandler;

            // Focus input and setup enter key
            const inputEl = document.getElementById('input-text');
            inputEl.focus();
            inputEl.addEventListener('keydown', (e) => {
                if (e.key === 'Enter') {
                    e.preventDefault();
                    sendInput();
                }
            });
        }

        function sendInput() {
            const inputEl = document.getElementById('input-text');
            const text = inputEl.value;
            if (ws && ws.readyState === WebSocket.OPEN && text) {
                ws.send(JSON.stringify({ type: 'input', data: text }));
                ws.send(JSON.stringify({ type: 'special', key: 'Enter' }));
                inputEl.value = '';
            }
            inputEl.focus();
        }

        function sendSpecial(key) {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: 'special', key: key }));
            }
            document.getElementById('input-text').focus();
        }

        function refreshPane() {
            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: 'refresh' }));
            }
        }

        function connectWebSocket(paneId) {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            // Encode pane ID (it has % in it)
            ws = new WebSocket(`${protocol}//${window.location.host}/ws/pane/${encodeURIComponent(paneId)}`);

            ws.onopen = () => {
                // Request initial content and set size
                sendResize();
            };

            ws.onmessage = (event) => {
                const msg = JSON.parse(event.data);
                if (msg.type === 'output') {
                    term.write(msg.data);
                } else if (msg.type === 'clear') {
                    term.clear();
                }
            };

            ws.onclose = () => {
                term.write('\\r\\n[Disconnected]\\r\\n');
            };

            ws.onerror = () => {
                term.write('\\r\\n[Connection error]\\r\\n');
            };
        }

        function sendResize() {
            if (ws && ws.readyState === WebSocket.OPEN && term) {
                ws.send(JSON.stringify({
                    type: 'resize',
                    cols: term.cols,
                    rows: term.rows
                }));
            }
        }

        function goBack() {
            if (ws) {
                ws.close();
                ws = null;
            }
            if (term && term._resizeHandler) {
                window.removeEventListener('resize', term._resizeHandler);
            }
            // Reset terminal view height
            document.getElementById('terminal-view').style.height = '';
            document.getElementById('terminal-view').classList.remove('active');
            document.getElementById('list-view').classList.remove('hidden');
            currentPane = null;

            // Update URL
            history.pushState({}, '', '/');
            loadPanes();
        }

        // Handle browser back/forward buttons
        window.addEventListener('popstate', (event) => {
            if (event.state && event.state.paneId) {
                // Going forward to a pane
                connectTo(event.state.paneId, event.state.windowName, event.state.label, true);
            } else {
                // Going back to list
                if (ws) {
                    ws.close();
                    ws = null;
                }
                if (term && term._resizeHandler) {
                    window.removeEventListener('resize', term._resizeHandler);
                }
                document.getElementById('terminal-view').classList.remove('active');
                document.getElementById('list-view').classList.remove('hidden');
                currentPane = null;
                loadPanes();
            }
        });

        // Initial load - check for hash in URL
        async function init() {
            await loadPanes();

            // If URL has a pane hash, connect to it
            if (window.location.hash) {
                const paneId = decodeURIComponent(window.location.hash.slice(1));
                // Find pane info from loaded panes
                const resp = await fetch('/api/panes');
                const data = await resp.json();
                const pane = data.panes.find(p => p.id === paneId);
                if (pane) {
                    connectTo(pane.id, pane.window, pane.label, true);
                }
            }
        }

        init();
    </script>
</body>
</html>
"""


def _get_panes() -> list[dict]:
    """Get all tmux panes for the hive session."""
    try:
        result = subprocess.run(
            [
                "tmux", "list-panes", "-s", "-t", SESSION_NAME,
                "-F", "#{pane_id}\t#{window_name}\t#{window_index}\t#{pane_index}\t#{pane_title}\t#{pane_current_command}\t#{pane_width}\t#{pane_height}\t#{window_panes}"
            ],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            return []

        panes = []
        for line in result.stdout.strip().split("\n"):
            if not line:
                continue
            parts = line.split("\t")
            if len(parts) >= 9:
                window = parts[1]
                window_index = int(parts[2])
                pane_index = int(parts[3])
                title = parts[4]
                cmd = parts[5]
                total_panes = int(parts[8])

                # Check if title is a real custom title (not hostname or default)
                is_custom_title = (
                    title and
                    title not in ("", "zsh", "bash", "sh", cmd) and
                    " " not in title and  # Hostnames often have spaces like "Josh's Mac"
                    not title.endswith("-mini") and
                    not title.endswith("-pro") and
                    not title.startswith("ttys")
                )

                if is_custom_title:
                    label = title
                elif cmd == "claude" or "claude" in cmd.lower():
                    label = "claude"
                elif window not in ("planner", "shell") and total_panes == 2:
                    # Worker window with 2 panes: first is claude, second is shell
                    label = "claude" if pane_index == 0 else "shell"
                else:
                    label = cmd or f"pane {pane_index}"

                panes.append({
                    "id": parts[0],
                    "window": window,
                    "window_index": window_index,
                    "index": pane_index,
                    "label": label,
                    "width": int(parts[6]),
                    "height": int(parts[7]),
                })
        return panes
    except Exception:
        return []


def web(
    ctx: typer.Context,
    port: Annotated[
        int,
        typer.Option("--port", "-p", help="Port to run on."),
    ] = 8080,
    host: Annotated[
        str,
        typer.Option("--host", "-h", help="Host to bind to."),
    ] = "0.0.0.0",
) -> None:
    """Start mobile-friendly web UI for hive.

    Opens a web interface for navigating tmux panes.
    Access from your phone at http://<your-ip>:8080
    """
    try:
        from aiohttp import web as aiohttp_web
    except ImportError:
        typer.secho("Missing dependency. Install with:", fg="yellow")
        typer.echo("  pip install aiohttp")
        raise typer.Exit(1)

    import aiohttp

    # Get context for data fetching
    root = ctx.obj.get("root") if ctx.obj else None
    context = get_context(root=root)

    async def handle_index(request):
        return aiohttp_web.Response(text=HTML_TEMPLATE, content_type="text/html")

    async def handle_tasks(request):
        return aiohttp_web.Response(text=TASKS_TEMPLATE, content_type="text/html")

    async def handle_prs_page(request):
        return aiohttp_web.Response(text=PRS_TEMPLATE, content_type="text/html")

    async def handle_panes(request):
        panes = _get_panes()
        return aiohttp_web.json_response({"panes": panes})

    async def handle_issues_api(request):
        issues = _get_issues_data(context)
        return aiohttp_web.json_response({"issues": issues})

    async def handle_prs_api(request):
        prs = _get_prs_data(context)
        return aiohttp_web.json_response({"prs": prs})

    async def handle_workers_api(request):
        workers = _get_workers_data()
        return aiohttp_web.json_response({"workers": workers})

    async def handle_start_issue(request):
        number = request.match_info["number"]
        try:
            # Run hive start in background
            result = subprocess.run(
                ["hive", "start", number, "--with-claude"],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if result.returncode == 0:
                return aiohttp_web.json_response({"success": True, "number": number})
            else:
                return aiohttp_web.json_response({
                    "error": result.stderr or "Failed to start issue"
                }, status=500)
        except subprocess.TimeoutExpired:
            return aiohttp_web.json_response({"error": "Timeout starting issue"}, status=500)
        except Exception as e:
            return aiohttp_web.json_response({"error": str(e)}, status=500)

    async def handle_close_worker(request):
        window_name = request.match_info["window"]
        try:
            # Close the tmux window (keeps worktree)
            result = subprocess.run(
                ["tmux", "kill-window", "-t", f"{SESSION_NAME}:{window_name}"],
                capture_output=True,
                text=True,
                timeout=5,
            )
            if result.returncode == 0:
                return aiohttp_web.json_response({"success": True, "window": window_name})
            else:
                return aiohttp_web.json_response({
                    "error": result.stderr or "Failed to close worker"
                }, status=500)
        except subprocess.TimeoutExpired:
            return aiohttp_web.json_response({"error": "Timeout closing worker"}, status=500)
        except Exception as e:
            return aiohttp_web.json_response({"error": str(e)}, status=500)

    # Create temp directory for pipe files
    pipe_dir = Path(tempfile.gettempdir()) / "hive-web-pipes"
    pipe_dir.mkdir(exist_ok=True)

    async def handle_websocket(request):
        pane_id = request.match_info["pane_id"]

        ws = aiohttp_web.WebSocketResponse()
        await ws.prepare(request)

        # Create pipe file for this connection
        pipe_file = pipe_dir / f"{pane_id.replace('%', 'p')}-{id(ws)}.pipe"
        tail_process = None
        running = True

        def send_to_pane(keys: str):
            """Send literal text to the pane."""
            subprocess.run(
                ["tmux", "send-keys", "-t", pane_id, "-l", "--", keys],
                capture_output=True,
            )

        def send_special_key(key: str):
            """Send special key to the pane."""
            subprocess.run(
                ["tmux", "send-keys", "-t", pane_id, key],
                capture_output=True,
            )

        def get_pane_size():
            """Get current pane dimensions."""
            result = subprocess.run(
                ["tmux", "display-message", "-t", pane_id, "-p", "#{pane_width}\t#{pane_height}"],
                capture_output=True,
                text=True,
            )
            if result.returncode == 0:
                parts = result.stdout.strip().split("\t")
                if len(parts) == 2:
                    return int(parts[0]), int(parts[1])
            return None, None

        def resize_pane(cols: int, rows: int):
            """Resize the tmux pane."""
            subprocess.run(
                ["tmux", "resize-pane", "-t", pane_id, "-x", str(cols), "-y", str(rows)],
                capture_output=True,
            )

        # Save original dimensions to restore later
        original_cols, original_rows = get_pane_size()

        def start_pipe():
            """Start piping pane output to file."""
            # Clear any existing pipe
            subprocess.run(
                ["tmux", "pipe-pane", "-t", pane_id],
                capture_output=True,
            )
            # Start new pipe
            pipe_file.touch()
            subprocess.run(
                ["tmux", "pipe-pane", "-t", pane_id, f"cat >> {pipe_file}"],
                capture_output=True,
            )

        def stop_pipe():
            """Stop piping pane output."""
            subprocess.run(
                ["tmux", "pipe-pane", "-t", pane_id],
                capture_output=True,
            )

        def capture_initial():
            """Capture current pane content for initial display."""
            result = subprocess.run(
                ["tmux", "capture-pane", "-t", pane_id, "-p", "-e", "-J"],
                capture_output=True,
                text=True,
            )
            if result.returncode != 0:
                return ""
            # Strip trailing empty lines but keep content intact
            lines = result.stdout.rstrip('\n').split('\n')
            while lines and not lines[-1].strip():
                lines.pop()
            return '\n'.join(lines) + '\n' if lines else ""

        async def tail_pipe():
            """Tail the pipe file and stream to websocket."""
            nonlocal tail_process, running
            try:
                # Start tail process
                tail_process = await asyncio.create_subprocess_exec(
                    "tail", "-n", "0", "-F", str(pipe_file),
                    stdout=asyncio.subprocess.PIPE,
                    stderr=asyncio.subprocess.DEVNULL,
                )

                while running and tail_process.stdout:
                    try:
                        data = await asyncio.wait_for(
                            tail_process.stdout.read(4096),
                            timeout=0.5
                        )
                        if data:
                            text = data.decode("utf-8", errors="replace")
                            await ws.send_json({"type": "output", "data": text})
                    except asyncio.TimeoutError:
                        continue
                    except Exception:
                        break
            except asyncio.CancelledError:
                pass
            finally:
                if tail_process:
                    try:
                        tail_process.terminate()
                        await tail_process.wait()
                    except Exception:
                        pass

        # Setup: start pipe and send initial content
        start_pipe()
        initial_content = capture_initial()
        if initial_content:
            await ws.send_json({"type": "output", "data": initial_content})

        # Start tailing in background
        tail_task = asyncio.create_task(tail_pipe())

        try:
            async for msg in ws:
                if msg.type == aiohttp.WSMsgType.TEXT:
                    data = json.loads(msg.data)

                    if data["type"] == "input":
                        text = data.get("data", "")
                        if text:
                            send_to_pane(text)

                    elif data["type"] == "special":
                        key = data.get("key", "")
                        if key:
                            send_special_key(key)

                    elif data["type"] == "refresh":
                        # Re-capture full pane content
                        content = capture_initial()
                        if content:
                            await ws.send_json({"type": "output", "data": "\033[2J\033[H" + content})

                    elif data["type"] == "resize":
                        cols = data.get("cols", 80)
                        rows = data.get("rows", 24)
                        resize_pane(cols, rows)

                elif msg.type == aiohttp.WSMsgType.ERROR:
                    break
        finally:
            # Cleanup
            running = False
            stop_pipe()
            tail_task.cancel()
            try:
                await tail_task
            except asyncio.CancelledError:
                pass
            try:
                pipe_file.unlink(missing_ok=True)
            except Exception:
                pass

            # Restore original pane dimensions
            if original_cols and original_rows:
                resize_pane(original_cols, original_rows)

        return ws

    # Create app
    app = aiohttp_web.Application()
    app.router.add_get("/", handle_index)
    app.router.add_get("/tasks", handle_tasks)
    app.router.add_get("/prs", handle_prs_page)
    app.router.add_get("/api/panes", handle_panes)
    app.router.add_get("/api/issues", handle_issues_api)
    app.router.add_get("/api/prs", handle_prs_api)
    app.router.add_get("/api/workers", handle_workers_api)
    app.router.add_post("/api/start/{number}", handle_start_issue)
    app.router.add_delete("/api/worker/{window}", handle_close_worker)
    app.router.add_get("/ws/pane/{pane_id}", handle_websocket)

    # Get local IP for mobile access
    import socket
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("8.8.8.8", 80))
        local_ip = s.getsockname()[0]
        s.close()
    except Exception:
        local_ip = "localhost"

    typer.echo()
    typer.secho("🐝 Hive Web UI", fg="green", bold=True)
    typer.echo()
    typer.echo(f"  Local:   http://localhost:{port}")
    typer.echo(f"  Mobile:  http://{local_ip}:{port}")
    typer.echo()
    typer.secho("  Press Ctrl+C to stop", fg="bright_black")
    typer.echo()

    aiohttp_web.run_app(app, host=host, port=port, print=None)
