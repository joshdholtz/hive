# Hive

Terminal-first polyrepo coordinator. Orchestrates multi-repo work via plans stored in YAML, git worktrees, tmux sessions, and GitHub PR management.

## Features

- **Workspace management**: Clone and manage multiple repositories from a GitHub organization
- **Task isolation**: Each task gets isolated git worktrees, avoiding branch conflicts
- **tmux integration**: Automatic session creation with windows per repo, dashboard, and concierge
- **PR automation**: Create and manage PRs across multiple repos from a single plan
- **LLM-friendly**: JSON output support for programmatic consumption

## Requirements

- Python 3.11+
- Git
- tmux (optional, for session management)
- GitHub CLI (`gh`) for PR operations

## Installation

```bash
# Clone and install in development mode
git clone <repo-url>
cd hive
python -m venv .venv
source .venv/bin/activate
pip install -e .
```

## Quick Start

### 1. Initialize a workspace

Clone repos from a GitHub organization:

```bash
hive init myorg --match "^api-" --clone
```

Or create `workspace.yaml` manually:

```yaml
schema_version: 1
defaults:
  base_branch: main
repos:
  frontend:
    path: ./repos/frontend
  backend:
    path: ./repos/backend
```

### 2. Create a task plan

```bash
hive draft "Add user authentication"
```

This creates `.tasks/<task-id>/plan.yaml`:

```yaml
schema_version: 1
id: 2026-02-12-add-user-authentication
title: Add user authentication
branch: feature/2026-02-12-add-user-authentication
repos:
  frontend:
    tasks:
      - Add login form
      - Add auth context
  backend:
    tasks:
      - Add /auth/login endpoint
      - Add JWT middleware
```

### 3. Apply the plan

Creates git worktrees and tmux session:

```bash
hive apply 2026-02-12-add-user-authentication
```

Worktrees are created at `.wt/<task-id>/<repo>/`.

### 4. Work on the task

Open the tmux session:

```bash
hive open 2026-02-12-add-user-authentication
```

Check status:

```bash
hive status 2026-02-12-add-user-authentication
```

### 5. Create PRs

```bash
hive pr create 2026-02-12-add-user-authentication
```

### 6. Clean up

Remove worktrees when done (plan is preserved):

```bash
hive clean 2026-02-12-add-user-authentication --yes
```

## Commands

| Command | Description |
|---------|-------------|
| `hive init <org>` | Clone repos from GitHub org and generate workspace.yaml |
| `hive doctor` | Check workspace health and dependencies |
| `hive draft <title>` | Create a new task plan |
| `hive list` | List all tasks |
| `hive show <task-id>` | Display parsed plan |
| `hive apply <task-id>` | Create worktrees and tmux session |
| `hive open <task-id>` | Attach to tmux session |
| `hive status <task-id>` | Show task status (git status per repo) |
| `hive clean <task-id>` | Remove worktrees |
| `hive pr create <task-id>` | Create PRs for all repos |
| `hive pr status <task-id>` | Show PR status |
| `hive issue create <task-id>` | Create umbrella GitHub issue |
| `hive issue sync <task-id>` | Sync task status to umbrella issue |

## Directory Structure

```
workspace/
├── workspace.yaml          # Workspace configuration
├── .hive/                   # Hive working directory (gitignored)
│   ├── tasks/              # Task plans and state
│   │   └── <task-id>/
│   │       ├── plan.yaml   # Task plan
│   │       ├── state.json  # Runtime state
│   │       └── prs.json    # PR tracking
│   └── wt/                 # Git worktrees
│       └── <task-id>/
│           ├── frontend/   # Worktree for frontend repo
│           └── backend/    # Worktree for backend repo
└── repos/                   # Main repository clones (or sibling dirs)
    ├── frontend/
    └── backend/
```

## Configuration

### workspace.yaml

```yaml
schema_version: 1
defaults:
  base_branch: main        # Default base branch for worktrees
repos:
  frontend:
    path: ./repos/frontend
    base_branch: develop   # Override per-repo
  backend:
    path: ./repos/backend
```

### plan.yaml

```yaml
schema_version: 1
id: 2026-02-12-task-name
title: Human-readable title
branch: feature/branch-name
repos:
  frontend:
    base_branch: main      # Override base branch
    tasks:
      - Task description
    test: npm test         # Test command (informational)
  backend:
    tasks:
      - Another task
tmux:
  enabled: true            # Default: true
  windows:
    include_dashboard: true
    include_concierge: true
pr:
  enabled: true
  draft: false
issue:
  enabled: true           # Enable umbrella issue tracking
  repo: "org/hive"        # GitHub repo for the umbrella issue
  sync_mode: "comment"    # "comment" or "body"
```

## GitHub Umbrella Issues

Hive can create a single "umbrella" GitHub issue to track a multi-repo task:

```bash
# Create umbrella issue
hive issue create <task-id> --repo org/hive

# Sync status to the issue
hive issue sync <task-id>
```

The umbrella issue shows:
- Task title and branch
- Checklist of repos with PR links
- Merge status per repo

PRs created with `hive pr create` automatically include a "Tracks:" link to the umbrella issue.

## JSON Output

Most commands support `--json` for LLM-friendly output:

```bash
hive status <task-id> --json
hive list --json
hive pr status <task-id> --json
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Missing dependencies |
| 3 | Invalid workspace configuration |

## Development

```bash
# Run tests
pytest tests/ -v

# Run specific test file
pytest tests/test_tmux.py -v
```

## License

MIT
