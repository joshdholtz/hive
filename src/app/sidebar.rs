use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::app::state::ClientPane;
use crate::app::types::PaneType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarSelection {
    Group(String),
    Pane(String),
}

#[derive(Clone, Debug)]
pub struct SidebarState {
    pub visible: bool,
    pub focused: bool,
    pub selection: SidebarSelection,
    expanded: HashMap<String, bool>,
}

#[derive(Clone, Debug)]
pub enum SidebarRowKind {
    Group {
        name: String,
        count: usize,
        expanded: bool,
    },
    Pane {
        pane_id: String,
        group: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct SidebarRow {
    pub kind: SidebarRowKind,
    pub indent: usize,
}

impl SidebarState {
    pub fn new() -> Self {
        Self {
            visible: true,
            focused: false,
            selection: SidebarSelection::Pane("architect".to_string()),
            expanded: HashMap::new(),
        }
    }

    pub fn rows(&self, panes: &[ClientPane]) -> Vec<SidebarRow> {
        let mut rows = Vec::new();

        if let Some(architect) = panes
            .iter()
            .find(|pane| matches!(pane.pane_type, PaneType::Architect))
        {
            rows.push(SidebarRow {
                kind: SidebarRowKind::Pane {
                    pane_id: architect.id.clone(),
                    group: None,
                },
                indent: 0,
            });
        }

        let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut standalone: Vec<String> = Vec::new();

        for pane in panes {
            if matches!(pane.pane_type, PaneType::Architect) {
                continue;
            }
            if let Some(group) = &pane.group {
                grouped.entry(group.clone()).or_default().push(pane.id.clone());
            } else {
                standalone.push(pane.id.clone());
            }
        }

        for children in grouped.values_mut() {
            children.sort();
        }
        standalone.sort();

        for (group, children) in grouped {
            let expanded = self.expanded.get(&group).copied().unwrap_or(true);
            rows.push(SidebarRow {
                kind: SidebarRowKind::Group {
                    name: group.clone(),
                    count: children.len(),
                    expanded,
                },
                indent: 0,
            });
            if expanded {
                for child in children {
                    rows.push(SidebarRow {
                        kind: SidebarRowKind::Pane {
                            pane_id: child,
                            group: Some(group.clone()),
                        },
                        indent: 2,
                    });
                }
            }
        }

        for pane_id in standalone {
            rows.push(SidebarRow {
                kind: SidebarRowKind::Pane {
                    pane_id,
                    group: None,
                },
                indent: 0,
            });
        }

        rows
    }

    pub fn selected_index(&self, panes: &[ClientPane]) -> usize {
        let selections = self.row_selections(panes);
        selections
            .iter()
            .position(|sel| sel == &self.selection)
            .unwrap_or(0)
    }

    pub fn ensure_selection(&mut self, panes: &[ClientPane]) {
        let selections = self.row_selections(panes);
        if selections.is_empty() {
            return;
        }
        if !selections.iter().any(|sel| sel == &self.selection) {
            self.selection = selections[0].clone();
        }
    }

    pub fn move_up(&mut self, panes: &[ClientPane]) {
        let selections = self.row_selections(panes);
        if selections.is_empty() {
            return;
        }
        let idx = selections
            .iter()
            .position(|sel| sel == &self.selection)
            .unwrap_or(0);
        let next = if idx == 0 { selections.len() - 1 } else { idx - 1 };
        self.selection = selections[next].clone();
    }

    pub fn move_down(&mut self, panes: &[ClientPane]) {
        let selections = self.row_selections(panes);
        if selections.is_empty() {
            return;
        }
        let idx = selections
            .iter()
            .position(|sel| sel == &self.selection)
            .unwrap_or(0);
        let next = (idx + 1) % selections.len();
        self.selection = selections[next].clone();
    }

    pub fn toggle_selected(&mut self, panes: &mut [ClientPane]) -> Vec<(String, bool)> {
        match &self.selection {
            SidebarSelection::Pane(pane_id) => {
                if let Some(pane) = panes.iter_mut().find(|pane| &pane.id == pane_id) {
                    pane.visible = !pane.visible;
                    return vec![(pane.id.clone(), pane.visible)];
                }
            }
            SidebarSelection::Group(group) => {
                let any_hidden = panes.iter().any(|pane| {
                    pane.group.as_deref() == Some(group.as_str()) && !pane.visible
                });
                let target = any_hidden;
                let mut changes = Vec::new();
                for pane in panes.iter_mut() {
                    if pane.group.as_deref() == Some(group.as_str()) {
                        pane.visible = target;
                        changes.push((pane.id.clone(), target));
                    }
                }
                return changes;
            }
        }
        Vec::new()
    }

    pub fn select_all(&mut self, panes: &mut [ClientPane]) -> Vec<(String, bool)> {
        self.set_visibility(panes, true)
    }

    pub fn select_none(&mut self, panes: &mut [ClientPane]) -> Vec<(String, bool)> {
        self.set_visibility(panes, false)
    }

    pub fn collapse_selected(&mut self) {
        if let SidebarSelection::Group(group) = &self.selection {
            self.expanded.insert(group.clone(), false);
        }
    }

    pub fn expand_selected(&mut self) {
        if let SidebarSelection::Group(group) = &self.selection {
            self.expanded.insert(group.clone(), true);
        }
    }

    pub fn selected_pane_id(&self) -> Option<String> {
        match &self.selection {
            SidebarSelection::Pane(pane_id) => Some(pane_id.clone()),
            _ => None,
        }
    }

    fn set_visibility(&mut self, panes: &mut [ClientPane], visible: bool) -> Vec<(String, bool)> {
        let mut changes = Vec::new();
        match &self.selection {
            SidebarSelection::Group(group) => {
                for pane in panes.iter_mut() {
                    if pane.group.as_deref() == Some(group.as_str()) {
                        pane.visible = visible;
                        changes.push((pane.id.clone(), visible));
                    }
                }
            }
            _ => {
                for pane in panes.iter_mut() {
                    pane.visible = visible;
                    changes.push((pane.id.clone(), visible));
                }
            }
        }
        changes
    }

    fn row_selections(&self, panes: &[ClientPane]) -> Vec<SidebarSelection> {
        self.rows(panes)
            .into_iter()
            .map(|row| match row.kind {
                SidebarRowKind::Group { name, .. } => SidebarSelection::Group(name),
                SidebarRowKind::Pane { pane_id, .. } => SidebarSelection::Pane(pane_id),
            })
            .collect()
    }
}
