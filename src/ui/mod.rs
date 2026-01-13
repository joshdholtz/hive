pub mod help;
pub mod layout;
pub mod palette;
pub mod pane;
pub mod projects;
pub mod sidebar;
pub mod status_bar;
pub mod task_queue;
mod terminal;
pub mod title_bar;

use ratatui::prelude::*;

use crate::app::state::App;

pub fn render(frame: &mut Frame, app: &App) {
    ratatui::widgets::Clear.render(frame.area(), frame.buffer_mut());
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(frame.area());

    title_bar::render_title_bar(frame, chunks[0], app);
    let workers_per_page = render_body(frame, chunks[1], app);
    status_bar::render_status_bar(frame, chunks[2], app, workers_per_page);

    if app.show_help {
        help::render_help_overlay(frame, app);
    }

    if app.show_projects {
        projects::render_projects(frame, app);
    }

    if app.show_palette {
        palette::render_palette(frame, app);
    }

    if app.show_task_queue {
        task_queue::render_task_queue(frame, app);
    }
}

fn render_body(frame: &mut Frame, area: Rect, app: &App) -> usize {
    if app.sidebar.visible {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(24), Constraint::Min(0)])
            .split(area);
        sidebar::render_sidebar(frame, chunks[0], app);
        render_panes(frame, chunks[1], app)
    } else {
        render_panes(frame, area, app)
    }
}

fn render_panes(frame: &mut Frame, area: Rect, app: &App) -> usize {
    let has_architect = app
        .panes
        .iter()
        .any(|p| p.visible && matches!(p.pane_type, crate::app::types::PaneType::Architect));
    let workers_per_page = layout::calculate_workers_per_page(
        area,
        has_architect,
        app.min_pane_width,
        app.min_pane_height,
    );
    let layout = layout::calculate_layout(app, area, workers_per_page, app.min_pane_width);
    for (idx, rect) in layout {
        let focused = idx == app.focused_pane && !app.sidebar.focused;
        // Pass scroll_buffer for focused pane when in scroll mode
        let scroll_buffer = if focused && app.scroll_mode {
            app.scroll_buffer.as_ref()
        } else {
            None
        };
        pane::render_pane(
            frame,
            rect,
            &app.panes[idx],
            focused,
            app.sidebar.focused,
            scroll_buffer,
        );
    }
    workers_per_page
}
