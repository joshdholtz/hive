use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::state::{App, LayoutKind, LayoutMode};
use crate::app::types::PaneType;

pub fn calculate_layout(app: &App, area: Rect) -> Vec<(usize, Rect)> {
    if app.zoomed {
        return vec![(app.focused_pane, area)];
    }

    match app.layout_mode {
        LayoutMode::Default => calculate_default_layout(app, area),
        LayoutMode::Custom => calculate_custom_layout(app, area),
    }
}

fn calculate_default_layout(app: &App, area: Rect) -> Vec<(usize, Rect)> {
    let mut result = Vec::new();

    let architect_idx = app
        .panes
        .iter()
        .position(|p| matches!(p.pane_type, PaneType::Architect));

    let worker_indices: Vec<usize> = app
        .panes
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p.pane_type, PaneType::Worker { .. }))
        .map(|(i, _)| i)
        .collect();

    if architect_idx.is_none() {
        return stack_vertical(area, &worker_indices);
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    if let Some(idx) = architect_idx {
        result.push((idx, chunks[0]));
    }

    let worker_layout = stack_vertical(chunks[1], &worker_indices);
    result.extend(worker_layout);

    result
}

fn calculate_custom_layout(app: &App, area: Rect) -> Vec<(usize, Rect)> {
    let Some(window) = app.windows.get(app.focused_window) else {
        return Vec::new();
    };

    let pane_indices = &window.pane_indices;
    match window.layout {
        LayoutKind::EvenHorizontal => stack_horizontal(area, pane_indices),
        LayoutKind::EvenVertical => stack_vertical(area, pane_indices),
    }
}

fn stack_vertical(area: Rect, pane_indices: &[usize]) -> Vec<(usize, Rect)> {
    if pane_indices.is_empty() {
        return Vec::new();
    }

    let constraints = vec![Constraint::Ratio(1, pane_indices.len() as u32); pane_indices.len()];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    pane_indices
        .iter()
        .enumerate()
        .map(|(idx, pane_id)| (*pane_id, chunks[idx]))
        .collect()
}

fn stack_horizontal(area: Rect, pane_indices: &[usize]) -> Vec<(usize, Rect)> {
    if pane_indices.is_empty() {
        return Vec::new();
    }

    let constraints = vec![Constraint::Ratio(1, pane_indices.len() as u32); pane_indices.len()];
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    pane_indices
        .iter()
        .enumerate()
        .map(|(idx, pane_id)| (*pane_id, chunks[idx]))
        .collect()
}
