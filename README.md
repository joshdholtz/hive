# Hive

A CLI tool for orchestrating multiple AI workers (Claude or Codex) through tmux sessions.

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/joshdholtz/hive/main/install.sh | bash
```

Or manually:
```bash
curl -fsSL https://raw.githubusercontent.com/joshdholtz/hive/main/hive -o ~/.local/bin/hive
chmod +x ~/.local/bin/hive
```

To uninstall:
```bash
rm ~/.local/bin/hive
```

## Quick Start

```bash
cd my-project
hive init      # Create .hive.yaml interactively
hive up        # Start workers
tmux attach    # Attach to session
```

## Commands

| Command | Description |
|---------|-------------|
| `hive init` | Interactive wizard to create .hive.yaml |
| `hive up` | Start worker session |
| `hive stop` | Stop worker session |
| `hive nudge [worker]` | Nudge idle workers (or specific one) |
| `hive status` | Show worker status and task counts |
| `hive role [worker]` | Generate CLAUDE_ROLE.md for worker(s) |

## Configuration

Place `.hive.yaml` in your project root:

```yaml
backend: claude              # or "codex"
session: my-project

tasks:
  source: yaml               # or "github"
  file: ./tasks.yaml

windows:
  - name: workers
    layout: even-horizontal
    workers:
      - id: backend-a
        dir: ./backend-clone-a
        lane: reliability
        branch:
          local: "background-a/reliability"
          remote: "a/reliability"

      - id: backend-b
        dir: ./backend-clone-b
        lane: features
```

### Task Sources

**YAML (default)**
```yaml
tasks:
  source: yaml
  file: ./tasks.yaml
```

Tasks file structure:
```yaml
reliability:
  backlog:
    - title: Fix memory leak
  in_progress: []
  done: []
```

**GitHub Projects**
```yaml
tasks:
  source: github
  github_org: My-Organization
  github_project: 4
  github_project_id: PVT_kwXXXXXX
  github_status_field_id: PVTSSF_statusXXX
  github_lane_field_id: PVTSSF_laneXXX
```

### Branch Naming

Configure branch naming conventions per worker:

```yaml
workers:
  - id: backend-a
    branch:
      local: "background-a/reliability"
      remote: "a/reliability"
```

Workers will be instructed to:
- Create local branches like `background-a/reliability/my-feature`
- Push with: `git push origin background-a/reliability/my-feature:a/reliability/my-feature`

## How It Works

1. **`hive up`** generates a tmuxp config and starts a tmux session with:
   - One pane per worker, each running claude/codex
   - A watch pane that monitors the task source

2. **`hive watch`** (runs automatically) monitors the task source and:
   - For YAML: watches file changes with fswatch or polling
   - For GitHub: polls the project every 60 seconds

3. **`hive nudge`** checks each worker's lane for:
   - Backlog items > 0
   - In-progress items == 0 (worker is idle)
   - Sends a message to the tmux pane if conditions are met

4. Workers receive messages instructing them to:
   - Push any previous work to a PR first
   - Claim one task from their backlog
   - Use the configured branch naming convention

## Dependencies

Required:
- `bash` 4.0+
- `yq` - YAML parsing (`brew install yq`)
- `tmux` - Terminal multiplexing (`brew install tmux`)
- `tmuxp` - tmux session management (`pip install tmuxp`)
- `claude` or `codex` - AI CLI

For GitHub task source:
- `gh` - GitHub CLI (`brew install gh`)
- `jq` - JSON processing (`brew install jq`)

Optional:
- `fswatch` - Efficient file watching (`brew install fswatch`)

## Examples

See the `examples/` directory for sample configurations.
