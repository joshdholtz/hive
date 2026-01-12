use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};

use crate::app::sidebar::SidebarRowKind;
use crate::app::state::App;

pub fn render_sidebar(frame: &mut Frame, area: Rect, app: &App) {
    let rows = app.sidebar.rows(&app.panes);
    let selected = app.sidebar.selected_index(&app.panes);
    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(selected));
    }

    let focused_id = app.panes.get(app.focused_pane).map(|pane| pane.id.as_str());
    let items: Vec<ListItem> = rows
        .iter()
        .map(|row| {
            let (prefix, label, focused) = match &row.kind {
                SidebarRowKind::Group {
                    name,
                    count,
                    expanded,
                } => {
                    let icon = if *expanded { "v" } else { ">" };
                    (
                        format!("{} ", icon),
                        format!("{} ({})", name, count),
                        false,
                    )
                }
                SidebarRowKind::Pane { pane_id, group: _ } => {
                    let pane = app.panes.iter().find(|pane| &pane.id == pane_id);
                    let visible = pane.map(|p| p.visible).unwrap_or(false);
                    let lane = pane.and_then(|p| p.lane.as_ref());
                    let icon = if visible { "*" } else { "o" };

                    // Show lane name for workers (which is repo name for single-worker repos)
                    // Fall back to pane_id for architect or if no lane
                    let label = lane.cloned().unwrap_or_else(|| pane_id.clone());

                    (
                        format!("{} ", icon),
                        label,
                        focused_id == Some(pane_id.as_str()),
                    )
                }
            };

            let indent = " ".repeat(row.indent);
            let mut spans = Vec::new();
            spans.push(Span::raw(format!("{}{}", indent, prefix)));
            if focused {
                spans.push(Span::styled(label, Style::default().fg(Color::Yellow)));
            } else {
                spans.push(Span::raw(label));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let border_style = if app.sidebar.focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title("panes")
                .borders(Borders::ALL)
                .border_style(border_style),
        )
        .highlight_style(Style::default().fg(Color::Yellow).bg(Color::DarkGray));

    frame.render_stateful_widget(list, area, &mut state);
}
