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
    nav_mode: bool,
    sidebar_focused: bool,
) {
    let border_style = if focused {
        if nav_mode {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Yellow)
        }
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let mut title = match &pane.pane_type {
        PaneType::Architect => "architect".to_string(),
        PaneType::Worker { lane } => format!("{} ({})", pane.id, lane),
    };
    let scroll_offset = pane.output_buffer.scroll_offset();
    if scroll_offset > 0 {
        title.push_str(&format!(" [scroll {}]", scroll_offset));
    }

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);

    let terminal_style = if (nav_mode || sidebar_focused) && !focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };

    let terminal = PseudoTerminal::new(pane.output_buffer.screen())
        .block(block)
        .style(terminal_style);

    frame.render_widget(terminal, area);

    if (nav_mode || sidebar_focused) && !focused {
        frame
            .buffer_mut()
            .set_style(inner, Style::default().add_modifier(Modifier::DIM));
    }
}
