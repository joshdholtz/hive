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
    pub projects: HashMap<String, ProjectEntry>,
}

/// A project entry can be either:
/// - Direct lane tasks (for single-lane projects like android-sdk)
/// - Nested lanes (for multi-lane projects like backend with fixes/features/misc)
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ProjectEntry {
    /// Direct lane tasks: android-sdk: { backlog: [], in_progress: [] }
    /// Must come first - keys are "backlog", "in_progress", "done"
    Direct(LaneTasks),
    /// Nested lanes: backend: { fixes: { backlog: [] }, features: { backlog: [] } }
    /// Keys are lane names like "fixes", "features", etc.
    Nested(HashMap<String, LaneTasks>),
}

#[derive(Debug, Deserialize, Serialize, Default)]
pub struct WorkerProtocol {
    pub claim: Option<String>,
    pub complete: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(deny_unknown_fields)]
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
    /// Task title (short summary)
    pub title: Option<String>,
    /// Task description (detailed explanation)
    pub description: Option<String>,
    pub priority: Option<String>,
    pub acceptance: Option<Vec<String>>,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<String>,
    pub completed_at: Option<String>,
    pub summary: Option<String>,
    pub files_changed: Option<Vec<String>>,
    pub question: Option<String>,
    /// PR URL for completed tasks
    pub pr_url: Option<String>,
    /// Branch name for the task
    pub branch: Option<String>,
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

/// Get task counts for a lane. Lane format:
/// - "project/lane" for nested (e.g., "backend/fixes")
/// - "project" for direct (e.g., "android-sdk")
pub fn counts_for_lane(tasks: &TasksFile, lane: &str) -> TaskCounts {
    // Check if lane has a slash (nested format: project/lane)
    if let Some((project, sublane)) = lane.split_once('/') {
        if let Some(ProjectEntry::Nested(lanes)) = tasks.projects.get(project) {
            if let Some(lane_tasks) = lanes.get(sublane) {
                return TaskCounts {
                    backlog: lane_tasks.backlog.len(),
                    in_progress: lane_tasks.in_progress.len(),
                    done: lane_tasks.done.len(),
                };
            }
        }
    } else {
        // Direct format: project name is the lane
        match tasks.projects.get(lane) {
            Some(ProjectEntry::Direct(lane_tasks)) => {
                return TaskCounts {
                    backlog: lane_tasks.backlog.len(),
                    in_progress: lane_tasks.in_progress.len(),
                    done: lane_tasks.done.len(),
                };
            }
            Some(ProjectEntry::Nested(_)) => {
                // Project has nested lanes but was queried without sublane
                // This shouldn't happen with proper config
            }
            None => {}
        }
    }
    TaskCounts::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_direct_project() {
        let yaml = r#"
android-sdk:
  backlog:
    - id: task1
      title: Test task
  in_progress: []
  done:
    - id: task2
    - id: task3
"#;
        let tasks: TasksFile = serde_yaml::from_str(yaml).unwrap();

        // Check what type android-sdk was parsed as
        let entry = tasks.projects.get("android-sdk").expect("android-sdk should exist");
        match entry {
            ProjectEntry::Direct(lt) => {
                println!("Parsed as Direct: backlog={} done={}", lt.backlog.len(), lt.done.len());
                assert_eq!(lt.backlog.len(), 1, "backlog should have 1 task");
                assert_eq!(lt.done.len(), 2, "done should have 2 tasks");
            }
            ProjectEntry::Nested(lanes) => {
                println!("Parsed as Nested with lanes: {:?}", lanes.keys().collect::<Vec<_>>());
                panic!("Should be parsed as Direct, not Nested");
            }
        }
    }

    #[test]
    fn test_parse_mixed_nested_and_direct() {
        let yaml = r#"
worker_protocol:
  claim: Move the task
rules:
- Claim one task

backend:
  features:
    backlog: []
    in_progress: []
    done:
      - id: done-task
  fixes:
    backlog:
      - id: fix-task
    in_progress: []
    done: []

android-sdk:
  backlog:
    - id: android-task
  in_progress: []
  done:
    - id: done1
    - id: done2
"#;
        let tasks: TasksFile = serde_yaml::from_str(yaml).unwrap();

        // Check backend (should be Nested)
        let backend = tasks.projects.get("backend").expect("backend should exist");
        match backend {
            ProjectEntry::Nested(lanes) => {
                println!("backend -> Nested with lanes: {:?}", lanes.keys().collect::<Vec<_>>());
                let fixes = lanes.get("fixes").unwrap();
                assert_eq!(fixes.backlog.len(), 1, "fixes backlog should have 1 task");
            }
            ProjectEntry::Direct(_) => panic!("backend should be Nested"),
        }

        // Check android-sdk (should be Direct)
        let android = tasks.projects.get("android-sdk").expect("android-sdk should exist");
        match android {
            ProjectEntry::Direct(lt) => {
                println!("android-sdk -> Direct: backlog={} done={}", lt.backlog.len(), lt.done.len());
                assert_eq!(lt.backlog.len(), 1, "android backlog should have 1 task");
                assert_eq!(lt.done.len(), 2, "android done should have 2 tasks");
            }
            ProjectEntry::Nested(lanes) => {
                println!("android-sdk -> WRONGLY Nested with lanes: {:?}", lanes.keys().collect::<Vec<_>>());
                panic!("android-sdk should be Direct, not Nested");
            }
        }

        // Test counts_for_lane
        let android_counts = counts_for_lane(&tasks, "android-sdk");
        println!("counts_for_lane(android-sdk) = {:?}", android_counts);
        assert_eq!(android_counts.backlog, 1);
        assert_eq!(android_counts.done, 2);
    }
}
