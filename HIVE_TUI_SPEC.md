# Hive TUI Specification

**Goal**: Rewrite hive as a standalone Rust TUI application that replaces tmux/tmuxp dependency with native terminal multiplexing.

## Table of Contents

1. [Overview](#overview)
2. [Current Behavior (Bash + tmux)](#current-behavior-bash--tmux)
3. [New Design (Rust TUI)](#new-design-rust-tui)
4. [Technical Stack](#technical-stack)
5. [Architecture](#architecture)
6. [Data Structures](#data-structures)
7. [Configuration Format](#configuration-format)
8. [Commands](#commands)
9. [PTY Management](#pty-management)
10. [Layout System](#layout-system)
11. [Task Watching](#task-watching)
12. [Nudging System](#nudging-system)
13. [UI Components](#ui-components)
14. [Keybindings](#keybindings)
15. [Session Persistence](#session-persistence)
16. [Implementation Phases](#implementation-phases)

---

## Overview

Hive orchestrates multiple AI agents (Claude or Codex) working in parallel on a codebase:

- **Architect**: Plans work, adds tasks to queue (does NOT write code)
- **Workers**: Execute tasks from their assigned lane, push PRs
- **Watcher**: Monitors task queue, nudges idle workers

Current implementation uses bash + tmux + tmuxp. New implementation will be a native Rust TUI that manages PTY processes directly.

### Benefits of Rust TUI over tmux

1. **Native task integration** - Display task status directly in UI
2. **Built-in watcher** - No separate process needed
3. **Instant layout switching** - No pane shuffling
4. **Session persistence** - Detach/reattach like tmux
5. **Custom keybindings** - Hive-specific shortcuts
6. **Better UX** - Task preview, status bar, etc.

---

## Current Behavior (Bash + tmux)

### Commands

| Command | Description |
|---------|-------------|
| `hive init` | Interactive setup wizard - creates `.hive.yaml` |
| `hive deinit` | Remove hive config and generated files |
| `hive up` | Start tmux session with architect, workers, watcher |
| `hive stop` | Kill tmux session |
| `hive status` | Show worker status and task counts |
| `hive nudge [worker]` | Send message to idle workers with pending tasks |
| `hive watch` | Run watcher loop (internal, started by `up`) |
| `hive role [worker]` | Generate WORKER.md and ARCHITECT.md files |
| `hive doctor` | Check and fix common issues |
| `hive layout [mode]` | Switch between default/custom layout |

### Timing Parameters

```bash
DEBOUNCE_SECONDS=10      # Minimum time between nudges
SETTLE_SECONDS=5         # Wait after file change before nudging
GITHUB_POLL_INTERVAL=60  # Seconds between GitHub API polls
MAX_RETRIES=60           # Max retries waiting for windows
RETRY_INTERVAL=2         # Seconds between retries
```

### Default Messages

**Startup message** (sent when worker starts):
```
Read .hive/workers/{lane}/WORKER.md if it exists. You are assigned to lane '{lane}'.
Check your task backlog. If backlog is EMPTY, report 'No tasks in backlog for {lane}'
and STOP - do NOT explore or look for other work. If tasks exist, claim ONE task and
work on it. When finished, create a git branch, commit, push, and create a Pull Request.
```

**Nudge message** (sent when tasks available):
```
FIRST: If you have uncommitted changes or an unpushed branch from a previous task,
you MUST create a PR NOW using 'gh pr create' before starting anything new.
You have {backlog_count} task(s) in your backlog for lane '{lane}'. Claim ONE task
and work on it. REMINDER: When done, create a branch, commit, push, and run
'gh pr create' - do NOT stop until the PR URL is displayed.
```

### Architect Message

```
Read .hive/ARCHITECT.md. You are the architect - plan tasks but do NOT edit code.
Add tasks to the tasks file for workers to pick up.
```

### Layout Modes

1. **default**: All panes in one window
   - Architect on left (50% width)
   - Workers stacked on right
   - Uses tmux `main-vertical` layout

2. **custom**: Separate windows
   - Architect in own window
   - Workers grouped into configured windows
   - Each window has its own layout (default: `even-horizontal`)

### Agent Commands

**Claude**:
```bash
claude "message"
```

**Codex**:
```bash
env -u CODEX_SANDBOX -u CODEX_SANDBOX_NETWORK_DISABLED codex --sandbox danger-full-access --ask-for-approval never "message"
```

### Task Counts Logic

```bash
# Only nudge if:
# 1. Lane has tasks in backlog (backlog_count > 0)
# 2. Lane has no tasks in progress (in_progress_count == 0)
if [[ "$backlog_count" -gt 0 && "$in_progress_count" -eq 0 ]]; then
  # Send nudge message
fi
```

### Git Exclude Setup

Hive adds `.hive/` to `.git/info/exclude` (local only, not committed). For worktrees, it resolves to the main repo's exclude file.

---

## New Design (Rust TUI)

### High-Level Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                          Hive TUI                                 │
├──────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐   │
│  │    Architect    │  │    Worker 1     │  │    Worker 2     │   │
│  │  ┌───────────┐  │  │  ┌───────────┐  │  │  ┌───────────┐  │   │
│  │  │    PTY    │  │  │  │    PTY    │  │  │  │    PTY    │  │   │
│  │  │  (claude) │  │  │  │  (claude) │  │  │  │  (claude) │  │   │
│  │  └───────────┘  │  │  └───────────┘  │  │  └───────────┘  │   │
│  │  Lane: -        │  │  Lane: api      │  │  Lane: auth     │   │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘   │
├──────────────────────────────────────────────────────────────────┤
│  Status: 3 workers | api: 2 backlog | auth: 0 backlog | Watching │
└──────────────────────────────────────────────────────────────────┘
```

### Process Architecture

```
hive up (foreground)     hive up --daemon (background)
       │                         │
       ▼                         ▼
┌─────────────┐           ┌─────────────┐
│   TUI App   │           │   Daemon    │
│  (ratatui)  │           │  (headless) │
└──────┬──────┘           └──────┬──────┘
       │                         │
       ├──────────┬──────────────┤
       ▼          ▼              ▼
   ┌──────┐  ┌──────┐       ┌──────┐
   │ PTY  │  │ PTY  │  ...  │ PTY  │
   │(arch)│  │(wrk1)│       │(wrkN)│
   └──────┘  └──────┘       └──────┘
```

### Client/Server Model (Optional, Phase 2)

Similar to Zellij, support detach/attach:

```
hive up              # Starts server + attaches client
hive attach          # Attach to existing server
hive detach          # Detach client (Ctrl+B d)
```

---

## Technical Stack

### Required Crates

```toml
[dependencies]
# TUI framework
ratatui = "0.29"
crossterm = "0.28"

# PTY management
portable-pty = "0.8"

# Async runtime
tokio = { version = "1", features = ["full"] }

# Configuration
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"

# File watching
notify = "7"

# CLI parsing
clap = { version = "4", features = ["derive"] }

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Error handling
anyhow = "1"
thiserror = "1"

# Terminal detection
termsize = "0.1"
```

### Optional Crates

```toml
# GitHub API (for github task source)
octocrab = "0.41"

# Better terminal handling
vt100 = "0.15"  # VT100 terminal emulator for PTY output parsing
```

---

## Architecture

### Module Structure

```
hive-tui/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point, CLI parsing
│   ├── lib.rs               # Library exports
│   │
│   ├── app/
│   │   ├── mod.rs           # App module exports
│   │   ├── state.rs         # Application state
│   │   ├── actions.rs       # User actions/events
│   │   └── handler.rs       # Event handling
│   │
│   ├── config/
│   │   ├── mod.rs           # Config module exports
│   │   ├── parser.rs        # .hive.yaml parsing
│   │   └── validation.rs    # Config validation
│   │
│   ├── pty/
│   │   ├── mod.rs           # PTY module exports
│   │   ├── manager.rs       # PTY process management
│   │   ├── pane.rs          # Individual pane/PTY
│   │   └── output.rs        # Output buffer management
│   │
│   ├── tasks/
│   │   ├── mod.rs           # Tasks module exports
│   │   ├── yaml.rs          # YAML task source
│   │   ├── github.rs        # GitHub Projects task source
│   │   └── watcher.rs       # File/poll watching
│   │
│   ├── ui/
│   │   ├── mod.rs           # UI module exports
│   │   ├── layout.rs        # Layout management
│   │   ├── pane.rs          # Pane widget
│   │   ├── status_bar.rs    # Status bar widget
│   │   ├── tab_bar.rs       # Tab/window bar
│   │   └── help.rs          # Help overlay
│   │
│   ├── commands/
│   │   ├── mod.rs           # Command module exports
│   │   ├── init.rs          # hive init
│   │   ├── up.rs            # hive up
│   │   ├── stop.rs          # hive stop
│   │   ├── nudge.rs         # hive nudge
│   │   ├── status.rs        # hive status
│   │   ├── role.rs          # hive role
│   │   └── doctor.rs        # hive doctor
│   │
│   └── utils/
│       ├── mod.rs           # Utils module exports
│       ├── git.rs           # Git helpers (exclude, worktrees)
│       └── shell.rs         # Shell/env helpers
```

### Core Types

```rust
// src/app/state.rs

pub struct App {
    pub config: HiveConfig,
    pub layout_mode: LayoutMode,
    pub panes: Vec<Pane>,
    pub focused_pane: usize,
    pub watcher: TaskWatcher,
    pub running: bool,
}

pub enum LayoutMode {
    Default,  // All in one view
    Custom,   // Separate windows/tabs
}

// src/pty/pane.rs

pub struct Pane {
    pub id: String,
    pub pane_type: PaneType,
    pub pty: PtyPair,
    pub output_buffer: OutputBuffer,
    pub lane: Option<String>,
    pub working_dir: PathBuf,
}

pub enum PaneType {
    Architect,
    Worker { lane: String },
    Watcher,
}

// src/pty/output.rs

pub struct OutputBuffer {
    pub lines: VecDeque<String>,
    pub max_lines: usize,
    pub scroll_offset: usize,
}
```

---

## Data Structures

### Configuration Structures

```rust
// src/config/parser.rs

#[derive(Debug, Deserialize)]
pub struct HiveConfig {
    pub architect: ArchitectConfig,
    pub workers: WorkersConfig,
    pub session: String,
    pub tasks: TasksConfig,
    pub windows: Vec<WindowConfig>,
    pub setup: Option<Vec<String>>,
    pub messages: Option<MessagesConfig>,
    pub worker_instructions: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ArchitectConfig {
    pub backend: Backend,
}

#[derive(Debug, Deserialize)]
pub struct WorkersConfig {
    pub backend: Backend,
}

#[derive(Debug, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Claude,
    Codex,
}

#[derive(Debug, Deserialize)]
pub struct TasksConfig {
    pub source: TaskSource,
    pub file: Option<String>,
    pub github_org: Option<String>,
    pub github_project: Option<u32>,
    pub github_project_id: Option<String>,
    pub github_status_field_id: Option<String>,
    pub github_lane_field_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskSource {
    Yaml,
    Github,
}

#[derive(Debug, Deserialize)]
pub struct WindowConfig {
    pub name: String,
    pub layout: Option<String>,  // "even-horizontal", "even-vertical", etc.
    pub workers: Vec<WorkerConfig>,
}

#[derive(Debug, Deserialize)]
pub struct WorkerConfig {
    pub id: String,
    pub dir: Option<String>,
    pub lane: Option<String>,
    pub branch: Option<BranchConfig>,
}

#[derive(Debug, Deserialize)]
pub struct BranchConfig {
    pub local: String,
    pub remote: String,
}

#[derive(Debug, Deserialize)]
pub struct MessagesConfig {
    pub startup: Option<String>,
    pub nudge: Option<String>,
}
```

### Task Structures

```rust
// src/tasks/yaml.rs

#[derive(Debug, Deserialize, Serialize)]
pub struct TasksFile {
    pub worker_protocol: Option<WorkerProtocol>,
    pub rules: Option<Vec<String>>,
    pub global_backlog: Option<Vec<Task>>,
    #[serde(flatten)]
    pub lanes: HashMap<String, LaneTasks>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LaneTasks {
    pub backlog: Vec<Task>,
    pub in_progress: Vec<Task>,
    pub done: Vec<Task>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub acceptance: Option<Vec<String>>,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<String>,
    pub summary: Option<String>,
    pub files_changed: Option<Vec<String>>,
    pub question: Option<String>,
}

// Task counts for a lane
pub struct TaskCounts {
    pub backlog: usize,
    pub in_progress: usize,
    pub done: usize,
}
```

---

## Configuration Format

The `.hive.yaml` format remains the same:

```yaml
architect:
  backend: claude          # claude or codex

workers:
  backend: claude          # claude or codex

session: my-project

tasks:
  source: yaml             # yaml or github
  file: .hive/tasks.yaml   # for yaml source

# For GitHub source:
# tasks:
#   source: github
#   github_org: my-org
#   github_project: 4
#   github_project_id: PVT_xxx
#   github_status_field_id: PVTSSF_xxx
#   github_lane_field_id: PVTSSF_xxx

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

# Optional: Run before starting
setup:
  - mise install
  - npm install

# Optional: Custom messages
messages:
  startup: |
    Read .hive/workers/{lane}/WORKER.md if it exists...
  nudge: |
    You have {backlog_count} task(s)...

# Optional: Added to all WORKER.md files
worker_instructions: |
  Always run tests before pushing.
```

---

## Commands

### `hive up`

1. Load and validate `.hive.yaml`
2. Run setup commands if configured
3. Ensure `.hive/` is in `.git/info/exclude`
4. Prompt for layout mode (default/custom)
5. Spawn PTY for architect with startup message
6. Spawn PTY for each worker with startup message
7. Start task watcher (built-in, not separate pane)
8. Enter TUI event loop
9. On exit, kill all PTY processes

### `hive stop`

1. Find running hive session
2. Send SIGTERM to all PTY processes
3. Clean up

### `hive status`

Print to stdout (non-TUI):
```
Session: my-project
Backend: claude/codex
Task Source: yaml
Status: RUNNING/STOPPED

WORKER              LANE            BACKLOG     IN_PROGRESS
------              ----            -------     -----------
backend-api         api             2           0
backend-auth        auth            0           1
```

### `hive nudge [worker]`

1. Load config
2. For each worker (or specific worker):
   - Get task counts for lane
   - If backlog > 0 AND in_progress == 0:
     - Build nudge message with template vars
     - Send to PTY stdin
3. Print summary

### `hive init`

Interactive wizard (can remain bash for now, or port to Rust with `dialoguer` crate):

1. Choose architect backend
2. Choose workers backend
3. Set session name
4. Choose task source (yaml/github)
5. Scan directories, select workers
6. Configure worktrees if needed
7. Group workers into windows
8. Generate `.hive.yaml`
9. Generate task file if yaml source
10. Offer to generate role files

### `hive role [worker]`

Generate `.hive/ARCHITECT.md` and `.hive/workers/*/WORKER.md` files based on config.

### `hive doctor`

Check and fix:
1. tasks.yaml exists
2. ARCHITECT.md exists
3. WORKER.md for each worker exists
4. .hive/ in .git/info/exclude
5. messages section in config
6. All dependencies available

### `hive layout [default|custom]`

Switch layout mode while running:
- In TUI: Rearrange panes
- Send command via socket/file if using client/server model

---

## PTY Management

### Spawning a PTY

```rust
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

fn spawn_agent(backend: Backend, message: &str, working_dir: &Path) -> Result<PtyPair> {
    let pty_system = native_pty_system();

    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let cmd = match backend {
        Backend::Claude => {
            let mut cmd = CommandBuilder::new("claude");
            cmd.arg(message);
            cmd.cwd(working_dir);
            cmd
        }
        Backend::Codex => {
            let mut cmd = CommandBuilder::new("env");
            cmd.args([
                "-u", "CODEX_SANDBOX",
                "-u", "CODEX_SANDBOX_NETWORK_DISABLED",
                "codex",
                "--sandbox", "danger-full-access",
                "--ask-for-approval", "never",
                message,
            ]);
            cmd.cwd(working_dir);
            cmd
        }
    };

    let child = pair.slave.spawn_command(cmd)?;

    Ok(PtyPair {
        master: pair.master,
        child,
    })
}
```

### Reading PTY Output

```rust
use std::io::Read;
use tokio::sync::mpsc;

async fn read_pty_output(
    mut reader: Box<dyn Read + Send>,
    tx: mpsc::Sender<PaneEvent>,
    pane_id: String,
) {
    let mut buf = [0u8; 4096];
    loop {
        match reader.read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(n) => {
                let data = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = tx.send(PaneEvent::Output { pane_id: pane_id.clone(), data }).await;
            }
            Err(e) => {
                let _ = tx.send(PaneEvent::Error { pane_id: pane_id.clone(), error: e.to_string() }).await;
                break;
            }
        }
    }
    let _ = tx.send(PaneEvent::Exited { pane_id }).await;
}
```

### Writing to PTY (Nudge)

```rust
fn send_to_pane(pane: &mut Pane, message: &str) -> Result<()> {
    use std::io::Write;

    let mut writer = pane.pty.master.take_writer()?;
    writeln!(writer, "{}", message)?;
    Ok(())
}
```

### Resizing PTY

```rust
fn resize_pane(pane: &mut Pane, rows: u16, cols: u16) -> Result<()> {
    pane.pty.master.resize(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;
    Ok(())
}
```

---

## Layout System

### Default Layout

All panes in one view:
- Architect on left (50% width)
- Workers stacked vertically on right

```
┌─────────────────────┬─────────────────────┐
│                     │      Worker 1       │
│                     ├─────────────────────┤
│     Architect       │      Worker 2       │
│                     ├─────────────────────┤
│                     │      Worker 3       │
└─────────────────────┴─────────────────────┘
```

### Custom Layout

Tabs/windows as configured:
- Each window is a tab
- Workers within window arranged by layout setting

```
[Architect] [Backend] [Frontend] [Watch]

┌─────────────────────────────────────────────┐
│  Worker 1  │  Worker 2  │  Worker 3         │
│  (api)     │  (auth)    │  (tests)          │
└─────────────────────────────────────────────┘
```

### Layout Calculation

```rust
pub fn calculate_layout(
    area: Rect,
    layout_mode: LayoutMode,
    panes: &[Pane],
    focused_window: usize,
) -> Vec<(usize, Rect)> {
    match layout_mode {
        LayoutMode::Default => calculate_default_layout(area, panes),
        LayoutMode::Custom => calculate_custom_layout(area, panes, focused_window),
    }
}

fn calculate_default_layout(area: Rect, panes: &[Pane]) -> Vec<(usize, Rect)> {
    let mut result = Vec::new();

    // Find architect
    let architect_idx = panes.iter().position(|p| matches!(p.pane_type, PaneType::Architect));

    // Workers (excluding architect and watcher)
    let worker_indices: Vec<usize> = panes.iter()
        .enumerate()
        .filter(|(_, p)| matches!(p.pane_type, PaneType::Worker { .. }))
        .map(|(i, _)| i)
        .collect();

    // Split area: 50% left for architect, 50% right for workers
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Architect gets left side
    if let Some(idx) = architect_idx {
        result.push((idx, chunks[0]));
    }

    // Workers split vertically on right
    if !worker_indices.is_empty() {
        let worker_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Ratio(1, worker_indices.len() as u32); worker_indices.len()])
            .split(chunks[1]);

        for (i, &idx) in worker_indices.iter().enumerate() {
            result.push((idx, worker_chunks[i]));
        }
    }

    result
}
```

---

## Task Watching

### YAML File Watching

```rust
use notify::{Watcher, RecursiveMode, watcher};
use std::sync::mpsc::channel;
use std::time::Duration;

pub struct YamlTaskWatcher {
    tasks_file: PathBuf,
    debounce_secs: u64,
    last_nudge: Instant,
}

impl YamlTaskWatcher {
    pub fn new(tasks_file: PathBuf) -> Self {
        Self {
            tasks_file,
            debounce_secs: 10,
            last_nudge: Instant::now() - Duration::from_secs(100),
        }
    }

    pub async fn watch(&mut self, nudge_tx: mpsc::Sender<NudgeRequest>) -> Result<()> {
        let (tx, rx) = channel();

        let mut watcher = notify::recommended_watcher(tx)?;
        watcher.watch(&self.tasks_file, RecursiveMode::NonRecursive)?;

        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(_event) => {
                    let elapsed = self.last_nudge.elapsed().as_secs();
                    if elapsed >= self.debounce_secs {
                        // Wait for file to settle
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        self.last_nudge = Instant::now();
                        let _ = nudge_tx.send(NudgeRequest::All).await;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
}
```

### GitHub Polling

```rust
pub struct GitHubTaskWatcher {
    config: GitHubConfig,
    poll_interval_secs: u64,
    cache: Option<GitHubItemsCache>,
}

impl GitHubTaskWatcher {
    pub async fn poll_loop(&mut self, nudge_tx: mpsc::Sender<NudgeRequest>) -> Result<()> {
        loop {
            self.refresh_cache().await?;
            let _ = nudge_tx.send(NudgeRequest::All).await;
            tokio::time::sleep(Duration::from_secs(self.poll_interval_secs)).await;
        }
    }

    async fn refresh_cache(&mut self) -> Result<()> {
        // Use octocrab or gh CLI to fetch project items
        // Parse and cache results
        Ok(())
    }

    pub fn get_task_counts(&self, lane: &str) -> TaskCounts {
        // Query cache for lane's backlog/in_progress counts
        TaskCounts::default()
    }
}
```

---

## Nudging System

### Nudge Logic

```rust
pub async fn nudge_workers(
    app: &mut App,
    specific_worker: Option<&str>,
) -> Result<Vec<String>> {
    let mut nudged = Vec::new();

    for pane in &mut app.panes {
        // Skip if not a worker
        let lane = match &pane.pane_type {
            PaneType::Worker { lane } => lane.clone(),
            _ => continue,
        };

        // Skip if targeting specific worker and this isn't it
        if let Some(target) = specific_worker {
            if pane.id != target {
                continue;
            }
        }

        // Get task counts
        let counts = app.get_task_counts(&lane)?;

        // Only nudge if has backlog and not busy
        if counts.backlog > 0 && counts.in_progress == 0 {
            let message = build_nudge_message(&app.config, &lane, counts.backlog, &pane.branch);
            send_to_pane(pane, &message)?;
            nudged.push(pane.id.clone());
        }
    }

    Ok(nudged)
}

fn build_nudge_message(
    config: &HiveConfig,
    lane: &str,
    backlog_count: usize,
    branch: &Option<BranchConfig>,
) -> String {
    let template = config.messages.as_ref()
        .and_then(|m| m.nudge.as_ref())
        .map(|s| s.as_str())
        .unwrap_or(DEFAULT_NUDGE_MSG);

    let mut msg = template
        .replace("{lane}", lane)
        .replace("{backlog_count}", &backlog_count.to_string());

    // Add branch convention if configured
    if let Some(branch) = branch {
        msg.push_str(&format!(
            " BRANCH CONVENTION: Your LOCAL branch names MUST start with '{}/'. Push to remote with: git push origin {}/my-feature:{}/my-feature",
            branch.local, branch.local, branch.remote
        ));
    }

    // Collapse to single line
    msg.split_whitespace().collect::<Vec<_>>().join(" ")
}
```

---

## UI Components

### Main UI Structure

```rust
pub fn ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Tab bar
            Constraint::Min(0),     // Panes
            Constraint::Length(1),  // Status bar
        ])
        .split(frame.area());

    render_tab_bar(frame, chunks[0], app);
    render_panes(frame, chunks[1], app);
    render_status_bar(frame, chunks[2], app);

    if app.show_help {
        render_help_overlay(frame, app);
    }
}
```

### Pane Widget

```rust
pub fn render_pane(frame: &mut Frame, area: Rect, pane: &Pane, focused: bool) {
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = match &pane.pane_type {
        PaneType::Architect => "architect".to_string(),
        PaneType::Worker { lane } => format!("{} ({})", pane.id, lane),
        PaneType::Watcher => "watcher".to_string(),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    // Render terminal output
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let output = pane.output_buffer.visible_lines(inner.height as usize);
    let paragraph = Paragraph::new(output.join("\n"))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner);
}
```

### Status Bar

```rust
pub fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut parts = Vec::new();

    // Worker count
    let worker_count = app.panes.iter()
        .filter(|p| matches!(p.pane_type, PaneType::Worker { .. }))
        .count();
    parts.push(format!("{} workers", worker_count));

    // Task counts per lane
    for (lane, counts) in &app.task_counts {
        if counts.backlog > 0 {
            parts.push(format!("{}: {} backlog", lane, counts.backlog));
        }
    }

    // Watcher status
    parts.push("Watching".to_string());

    let status = parts.join(" | ");
    let paragraph = Paragraph::new(status)
        .style(Style::default().bg(Color::DarkGray));

    frame.render_widget(paragraph, area);
}
```

---

## Keybindings

### Default Keybindings

| Key | Action |
|-----|--------|
| `Ctrl+B` | Prefix key (like tmux) |
| `Ctrl+B` `n` | Nudge all workers |
| `Ctrl+B` `N` | Nudge focused worker |
| `Ctrl+B` `l` | Toggle layout (default/custom) |
| `Ctrl+B` `1-9` | Focus window/tab |
| `Ctrl+B` `Arrow` | Move focus between panes |
| `Ctrl+B` `z` | Zoom/maximize focused pane |
| `Ctrl+B` `?` | Show help |
| `Ctrl+B` `d` | Detach (if server mode) |
| `Ctrl+B` `q` | Quit |
| `Ctrl+C` | Pass to focused pane |
| `Page Up/Down` | Scroll pane output |

### Input Handling

```rust
pub async fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<bool> {
    if app.prefix_mode {
        app.prefix_mode = false;
        match key.code {
            KeyCode::Char('n') => {
                nudge_workers(app, None).await?;
            }
            KeyCode::Char('N') => {
                let focused_id = app.panes[app.focused_pane].id.clone();
                nudge_workers(app, Some(&focused_id)).await?;
            }
            KeyCode::Char('l') => {
                app.toggle_layout();
            }
            KeyCode::Char('z') => {
                app.toggle_zoom();
            }
            KeyCode::Char('?') => {
                app.show_help = !app.show_help;
            }
            KeyCode::Char('d') => {
                return Ok(true); // Signal detach
            }
            KeyCode::Char('q') => {
                return Ok(true); // Signal quit
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                let idx = c.to_digit(10).unwrap() as usize - 1;
                if idx < app.windows.len() {
                    app.focused_window = idx;
                }
            }
            KeyCode::Up => app.focus_up(),
            KeyCode::Down => app.focus_down(),
            KeyCode::Left => app.focus_left(),
            KeyCode::Right => app.focus_right(),
            _ => {}
        }
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b') {
        app.prefix_mode = true;
    } else {
        // Pass key to focused pane
        send_key_to_pane(&mut app.panes[app.focused_pane], key)?;
    }

    Ok(false)
}
```

---

## Session Persistence

### Server/Client Model (Phase 2)

For detach/attach support, implement a client/server model:

**Server**:
- Manages all PTY processes
- Maintains state
- Listens on Unix socket
- Continues running when client detaches

**Client**:
- Renders TUI
- Sends input to server
- Receives output from server
- Can detach without killing session

### Socket Protocol

```rust
#[derive(Serialize, Deserialize)]
enum ClientMessage {
    Input { pane_id: String, data: Vec<u8> },
    Resize { rows: u16, cols: u16 },
    Nudge { worker: Option<String> },
    Layout { mode: LayoutMode },
    Detach,
}

#[derive(Serialize, Deserialize)]
enum ServerMessage {
    Output { pane_id: String, data: Vec<u8> },
    State { app_state: AppState },
    PaneExited { pane_id: String },
}
```

---

## Implementation Phases

### Phase 1: Core TUI (MVP)

1. Config parsing
2. PTY spawning for claude/codex
3. Basic TUI with pane rendering
4. Default layout only
5. Input passthrough to focused pane
6. `hive up` and `hive stop` commands
7. Basic keybindings (focus, quit)

**Deliverable**: Can start hive, see panes, interact with agents

### Phase 2: Task Integration

1. YAML task file parsing
2. File watching with notify
3. Task counts in status bar
4. Nudge command (`Ctrl+B n`)
5. `hive nudge` CLI command

**Deliverable**: Auto-nudging works, task status visible

### Phase 3: Layout System

1. Custom layout mode (tabs)
2. Layout switching (`Ctrl+B l`)
3. Window configuration from .hive.yaml
4. `hive layout` CLI command

**Deliverable**: Can switch between default/custom layouts

### Phase 4: Polish

1. GitHub Projects support
2. Help overlay
3. Pane scrolling
4. Zoom mode
5. Better status bar
6. `hive init` in Rust
7. `hive doctor` in Rust
8. `hive status` CLI command

**Deliverable**: Feature parity with bash version

### Phase 5: Server Mode (Optional)

1. Client/server architecture
2. Detach/attach
3. `hive attach` command
4. Session persistence

**Deliverable**: Can detach and reattach like tmux

---

## Testing

### Unit Tests

- Config parsing
- Layout calculation
- Message template substitution
- Task counting

### Integration Tests

- PTY spawning
- File watching
- Nudge logic

### Manual Testing Checklist

- [ ] `hive up` starts all panes
- [ ] Architect receives startup message
- [ ] Workers receive startup message
- [ ] Task file changes trigger nudges
- [ ] Debouncing works (no rapid nudges)
- [ ] Layout switching works
- [ ] Keybindings work
- [ ] `Ctrl+C` reaches pane
- [ ] Exit kills all processes
- [ ] Resizing terminal resizes panes

---

## Migration Notes

### Backwards Compatibility

- Same `.hive.yaml` format
- Same `.hive/` directory structure
- Same task file format
- Same CLI interface

### Deprecations

- `hive watch` command becomes internal
- tmuxp dependency removed
- tmux dependency removed (for core functionality)

### New Features

- Built-in task status in UI
- Instant layout switching
- Better keybindings
- Help overlay
