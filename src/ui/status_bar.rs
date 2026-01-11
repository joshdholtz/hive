use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::backend_label;
use crate::app::state::App;
use crate::app::types::PaneType;

pub fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mut parts = Vec::new();

    let total_panes = app
        .panes
        .iter()
        .filter(|p| matches!(p.pane_type, PaneType::Architect | PaneType::Worker { .. }))
        .count();
    let visible_panes = app.panes.iter().filter(|p| p.visible).count();
    parts.push(format!("{}/{} visible", visible_panes, total_panes));

    for (lane, counts) in &app.task_counts {
        if counts.backlog > 0 {
            parts.push(format!("{}: {} backlog", lane, counts.backlog));
        }
    }

    let backend = backend_label(app.backend);
    parts.push(format!("backend: {}", backend));

    let mode = if app.show_palette {
        "PALETTE"
    } else if app.sidebar.focused {
        "SIDEBAR"
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
