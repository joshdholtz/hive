use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::state::App;

pub fn render_title_bar(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!("Hive - {}  |  Ctrl+K: palette", app.project_name);
    let paragraph = Paragraph::new(title).style(Style::default().bg(Color::DarkGray));
    frame.render_widget(paragraph, area);
}
