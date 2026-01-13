<p align="center">
  <img src="./banner.svg" alt="Hive - One architect plans, many workers execute" width="600">
</p>

<p align="center">
  <strong>Run a swarm of AI agents in parallel on your codebase</strong>
</p>

Hive is a native terminal multiplexer for orchestrating multiple Claude or Codex instances working in parallel. Instead of one AI doing tasks sequentially, run 3, 5, or 10+ workers simultaneously—each focused on their own lane of work, coordinated by an architect agent.

```
┌─────────────────────────────────────────────────────────────┐
│                        architect                             │
├────────────────────┬────────────────────┬───────────────────┤
│  backend/features  │   backend/fixes    │   backend/tests   │
├────────────────────┼────────────────────┼───────────────────┤
│      android       │        ios         │    javascript     │
└────────────────────┴────────────────────┴───────────────────┘
         6/7 visible | [1/2] | backend: claude | mode: INPUT
```

---

## Features

- **Native TUI** — No tmux required. Built-in terminal multiplexer with split panes
- **Worker Pagination** — Handle 10+ workers with `[`/`]` page navigation
- **Smart Mode** — Show only workers with active tasks (`Ctrl+S`)
- **Command Palette** — Quick actions with `Ctrl+P`
- **Sidebar** — Toggle worker visibility, reorder panes
- **Scrollback** — Scroll through output with `Ctrl+U`/`Ctrl+D`
- **Multi-Project Workspaces** — Manage multiple repos with a single hive
- **Project Registry** — Quick-switch between projects
- **Recursive workspace discovery** — Setup wizard scans nested folders (3 levels deep, skipping common build dirs) so you can run it from a parent directory and still find all repos

---

## Quick Start

```bash
# Install (requires Rust)
cargo install --git https://github.com/joshdholtz/hive

# Start the swarm (runs setup wizard on first run)
cd your-project
hive up

# Attach to the TUI (if detached)
hive attach
```

---

## Commands

| Command | Description |
|---------|-------------|
| `hive up` | Start the hive (runs setup wizard if no config exists) |
| `hive attach` | Attach to a running hive's TUI |
| `hive down` | Stop the hive server |
| `hive status` | Show worker status and task counts |
| `hive nudge [worker]` | Nudge workers to check for tasks |
| `hive role [worker]` | Regenerate worker role files |
| `hive list` | List registered projects |
| `hive open [project]` | Open a project from the registry |
| `hive doctor` | Check and fix common issues |
| `hive deinit` | Remove hive configuration |

---

## TUI Keybindings

### Modes

The TUI has two modes indicated by border color:
- **Yellow border** = Input mode (keystrokes go to the focused agent)
- **Cyan border** = Nav mode (keystrokes control the TUI)

Press `Escape` to enter nav mode, `Enter` to return to input mode.

### Nav Mode Keys

| Key | Action |
|-----|--------|
| `h` / `l` | Move left/right in grid |
| `j` / `k` | Move down/up in grid |
| Arrows | Navigate grid (auto-changes page at edges) |
| `[` / `]` | Previous/next page of workers |
| `p` | Open command palette |
| `Ctrl+U` / `Ctrl+D` | Scroll up/down half page |
| `PageUp` / `PageDown` | Scroll up/down |
| `Home` / `End` | Scroll to top/bottom |
| `z` | Toggle zoom on focused pane |
| `n` | Nudge all workers |
| `N` | Nudge focused worker |
| `d` | Detach from TUI (hive keeps running) |
| `?` | Toggle help |
| `Tab` | Focus sidebar |

### Input Mode Keys (Ctrl+ combinations)

| Key | Action |
|-----|--------|
| `Ctrl+H/J/K/L` | Navigate grid (left/down/up/right) |
| `Ctrl+S` | Toggle smart mode (show only active workers) |
| `Ctrl+O` | Toggle sidebar |
| `Ctrl+Z` | Toggle zoom |
| `Ctrl+D` | Detach |
| `Escape` | Enter nav mode |

### Sidebar Keys (when focused)

| Key | Action |
|-----|--------|
| `j`/`k` or arrows | Navigate items |
| `Space` | Toggle visibility |
| `Ctrl+U` / `Ctrl+D` | Reorder pane up/down |
| `Tab` or `Escape` | Return to panes |

---

## Configuration

### Single Project (`.hive.yaml`)

```yaml
architect:
  backend: claude

workers:
  backend: claude
  skip_permissions: true   # Auto-approve all actions (Claude only)

session: my-project

tasks:
  source: yaml
  file: .hive/tasks.yaml

windows:
  - name: backend
    workers:
      - id: api
        lane: api
      - id: auth
        lane: auth
```

### Workers Config

```yaml
workers:
  backend: claude          # claude or codex
  skip_permissions: true   # Skip approval prompts (adds --dangerously-skip-permissions)
```

**Note:** `skip_permissions` only affects Claude. Codex always runs with `--sandbox danger-full-access --ask-for-approval never`.

### Multi-Project Workspace (`.hive/workspace.yaml`)

For managing multiple repositories:

```yaml
name: my-workspace
root: /path/to/workspace

projects:
  - path: ./frontend
    workers: 2
    lanes: [ui, components]
  - path: ./backend
    workers: 3
    lanes: [api, auth, database]

architect:
  backend: claude

workers:
  backend: claude
```

Run `hive up` to create this interactively via the setup wizard.

### Task File Structure

```yaml
api:
  backlog:
    - id: add-user-endpoint
      description: Add POST /users endpoint
      acceptance:
        - Returns 201 on success
        - Validates email format
  in_progress: []
  done: []

auth:
  backlog: []
  in_progress: []
  done: []
```

### Custom Messages

```yaml
messages:
  startup: |
    Read .hive/workers/{lane}/WORKER.md. You are assigned to lane '{lane}'.
    Check your task backlog. If empty, STOP. If tasks exist, claim ONE.
  nudge: |
    You have {backlog_count} task(s) in lane '{lane}'. Claim ONE task.
```

### Branch Naming

```yaml
windows:
  - name: backend
    workers:
      - id: api-worker
        lane: api
        branch:
          local: "api-worker/api"    # Local branch prefix
          remote: "api"              # Remote branch prefix
```

---

## Architecture

```
┌─────────────┐     ┌───────────────────┐     ┌─────────────┐
│  ARCHITECT  │────▶│  .hive/tasks.yaml │◀────│   WATCHER   │
│   (plans)   │     │                   │     │  (nudges)   │
└─────────────┘     └───────────────────┘     └─────────────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │  worker  │ │  worker  │ │  worker  │
        │   (api)  │ │  (auth)  │ │ (tests)  │
        └──────────┘ └──────────┘ └──────────┘
```

### Separation of Concerns

- **Architect** — Plans work, researches codebase, writes tasks. Does NOT write code.
- **Workers** — Execute tasks, one at a time. Push PRs when done.
- **Watcher** — Monitors task file, nudges idle workers when backlog appears.

This separation prevents the AI from rushing into coding without proper planning.

---

## Project Registry

Hive maintains a registry of your projects at `~/.hive/projects.json`. Use it to quickly switch between projects:

```bash
# List all registered projects
hive list

# Open a project by name
hive open my-project

# Projects are auto-registered when you run hive up
```

Access the project manager in the TUI with `Ctrl+P` → "Project manager" or press `Ctrl+O`.

---

## Git Worktrees

For multi-worker setups, each worker needs its own checkout. Hive can create git worktrees:

```
my-project/
├── main/                 # Main repo
├── .hive-worktrees/
│   ├── api/              # Worktree for API worker
│   ├── auth/             # Worktree for Auth worker
│   └── tests/            # Worktree for Tests worker
```

The setup wizard (triggered by `hive up`) handles this automatically.

---

## Dependencies

**Required:**
- Rust (for installation)
- `claude` CLI and/or `codex` CLI

**Optional:**
- `gh` — GitHub CLI (for GitHub Projects integration)
- `git` — For worktree support

---

## Best Practices

1. **Use Claude for the architect** — Planning requires stronger reasoning
2. **Keep tasks small** — "Add POST /login endpoint" not "Add authentication"
3. **One task at a time** — Workers finish and push before claiming new work
4. **Let architect propose, you approve** — Review tasks before workers start
5. **Use lanes** — Group related work (api, frontend, tests)
6. **Use smart mode** — `Ctrl+S` to focus on active workers in large hives

---

## Troubleshooting

### Codex TUI Issues
Codex has known issues with small terminal panes. Hive uses larger PTY sizes for Codex (40x120 initial, 16x60 minimum) to mitigate this.

### Claude Permission Prompts
Add `skip_permissions: true` to your workers config to auto-approve actions.

### Workers Not Getting Nudged
Manual nudges (`N` in nav mode) now work even if a worker has tasks in progress.

---

## License

MIT
