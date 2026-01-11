use crate::app::state::App;
use crate::app::types::PaneType;

#[derive(Clone)]
pub struct PaletteItem {
    pub label: String,
    pub action: PaletteAction,
}

#[derive(Clone)]
pub enum PaletteAction {
    FocusNext,
    FocusPrev,
    FocusPane(usize),
    FocusWindow(usize),
    ToggleLayout,
    ToggleZoom,
    NudgeAll,
    NudgeFocused,
    ToggleHelp,
    Quit,
}

pub fn build_items(app: &App) -> Vec<PaletteItem> {
    let mut items = vec![
        PaletteItem {
            label: "Focus next pane".to_string(),
            action: PaletteAction::FocusNext,
        },
        PaletteItem {
            label: "Focus previous pane".to_string(),
            action: PaletteAction::FocusPrev,
        },
        PaletteItem {
            label: "Toggle layout".to_string(),
            action: PaletteAction::ToggleLayout,
        },
        PaletteItem {
            label: "Toggle zoom".to_string(),
            action: PaletteAction::ToggleZoom,
        },
        PaletteItem {
            label: "Nudge all workers".to_string(),
            action: PaletteAction::NudgeAll,
        },
        PaletteItem {
            label: "Nudge focused worker".to_string(),
            action: PaletteAction::NudgeFocused,
        },
        PaletteItem {
            label: "Toggle help".to_string(),
            action: PaletteAction::ToggleHelp,
        },
        PaletteItem {
            label: "Quit hive".to_string(),
            action: PaletteAction::Quit,
        },
    ];

    for (idx, window) in app.windows.iter().enumerate() {
        items.push(PaletteItem {
            label: format!("Focus window: {}", window.name),
            action: PaletteAction::FocusWindow(idx),
        });
    }

    for (idx, pane) in app.panes.iter().enumerate() {
        let title = match &pane.pane_type {
            PaneType::Architect => "architect".to_string(),
            PaneType::Worker { lane } => format!("{} ({})", pane.id, lane),
        };
        items.push(PaletteItem {
            label: format!("Focus pane: {}", title),
            action: PaletteAction::FocusPane(idx),
        });
    }

    items
}

pub fn filter_indices(items: &[PaletteItem], query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..items.len()).collect();
    }

    let query = query.to_lowercase();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.label.to_lowercase().contains(&query))
        .map(|(idx, _)| idx)
        .collect()
}
