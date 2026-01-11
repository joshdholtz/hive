use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct TasksFile {
    pub worker_protocol: Option<WorkerProtocol>,
    pub rules: Option<Vec<String>>,
    pub global_backlog: Option<Vec<Task>>,
    #[serde(flatten)]
    pub lanes: HashMap<String, LaneTasks>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct WorkerProtocol {
    pub claim: Option<String>,
    pub complete: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct LaneTasks {
    #[serde(default)]
    pub backlog: Vec<Task>,
    #[serde(default)]
    pub in_progress: Vec<Task>,
    #[serde(default)]
    pub done: Vec<Task>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub acceptance: Option<Vec<String>>,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<String>,
    pub summary: Option<String>,
    pub files_changed: Option<Vec<String>>,
    pub question: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TaskCounts {
    pub backlog: usize,
    pub in_progress: usize,
    pub done: usize,
}

pub fn load_tasks(path: &Path) -> Result<TasksFile> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed reading tasks file at {}", path.display()))?;
    let tasks: TasksFile = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed parsing tasks file at {}", path.display()))?;
    Ok(tasks)
}

pub fn counts_for_lane(tasks: &TasksFile, lane: &str) -> TaskCounts {
    if let Some(lane_tasks) = tasks.lanes.get(lane) {
        TaskCounts {
            backlog: lane_tasks.backlog.len(),
            in_progress: lane_tasks.in_progress.len(),
            done: lane_tasks.done.len(),
        }
    } else {
        TaskCounts::default()
    }
}
