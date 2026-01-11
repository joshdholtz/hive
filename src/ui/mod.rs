pub mod help;
pub mod layout;
pub mod palette;
pub mod pane;
pub mod status_bar;
pub mod tab_bar;

use ratatui::prelude::*;

use crate::app::state::App;

pub fn render(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    tab_bar::render_tab_bar(frame, chunks[0], app);
    render_panes(frame, chunks[1], app);
    status_bar::render_status_bar(frame, chunks[2], app);

    if app.show_help {
        help::render_help_overlay(frame, app);
    }

    if app.show_palette {
        palette::render_palette(frame, app);
    }
}

fn render_panes(frame: &mut Frame, area: Rect, app: &App) {
    let layout = layout::calculate_layout(app, area);
    for (idx, rect) in layout {
        let focused = idx == app.focused_pane;
        pane::render_pane(frame, rect, &app.panes[idx], focused, app.nav_mode);
    }
}
