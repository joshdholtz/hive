use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::app::state::App;

pub fn render_help_overlay(frame: &mut Frame, _app: &App) {
    let area = centered_rect(60, 60, frame.area());
    let text = [
        "Hive TUI Help",
        "",
        "Ctrl+P        - Command palette",
        "Ctrl+T        - Task queue",
        "Ctrl+O        - Toggle sidebar",
        "Ctrl+H/J/K/L  - Navigate panes",
        "Ctrl+Z        - Zoom focused pane",
        "Ctrl+S        - Smart mode (active only)",
        "Ctrl+[        - Enter scroll mode",
        "Ctrl+D        - Detach from session",
        "",
        "Scroll mode (after Ctrl+[)",
        "  j/k or ↑/↓    - Scroll line",
        "  Ctrl+U/D      - Scroll half page",
        "  g/G           - Top/bottom",
        "  q/Esc         - Exit scroll mode",
        "",
        "Sidebar (when focused)",
        "  Up/Down or j/k  - Move selection",
        "  Space           - Toggle visibility",
        "  Enter           - Show + focus pane",
        "  Left/Right h/l  - Collapse/expand group",
        "  a               - Show all (group/all)",
        "  n               - Hide all (group/all)",
        "  Ctrl+U/D        - Reorder up/down",
        "  Tab/Esc         - Return to panes",
        "",
        "Project manager (via palette)",
        "  a               - Add current project",
        "  A               - Add project by path",
        "  d               - Remove selected project",
        "  Esc             - Close",
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
