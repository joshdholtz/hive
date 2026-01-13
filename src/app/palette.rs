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
    ToggleZoom,
    ToggleArchitectPosition,
    ToggleSidebar,
    FocusSidebar,
    ProjectManager,
    ToggleTaskQueue,
    NudgeAll,
    NudgeFocused,
    ToggleHelp,
    Detach,
    Stop,
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
            label: "Toggle sidebar".to_string(),
            action: PaletteAction::ToggleSidebar,
        },
        PaletteItem {
            label: "Focus sidebar".to_string(),
            action: PaletteAction::FocusSidebar,
        },
        PaletteItem {
            label: "Project manager".to_string(),
            action: PaletteAction::ProjectManager,
        },
        PaletteItem {
            label: "Task queue".to_string(),
            action: PaletteAction::ToggleTaskQueue,
        },
        PaletteItem {
            label: "Toggle zoom".to_string(),
            action: PaletteAction::ToggleZoom,
        },
        PaletteItem {
            label: "Architect position (top/left)".to_string(),
            action: PaletteAction::ToggleArchitectPosition,
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
            label: "Detach from session".to_string(),
            action: PaletteAction::Detach,
        },
        PaletteItem {
            label: "Stop server and exit".to_string(),
            action: PaletteAction::Stop,
        },
    ];

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
    let trimmed = query.trim();

    // ">" prefix filters to only pane items
    if trimmed.starts_with('>') {
        let pane_query = trimmed[1..].trim().to_lowercase();
        return items
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                matches!(item.action, PaletteAction::FocusPane(_))
                    && (pane_query.is_empty() || item.label.to_lowercase().contains(&pane_query))
            })
            .map(|(idx, _)| idx)
            .collect();
    }

    if trimmed.is_empty() {
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
