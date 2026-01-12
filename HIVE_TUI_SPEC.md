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
│   ├── main.rs              # Entry point, CLI parsing with clap
│   ├── lib.rs               # Library exports
│   │
│   ├── app/
│   │   ├── mod.rs           # App struct and core methods
│   │   ├── state.rs         # AppState, focus management
│   │   ├── events.rs        # Event types (Key, Pty, Timer)
│   │   └── handler.rs       # Event routing and handling
│   │
│   ├── config/
│   │   ├── mod.rs           # HiveConfig and all config structs
│   │   ├── parser.rs        # .hive.yaml parsing with serde
│   │   └── validation.rs    # Config validation and defaults
│   │
│   ├── pty/
│   │   ├── mod.rs           # PTY module exports
│   │   ├── manager.rs       # Spawn/kill PTY processes
│   │   ├── pane.rs          # Pane struct (PTY + state)
│   │   └── output.rs        # OutputBuffer with scrollback
│   │
│   ├── tasks/
│   │   ├── mod.rs           # TaskSource trait, TaskCounts
│   │   ├── yaml.rs          # YAML file task source
│   │   ├── github.rs        # GitHub Projects task source
│   │   └── watcher.rs       # File watching with notify
│   │
│   ├── ui/
│   │   ├── mod.rs           # Main ui() function
│   │   ├── sidebar.rs       # Sidebar struct and rendering
│   │   ├── pane_widget.rs   # Pane rendering
│   │   ├── layout.rs        # calculate_main_layout()
│   │   ├── status_bar.rs    # Status bar rendering
│   │   ├── title_bar.rs     # Title bar rendering
│   │   └── help.rs          # Help overlay
│   │
│   ├── commands/
│   │   ├── mod.rs           # Command dispatch
│   │   ├── up.rs            # hive up (main TUI loop)
│   │   ├── stop.rs          # hive stop
│   │   ├── status.rs        # hive status (non-TUI)
│   │   ├── nudge.rs         # hive nudge (CLI or in-app)
│   │   ├── init.rs          # hive init (interactive setup)
│   │   ├── role.rs          # hive role (generate .md files)
│   │   └── doctor.rs        # hive doctor
│   │
│   └── utils/
│       ├── mod.rs           # Utility exports
│       ├── git.rs           # Git exclude, worktree detection
│       └── messages.rs      # Message template substitution
```

### Core Types

```rust
// src/app/state.rs

pub struct App {
    pub config: HiveConfig,
    pub panes: Vec<Pane>,
    pub sidebar: Sidebar,
    pub watcher: TaskWatcher,
    pub task_counts: HashMap<String, TaskCounts>,
    pub running: bool,
    pub show_help: bool,
    pub sidebar_visible: bool,
}

impl App {
    /// Get panes that are currently visible (shown in main area)
    pub fn visible_panes(&self) -> Vec<&Pane> {
        self.panes.iter().filter(|p| p.visible).collect()
    }

    /// Get the currently focused pane
    pub fn focused_pane(&self) -> Option<&Pane> {
        self.panes.iter().find(|p| p.focused)
    }

    /// Get mutable reference to focused pane
    pub fn focused_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.focused)
    }
}

// src/ui/sidebar.rs

pub struct Sidebar {
    pub items: Vec<SidebarItem>,
    pub selected_index: usize,
    pub focused: bool,  // Whether sidebar has input focus
}

#[derive(Debug, Clone)]
pub enum SidebarItem {
    Pane {
        pane_id: String,
    },
    Group {
        name: String,
        source_dir: String,
        expanded: bool,
        children: Vec<String>,  // pane_ids
    },
}

// src/pty/pane.rs

pub struct Pane {
    pub id: String,
    pub pane_type: PaneType,
    pub pty: PtyPair,
    pub output_buffer: OutputBuffer,
    pub lane: Option<String>,
    pub working_dir: PathBuf,
    pub branch: Option<BranchConfig>,

    // UI state
    pub visible: bool,      // Shown in main area
    pub focused: bool,      // Receives keyboard input
}

impl Pane {
    pub fn new_architect(pty: PtyPair, working_dir: PathBuf) -> Self {
        Self {
            id: "architect".to_string(),
            pane_type: PaneType::Architect,
            pty,
            output_buffer: OutputBuffer::new(10_000),
            lane: None,
            working_dir,
            branch: None,
            visible: true,   // Architect visible by default
            focused: true,   // Architect focused by default
        }
    }

    pub fn new_worker(
        id: String,
        lane: String,
        pty: PtyPair,
        working_dir: PathBuf,
        branch: Option<BranchConfig>,
    ) -> Self {
        Self {
            id,
            pane_type: PaneType::Worker { lane: lane.clone() },
            pty,
            output_buffer: OutputBuffer::new(10_000),
            lane: Some(lane),
            working_dir,
            branch,
            visible: false,  // Workers hidden by default
            focused: false,
        }
    }
}

pub enum PaneType {
    Architect,
    Worker { lane: String },
}

// src/pty/output.rs

pub struct OutputBuffer {
    pub lines: VecDeque<String>,
    pub max_lines: usize,
    pub scroll_offset: usize,  // For scrollback
}

impl OutputBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max_lines),
            max_lines,
            scroll_offset: 0,
        }
    }

    pub fn push(&mut self, line: String) {
        if self.lines.len() >= self.max_lines {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Get lines visible in a given height, accounting for scroll offset
    pub fn visible_lines(&self, height: usize) -> Vec<&str> {
        let total = self.lines.len();
        let start = total.saturating_sub(height + self.scroll_offset);
        let end = total.saturating_sub(self.scroll_offset);
        self.lines.range(start..end).map(|s| s.as_str()).collect()
    }

    pub fn scroll_up(&mut self, lines: usize) {
        let max_scroll = self.lines.len().saturating_sub(1);
        self.scroll_offset = (self.scroll_offset + lines).min(max_scroll);
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
    }
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

### IDE-Style Layout

The UI follows an IDE-like design with a **sidebar** for navigation and a **main area** for pane display:

```
┌──────────────────────────────────────────────────────────────────────┐
│  Hive - my-project                                        [?] Help   │
├────────────────┬─────────────────────────────────────────────────────┤
│ PANES          │                                                     │
│                │  ┌─────────────────────┬─────────────────────┐      │
│ ▶ ● architect  │  │                     │                     │      │
│                │  │     architect       │    backend-api      │      │
│ ▼ backend (3)  │  │                     │      (api)          │      │
│   ● backend-api│  │                     │                     │      │
│   ○ backend-aut│  ├─────────────────────┼─────────────────────┤      │
│   ○ backend-tes│  │                     │                     │      │
│                │  │   backend-tests     │   backend-auth      │      │
│ ▼ frontend (2) │  │     (tests)         │     (auth)          │      │
│   ○ frontend-we│  │                     │                     │      │
│   ○ frontend-mo│  └─────────────────────┴─────────────────────┘      │
│                │                                                     │
│ ▶ sdks (4)     │                                                     │
│                │                                                     │
├────────────────┴─────────────────────────────────────────────────────┤
│  4 visible │ api: 2 tasks │ auth: 0 │ tests: 1 │ Watching            │
└──────────────────────────────────────────────────────────────────────┘
```

### Sidebar Tree Structure

The sidebar displays panes in a hierarchical tree:

```
PANES
├── ● architect                    # Always at top, standalone
│
├── ▼ backend (3)                  # Group: worktrees from ./backend
│   ├── ● backend-api              # Worktree worker (visible)
│   ├── ○ backend-auth             # Worktree worker (hidden)
│   └── ○ backend-tests            # Worktree worker (hidden)
│
├── ▼ frontend (2)                 # Group: worktrees from ./frontend
│   ├── ○ frontend-web             # Worktree worker
│   └── ○ frontend-mobile          # Worktree worker
│
├── ▶ sdks (4)                     # Group: collapsed (▶)
│
└── ● docs                         # Standalone worker (no worktrees)
```

**Legend:**
- `●` = Visible (displayed in main area)
- `○` = Hidden (PTY still running, just not displayed)
- `▶` = Group collapsed
- `▼` = Group expanded
- `(N)` = Number of workers in group

### Sidebar Interactions

When the sidebar is focused:

| Key | Action |
|-----|--------|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `Space` | Toggle visibility of selected item |
| `Enter` | Toggle visibility AND focus the pane |
| `a` | Select all in group / Select all |
| `n` | Select none in group / Select none |
| `Tab` | Move focus to main area |
| `←` / `h` | Collapse group |
| `→` / `l` | Expand group |

**Group selection behavior:**
- Pressing `Space` on a group header toggles ALL items in that group
- Pressing `a` on a group selects all items in that group
- Pressing `n` on a group deselects all items in that group

### Visibility vs Focus

Two separate concepts:

1. **Visibility** (`visible: bool`): Whether the pane is displayed in the main area
   - Hidden panes still run their PTY process
   - Hidden panes still receive nudges
   - Toggle with `Space` in sidebar

2. **Focus** (`focused: bool`): Which pane receives keyboard input
   - Only one pane can be focused at a time
   - Indicated by highlighted border
   - Change with arrow keys in main area or `Enter` in sidebar

### Main Area Layout

The main area dynamically arranges **only visible panes**:

**1 pane visible:**
```
┌─────────────────────────────────────────┐
│                                         │
│              architect                  │
│                                         │
└─────────────────────────────────────────┘
```

**2 panes visible:**
```
┌───────────────────┬─────────────────────┐
│                   │                     │
│    architect      │    backend-api      │
│                   │                     │
└───────────────────┴─────────────────────┘
```

**3 panes visible:**
```
┌───────────────────┬─────────────────────┐
│                   │    backend-api      │
│    architect      ├─────────────────────┤
│                   │    backend-auth     │
└───────────────────┴─────────────────────┘
```

**4 panes visible:**
```
┌───────────────────┬─────────────────────┐
│    architect      │    backend-api      │
├───────────────────┼─────────────────────┤
│   backend-auth    │   backend-tests     │
└───────────────────┴─────────────────────┘
```

**5+ panes visible:** Grid layout, rows of 2-3

### Layout Algorithm

```rust
pub fn calculate_main_layout(area: Rect, visible_panes: &[&Pane]) -> Vec<Rect> {
    let count = visible_panes.len();

    match count {
        0 => vec![],
        1 => vec![area],
        2 => {
            // Side by side
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area)
                .to_vec()
        }
        3 => {
            // Left pane full height, right split vertically
            let cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let right = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(cols[1]);
            vec![cols[0], right[0], right[1]]
        }
        4 => {
            // 2x2 grid
            let rows = Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let top = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[0]);
            let bottom = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);
            vec![top[0], top[1], bottom[0], bottom[1]]
        }
        n => {
            // Grid: ceil(n/2) rows, 2 columns
            let num_rows = (n + 1) / 2;
            let row_constraints = vec![Constraint::Ratio(1, num_rows as u32); num_rows];
            let rows = Layout::vertical(row_constraints).split(area);

            let mut rects = Vec::new();
            for (i, row) in rows.iter().enumerate() {
                let items_in_row = if i == num_rows - 1 && n % 2 == 1 { 1 } else { 2 };
                let col_constraints = vec![Constraint::Ratio(1, items_in_row as u32); items_in_row];
                let cols = Layout::horizontal(col_constraints).split(*row);
                rects.extend(cols.iter().cloned());
            }
            rects
        }
    }
}
```

### Sidebar Data Structure

```rust
/// Represents an item in the sidebar tree
#[derive(Debug)]
pub enum SidebarItem {
    /// A single pane (architect or standalone worker)
    Pane {
        pane_id: String,
        visible: bool,
    },
    /// A group of related workers (worktrees from same source)
    Group {
        name: String,           // e.g., "backend"
        source_dir: String,     // e.g., "./backend"
        expanded: bool,
        children: Vec<String>,  // pane_ids of workers in this group
    },
}

/// Sidebar state
pub struct Sidebar {
    pub items: Vec<SidebarItem>,
    pub selected_index: usize,
    pub focused: bool,
}

impl Sidebar {
    /// Build sidebar from config, grouping worktrees by source directory
    pub fn from_config(config: &HiveConfig, panes: &[Pane]) -> Self {
        let mut items = Vec::new();

        // Architect always first
        items.push(SidebarItem::Pane {
            pane_id: "architect".to_string(),
            visible: true,
        });

        // Group workers by their source directory (worktree parent)
        // Workers with same source dir prefix are grouped together
        // e.g., backend-api, backend-auth from ./backend become a group

        // ... grouping logic ...

        Self {
            items,
            selected_index: 0,
            focused: false,
        }
    }

    /// Toggle visibility of selected item (or all items in group)
    pub fn toggle_selected(&mut self, panes: &mut [Pane]) {
        match &mut self.items[self.selected_index] {
            SidebarItem::Pane { pane_id, visible } => {
                *visible = !*visible;
                if let Some(pane) = panes.iter_mut().find(|p| &p.id == pane_id) {
                    pane.visible = *visible;
                }
            }
            SidebarItem::Group { children, .. } => {
                // If any visible, hide all. Otherwise, show all.
                let any_visible = children.iter().any(|id| {
                    panes.iter().find(|p| &p.id == id).map(|p| p.visible).unwrap_or(false)
                });
                let new_state = !any_visible;
                for child_id in children {
                    if let Some(pane) = panes.iter_mut().find(|p| &p.id == child_id) {
                        pane.visible = new_state;
                    }
                }
            }
        }
    }
}
```

### Determining Groups from Config

Groups are automatically determined by analyzing worker directory patterns. Workers with similar directory prefixes are grouped together.

**Grouping Rules:**

1. **Worktree pattern**: `./prefix-suffix` → grouped under "prefix"
   - `./backend-api`, `./backend-auth`, `./backend-tests` → group "backend"
   - `./frontend-web`, `./frontend-mobile` → group "frontend"

2. **Standalone**: Workers that don't match a pattern remain ungrouped
   - `./docs` → standalone (no hyphen)
   - `.` → standalone (current directory)

3. **Single match**: If only one worker matches a prefix, it's standalone
   - `./sdk-ios` alone → standalone, not a group of 1

**Example config → sidebar:**

```yaml
windows:
  - name: backend
    workers:
      - id: backend-api
        dir: ./backend-api
        lane: api
      - id: backend-auth
        dir: ./backend-auth
        lane: auth
      - id: backend-tests
        dir: ./backend-tests
        lane: tests
  - name: frontend
    workers:
      - id: frontend-web
        dir: ./frontend-web
        lane: web
  - name: docs
    workers:
      - id: docs
        dir: ./docs
        lane: docs
```

**Results in sidebar:**
```
PANES
├── ● architect
├── ▼ backend (3)           <- Group (3 workers with "backend-" prefix)
│   ├── ○ backend-api
│   ├── ○ backend-auth
│   └── ○ backend-tests
├── ○ frontend-web          <- Standalone (only one "frontend-" worker)
└── ○ docs                  <- Standalone (no hyphen in dir)
```

**Implementation:**

```rust
/// Build sidebar items from config, automatically grouping by directory prefix
pub fn build_sidebar_items(config: &HiveConfig, panes: &[Pane]) -> Vec<SidebarItem> {
    let mut items = Vec::new();

    // Architect always first
    items.push(SidebarItem::Pane {
        pane_id: "architect".to_string(),
    });

    // Collect all workers with their directory prefixes
    let mut prefix_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut standalone: Vec<String> = Vec::new();

    for window in &config.windows {
        for worker in &window.workers {
            let dir = worker.dir.as_deref().unwrap_or(".");
            let dir = dir.strip_prefix("./").unwrap_or(dir);

            if let Some(prefix) = extract_prefix(dir) {
                prefix_map.entry(prefix).or_default().push(worker.id.clone());
            } else {
                standalone.push(worker.id.clone());
            }
        }
    }

    // Groups with 2+ workers become groups, others become standalone
    for (prefix, mut worker_ids) in prefix_map {
        if worker_ids.len() >= 2 {
            worker_ids.sort(); // Consistent ordering
            items.push(SidebarItem::Group {
                name: prefix,
                source_dir: format!("./{}-*", prefix),
                expanded: true,
                children: worker_ids,
            });
        } else {
            standalone.extend(worker_ids);
        }
    }

    // Add standalone workers
    standalone.sort();
    for id in standalone {
        items.push(SidebarItem::Pane { pane_id: id });
    }

    items
}

/// Extract prefix from directory: "backend-api" -> Some("backend")
fn extract_prefix(dir: &str) -> Option<String> {
    // Must contain a hyphen and have content on both sides
    if let Some(idx) = dir.rfind('-') {
        let prefix = &dir[..idx];
        let suffix = &dir[idx + 1..];
        if !prefix.is_empty() && !suffix.is_empty() {
            return Some(prefix.to_string());
        }
    }
    None
}
```

### Alternative: Explicit Groups in Config (Future Enhancement)

Users could optionally specify groups explicitly:

```yaml
# Future: explicit group configuration
sidebar:
  groups:
    - name: Backend Services
      workers: [backend-api, backend-auth, backend-tests]
    - name: SDKs
      workers: [sdk-ios, sdk-android, sdk-web]
```

This would override automatic detection. Not implemented in Phase 1.

### Sidebar Toggle Behavior

```
Scenario: User wants to view all backend workers

Before:
  PANES
  ├── ● architect
  ├── ▼ backend (3)
  │   ├── ○ backend-api
  │   ├── ○ backend-auth
  │   └── ○ backend-tests
  └── ● docs

User presses ↓↓ to select "backend" group, then Space:

After:
  PANES
  ├── ● architect
  ├── ▼ backend (3)
  │   ├── ● backend-api      <- now visible
  │   ├── ● backend-auth     <- now visible
  │   └── ● backend-tests    <- now visible
  └── ● docs

Main area now shows: architect + 3 backend workers (4 panes in grid)
```

### Sidebar Width

The sidebar has a fixed width (configurable):

```rust
const SIDEBAR_WIDTH: u16 = 20;  // Characters

// Can be toggled with Ctrl+B b
pub fn toggle_sidebar(&mut self) {
    self.sidebar_visible = !self.sidebar_visible;
}
```

When sidebar is hidden, main area takes full width.

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
    let area = frame.area();

    // Main layout: title bar, content, status bar
    let main_chunks = Layout::vertical([
        Constraint::Length(1),  // Title bar
        Constraint::Min(0),     // Content (sidebar + panes)
        Constraint::Length(1),  // Status bar
    ]).split(area);

    render_title_bar(frame, main_chunks[0], app);

    // Content: optional sidebar + main pane area
    if app.sidebar_visible {
        let content_chunks = Layout::horizontal([
            Constraint::Length(SIDEBAR_WIDTH),
            Constraint::Min(0),
        ]).split(main_chunks[1]);

        render_sidebar(frame, content_chunks[0], app);
        render_main_area(frame, content_chunks[1], app);
    } else {
        render_main_area(frame, main_chunks[1], app);
    }

    render_status_bar(frame, main_chunks[2], app);

    if app.show_help {
        render_help_overlay(frame, app);
    }
}

const SIDEBAR_WIDTH: u16 = 22;
```

### Title Bar

```rust
pub fn render_title_bar(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(" Hive - {} ", app.config.session);
    let help_hint = "[?] Help  [Ctrl+B b] Toggle Sidebar";

    let title_style = Style::default()
        .fg(Color::White)
        .bg(Color::Blue)
        .add_modifier(Modifier::BOLD);

    // Title on left, help hint on right
    let line = Line::from(vec![
        Span::styled(title, title_style),
        Span::raw(" ".repeat((area.width as usize).saturating_sub(title.len() + help_hint.len() + 2))),
        Span::styled(help_hint, Style::default().fg(Color::DarkGray).bg(Color::Blue)),
    ]);

    frame.render_widget(Paragraph::new(line).style(Style::default().bg(Color::Blue)), area);
}
```

### Sidebar Widget

```rust
pub fn render_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let border_style = if app.sidebar.focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" PANES ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Build list items from sidebar tree
    let mut list_items = Vec::new();
    let mut item_index = 0;

    for item in &app.sidebar.items {
        match item {
            SidebarItem::Pane { pane_id } => {
                let pane = app.panes.iter().find(|p| &p.id == pane_id);
                let visible = pane.map(|p| p.visible).unwrap_or(false);
                let focused = pane.map(|p| p.focused).unwrap_or(false);

                let icon = if visible { "●" } else { "○" };
                let style = if app.sidebar.selected_index == item_index {
                    Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
                } else if focused {
                    Style::default().fg(Color::Yellow)
                } else if visible {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                list_items.push(ListItem::new(format!("{} {}", icon, pane_id)).style(style));
                item_index += 1;
            }
            SidebarItem::Group { name, expanded, children, .. } => {
                let arrow = if *expanded { "▼" } else { "▶" };
                let visible_count = children.iter()
                    .filter(|id| app.panes.iter().find(|p| &p.id == *id).map(|p| p.visible).unwrap_or(false))
                    .count();

                let style = if app.sidebar.selected_index == item_index {
                    Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan)
                };

                list_items.push(ListItem::new(format!(
                    "{} {} ({}/{})",
                    arrow, name, visible_count, children.len()
                )).style(style));
                item_index += 1;

                // Show children if expanded
                if *expanded {
                    for child_id in children {
                        let pane = app.panes.iter().find(|p| &p.id == child_id);
                        let visible = pane.map(|p| p.visible).unwrap_or(false);
                        let focused = pane.map(|p| p.focused).unwrap_or(false);

                        let icon = if visible { "●" } else { "○" };
                        let style = if app.sidebar.selected_index == item_index {
                            Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
                        } else if focused {
                            Style::default().fg(Color::Yellow)
                        } else if visible {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default().fg(Color::DarkGray)
                        };

                        // Indent children
                        list_items.push(ListItem::new(format!("  {} {}", icon, child_id)).style(style));
                        item_index += 1;
                    }
                }
            }
        }
    }

    let list = List::new(list_items);
    frame.render_widget(list, inner);
}
```

### Main Pane Area

```rust
pub fn render_main_area(frame: &mut Frame, area: Rect, app: &App) {
    let visible_panes: Vec<&Pane> = app.panes.iter().filter(|p| p.visible).collect();

    if visible_panes.is_empty() {
        // Show hint when no panes visible
        let hint = Paragraph::new("No panes visible.\n\nUse sidebar to select panes to display.")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, area);
        return;
    }

    // Calculate layout for visible panes
    let rects = calculate_main_layout(area, &visible_panes);

    // Render each pane
    for (pane, rect) in visible_panes.iter().zip(rects.iter()) {
        render_pane(frame, *rect, pane);
    }
}
```

### Pane Widget

```rust
pub fn render_pane(frame: &mut Frame, area: Rect, pane: &Pane) {
    let border_style = if pane.focused {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = match &pane.pane_type {
        PaneType::Architect => " architect ".to_string(),
        PaneType::Worker { lane } => format!(" {} ({}) ", pane.id, lane),
    };

    // Show task count in title for workers
    let title_with_tasks = if let Some(lane) = &pane.lane {
        // Would need access to task_counts here, or include in pane
        title
    } else {
        title
    };

    let block = Block::default()
        .title(title_with_tasks)
        .borders(Borders::ALL)
        .border_style(border_style)
        .border_type(if pane.focused { BorderType::Thick } else { BorderType::Plain });

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Render terminal output
    let height = inner.height as usize;
    let output = pane.output_buffer.visible_lines(height);

    // Join with newlines and render
    let text = output.join("\n");
    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, inner);

    // Show scroll indicator if not at bottom
    if pane.output_buffer.scroll_offset > 0 {
        let indicator = format!("↑{}", pane.output_buffer.scroll_offset);
        let indicator_area = Rect::new(
            inner.x + inner.width - indicator.len() as u16 - 1,
            inner.y,
            indicator.len() as u16,
            1,
        );
        frame.render_widget(
            Paragraph::new(indicator).style(Style::default().fg(Color::Yellow)),
            indicator_area,
        );
    }
}
```

### Status Bar

```rust
pub fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut parts = Vec::new();

    // Visible pane count
    let visible_count = app.panes.iter().filter(|p| p.visible).count();
    let total_count = app.panes.len();
    parts.push(format!("{}/{} visible", visible_count, total_count));

    // Task counts per lane (only show lanes with backlog)
    for (lane, counts) in &app.task_counts {
        if counts.backlog > 0 {
            parts.push(format!("{}: {}", lane, counts.backlog));
        }
    }

    // Watcher status
    parts.push("● Watching".to_string());

    let status = parts.join(" │ ");

    let style = Style::default()
        .fg(Color::White)
        .bg(Color::DarkGray);

    frame.render_widget(Paragraph::new(status).style(style), area);
}
```

### Help Overlay

```rust
pub fn render_help_overlay(frame: &mut Frame, app: &App) {
    let area = centered_rect(60, 80, frame.area());

    // Clear background
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let help_text = r#"
GLOBAL KEYBINDINGS
  Ctrl+B b     Toggle sidebar
  Ctrl+B n     Nudge all workers
  Ctrl+B N     Nudge focused worker
  Ctrl+B ?     Toggle this help
  Ctrl+B q     Quit hive
  Tab          Switch focus: sidebar ↔ panes

SIDEBAR (when focused)
  ↑/k, ↓/j     Navigate items
  Space        Toggle visibility
  Enter        Toggle visibility + focus pane
  ←/h          Collapse group
  →/l          Expand group
  a            Select all (in group or all)
  n            Select none (in group or all)

MAIN AREA (when focused)
  ↑↓←→         Move focus between panes
  Page Up/Down Scroll pane output
  Home         Scroll to top
  End          Scroll to bottom
  Ctrl+C       Send interrupt to pane

SYMBOLS
  ●  Visible pane (shown in main area)
  ○  Hidden pane (still running)
  ▶  Collapsed group
  ▼  Expanded group
"#;

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

/// Helper to create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ]).split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ]).split(popup_layout[1])[1]
}
```

---

## Keybindings

### Global Keybindings (Always Active)

| Key | Action |
|-----|--------|
| `Ctrl+B` | Prefix key (like tmux) |
| `Ctrl+B` `b` | Toggle sidebar visibility |
| `Ctrl+B` `n` | Nudge all workers |
| `Ctrl+B` `N` | Nudge focused worker |
| `Ctrl+B` `z` | Zoom/maximize focused pane |
| `Ctrl+B` `?` | Toggle help overlay |
| `Ctrl+B` `d` | Detach (if server mode) |
| `Ctrl+B` `q` | Quit hive |
| `Tab` | Switch focus between sidebar and main area |
| `Escape` | Close help / Cancel |

### Sidebar Keybindings (When Sidebar Focused)

| Key | Action |
|-----|--------|
| `↑` / `k` | Move selection up |
| `↓` / `j` | Move selection down |
| `Space` | Toggle visibility of selected item |
| `Enter` | Toggle visibility AND focus the pane |
| `←` / `h` | Collapse group (if on group) |
| `→` / `l` | Expand group (if on group) |
| `a` | Select all visible (in group or globally) |
| `n` | Select none (in group or globally) |
| `1-9` | Quick toggle: show only pane N |

### Main Area Keybindings (When Pane Focused)

| Key | Action |
|-----|--------|
| `↑` `↓` `←` `→` | Move focus between visible panes |
| `Page Up` | Scroll pane output up |
| `Page Down` | Scroll pane output down |
| `Home` | Scroll to top of output |
| `End` | Scroll to bottom of output |
| `Ctrl+C` | Send SIGINT to focused pane |
| All other keys | Pass through to focused pane's PTY |

### Input Handling

```rust
pub async fn handle_key_event(app: &mut App, key: KeyEvent) -> Result<bool> {
    // Handle help overlay
    if app.show_help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')) {
            app.show_help = false;
        }
        return Ok(false);
    }

    // Handle prefix mode (Ctrl+B was pressed)
    if app.prefix_mode {
        app.prefix_mode = false;
        return handle_prefix_key(app, key).await;
    }

    // Check for prefix key
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b') {
        app.prefix_mode = true;
        return Ok(false);
    }

    // Tab switches focus between sidebar and main area
    if key.code == KeyCode::Tab {
        if app.sidebar_visible {
            app.sidebar.focused = !app.sidebar.focused;
            // If moving to main area, ensure something is focused
            if !app.sidebar.focused && app.focused_pane().is_none() {
                if let Some(pane) = app.panes.iter_mut().find(|p| p.visible) {
                    pane.focused = true;
                }
            }
        }
        return Ok(false);
    }

    // Route to appropriate handler
    if app.sidebar.focused {
        handle_sidebar_key(app, key).await
    } else {
        handle_pane_key(app, key).await
    }
}

async fn handle_prefix_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Char('b') => {
            app.sidebar_visible = !app.sidebar_visible;
            if !app.sidebar_visible {
                app.sidebar.focused = false;
            }
        }
        KeyCode::Char('n') => {
            nudge_workers(app, None).await?;
        }
        KeyCode::Char('N') => {
            if let Some(pane) = app.focused_pane() {
                nudge_workers(app, Some(&pane.id.clone())).await?;
            }
        }
        KeyCode::Char('z') => {
            app.toggle_zoom();
        }
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }
        KeyCode::Char('d') => {
            // Detach (server mode only)
            return Ok(true);
        }
        KeyCode::Char('q') => {
            // Quit
            return Ok(true);
        }
        _ => {}
    }
    Ok(false)
}

async fn handle_sidebar_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            app.sidebar.move_up();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.sidebar.move_down();
        }
        KeyCode::Char(' ') => {
            app.sidebar.toggle_selected(&mut app.panes);
        }
        KeyCode::Enter => {
            // Toggle visibility and focus the pane
            if let Some(pane_id) = app.sidebar.selected_pane_id() {
                // Make visible
                if let Some(pane) = app.panes.iter_mut().find(|p| p.id == pane_id) {
                    pane.visible = true;
                }
                // Focus it
                for pane in &mut app.panes {
                    pane.focused = pane.id == pane_id;
                }
                // Switch to main area
                app.sidebar.focused = false;
            }
        }
        KeyCode::Left | KeyCode::Char('h') => {
            app.sidebar.collapse_selected();
        }
        KeyCode::Right | KeyCode::Char('l') => {
            app.sidebar.expand_selected();
        }
        KeyCode::Char('a') => {
            app.sidebar.select_all(&mut app.panes);
        }
        KeyCode::Char('n') => {
            app.sidebar.select_none(&mut app.panes);
        }
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            // Quick toggle: show only pane N
            let idx = c.to_digit(10).unwrap() as usize - 1;
            app.show_only_pane(idx);
        }
        _ => {}
    }
    Ok(false)
}

async fn handle_pane_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Up => app.focus_direction(Direction::Up),
        KeyCode::Down => app.focus_direction(Direction::Down),
        KeyCode::Left => app.focus_direction(Direction::Left),
        KeyCode::Right => app.focus_direction(Direction::Right),
        KeyCode::PageUp => {
            if let Some(pane) = app.focused_pane_mut() {
                pane.output_buffer.scroll_up(10);
            }
        }
        KeyCode::PageDown => {
            if let Some(pane) = app.focused_pane_mut() {
                pane.output_buffer.scroll_down(10);
            }
        }
        KeyCode::Home => {
            if let Some(pane) = app.focused_pane_mut() {
                pane.output_buffer.scroll_offset = pane.output_buffer.lines.len();
            }
        }
        KeyCode::End => {
            if let Some(pane) = app.focused_pane_mut() {
                pane.output_buffer.scroll_to_bottom();
            }
        }
        _ => {
            // Pass through to PTY
            if let Some(pane) = app.focused_pane_mut() {
                send_key_to_pane(pane, key)?;
            }
        }
    }
    Ok(false)
}
```

### Focus Navigation in Main Area

```rust
impl App {
    /// Move focus in the given direction among visible panes
    pub fn focus_direction(&mut self, direction: Direction) {
        let visible: Vec<usize> = self.panes.iter()
            .enumerate()
            .filter(|(_, p)| p.visible)
            .map(|(i, _)| i)
            .collect();

        if visible.is_empty() {
            return;
        }

        // Find currently focused pane index
        let current_idx = visible.iter()
            .position(|&i| self.panes[i].focused)
            .unwrap_or(0);

        // Calculate grid position based on layout
        let cols = if visible.len() <= 2 { visible.len() } else { 2 };
        let rows = (visible.len() + cols - 1) / cols;

        let current_row = current_idx / cols;
        let current_col = current_idx % cols;

        let (new_row, new_col) = match direction {
            Direction::Up => (current_row.saturating_sub(1), current_col),
            Direction::Down => ((current_row + 1).min(rows - 1), current_col),
            Direction::Left => (current_row, current_col.saturating_sub(1)),
            Direction::Right => (current_row, (current_col + 1).min(cols - 1)),
        };

        let new_idx = (new_row * cols + new_col).min(visible.len() - 1);

        // Update focus
        for (i, pane) in self.panes.iter_mut().enumerate() {
            pane.focused = i == visible[new_idx];
        }
    }
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

**Goal**: Basic working TUI that can display and interact with panes

1. **Config parsing** (`src/config/`)
   - Parse `.hive.yaml` into `HiveConfig` struct
   - Validate required fields
   - Support both architect/workers backend config

2. **PTY spawning** (`src/pty/`)
   - Spawn claude/codex processes with correct args
   - Read output into buffer
   - Write input to PTY
   - Handle process exit

3. **Basic UI** (`src/ui/`)
   - Main layout with single pane (no sidebar yet)
   - Pane border with title
   - Output rendering
   - Status bar (basic)

4. **Core app loop** (`src/app/`)
   - Event loop (keyboard + PTY output)
   - Focus management (single pane for now)
   - Quit handling

5. **CLI** (`src/main.rs`)
   - `hive up` - start TUI
   - `hive stop` - kill running session

**Deliverable**: Can start hive, see architect pane, type and see output

### Phase 2: Sidebar & Multi-Pane

**Goal**: IDE-like sidebar with visibility toggles

1. **Sidebar data structure** (`src/ui/sidebar.rs`)
   - `SidebarItem` enum (Pane, Group)
   - Build tree from config (group worktrees)
   - Selection state
   - Expand/collapse state

2. **Sidebar rendering**
   - Tree view with icons (●/○/▶/▼)
   - Selection highlighting
   - Indentation for children
   - Group headers with counts

3. **Visibility toggle**
   - Space to toggle item/group
   - 'a' to select all, 'n' to select none
   - Enter to toggle + focus

4. **Multi-pane main area**
   - Dynamic grid layout based on visible count
   - Layout calculation algorithm
   - Focus navigation with arrow keys

5. **Tab switching**
   - Tab key switches sidebar ↔ main area
   - Focus indication (border color)

**Deliverable**: Can toggle which panes are visible, navigate with sidebar

### Phase 3: Task Integration

**Goal**: Auto-nudging and task status display

1. **YAML task source** (`src/tasks/yaml.rs`)
   - Parse tasks.yaml format
   - Count backlog/in_progress per lane
   - Reload on change

2. **File watching** (`src/tasks/watcher.rs`)
   - Use `notify` crate
   - Debouncing (10 second minimum)
   - Settle time (5 seconds after change)

3. **Nudge system** (`src/commands/nudge.rs`)
   - Build nudge message with template vars
   - Send to PTY stdin
   - Track last nudge time per lane

4. **Status bar improvements**
   - Show task counts per lane
   - Show "Watching" indicator
   - Show visible/total pane count

5. **Keybindings**
   - `Ctrl+B n` nudge all
   - `Ctrl+B N` nudge focused

**Deliverable**: Tasks show in status bar, auto-nudging works

### Phase 4: Polish & Features

**Goal**: Full feature set with good UX

1. **GitHub Projects support** (`src/tasks/github.rs`)
   - GraphQL query for project items
   - Parse Lane/Status fields
   - Polling with cache

2. **Help overlay**
   - `Ctrl+B ?` to toggle
   - All keybindings documented
   - Symbol legend

3. **Scrollback**
   - Page Up/Down to scroll
   - Home/End for top/bottom
   - Scroll indicator in pane

4. **Zoom mode**
   - `Ctrl+B z` to maximize focused pane
   - Press again to restore

5. **Additional CLI commands**
   - `hive status` - print status to stdout
   - `hive nudge [worker]` - CLI nudge
   - `hive init` - interactive setup (can use dialoguer)
   - `hive doctor` - check/fix issues
   - `hive role` - generate role files

**Deliverable**: Feature parity with bash version

### Phase 5: Server Mode (Optional)

**Goal**: Detach/attach like tmux

1. **Server architecture**
   - Unix socket for IPC
   - Server manages PTYs
   - Client renders TUI

2. **Protocol**
   - ClientMessage enum (Input, Resize, Nudge, Detach)
   - ServerMessage enum (Output, State, PaneExited)
   - Serialization with serde

3. **Commands**
   - `hive up --daemon` - start server only
   - `hive attach` - attach client to server
   - `Ctrl+B d` - detach client

4. **Session persistence**
   - Server continues when client detaches
   - State recovery on attach
   - PID file for session tracking

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
