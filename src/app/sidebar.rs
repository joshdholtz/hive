use indexmap::IndexMap;
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

        // Use IndexMap to preserve insertion order (config order)
        let mut grouped: IndexMap<String, Vec<String>> = IndexMap::new();
        let mut standalone: Vec<String> = Vec::new();

        for pane in panes {
            if matches!(pane.pane_type, PaneType::Architect) {
                continue;
            }
            if let Some(group) = &pane.group {
                grouped
                    .entry(group.clone())
                    .or_default()
                    .push(pane.id.clone());
            } else {
                standalone.push(pane.id.clone());
            }
        }

        // Don't sort - preserve config order

        for (group, children) in grouped {
            // Single-worker groups become standalone (no nested group)
            if children.len() == 1 {
                standalone.push(children.into_iter().next().unwrap());
                continue;
            }

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
        let next = if idx == 0 {
            selections.len() - 1
        } else {
            idx - 1
        };
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
                let any_hidden = panes
                    .iter()
                    .any(|pane| pane.group.as_deref() == Some(group.as_str()) && !pane.visible);
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

    /// Get the visual order of pane IDs with their groups from sidebar rows (excludes group headers)
    fn visual_pane_order(&self, panes: &[ClientPane]) -> Vec<(String, Option<String>)> {
        self.rows(panes)
            .into_iter()
            .filter_map(|row| match row.kind {
                SidebarRowKind::Pane { pane_id, group } => Some((pane_id, group)),
                _ => None,
            })
            .collect()
    }

    /// Get the order of group names as they appear in the sidebar
    fn group_order(&self, panes: &[ClientPane]) -> Vec<String> {
        let mut groups = Vec::new();
        for pane in panes {
            if matches!(pane.pane_type, PaneType::Architect) {
                continue;
            }
            if let Some(group) = &pane.group {
                if !groups.contains(group) {
                    // Only include groups with 2+ members (single-member groups become standalone)
                    let count = panes
                        .iter()
                        .filter(|p| p.group.as_ref() == Some(group))
                        .count();
                    if count >= 2 {
                        groups.push(group.clone());
                    }
                }
            }
        }
        groups
    }

    /// Move selected group up (swap with previous group)
    fn reorder_group_up(&self, panes: &mut Vec<ClientPane>, group_name: &str) -> bool {
        let groups = self.group_order(panes);
        let Some(group_pos) = groups.iter().position(|g| g == group_name) else {
            return false;
        };

        // Can't move first group up
        if group_pos == 0 {
            return false;
        }

        let swap_with_group = &groups[group_pos - 1];

        // Find indices of panes in our group and the target group
        let our_indices: Vec<usize> = panes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.group.as_deref() == Some(group_name))
            .map(|(i, _)| i)
            .collect();

        let target_indices: Vec<usize> = panes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.group.as_deref() == Some(swap_with_group))
            .map(|(i, _)| i)
            .collect();

        if our_indices.is_empty() || target_indices.is_empty() {
            return false;
        }

        // Swap each pane in our group with corresponding pane in target group
        // This works because groups are contiguous in the array
        let min_len = our_indices.len().min(target_indices.len());
        for i in 0..min_len {
            panes.swap(our_indices[i], target_indices[i]);
        }

        // If groups have different sizes, we need to rotate the remaining panes
        if our_indices.len() != target_indices.len() {
            let start = target_indices[0];
            let end = our_indices[our_indices.len() - 1];
            panes[start..=end].rotate_left(target_indices.len());
        }

        true
    }

    /// Move selected group down (swap with next group)
    fn reorder_group_down(&self, panes: &mut Vec<ClientPane>, group_name: &str) -> bool {
        let groups = self.group_order(panes);
        let Some(group_pos) = groups.iter().position(|g| g == group_name) else {
            return false;
        };

        // Can't move last group down
        if group_pos >= groups.len() - 1 {
            return false;
        }

        let swap_with_group = &groups[group_pos + 1];

        // Find indices of panes in our group and the target group
        let our_indices: Vec<usize> = panes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.group.as_deref() == Some(group_name))
            .map(|(i, _)| i)
            .collect();

        let target_indices: Vec<usize> = panes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.group.as_deref() == Some(swap_with_group))
            .map(|(i, _)| i)
            .collect();

        if our_indices.is_empty() || target_indices.is_empty() {
            return false;
        }

        // Rotate the slice containing both groups to put target group first
        let start = our_indices[0];
        let end = target_indices[target_indices.len() - 1];
        panes[start..=end].rotate_left(our_indices.len());

        true
    }

    /// Move selected pane up in the visual order (only within same group/section)
    pub fn reorder_up(&self, panes: &mut Vec<ClientPane>) -> bool {
        match &self.selection {
            SidebarSelection::Group(group_name) => {
                return self.reorder_group_up(panes, group_name);
            }
            SidebarSelection::Pane(_) => {}
        }

        let pane_id = match &self.selection {
            SidebarSelection::Pane(id) => id,
            _ => return false,
        };

        // Get visual order of panes with their groups
        let visual_order = self.visual_pane_order(panes);

        // Find position in visual order
        let Some(visual_pos) = visual_order.iter().position(|(id, _)| id == pane_id) else {
            return false;
        };

        // Can't move first item up, and don't move architect (position 0)
        if visual_pos <= 1 {
            return false;
        }

        let current_group = &visual_order[visual_pos].1;
        let target_group = &visual_order[visual_pos - 1].1;

        // Only allow reordering within the same group/section
        if current_group != target_group {
            return false;
        }

        // Get the pane ID we're swapping with (previous in visual order)
        let swap_with_id = &visual_order[visual_pos - 1].0;

        // Find their indices in the panes vector
        let Some(idx) = panes.iter().position(|p| &p.id == pane_id) else {
            return false;
        };
        let Some(swap_idx) = panes.iter().position(|p| &p.id == swap_with_id) else {
            return false;
        };

        panes.swap(idx, swap_idx);
        true
    }

    /// Move selected pane down in the visual order (only within same group/section)
    pub fn reorder_down(&self, panes: &mut Vec<ClientPane>) -> bool {
        match &self.selection {
            SidebarSelection::Group(group_name) => {
                return self.reorder_group_down(panes, group_name);
            }
            SidebarSelection::Pane(_) => {}
        }

        let pane_id = match &self.selection {
            SidebarSelection::Pane(id) => id,
            _ => return false,
        };

        // Get visual order of panes with their groups
        let visual_order = self.visual_pane_order(panes);

        // Find position in visual order
        let Some(visual_pos) = visual_order.iter().position(|(id, _)| id == pane_id) else {
            return false;
        };

        // Can't move architect (position 0) or last item down
        if visual_pos == 0 || visual_pos >= visual_order.len() - 1 {
            return false;
        }

        let current_group = &visual_order[visual_pos].1;
        let target_group = &visual_order[visual_pos + 1].1;

        // Only allow reordering within the same group/section
        if current_group != target_group {
            return false;
        }

        // Get the pane ID we're swapping with (next in visual order)
        let swap_with_id = &visual_order[visual_pos + 1].0;

        // Find their indices in the panes vector
        let Some(idx) = panes.iter().position(|p| &p.id == pane_id) else {
            return false;
        };
        let Some(swap_idx) = panes.iter().position(|p| &p.id == swap_with_id) else {
            return false;
        };

        panes.swap(idx, swap_idx);
        true
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
