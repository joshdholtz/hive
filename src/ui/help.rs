use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::state::App;

pub fn render_help_overlay(frame: &mut Frame, _app: &App) {
    let area = centered_rect(60, 60, frame.area());
    let text = [
        "Hive TUI Help",
        "",
        "Ctrl+K    - Command palette",
        "Ctrl+g    - Toggle nav mode (see status bar)",
        "Nav mode: arrows/tab to move focus",
        "Nav mode: h/j/k/l or arrows to move focus",
        "Nav mode: z zoom, n/N nudge",
        "Nav mode: q quit, ? help, Esc exit",
        "Use Ctrl+K for command palette actions",
        "Arrow keys - Move focus",
        "PageUp/Down - Scroll output",
    ]
    .join("\n");

    let block = Block::default()
        .title("help")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let paragraph = Paragraph::new(text)
        .block(block)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));

    frame.render_widget(paragraph, area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
