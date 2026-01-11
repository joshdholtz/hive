use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::state::App;

pub fn render_projects(frame: &mut Frame, app: &App) {
    let area = centered_rect(70, 70, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title("Projects")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(inner);

    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|project| {
            let line = format!("{}  {}", project.name, project.path);
            ListItem::new(Line::from(line))
        })
        .collect();

    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(app.projects_selection.min(items.len() - 1)));
    }

    let list = List::new(items)
        .highlight_style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::NONE));
    frame.render_stateful_widget(list, chunks[0], &mut state);

    let input_label = if app.projects_input_mode {
        "Add project path:"
    } else {
        "Keys: a add current | A add path | d remove | Esc close"
    };
    let mut input_text = input_label.to_string();
    if app.projects_input_mode {
        input_text.push_str(&format!("\n> {}", app.projects_input));
    }
    let input = Paragraph::new(input_text)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    frame.render_widget(input, chunks[1]);

    if let Some(message) = &app.projects_message {
        let msg = Paragraph::new(message.clone())
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(msg, chunks[2]);
    }
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
