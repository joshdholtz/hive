use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::state::App;
use crate::tasks::{load_tasks, LaneTasks, ProjectEntry, Task, TasksFile};

/// Represents a lane with its tasks for display
struct LaneDisplay {
    name: String,
    tasks: LaneTasks,
}

pub fn render_task_queue(frame: &mut Frame, app: &App) {
    let area = centered_rect(80, 80, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(" Task Queue ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);

    // Load tasks from file
    let tasks_path = app.project_dir.join("tasks.yaml");
    let tasks_file = match load_tasks(&tasks_path) {
        Ok(t) => t,
        Err(_) => {
            let error_msg =
                Paragraph::new("Failed to load tasks.yaml").style(Style::default().fg(Color::Red));
            frame.render_widget(error_msg, inner);
            return;
        }
    };

    // Build list of lanes
    let lanes = collect_lanes(&tasks_file);

    if lanes.is_empty() {
        let empty_msg =
            Paragraph::new("No tasks found").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(empty_msg, inner);
        return;
    }

    // Build display lines
    let mut items: Vec<ListItem> = Vec::new();
    let mut line_idx = 0;

    for lane in &lanes {
        let expanded = *app.task_queue_expanded.get(&lane.name).unwrap_or(&true);
        let backlog_count = lane.tasks.backlog.len();
        let in_progress_count = lane.tasks.in_progress.len();
        let done_count = lane.tasks.done.len();

        // Lane header
        let arrow = if expanded { "▼" } else { "▶" };
        let header = format!(
            "{} {} ({} backlog, {} in progress, {} done)",
            arrow, lane.name, backlog_count, in_progress_count, done_count
        );
        let header_style = if line_idx == app.task_queue_selection {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        };
        items.push(ListItem::new(Line::from(header).style(header_style)));
        line_idx += 1;

        if expanded {
            // Backlog tasks
            for task in &lane.tasks.backlog {
                let line = format_task_line(task, "○", "backlog");
                let style = if line_idx == app.task_queue_selection {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                };
                items.push(ListItem::new(Line::from(line).style(style)));
                line_idx += 1;
            }

            // In-progress tasks
            for task in &lane.tasks.in_progress {
                let line = format_task_line(task, "◐", "in_progress");
                let style = if line_idx == app.task_queue_selection {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Blue)
                };
                items.push(ListItem::new(Line::from(line).style(style)));
                line_idx += 1;

                // Show claimed_by
                if let Some(claimed_by) = &task.claimed_by {
                    let claimed_line = format!("     └─ claimed by {}", claimed_by);
                    let claimed_style = if line_idx == app.task_queue_selection {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    items.push(ListItem::new(Line::from(claimed_line).style(claimed_style)));
                    line_idx += 1;
                }
            }

            // Done tasks
            for task in &lane.tasks.done {
                let line = format_task_line(task, "✓", "done");
                let style = if line_idx == app.task_queue_selection {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Green)
                };
                items.push(ListItem::new(Line::from(line).style(style)));
                line_idx += 1;

                // Show metadata for done tasks
                let mut meta_parts = Vec::new();
                if let Some(pr_url) = &task.pr_url {
                    meta_parts.push(format!("PR: {}", pr_url));
                }
                if let Some(branch) = &task.branch {
                    meta_parts.push(format!("Branch: {}", branch));
                }
                if !meta_parts.is_empty() {
                    let meta_line = format!("     └─ {}", meta_parts.join("  "));
                    let meta_style = if line_idx == app.task_queue_selection {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    items.push(ListItem::new(Line::from(meta_line).style(meta_style)));
                    line_idx += 1;
                }
                if let Some(summary) = &task.summary {
                    let summary_line = format!("     └─ {}", summary);
                    let summary_style = if line_idx == app.task_queue_selection {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };
                    items.push(ListItem::new(Line::from(summary_line).style(summary_style)));
                    line_idx += 1;
                }
            }
        }
    }

    // Split inner area for list and help text
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);

    let list = List::new(items);
    frame.render_widget(list, chunks[0]);

    // Help text at bottom
    let help =
        Paragraph::new("[q/Esc] Close  [↑↓/jk] Navigate  [Space] Toggle  [Enter] Jump to lane")
            .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, chunks[1]);
}

fn format_task_line(task: &Task, icon: &str, _status: &str) -> String {
    let title = task.title.as_deref().unwrap_or(&task.id);
    format!("   {} {}", icon, title)
}

fn collect_lanes(tasks: &TasksFile) -> Vec<LaneDisplay> {
    let mut lanes = Vec::new();

    for (project_name, entry) in &tasks.projects {
        match entry {
            ProjectEntry::Direct(lane_tasks) => {
                lanes.push(LaneDisplay {
                    name: project_name.clone(),
                    tasks: LaneTasks {
                        backlog: lane_tasks.backlog.clone(),
                        in_progress: lane_tasks.in_progress.clone(),
                        done: lane_tasks.done.clone(),
                    },
                });
            }
            ProjectEntry::Nested(nested_lanes) => {
                for (lane_name, lane_tasks) in nested_lanes {
                    lanes.push(LaneDisplay {
                        name: format!("{}/{}", project_name, lane_name),
                        tasks: LaneTasks {
                            backlog: lane_tasks.backlog.clone(),
                            in_progress: lane_tasks.in_progress.clone(),
                            done: lane_tasks.done.clone(),
                        },
                    });
                }
            }
        }
    }

    // Sort by name for consistent display
    lanes.sort_by(|a, b| a.name.cmp(&b.name));
    lanes
}

/// Count total displayable lines for navigation bounds
pub fn count_lines(app: &App) -> usize {
    let tasks_path = app.project_dir.join("tasks.yaml");
    let tasks_file = match load_tasks(&tasks_path) {
        Ok(t) => t,
        Err(_) => return 0,
    };

    let lanes = collect_lanes(&tasks_file);
    let mut count = 0;

    for lane in &lanes {
        count += 1; // Lane header
        let expanded = *app.task_queue_expanded.get(&lane.name).unwrap_or(&true);
        if expanded {
            count += lane.tasks.backlog.len();
            // In-progress: task + optional claimed_by line
            for task in &lane.tasks.in_progress {
                count += 1;
                if task.claimed_by.is_some() {
                    count += 1;
                }
            }
            // Done: task + optional metadata lines
            for task in &lane.tasks.done {
                count += 1;
                if task.pr_url.is_some() || task.branch.is_some() {
                    count += 1;
                }
                if task.summary.is_some() {
                    count += 1;
                }
            }
        }
    }

    count
}

/// Get lane name at the current selection (if it's a lane header)
pub fn get_selected_lane(app: &App) -> Option<String> {
    let tasks_path = app.project_dir.join("tasks.yaml");
    let tasks_file = match load_tasks(&tasks_path) {
        Ok(t) => t,
        Err(_) => return None,
    };

    let lanes = collect_lanes(&tasks_file);
    let mut line_idx = 0;

    for lane in &lanes {
        if line_idx == app.task_queue_selection {
            return Some(lane.name.clone());
        }
        line_idx += 1;

        let expanded = *app.task_queue_expanded.get(&lane.name).unwrap_or(&true);
        if expanded {
            line_idx += lane.tasks.backlog.len();
            for task in &lane.tasks.in_progress {
                line_idx += 1;
                if task.claimed_by.is_some() {
                    line_idx += 1;
                }
            }
            for task in &lane.tasks.done {
                line_idx += 1;
                if task.pr_url.is_some() || task.branch.is_some() {
                    line_idx += 1;
                }
                if task.summary.is_some() {
                    line_idx += 1;
                }
            }
        }
    }

    None
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
