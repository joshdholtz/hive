use ratatui::prelude::*;
use ratatui::style::Modifier;
use ratatui::widgets::{Block, Borders};
use tui_term::widget::PseudoTerminal;

use crate::app::state::ClientPane;
use crate::app::types::PaneType;

pub fn render_pane(
    frame: &mut Frame,
    area: Rect,
    pane: &ClientPane,
    focused: bool,
    sidebar_focused: bool,
) {
    let (border_color, title_color) = if focused {
        (Color::Yellow, Color::Yellow)
    } else {
        (Color::DarkGray, Color::Blue)
    };

    let border_style = Style::default().fg(border_color);

    // Build title: "group/lane" or just "lane" (or "architect")
    // Don't show group prefix if lane already contains it (e.g., "backend/features")
    let mut title = match &pane.pane_type {
        PaneType::Architect => "architect".to_string(),
        PaneType::Worker { lane } => {
            if let Some(group) = &pane.group {
                // Skip group prefix if lane already starts with "group/"
                let group_prefix = format!("{}/", group);
                if lane.starts_with(&group_prefix) || group == lane {
                    lane.clone()
                } else {
                    format!("{}/{}", group, lane)
                }
            } else {
                lane.clone()
            }
        }
    };
    let scroll_offset = pane.output_buffer.scroll_offset();
    if scroll_offset > 0 {
        title.push_str(&format!(" [scroll {}]", scroll_offset));
    }

    let title_style = Style::default().fg(title_color);
    let block = Block::default()
        .title(Line::from(title).style(title_style))
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);

    let terminal_style = if sidebar_focused && !focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };

    let terminal = PseudoTerminal::new(pane.output_buffer.screen())
        .block(block)
        .style(terminal_style);

    frame.render_widget(terminal, area);

    if sidebar_focused && !focused {
        frame
            .buffer_mut()
            .set_style(inner, Style::default().add_modifier(Modifier::DIM));
    }
}
