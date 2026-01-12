use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::state::App;
use crate::app::types::PaneType;

/// Minimum dimensions for a worker pane to be usable
const MIN_PANE_HEIGHT: u16 = 10;
const MIN_PANE_WIDTH: u16 = 40;


/// Calculate how many workers fit per page based on available area
/// Layout is dynamic: columns based on width, rows based on height
pub fn calculate_workers_per_page(area: Rect, has_architect: bool) -> usize {
    // Account for architect taking one row if visible
    let worker_area_height = if has_architect {
        if area.height < MIN_PANE_HEIGHT * 2 {
            return 1;
        }
        // Architect gets 1 row, workers get the rest
        area.height.saturating_sub(MIN_PANE_HEIGHT)
    } else {
        area.height
    };

    // Calculate columns based on width
    let cols = (area.width / MIN_PANE_WIDTH).max(1) as usize;
    // Calculate rows based on height
    let rows = (worker_area_height / MIN_PANE_HEIGHT).max(1) as usize;

    cols * rows
}

/// Calculate number of columns for a given width and worker count
fn calculate_columns(width: u16, num_workers: usize) -> usize {
    let max_cols = (width / MIN_PANE_WIDTH).max(1) as usize;
    // Use as many columns as we have workers, up to max
    num_workers.min(max_cols)
}

pub fn calculate_layout(app: &App, area: Rect, workers_per_page: usize) -> Vec<(usize, Rect)> {
    if app.zoomed {
        return vec![(app.focused_pane, area)];
    }

    // Find architect index if visible
    let architect_idx = app
        .panes
        .iter()
        .enumerate()
        .find(|(_, pane)| pane.visible && matches!(pane.pane_type, PaneType::Architect))
        .map(|(idx, _)| idx);

    // Get all visible worker indices (filtered by task status in smart mode)
    let all_workers: Vec<usize> = app
        .panes
        .iter()
        .enumerate()
        .filter(|(idx, pane)| {
            if !pane.visible || matches!(pane.pane_type, PaneType::Architect) {
                return false;
            }
            // In smart mode, only show workers with tasks (in_progress or backlog)
            if app.smart_mode {
                return app.pane_has_work(*idx);
            }
            true
        })
        .map(|(idx, _)| idx)
        .collect();

    // Paginate workers
    let page_start = app.worker_page * workers_per_page;
    let page_workers: Vec<usize> = all_workers
        .into_iter()
        .skip(page_start)
        .take(workers_per_page)
        .collect();

    match (architect_idx, page_workers.len()) {
        (None, 0) => Vec::new(),
        (Some(arch), 0) => vec![(arch, area)],
        (None, _) => layout_workers_grid(area, &page_workers),
        (Some(arch), _) => layout_architect_plus_workers(area, arch, &page_workers),
    }
}

/// Layout workers in a dynamic grid (columns based on width)
fn layout_workers_grid(area: Rect, workers: &[usize]) -> Vec<(usize, Rect)> {
    if workers.is_empty() {
        return Vec::new();
    }
    if workers.len() == 1 {
        return vec![(workers[0], area)];
    }

    // Calculate number of columns based on width and worker count
    let num_cols = calculate_columns(area.width, workers.len());
    let num_rows = (workers.len() + num_cols - 1) / num_cols;

    let row_constraints = vec![Constraint::Ratio(1, num_rows as u32); num_rows];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    let mut rects = Vec::new();
    let mut worker_idx = 0;

    for (row_idx, row) in rows.iter().enumerate() {
        // Last row might have fewer items
        let items_in_row = if row_idx == num_rows - 1 {
            workers.len() - worker_idx
        } else {
            num_cols
        };

        let col_constraints = vec![Constraint::Ratio(1, items_in_row as u32); items_in_row];
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints)
            .split(*row);

        for rect in cols.iter() {
            if let Some(&pane_idx) = workers.get(worker_idx) {
                rects.push((pane_idx, *rect));
                worker_idx += 1;
            }
        }
    }
    rects
}

/// Layout architect on top row, workers in grid below
fn layout_architect_plus_workers(
    area: Rect,
    architect_idx: usize,
    workers: &[usize],
) -> Vec<(usize, Rect)> {
    if workers.is_empty() {
        return vec![(architect_idx, area)];
    }

    // Calculate worker grid dimensions
    let num_cols = calculate_columns(area.width, workers.len());
    let worker_rows = (workers.len() + num_cols - 1) / num_cols;

    // Architect gets 1 row, workers get the rest
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Ratio(1, (worker_rows + 1) as u32),
            Constraint::Ratio(worker_rows as u32, (worker_rows + 1) as u32),
        ])
        .split(area);

    let mut rects = vec![(architect_idx, rows[0])];
    rects.extend(layout_workers_grid(rows[1], workers));
    rects
}
