use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::app::sidebar::SidebarState;
use crate::app::types::PaneType;
use crate::config::{Backend, BranchConfig};
use crate::pty::output::OutputBuffer;
use crate::ipc::{AppState, PaneInfo, WindowInfo};
use crate::projects::ProjectEntry;
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
    /// Raw output history for tmux-style scrollback
    pub raw_history: std::collections::VecDeque<u8>,
    pub raw_history_max: usize,
}

pub struct App {
    pub project_name: String,
    pub project_dir: PathBuf,
    pub backend: Backend,
    pub layout_mode: LayoutMode,
    pub panes: Vec<ClientPane>,
    pub focused_pane: usize,
    pub windows: Vec<AppWindow>,
    pub focused_window: usize,
    pub show_help: bool,
    pub prefix_mode: bool,
    pub sidebar: SidebarState,
    pub show_palette: bool,
    pub palette_query: String,
    pub palette_selection: usize,
    pub show_projects: bool,
    pub projects: Vec<ProjectEntry>,
    pub projects_selection: usize,
    pub projects_input: String,
    pub projects_input_mode: bool,
    pub projects_message: Option<String>,
    pub running: bool,
    pub task_counts: HashMap<String, TaskCounts>,
    pub zoomed: bool,
    pub worker_page: usize,
    pub smart_mode: bool,
    pub architect_left: bool,
    pub show_task_queue: bool,
    pub task_queue_selection: usize,
    pub task_queue_expanded: HashMap<String, bool>,
    pub scroll_mode: bool,
    /// Temporary buffer for scroll mode (parsed from raw_history)
    pub scroll_buffer: Option<crate::pty::output::OutputBuffer>,
}

impl App {
    pub fn new(
        backend: Backend,
        panes: Vec<ClientPane>,
        windows: Vec<AppWindow>,
        project_dir: PathBuf,
    ) -> Self {
        Self {
            project_name: "hive".to_string(),
            project_dir,
            backend,
            layout_mode: LayoutMode::Default,
            panes,
            focused_pane: 0,
            windows,
            focused_window: 0,
            show_help: false,
            prefix_mode: false,
            sidebar: SidebarState::new(),
            show_palette: false,
            palette_query: String::new(),
            palette_selection: 0,
            show_projects: false,
            projects: Vec::new(),
            projects_selection: 0,
            projects_input: String::new(),
            projects_input_mode: false,
            projects_message: None,
            running: true,
            task_counts: HashMap::new(),
            zoomed: false,
            worker_page: 0,
            smart_mode: false,
            architect_left: false,
            show_task_queue: false,
            task_queue_selection: 0,
            task_queue_expanded: HashMap::new(),
            scroll_mode: false,
            scroll_buffer: None,
        }
    }

    pub fn toggle_architect_position(&mut self) {
        self.architect_left = !self.architect_left;
    }

    /// Check if a pane has work (tasks in progress or backlog)
    pub fn pane_has_work(&self, pane_idx: usize) -> bool {
        if let Some(pane) = self.panes.get(pane_idx) {
            // Get the lane name for this pane
            if let Some(lane) = &pane.lane {
                if let Some(counts) = self.task_counts.get(lane) {
                    return counts.in_progress > 0 || counts.backlog > 0;
                }
            }
        }
        false
    }

    pub fn visible_worker_count(&self) -> usize {
        self.panes
            .iter()
            .filter(|p| p.visible && !matches!(p.pane_type, PaneType::Architect))
            .count()
    }

    pub fn total_worker_pages(&self, workers_per_page: usize) -> usize {
        if workers_per_page == 0 {
            return 1;
        }
        let visible = self.visible_worker_count();
        if visible == 0 {
            return 1;
        }
        (visible + workers_per_page - 1) / workers_per_page
    }

    pub fn next_worker_page(&mut self, workers_per_page: usize) {
        let total = self.total_worker_pages(workers_per_page);
        if self.worker_page + 1 < total {
            self.worker_page += 1;
        }
    }

    pub fn prev_worker_page(&mut self) {
        if self.worker_page > 0 {
            self.worker_page -= 1;
        }
    }

    pub fn clamp_worker_page(&mut self, workers_per_page: usize) {
        let total = self.total_worker_pages(workers_per_page);
        if self.worker_page >= total {
            self.worker_page = total.saturating_sub(1);
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
        self.architect_left = state.architect_left;

        self.windows = state
            .windows
            .into_iter()
            .map(window_info_to_app)
            .collect();

        let mut existing_buffers = std::collections::HashMap::new();
        for pane in self.panes.drain(..) {
            existing_buffers.insert(pane.id.clone(), (pane.output_buffer, pane.raw_history));
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
    buffers: &mut std::collections::HashMap<String, (OutputBuffer, std::collections::VecDeque<u8>)>,
) -> ClientPane {
    let (output_buffer, raw_history) = buffers
        .remove(&pane.id)
        .unwrap_or_else(|| (OutputBuffer::new(24, 80, 2000), std::collections::VecDeque::new()));

    ClientPane {
        id: pane.id,
        pane_type: pane.pane_type,
        output_buffer,
        lane: pane.lane,
        branch: pane.branch,
        group: pane.group,
        visible: pane.visible,
        raw_history,
        raw_history_max: 500_000, // 500KB of history
    }
}

fn window_info_to_app(window: WindowInfo) -> AppWindow {
    AppWindow {
        name: window.name,
        layout: window.layout,
        pane_indices: window.pane_indices,
    }
}
