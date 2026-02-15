# Hive

Terminal-first multi-repo task orchestration CLI. Manage work across multiple git repositories with worktrees, tmux, and GitHub/Linear integration.

## Features

- **Interactive TUI Dashboard** - Vim-style navigation for workers, issues, and PRs
- **Git Worktree Isolation** - Each task gets isolated branches per repo
- **Tmux Orchestration** - Automatic session/window/pane management
- **Issue Providers** - GitHub and Linear support (pluggable)
- **Claude Integration** - Optional AI assistant in worker panes
- **Auto-close Umbrellas** - Close parent issues when all sub-issues complete

## Requirements

- Python 3.11+
- Git
- tmux
- GitHub CLI (`gh`) for GitHub provider
- Claude CLI (optional, for AI assistance)

## Installation

```bash
pip install git+https://github.com/joshdholtz/hive.git
```

Or for development:

```bash
git clone https://github.com/joshdholtz/hive.git
cd hive
python -m venv .venv
source .venv/bin/activate
pip install -e .
```

## Quick Start

### 1. Create workspace.yaml

```yaml
schema_version: 1

defaults:
  base_branch: main
  issues_repo: my-project       # Where umbrella issues live
  setup_commands:
    - "mise trust"              # Run in new worktrees
  claude_yolo: true             # Skip Claude permission prompts

repos:
  frontend: ./frontend
  backend: ./backend
  docs: ../docs
```

### 2. Start the planner

```bash
hive planner
```

This creates a tmux session with:
- Left pane: Claude (planner agent)
- Right top: Workers/Issues TUI
- Right bottom: PRs panel

### 3. Create an issue

```bash
hive issue new "Add user auth" --repos frontend,backend
```

Creates:
- Umbrella issue in `issues_repo`
- Sub-issues in each specified repo

### 4. Start working

```bash
hive start <issue_number>
# or use the TUI - press Enter on an issue
```

Creates:
- Git worktrees in `.hive/wt/<id>/<repo>/`
- Tmux windows with Claude + shell panes
- TASK.md with issue context

### 5. Monitor PRs

The PR panel shows open PRs across all repos. Press:
- `j/k` - Navigate
- `Enter` - Open in browser
- `u` - Show URL for copying
- `r` - Refresh

## TUI Keybindings

### Workers/Issues Panel
| Key | Action |
|-----|--------|
| `j/k` | Navigate up/down |
| `Tab` | Switch section |
| `Enter` | Jump to worker / Start issue |
| `x` | Close worker (keep worktree) |
| `X` | Close worker + clean worktree |
| `r` | Refresh (auto-closes completed umbrellas) |
| `q` | Quit |

### PRs Panel
| Key | Action |
|-----|--------|
| `j/k` | Navigate |
| `Enter` | Open PR in browser |
| `u` | Show URL for copying |
| `r` | Refresh |

## Commands

```bash
hive planner              # Start planner session
hive menu                 # Run TUI dashboard
hive menu prs             # Run PRs panel only

hive issue new "title" --repos r1,r2   # Create umbrella + sub-issues
hive issue list           # List umbrella issues
hive issue sync <id>      # Sync status to GitHub

hive start <id>           # Create worktrees + windows
hive pick start           # Interactive issue picker

hive clean <id> --yes     # Remove worktrees
hive status <id>          # Show task status
```

## Configuration

### workspace.yaml

```yaml
schema_version: 1

defaults:
  base_branch: main
  issues_repo: hive              # Repo key for umbrella issues
  setup_commands:                # Run in new worktree windows
    - "mise trust"
  claude_yolo: true              # Use --dangerously-skip-permissions
  symlink_files:                 # Symlink from main repo to worktrees
    - ".env"
    - ".envrc"

# For Linear instead of GitHub:
# provider:
#   type: linear
#   project: PROJECT_KEY
#   api_key_env: LINEAR_API_KEY

repos:
  frontend: ./frontend
  backend:
    path: ./backend
    base_branch: develop         # Override per-repo
```

## Directory Structure

```
project/
├── workspace.yaml
├── .hive/
│   ├── tasks/<id>/
│   │   ├── issues.json    # Issue tracking
│   │   └── state.json     # Task state
│   └── wt/<id>/
│       ├── frontend/      # Worktree
│       └── backend/       # Worktree
├── frontend/              # Main repos
└── backend/
```

## Development

```bash
# Run tests
pytest tests/ -v

# Run specific test
pytest tests/test_menu.py -v
```

## License

MIT
