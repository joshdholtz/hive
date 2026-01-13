use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::app::state::{LayoutKind, LayoutMode};
use crate::app::types::PaneType;
use crate::config::{Backend, BranchConfig};
use crate::tasks::TaskCounts;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneInfo {
    pub id: String,
    pub pane_type: PaneType,
    pub lane: Option<String>,
    pub branch: Option<BranchConfig>,
    pub group: Option<String>,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub name: String,
    pub layout: LayoutKind,
    pub pane_indices: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub project_name: String,
    pub backend: Backend,
    pub layout_mode: LayoutMode,
    pub panes: Vec<PaneInfo>,
    pub windows: Vec<WindowInfo>,
    pub task_counts: HashMap<String, TaskCounts>,
    #[serde(default)]
    pub architect_left: bool,
    #[serde(default = "default_min_pane_width")]
    pub min_pane_width: u16,
    #[serde(default = "default_min_pane_height")]
    pub min_pane_height: u16,
}

fn default_min_pane_width() -> u16 {
    100
}

fn default_min_pane_height() -> u16 {
    16
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PaneSize {
    pub pane_id: String,
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Input { pane_id: String, data: Vec<u8> },
    Resize { panes: Vec<PaneSize> },
    Nudge { worker: Option<String> },
    SetVisibility { pane_id: String, visible: bool },
    ReorderPanes { pane_ids: Vec<String> },
    SetArchitectLeft { left: bool },
    Layout { mode: LayoutMode },
    Detach,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    State { state: AppState },
    Output { pane_id: String, data: Vec<u8> },
    PaneExited { pane_id: String },
    Error { message: String },
}

pub fn encode_message(message: &ServerMessage) -> String {
    serde_json::to_string(message).unwrap_or_else(|_| "{}".to_string())
}

pub fn decode_client_message(line: &str) -> Option<ClientMessage> {
    serde_json::from_str(line).ok()
}

pub fn decode_server_message(line: &str) -> Option<ServerMessage> {
    serde_json::from_str(line).ok()
}
