# Hive - Multi-Repo Task Orchestration CLI

## What is Hive?

A terminal-first CLI tool for orchestrating work across multiple git repositories. It provides:

- **Git worktree management** - Isolated branches per task
- **Tmux session orchestration** - Automatic window/pane setup
- **Issue tracking** - GitHub and Linear provider support
- **Interactive TUI** - Dashboard for workers, issues, and PRs

## Architecture

```
src/hive/
├── cli.py              # Main CLI entry point
├── commands/           # CLI commands
│   ├── menu.py         # Interactive TUI dashboard
│   ├── planner.py      # Planner session management
│   ├── start.py        # Start working on an issue
│   ├── pick.py         # Interactive issue picker
│   ├── issue_v2.py     # Issue management (new, list, sync)
│   ├── clean.py        # Cleanup worktrees
│   └── ...
├── core/               # Core functionality
│   ├── context.py      # Workspace context
│   ├── workspace.py    # Workspace parsing/validation
│   ├── tmux.py         # Tmux operations
│   ├── git.py          # Git operations
│   └── ...
├── models/             # Data models
│   └── workspace.py    # Workspace, RepoConfig, etc.
└── providers/          # Issue provider abstraction
    ├── base.py         # Abstract provider interface
    ├── github.py       # GitHub provider (uses gh CLI)
    └── linear.py       # Linear provider (GraphQL API)
```

## Key Commands

```bash
hive planner              # Start interactive planner session
hive menu                 # TUI dashboard (workers, issues, PRs)
hive issue new "title" --repos repo1,repo2  # Create umbrella + sub-issues
hive start <issue_number> # Create worktrees and tmux windows
hive pick start           # Interactive issue picker
hive clean <id> --yes     # Remove worktrees
```

## Workspace Configuration

Users create `workspace.yaml` in their project root:

```yaml
schema_version: 1

defaults:
  base_branch: main
  issues_repo: my-project      # Repo for umbrella issues
  setup_commands:
    - "mise trust"             # Run in new worktrees
  claude_yolo: true            # --dangerously-skip-permissions

repos:
  frontend: ./frontend
  backend: ./backend
  docs: ../docs
```

## Key Concepts

- **Umbrella Issue**: Parent issue in issues_repo tracking multi-repo work
- **Sub-Issues**: Per-repo issues linked to umbrella
- **Workers**: Tmux windows with Claude + shell for each repo
- **Worktrees**: Git worktrees in `.hive/wt/<task-id>/<repo>/`

## Provider System

Issue providers are pluggable:

```python
from hive.providers import get_provider
from hive.providers.base import ProviderConfig

config = ProviderConfig(type="github", repo="owner/repo")
provider = get_provider(config)
issues = provider.list_issues(state="open")
```

## Testing

```bash
pytest tests/ -v
```

## Development Tips

- TUI code is in `menu.py` - uses raw terminal input (termios)
- Tmux operations in `core/tmux.py` - wraps tmux CLI
- Git worktree operations in `core/git.py`
- Provider abstraction makes adding new issue trackers easy
