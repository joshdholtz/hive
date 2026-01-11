use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::app::sidebar::SidebarState;
use crate::app::types::PaneType;
use crate::config::{Backend, BranchConfig};
use crate::pty::output::OutputBuffer;
use crate::ipc::{AppState, PaneInfo, WindowInfo};
use crate::tasks::TaskCounts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutMode {
    Default,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutKind {
    EvenHorizontal,
    EvenVertical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppWindow {
    pub name: String,
    pub layout: LayoutKind,
    pub pane_indices: Vec<usize>,
}

pub struct ClientPane {
    pub id: String,
    pub pane_type: PaneType,
    pub output_buffer: OutputBuffer,
    pub lane: Option<String>,
    pub branch: Option<BranchConfig>,
    pub group: Option<String>,
    pub visible: bool,
}

pub struct App {
    pub project_name: String,
    pub backend: Backend,
    pub layout_mode: LayoutMode,
    pub panes: Vec<ClientPane>,
    pub focused_pane: usize,
    pub windows: Vec<AppWindow>,
    pub focused_window: usize,
    pub show_help: bool,
    pub prefix_mode: bool,
    pub nav_mode: bool,
    pub sidebar: SidebarState,
    pub show_palette: bool,
    pub palette_query: String,
    pub palette_selection: usize,
    pub running: bool,
    pub task_counts: HashMap<String, TaskCounts>,
    pub zoomed: bool,
}

impl App {
    pub fn new(backend: Backend, panes: Vec<ClientPane>, windows: Vec<AppWindow>) -> Self {
        Self {
            project_name: "hive".to_string(),
            backend,
            layout_mode: LayoutMode::Default,
            panes,
            focused_pane: 0,
            windows,
            focused_window: 0,
            show_help: false,
            prefix_mode: false,
            nav_mode: false,
            sidebar: SidebarState::new(),
            show_palette: false,
            palette_query: String::new(),
            palette_selection: 0,
            running: true,
            task_counts: HashMap::new(),
            zoomed: false,
        }
    }

    pub fn toggle_layout(&mut self) {
        self.layout_mode = match self.layout_mode {
            LayoutMode::Default => LayoutMode::Custom,
            LayoutMode::Custom => LayoutMode::Default,
        };
        self.focused_window = 0;
    }

    pub fn toggle_zoom(&mut self) {
        self.zoomed = !self.zoomed;
    }

    pub fn focus_next(&mut self, visible: &[usize]) {
        if visible.is_empty() {
            return;
        }
        let current = visible
            .iter()
            .position(|idx| *idx == self.focused_pane)
            .unwrap_or(0);
        let next = (current + 1) % visible.len();
        self.focused_pane = visible[next];
    }

    pub fn focus_prev(&mut self, visible: &[usize]) {
        if visible.is_empty() {
            return;
        }
        let current = visible
            .iter()
            .position(|idx| *idx == self.focused_pane)
            .unwrap_or(0);
        let prev = if current == 0 { visible.len() - 1 } else { current - 1 };
        self.focused_pane = visible[prev];
    }

    pub fn focused_lane(&self) -> Option<String> {
        match &self.panes[self.focused_pane].pane_type {
            PaneType::Worker { lane } => Some(lane.clone()),
            _ => None,
        }
    }

    pub fn focused_branch(&self) -> Option<BranchConfig> {
        self.panes
            .get(self.focused_pane)
            .and_then(|pane| pane.branch.clone())
    }

    pub fn apply_state(&mut self, state: AppState) {
        self.project_name = state.project_name;
        self.backend = state.backend;
        self.layout_mode = state.layout_mode;
        self.task_counts = state.task_counts;

        self.windows = state
            .windows
            .into_iter()
            .map(window_info_to_app)
            .collect();

        let mut existing_buffers = std::collections::HashMap::new();
        for pane in self.panes.drain(..) {
            existing_buffers.insert(pane.id.clone(), pane.output_buffer);
        }

        self.panes = state
            .panes
            .into_iter()
            .map(|pane_info| pane_info_to_client(pane_info, &mut existing_buffers))
            .collect();

        if self.focused_pane >= self.panes.len() {
            self.focused_pane = self.panes.len().saturating_sub(1);
        }

        self.sidebar.ensure_selection(&self.panes);
        self.ensure_focus_visible();
    }

    pub fn ensure_focus_visible(&mut self) {
        if self.panes.is_empty() {
            return;
        }
        if self.panes.get(self.focused_pane).map(|pane| pane.visible) == Some(true) {
            return;
        }
        if let Some((idx, _)) = self
            .panes
            .iter()
            .enumerate()
            .find(|(_, pane)| pane.visible)
        {
            self.focused_pane = idx;
        }
    }
}

impl LayoutKind {
    pub fn from_str(value: &str) -> Self {
        match value {
            "even-vertical" => LayoutKind::EvenVertical,
            _ => LayoutKind::EvenHorizontal,
        }
    }
}

fn pane_info_to_client(
    pane: PaneInfo,
    buffers: &mut std::collections::HashMap<String, OutputBuffer>,
) -> ClientPane {
    let output_buffer = buffers
        .remove(&pane.id)
        .unwrap_or_else(|| OutputBuffer::new(24, 80, 2000));

    ClientPane {
        id: pane.id,
        pane_type: pane.pane_type,
        output_buffer,
        lane: pane.lane,
        branch: pane.branch,
        group: pane.group,
        visible: pane.visible,
    }
}

fn window_info_to_app(window: WindowInfo) -> AppWindow {
    AppWindow {
        name: window.name,
        layout: window.layout,
        pane_indices: window.pane_indices,
    }
}
