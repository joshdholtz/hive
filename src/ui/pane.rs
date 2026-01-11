use ratatui::prelude::*;
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

    let title = match &pane.pane_type {
        PaneType::Architect => "architect".to_string(),
        PaneType::Worker { lane } => format!("{} ({})", pane.id, lane),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let terminal = PseudoTerminal::new(pane.output_buffer.screen())
        .block(block);

    frame.render_widget(terminal, area);
}
