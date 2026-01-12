use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::projects::hive_home;

use super::config::WorkspaceConfig;

/// Metadata about a workspace
#[derive(Debug, Clone)]
pub struct WorkspaceMeta {
    pub name: String,
    pub dir: PathBuf,
    pub config: WorkspaceConfig,
}

impl WorkspaceMeta {
    /// Check if the workspace server is currently running
    pub fn is_running(&self) -> bool {
        let socket_path = self.dir.join("hive.sock");
        socket_path.exists()
    }
}

/// Get the workspaces directory (~/.hive/workspaces)
pub fn workspaces_dir() -> Result<PathBuf> {
    Ok(hive_home()?.join("workspaces"))
}

/// Get a specific workspace directory
pub fn workspace_dir(name: &str) -> Result<PathBuf> {
    Ok(workspaces_dir()?.join(name))
}

/// List all available workspaces
pub fn list_workspaces() -> Result<Vec<WorkspaceMeta>> {
    let dir = workspaces_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut workspaces = Vec::new();

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let config_path = path.join("workspace.yaml");
        if !config_path.exists() {
            continue;
        }

        if let Ok(config) = WorkspaceConfig::load(&path) {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            workspaces.push(WorkspaceMeta {
                name,
                dir: path,
                config,
            });
        }
    }

    workspaces.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(workspaces)
}

/// Find a workspace that contains the given path as one of its projects
pub fn find_workspace_for_path(path: &Path) -> Result<Option<WorkspaceMeta>> {
    let canonical = if path.exists() {
        path.canonicalize()?
    } else {
        path.to_path_buf()
    };

    let workspaces = list_workspaces()?;

    for workspace in workspaces {
        // First check if path matches or is inside the workspace root
        if let Some(ref root) = workspace.config.root {
            let root_canonical = if root.exists() {
                root.canonicalize().unwrap_or_else(|_| root.clone())
            } else {
                root.clone()
            };

            if canonical == root_canonical || canonical.starts_with(&root_canonical) {
                return Ok(Some(workspace));
            }
        }

        // Fall back to checking individual project paths
        for project in &workspace.config.projects {
            let project_path = if project.path.exists() {
                project.path.canonicalize().unwrap_or_else(|_| project.path.clone())
            } else {
                project.path.clone()
            };

            // Check if the given path is the project path or inside it
            if canonical == project_path || canonical.starts_with(&project_path) {
                return Ok(Some(workspace));
            }
        }
    }

    Ok(None)
}

/// Find a workspace by name
pub fn find_workspace_by_name(name: &str) -> Result<Option<WorkspaceMeta>> {
    let dir = workspace_dir(name)?;
    if !dir.exists() {
        return Ok(None);
    }

    let config_path = dir.join("workspace.yaml");
    if !config_path.exists() {
        return Ok(None);
    }

    let config = WorkspaceConfig::load(&dir)?;
    Ok(Some(WorkspaceMeta {
        name: name.to_string(),
        dir,
        config,
    }))
}

/// Create a new workspace directory with initial structure
pub fn create_workspace_dir(name: &str) -> Result<PathBuf> {
    let dir = workspace_dir(name)?;
    fs::create_dir_all(&dir)?;
    fs::create_dir_all(dir.join("lanes"))?;
    fs::create_dir_all(dir.join("worktrees"))?;
    Ok(dir)
}

/// Delete a workspace and all its contents
pub fn delete_workspace(name: &str) -> Result<()> {
    let dir = workspace_dir(name)?;
    if dir.exists() {
        // First remove any worktrees properly
        super::worktree::remove_worktrees(&dir)?;
        // Then remove the directory
        fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workspaces_dir() {
        // Just verify it doesn't panic
        let _ = workspaces_dir();
    }
}
