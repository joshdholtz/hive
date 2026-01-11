use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Tabs};

use crate::app::state::{App, LayoutMode};

pub fn render_tab_bar(frame: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<String> = match app.layout_mode {
        LayoutMode::Default => vec!["default".to_string()],
        LayoutMode::Custom => app.windows.iter().map(|w| w.name.clone()).collect(),
    };

    let selected = match app.layout_mode {
        LayoutMode::Default => 0,
        LayoutMode::Custom => app.focused_window,
    };

    let tabs = Tabs::new(titles)
        .select(selected)
        .block(Block::default().borders(Borders::BOTTOM))
        .highlight_style(Style::default().fg(Color::Yellow));

    frame.render_widget(tabs, area);
}
