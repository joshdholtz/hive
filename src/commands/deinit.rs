use std::path::Path;

use anyhow::{Context, Result};

use crate::config;
use crate::projects;
use crate::utils::git;
use crate::workspace::{find_workspace_for_path, remove_worktrees};

pub fn run(start_dir: &Path) -> Result<()> {
    // First try workspace-based deinit
    if let Some(workspace_meta) = find_workspace_for_path(start_dir)? {
        return deinit_workspace(&workspace_meta.dir, &workspace_meta.name);
    }

    // Fall back to legacy .hive.yaml deinit
    let config_path = config::find_config(start_dir)?;
    let project_dir = config::project_dir(&config_path);

    if config_path.exists() {
        std::fs::remove_file(&config_path)
            .with_context(|| format!("Failed removing {}", config_path.display()))?;
    }

    let hive_dir = project_dir.join(".hive");
    if hive_dir.exists() {
        std::fs::remove_dir_all(&hive_dir)
            .with_context(|| format!("Failed removing {}", hive_dir.display()))?;
    }

    git::remove_git_exclude(&project_dir).ok();
    let _ = projects::remove_project(&project_dir);

    println!("Hive deinit complete for {}", project_dir.display());
    Ok(())
}

fn deinit_workspace(workspace_dir: &Path, name: &str) -> Result<()> {
    println!("Removing workspace: {}", name);

    // Remove worktrees first (cleans up git worktree references)
    if let Err(e) = remove_worktrees(workspace_dir) {
        eprintln!("Warning: failed to remove some worktrees: {}", e);
    }

    // Remove the workspace directory
    if workspace_dir.exists() {
        std::fs::remove_dir_all(workspace_dir)
            .with_context(|| format!("Failed removing {}", workspace_dir.display()))?;
    }

    println!("Workspace '{}' removed", name);
    Ok(())
}
