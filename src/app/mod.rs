pub mod palette;
pub mod sidebar;
pub mod state;
pub mod types;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::Backend;
use crate::tasks::TaskCounts;
use state::App;

pub const DEFAULT_STARTUP_MSG: &str = "Read .hive/workers/{lane}/WORKER.md if it exists. You are assigned to lane '{lane}'. Check your task backlog. If backlog is EMPTY, report 'No tasks in backlog for {lane}' and STOP - do NOT explore or look for other work. If tasks exist, claim ONE task and work on it. When finished, create a git branch, commit, push, and create a Pull Request.";

pub const DEFAULT_NUDGE_MSG: &str = "FIRST: If you have uncommitted changes or an unpushed branch from a previous task, you MUST create a PR NOW using 'gh pr create' before starting anything new. You have {backlog_count} task(s) in your backlog for lane '{lane}'. Claim ONE task and work on it. REMINDER: When done, create a branch, commit, push, and run 'gh pr create' - do NOT stop until the PR URL is displayed.";

pub fn build_startup_message(config: &crate::config::HiveConfig, lane: &str) -> String {
    let template = config
        .messages
        .as_ref()
        .and_then(|m| m.startup.as_ref())
        .map(|s| s.as_str())
        .unwrap_or(DEFAULT_STARTUP_MSG);

    template.replace("{lane}", lane)
}

pub fn build_nudge_message(
    config: &crate::config::HiveConfig,
    lane: &str,
    backlog_count: usize,
    branch: &Option<crate::config::BranchConfig>,
) -> String {
    let template = config
        .messages
        .as_ref()
        .and_then(|m| m.nudge.as_ref())
        .map(|s| s.as_str())
        .unwrap_or(DEFAULT_NUDGE_MSG);

    let mut msg = template
        .replace("{lane}", lane)
        .replace("{backlog_count}", &backlog_count.to_string());

    if let Some(branch) = branch {
        msg.push_str(&format!(
            " BRANCH CONVENTION: Your LOCAL branch names MUST start with '{}/'. Push to remote with: git push origin {}/my-feature:{}/my-feature",
            branch.local, branch.local, branch.remote
        ));
    }

    msg.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn key_to_bytes(key: KeyEvent) -> Vec<u8> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                let ctrl = (c as u8) & 0x1f;
                vec![ctrl]
            } else {
                c.to_string().into_bytes()
            }
        }
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Backspace => b"\x7f".to_vec(),
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::Esc => b"\x1b".to_vec(),
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        _ => Vec::new(),
    }
}

pub fn update_task_counts(app: &mut App, counts: impl Iterator<Item = (String, TaskCounts)>) {
    app.task_counts = counts.collect();
}

pub fn layout_visible_panes(app: &App) -> Vec<usize> {
    if app.zoomed {
        return vec![app.focused_pane];
    }

    app.panes
        .iter()
        .enumerate()
        .filter(|(_, pane)| pane.visible)
        .map(|(idx, _)| idx)
        .collect()
}

pub fn backend_label(backend: Backend) -> &'static str {
    match backend {
        Backend::Claude => "claude",
        Backend::Codex => "codex",
    }
}
