<p align="center">
  <img src="./banner.svg" alt="Hive - One architect plans, many workers execute" width="600">
</p>

<p align="center">
  <strong>Run a swarm of AI agents in parallel on your codebase</strong>
</p>

Hive orchestrates multiple Claude or Codex instances working in parallel on your codebase. Instead of one AI doing tasks sequentially, run 3, 5, or 10 workers simultaneously—each focused on their own lane of work, coordinated by an architect agent.

```
┌─────────────┐     ┌───────────────────┐     ┌─────────────┐
│  ARCHITECT  │────▶│  hive_tasks.yaml  │◀────│   WATCHER   │
│   (plans)   │     │                   │     │  (nudges)   │
└─────────────┘     └───────────────────┘     └─────────────┘
                           │
              ┌────────────┼────────────┐
              ▼            ▼            ▼
        ┌──────────┐ ┌──────────┐ ┌──────────┐
        │ worker-1 │ │ worker-2 │ │ worker-3 │
        │   (api)  │ │  (auth)  │ │ (tests)  │
        └──────────┘ └──────────┘ └──────────┘
```

---

## The Problem

You have an AI coding assistant. It's great! But:

- 🐌 **Sequential execution** — One task at a time
- 📋 **Growing backlog** — 20 features, bugs, and improvements waiting
- ⏰ **Time wasted** — Watching one agent work while you could run five
- 🔀 **Context switching** — Hard for one agent to juggle multiple concerns

What if you could just... run more agents in parallel?

---

## The Hive Approach

Hive is opinionated about how AI agents should collaborate:

### 🧠 One Architect
- Plans and coordinates all work
- Researches the codebase before proposing tasks
- Writes clear, scoped tasks with acceptance criteria
- **Does NOT write code** — planning only
- Claude recommended for stronger reasoning

### 🐝 Many Workers
- Each worker has a "lane" (api, auth, frontend, etc.)
- Claims ONE task at a time from their lane's backlog
- Implements the task and pushes a PR
- Never starts new work until previous PR is pushed

### 👁 One Watcher
- Monitors the task queue for changes
- Nudges idle workers when new tasks appear
- Keeps the hive productive

### Why separate architect from workers?

When an AI both plans AND executes, it often rushes into coding without proper research. The architect/worker split enforces a planning phase. The architect must understand the codebase and get your approval before adding tasks. Workers just execute.

---

## Quick Start

```bash
# Install hive
curl -fsSL https://raw.githubusercontent.com/joshdholtz/hive/main/install.sh | bash

# Set up your project
cd your-project
hive init

# Start the swarm
hive up

# Attach to watch
tmux attach -t your-project
```

The `hive init` wizard will guide you through:
1. Choosing backends for architect and workers (Claude or Codex)
2. Configuring your task source (YAML file or GitHub Projects)
3. Setting up workers with optional git worktrees for parallel development

---

## Commands

| Command | Description |
|---------|-------------|
| `hive init` | Interactive setup wizard |
| `hive deinit` | Remove hive config and generated files |
| `hive up` | Start the architect, workers, and watcher |
| `hive stop` | Stop the tmux session |
| `hive status` | Show worker status and task counts |
| `hive nudge [worker]` | Nudge idle workers to check for tasks |
| `hive role [worker]` | Regenerate HIVE_ARCHITECT.md and HIVE_WORKER.md files |
| `hive doctor` | Check and fix common issues (missing files, etc.) |

---

## How It Works

1. **`hive up`** starts a tmux session with:
   - **Architect window** — AI that plans work and writes tasks
   - **Worker windows** — One or more panes, each running an AI agent
   - **Watch window** — Monitors tasks and nudges idle workers

2. **The architect** reads `HIVE_ARCHITECT.md` and waits for your instructions. Tell it what you want to build, and it will research the codebase, propose tasks, and (with your approval) add them to the task file.

3. **The watcher** monitors the task file. When it sees tasks in a lane's backlog and that lane's worker is idle, it sends a nudge.

4. **Workers** receive nudges, claim a task, implement it, and push a PR. Then they wait for the next nudge.

---

## Configuration

Hive uses `.hive.yaml` in your project root:

```yaml
architect:
  backend: claude          # Claude recommended for planning

workers:
  backend: claude          # Can use codex if preferred

session: my-project

tasks:
  source: yaml
  file: ./hive_tasks.yaml

windows:
  - name: backend
    layout: even-horizontal
    workers:
      - id: backend-api
        dir: ./backend-api
        lane: api
      - id: backend-auth
        dir: ./backend-auth
        lane: auth
```

### Setup Commands

Run commands before starting the session (e.g., install dependencies):

```yaml
setup:
  - mise install
  - npm install
```

Commands run in order from the project root. If any command fails, `hive up` stops.

### Worker Instructions

Add custom instructions sent to workers on startup and with each nudge:

```yaml
worker_instructions: |
  Always run mix test before pushing.
  Use conventional commit format.
  Target the develop branch for PRs.
```

### Task Sources

**YAML (default)** — Simple file-based task queue:

```yaml
tasks:
  source: yaml
  file: ./hive_tasks.yaml
```

Task file structure:
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

**GitHub Projects** — Use a GitHub Project board:

```yaml
tasks:
  source: github
  github_org: your-org
  github_project: 4
  github_project_id: PVT_kwXXXXXX
  github_status_field_id: PVTSSF_statusXXX
  github_lane_field_id: PVTSSF_laneXXX
```

### Branch Naming

Configure branch conventions per worker for clean git history:

```yaml
workers:
  - id: backend-api
    dir: ./backend-api
    lane: api
    branch:
      local: "backend-api/api"
      remote: "api"
```

Workers will create branches like `backend-api/api/add-users` and push to `api/add-users`.

---

## Git Worktrees

When you have multiple workers, each needs their own checkout of the repo to avoid conflicts. Hive's init wizard can create git worktrees for you:

```
my-project/
├── backend/              # Main repo
├── backend-api/          # Worktree for API worker
├── backend-auth/         # Worktree for Auth worker
└── backend-tests/        # Worktree for Tests worker
```

Each worktree is a full checkout on its own branch, so workers can commit and push independently.

---

## Best Practices

**Use Claude for the architect.** Planning requires stronger reasoning than execution. Claude excels at understanding codebases and scoping work.

**Keep tasks small and focused.** "Add user authentication" is too big. "Add POST /login endpoint with JWT" is better.

**One task per worker at a time.** Workers should finish and push before claiming new work. This keeps PRs reviewable.

**Let the architect propose, you approve.** Don't let the architect add tasks without your confirmation. Review the task list before it goes to workers.

**Use lanes to organize work.** Group related tasks (api, frontend, tests) so specialized workers can focus.

---

## Dependencies

**Required:**
- `bash` 3.2+ (macOS default works)
- [`yq`](https://github.com/mikefarah/yq) — YAML processing (`brew install yq`)
- [`tmux`](https://github.com/tmux/tmux) — Terminal multiplexer (`brew install tmux`)
- [`tmuxp`](https://github.com/tmux-python/tmuxp) — tmux session manager (`pip install tmuxp`)
- `claude` or `codex` — AI CLI

**For GitHub Projects:**
- [`gh`](https://cli.github.com/) — GitHub CLI (`brew install gh`)
- [`jq`](https://jqlang.github.io/jq/) — JSON processing (`brew install jq`)

**Optional:**
- [`fswatch`](https://github.com/emcrisostomo/fswatch) — Efficient file watching (`brew install fswatch`)
- [`gum`](https://github.com/charmbracelet/gum) — Pretty prompts in init wizard (`brew install gum`)

---

## Examples

### Simple: Single Worker

```yaml
architect:
  backend: claude

workers:
  backend: claude

session: my-app

tasks:
  source: yaml
  file: ./hive_tasks.yaml

windows:
  - name: dev
    workers:
      - id: main
        dir: .
        lane: default
```

### Multi-Worker with Worktrees

```yaml
architect:
  backend: claude

workers:
  backend: codex

session: backend-hive

tasks:
  source: yaml
  file: ./hive_tasks.yaml

windows:
  - name: backend
    layout: even-horizontal
    workers:
      - id: backend-api
        dir: ./backend-api
        lane: api
        branch:
          local: "backend-api/api"
          remote: "api"

      - id: backend-auth
        dir: ./backend-auth
        lane: auth
        branch:
          local: "backend-auth/auth"
          remote: "auth"

      - id: backend-tests
        dir: ./backend-tests
        lane: tests
        branch:
          local: "backend-tests/tests"
          remote: "tests"
```

---

## Uninstall

```bash
rm ~/.local/bin/hive
```

Or if you used `hive init` in a project:

```bash
hive deinit   # Removes config, role files, and optionally worktrees
```

---

## License

MIT
