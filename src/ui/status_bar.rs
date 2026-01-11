use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{backend_label, layout_visible_panes};
use crate::app::state::App;
use crate::app::types::PaneType;

pub fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut parts = Vec::new();

    let worker_count = app
        .panes
        .iter()
        .filter(|p| matches!(p.pane_type, PaneType::Worker { .. }))
        .count();
    parts.push(format!("{} workers", worker_count));

    for (lane, counts) in &app.task_counts {
        if counts.backlog > 0 {
            parts.push(format!("{}: {} backlog", lane, counts.backlog));
        }
    }

    let backend = backend_label(app.backend);
    parts.push(format!("backend: {}", backend));

    let visible = layout_visible_panes(app);
    parts.push(format!("view: {} panes", visible.len()));

    let mode = if app.show_palette {
        "PALETTE"
    } else if app.nav_mode {
        "NAV"
    } else {
        "INPUT"
    };
    parts.push(format!("mode: {}", mode));

    let status = parts.join(" | ");
    let paragraph = Paragraph::new(status).style(Style::default().bg(Color::DarkGray));

    frame.render_widget(paragraph, area);
}
