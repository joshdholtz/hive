use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::palette::{build_items, filter_indices};
use crate::app::state::App;

pub fn render_palette(frame: &mut Frame, app: &App) {
    let items = build_items(app);
    let filtered = filter_indices(&items, &app.palette_query);

    let area = centered_rect(70, 60, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title("Command Palette")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));

    let mut lines = Vec::new();
    lines.push(format!("> {}", app.palette_query));
    lines.push("".to_string());

    for (idx, item_idx) in filtered.iter().enumerate() {
        let item = &items[*item_idx];
        let selected = idx == app.palette_selection;
        let prefix = if selected { ">" } else { " " };
        // Show number shortcuts 1-9 for first 9 items
        let number = if idx < 9 {
            format!("{}", idx + 1)
        } else {
            " ".to_string()
        };
        lines.push(format!("{} {} {}", prefix, number, item.label));
    }

    let paragraph = Paragraph::new(lines.join("\n"))
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
