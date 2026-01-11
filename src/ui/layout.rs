use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::app::state::App;

pub fn calculate_layout(app: &App, area: Rect) -> Vec<(usize, Rect)> {
    if app.zoomed {
        return vec![(app.focused_pane, area)];
    }

    let visible: Vec<usize> = app
        .panes
        .iter()
        .enumerate()
        .filter(|(_, pane)| pane.visible)
        .map(|(idx, _)| idx)
        .collect();

    match visible.len() {
        0 => Vec::new(),
        1 => vec![(visible[0], area)],
        2 => {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            vec![(visible[0], chunks[0]), (visible[1], chunks[1])]
        }
        3 => {
            let cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let right = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(cols[1]);
            vec![
                (visible[0], cols[0]),
                (visible[1], right[0]),
                (visible[2], right[1]),
            ]
        }
        4 => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);
            let top = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[0]);
            let bottom = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(rows[1]);
            vec![
                (visible[0], top[0]),
                (visible[1], top[1]),
                (visible[2], bottom[0]),
                (visible[3], bottom[1]),
            ]
        }
        count => {
            let rows_count = (count + 1) / 2;
            let row_constraints =
                vec![Constraint::Ratio(1, rows_count as u32); rows_count];
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints(row_constraints)
                .split(area);
            let mut rects = Vec::new();
            for (row_idx, row) in rows.iter().enumerate() {
                let items_in_row = if row_idx == rows_count - 1 && count % 2 == 1 {
                    1
                } else {
                    2
                };
                let col_constraints =
                    vec![Constraint::Ratio(1, items_in_row as u32); items_in_row];
                let cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints(col_constraints)
                    .split(*row);
                for (col_idx, rect) in cols.iter().enumerate() {
                    let pane_idx = row_idx * 2 + col_idx;
                    if let Some(id) = visible.get(pane_idx) {
                        rects.push((*id, *rect));
                    }
                }
            }
            rects
        }
    }
}
