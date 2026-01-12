use indexmap::IndexMap;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::state::App;
use crate::app::types::PaneType;

/// Minimum dimensions for a worker pane to be usable
const MIN_PANE_HEIGHT: u16 = 16;
const MIN_PANE_WIDTH: u16 = 100;

/// Get worker pane indices in visual order (matching sidebar display)
/// Groups are shown first (in order of first appearance), then standalone panes
pub fn get_workers_in_visual_order(app: &App) -> Vec<usize> {
    let mut grouped: IndexMap<String, Vec<usize>> = IndexMap::new();
    let mut standalone: Vec<usize> = Vec::new();

    for (idx, pane) in app.panes.iter().enumerate() {
        // Skip architect and hidden panes
        if matches!(pane.pane_type, PaneType::Architect) || !pane.visible {
            continue;
        }
        // In smart mode, only include workers with tasks
        if app.smart_mode && !app.pane_has_work(idx) {
            continue;
        }

        if let Some(group) = &pane.group {
            grouped.entry(group.clone()).or_default().push(idx);
        } else {
            standalone.push(idx);
        }
    }

    let mut result = Vec::new();

    // Add grouped panes first (groups with 2+ members stay grouped, singles become standalone)
    for (_, indices) in grouped {
        if indices.len() >= 2 {
            result.extend(indices);
        } else {
            standalone.extend(indices);
        }
    }

    // Add standalone panes at the end
    result.extend(standalone);

    result
}


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

    // Get all visible worker indices in sidebar visual order
    // (grouped panes first, then standalone - matching sidebar display)
    let all_workers: Vec<usize> = get_workers_in_visual_order(app);

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
        (Some(arch), _) => {
            if app.architect_left {
                layout_architect_left_plus_workers(area, arch, &page_workers)
            } else {
                layout_architect_top_plus_workers(area, arch, &page_workers)
            }
        }
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
fn layout_architect_top_plus_workers(
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

/// Layout architect on left column, workers in grid to the right
fn layout_architect_left_plus_workers(
    area: Rect,
    architect_idx: usize,
    workers: &[usize],
) -> Vec<(usize, Rect)> {
    if workers.is_empty() {
        return vec![(architect_idx, area)];
    }

    // Calculate worker grid dimensions for the right side
    // Workers get more horizontal space, so recalculate columns for reduced width
    let worker_area_width = area.width.saturating_sub(MIN_PANE_WIDTH);
    let num_cols = (worker_area_width / MIN_PANE_WIDTH).max(1) as usize;
    let num_cols = workers.len().min(num_cols);

    // Architect gets 1 column, workers get the rest
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, (num_cols + 1) as u32),
            Constraint::Ratio(num_cols as u32, (num_cols + 1) as u32),
        ])
        .split(area);

    let mut rects = vec![(architect_idx, cols[0])];
    rects.extend(layout_workers_grid(cols[1], workers));
    rects
}

/// Grid position info for navigation
#[derive(Debug, Clone, Copy)]
pub struct GridPosition {
    pub row: usize,
    pub col: usize,
    pub num_cols: usize,
    pub num_rows: usize,
    pub is_architect: bool,
}

/// Infer grid structure from layout by analyzing rect positions
fn infer_grid_structure(layout: &[(usize, Rect)]) -> (Vec<Vec<usize>>, usize) {
    if layout.is_empty() {
        return (Vec::new(), 0);
    }

    // Group panes by their y position (row)
    let mut rows: Vec<(u16, Vec<(usize, u16)>)> = Vec::new();

    for (idx, rect) in layout.iter() {
        let y = rect.y;
        if let Some(row) = rows.iter_mut().find(|(row_y, _)| *row_y == y) {
            row.1.push((*idx, rect.x));
        } else {
            rows.push((y, vec![(*idx, rect.x)]));
        }
    }

    // Sort rows by y position
    rows.sort_by_key(|(y, _)| *y);

    // Sort items within each row by x position, extract just the indices
    let grid: Vec<Vec<usize>> = rows
        .into_iter()
        .map(|(_, mut items)| {
            items.sort_by_key(|(_, x)| *x);
            items.into_iter().map(|(idx, _)| idx).collect()
        })
        .collect();

    // Max columns across all rows
    let max_cols = grid.iter().map(|row| row.len()).max().unwrap_or(1);

    (grid, max_cols)
}

/// Get grid position for a pane in the current layout
pub fn get_grid_position(
    layout: &[(usize, Rect)],
    pane_idx: usize,
    has_architect: bool,
) -> Option<GridPosition> {
    let (grid, max_cols) = infer_grid_structure(layout);

    for (row_idx, row) in grid.iter().enumerate() {
        if let Some(col_idx) = row.iter().position(|&idx| idx == pane_idx) {
            return Some(GridPosition {
                row: row_idx,
                col: col_idx,
                num_cols: max_cols,
                num_rows: grid.len(),
                is_architect: has_architect && row_idx == 0,
            });
        }
    }

    None
}

/// Get pane index at grid position
pub fn get_pane_at_position(
    layout: &[(usize, Rect)],
    row: usize,
    col: usize,
    _has_architect: bool,
) -> Option<usize> {
    let (grid, _) = infer_grid_structure(layout);

    grid.get(row).and_then(|r| {
        // Clamp column to row length
        let clamped_col = col.min(r.len().saturating_sub(1));
        r.get(clamped_col).copied()
    })
}
