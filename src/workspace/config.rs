use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{ArchitectConfig, Backend, WorkersConfig};

/// Configuration for a workspace stored in ~/.hive/workspaces/{name}/workspace.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub name: String,
    /// The root directory where this workspace was created from
    #[serde(default)]
    pub root: Option<PathBuf>,
    pub projects: Vec<WorkspaceProject>,
    pub architect: ArchitectConfig,
    pub workers: WorkersConfig,
}

/// A project within a workspace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceProject {
    /// Path to the original git repository
    pub path: PathBuf,
    /// Number of workers for this project (1 = use original, 2+ = create worktrees)
    #[serde(default = "default_workers")]
    pub workers: usize,
    /// Lanes assigned to workers on this project
    pub lanes: Vec<String>,
}

fn default_workers() -> usize {
    1
}

impl WorkspaceConfig {
    /// Load workspace config from a workspace directory
    pub fn load(workspace_dir: &Path) -> Result<Self> {
        let config_path = workspace_dir.join("workspace.yaml");
        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("Failed reading {}", config_path.display()))?;
        let config: Self = serde_yaml::from_str(&content)
            .with_context(|| format!("Failed parsing {}", config_path.display()))?;
        Ok(config)
    }

    /// Save workspace config to a workspace directory
    pub fn save(&self, workspace_dir: &Path) -> Result<()> {
        let config_path = workspace_dir.join("workspace.yaml");
        let content = serde_yaml::to_string(self)?;
        std::fs::write(&config_path, content)
            .with_context(|| format!("Failed writing {}", config_path.display()))?;
        Ok(())
    }

    /// Get all unique lanes across all projects
    pub fn all_lanes(&self) -> Vec<String> {
        let mut lanes: Vec<String> = self
            .projects
            .iter()
            .flat_map(|p| p.lanes.iter().cloned())
            .collect();
        lanes.sort();
        lanes.dedup();
        lanes
    }

    /// Get total number of workers across all projects
    pub fn total_workers(&self) -> usize {
        self.projects.iter().map(|p| p.workers).sum()
    }
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            root: None,
            projects: Vec::new(),
            architect: ArchitectConfig {
                backend: Backend::Claude,
            },
            workers: WorkersConfig {
                backend: Backend::Claude,
                skip_permissions: false,
                setup: Vec::new(),
                symlink: Vec::new(),
            },
        }
    }
}

/// Branch naming configuration for a worker
#[derive(Debug, Clone)]
pub struct WorkerBranch {
    /// Local branch prefix (e.g., "repo-features/features")
    pub local: String,
    /// Remote branch prefix (e.g., "features")
    pub remote: String,
}

/// Runtime representation of a worker, with resolved paths
#[derive(Debug, Clone)]
pub struct RuntimeWorker {
    pub id: String,
    pub working_dir: PathBuf,
    pub lane: String,
    pub project_path: PathBuf,
    pub is_worktree: bool,
    /// Branch naming convention for this worker
    pub branch: Option<WorkerBranch>,
}

/// Expand a WorkspaceConfig into runtime workers with resolved directories
pub fn expand_workers(config: &WorkspaceConfig, workspace_dir: &Path) -> Vec<RuntimeWorker> {
    let mut workers = Vec::new();
    let mut worker_idx = 0;

    for project in &config.projects {
        let project_slug = slug_from_path(&project.path);

        // Each lane gets one worker
        for (i, lane) in project.lanes.iter().enumerate() {
            let (working_dir, is_worktree) = if project.lanes.len() == 1 || i == 0 {
                // First worker uses original repo
                (project.path.clone(), false)
            } else {
                // Subsequent workers use worktrees named by lane
                let worktree_name = format!("{}-{}", project_slug, lane);
                let worktree_path = workspace_dir.join("worktrees").join(&worktree_name);
                (worktree_path, true)
            };

            // Branch naming: local = <repo>-<lane>/<lane>, remote = <lane>
            let branch = Some(WorkerBranch {
                local: format!("{}-{}/{}", project_slug, lane, lane),
                remote: lane.clone(),
            });

            // Lane format:
            // - Multi-lane projects: "project/lane" (e.g., "backend/fixes")
            // - Single-lane projects: lane as-is (e.g., "android-sdk")
            let full_lane = if project.lanes.len() > 1 {
                format!("{}/{}", project_slug, lane)
            } else {
                lane.clone()
            };

            workers.push(RuntimeWorker {
                id: format!("worker-{}", worker_idx + 1),
                working_dir,
                lane: full_lane,
                project_path: project.path.clone(),
                is_worktree,
                branch,
            });

            worker_idx += 1;
        }
    }

    workers
}

/// Create a URL-safe slug from a path
pub fn slug_from_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_lowercase()
        .replace(' ', "-")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slug_from_path() {
        assert_eq!(slug_from_path(Path::new("/foo/bar/My Project")), "my-project");
        assert_eq!(slug_from_path(Path::new("/foo/bar/repo-1")), "repo-1");
    }

    #[test]
    fn test_expand_workers_single() {
        let config = WorkspaceConfig {
            name: "test".to_string(),
            projects: vec![WorkspaceProject {
                path: PathBuf::from("/code/repo"),
                workers: 1,
                lanes: vec!["default".to_string()],
            }],
            ..Default::default()
        };

        let workers = expand_workers(&config, Path::new("/home/.hive/workspaces/test"));
        assert_eq!(workers.len(), 1);
        assert_eq!(workers[0].working_dir, PathBuf::from("/code/repo"));
        assert!(!workers[0].is_worktree);
    }

    #[test]
    fn test_expand_workers_multiple() {
        let config = WorkspaceConfig {
            name: "test".to_string(),
            projects: vec![WorkspaceProject {
                path: PathBuf::from("/code/repo"),
                workers: 3,
                lanes: vec!["api".to_string(), "auth".to_string()],
            }],
            ..Default::default()
        };

        let workers = expand_workers(&config, Path::new("/home/.hive/workspaces/test"));
        assert_eq!(workers.len(), 3);

        // First worker uses original
        assert_eq!(workers[0].working_dir, PathBuf::from("/code/repo"));
        assert!(!workers[0].is_worktree);

        // Others use worktrees
        assert!(workers[1].working_dir.to_string_lossy().contains("worktrees"));
        assert!(workers[1].is_worktree);
        assert!(workers[2].is_worktree);
    }
}
