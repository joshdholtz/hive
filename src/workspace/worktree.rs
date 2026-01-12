use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use super::config::{slug_from_path, WorkspaceProject};

/// Information about a created worktree
#[derive(Debug, Clone)]
pub struct WorktreeInfo {
    pub worker_index: usize,
    pub path: PathBuf,
    pub branch: String,
}

/// Create worktrees for a project that needs multiple workers (lanes)
///
/// The first lane uses the original repository.
/// Additional lanes get their own worktrees named by lane in the workspace's worktrees directory.
pub fn create_worktrees(
    workspace_dir: &Path,
    project: &WorkspaceProject,
) -> Result<Vec<WorktreeInfo>> {
    if project.lanes.len() <= 1 {
        return Ok(Vec::new());
    }

    let worktrees_dir = workspace_dir.join("worktrees");
    std::fs::create_dir_all(&worktrees_dir)?;

    let project_slug = slug_from_path(&project.path);
    let mut results = Vec::new();

    // Lanes after the first get worktrees (first lane uses original repo)
    for (i, lane) in project.lanes.iter().enumerate().skip(1) {
        let worktree_name = format!("{}-{}", project_slug, lane);
        let worktree_path = worktrees_dir.join(&worktree_name);
        let branch_name = format!("hive/{}-{}", project_slug, lane);

        // Skip if worktree already exists
        if worktree_path.exists() {
            results.push(WorktreeInfo {
                worker_index: i + 1,
                path: worktree_path,
                branch: branch_name,
            });
            continue;
        }

        // Create the worktree with a new branch
        git_create_worktree(&project.path, &worktree_path, &branch_name)?;

        results.push(WorktreeInfo {
            worker_index: i + 1,
            path: worktree_path,
            branch: branch_name,
        });
    }

    Ok(results)
}

/// Remove all worktrees in a workspace
pub fn remove_worktrees(workspace_dir: &Path) -> Result<()> {
    let worktrees_dir = workspace_dir.join("worktrees");
    if !worktrees_dir.exists() {
        return Ok(());
    }

    // List all directories in worktrees/
    for entry in std::fs::read_dir(&worktrees_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        // Try to find the original repo for this worktree
        if let Ok(git_dir) = std::fs::read_to_string(path.join(".git")) {
            // .git file contains "gitdir: /path/to/original/.git/worktrees/name"
            if let Some(repo_path) = parse_gitdir_path(&git_dir) {
                // Try to remove the worktree properly
                let _ = git_remove_worktree(&repo_path, &path);
            }
        }

        // Force remove the directory if it still exists
        if path.exists() {
            std::fs::remove_dir_all(&path).ok();
        }
    }

    Ok(())
}

/// Get the working directory for a specific worker
pub fn worker_directory(
    workspace_dir: &Path,
    project: &WorkspaceProject,
    worker_index: usize,
) -> PathBuf {
    if project.workers == 1 || worker_index == 0 {
        // First worker (index 0) uses original repo
        project.path.clone()
    } else {
        // Subsequent workers use worktrees
        let slug = slug_from_path(&project.path);
        workspace_dir
            .join("worktrees")
            .join(format!("{}-worker-{}", slug, worker_index + 1))
    }
}

/// Create a git worktree
fn git_create_worktree(repo: &Path, dest: &Path, branch: &str) -> Result<()> {
    let output = Command::new("git")
        .args(["-C", &repo.to_string_lossy()])
        .args(["worktree", "add", "-b", branch, &dest.to_string_lossy()])
        .output()
        .context("Failed to run git worktree add")?;

    if !output.status.success() {
        // Try without -b in case branch already exists
        let output = Command::new("git")
            .args(["-C", &repo.to_string_lossy()])
            .args(["worktree", "add", &dest.to_string_lossy(), branch])
            .output()
            .context("Failed to run git worktree add")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("git worktree add failed: {}", stderr);
        }
    }

    Ok(())
}

/// Remove a git worktree
fn git_remove_worktree(repo: &Path, worktree: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["-C", &repo.to_string_lossy()])
        .args(["worktree", "remove", "--force", &worktree.to_string_lossy()])
        .output()
        .context("Failed to run git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree remove failed: {}", stderr);
    }

    Ok(())
}

/// Parse the gitdir path from a .git file contents
fn parse_gitdir_path(content: &str) -> Option<PathBuf> {
    let line = content.lines().next()?;
    let path = line.strip_prefix("gitdir: ")?;

    // The path points to .git/worktrees/name, we want the repo root
    let gitdir = PathBuf::from(path.trim());

    // Go up from .git/worktrees/name to repo root
    gitdir
        .parent() // worktrees
        .and_then(|p| p.parent()) // .git
        .and_then(|p| p.parent()) // repo root
        .map(|p| p.to_path_buf())
}

/// List existing worktrees for a project
pub fn list_worktrees(repo: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["-C", &repo.to_string_lossy()])
        .args(["worktree", "list", "--porcelain"])
        .output()
        .context("Failed to run git worktree list")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut worktrees = Vec::new();

    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            worktrees.push(PathBuf::from(path));
        }
    }

    Ok(worktrees)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_directory_single() {
        let project = WorkspaceProject {
            path: PathBuf::from("/code/repo"),
            workers: 1,
            lanes: vec!["default".to_string()],
        };

        let dir = worker_directory(Path::new("/home/.hive/workspaces/test"), &project, 0);
        assert_eq!(dir, PathBuf::from("/code/repo"));
    }

    #[test]
    fn test_worker_directory_multiple() {
        let project = WorkspaceProject {
            path: PathBuf::from("/code/repo"),
            workers: 3,
            lanes: vec!["api".to_string()],
        };

        let workspace = Path::new("/home/.hive/workspaces/test");

        // Worker 0 uses original
        assert_eq!(
            worker_directory(workspace, &project, 0),
            PathBuf::from("/code/repo")
        );

        // Workers 1+ use worktrees
        assert!(worker_directory(workspace, &project, 1)
            .to_string_lossy()
            .contains("worktrees"));
    }

    #[test]
    fn test_parse_gitdir_path() {
        let content = "gitdir: /Users/josh/code/repo/.git/worktrees/repo-worker-2\n";
        let path = parse_gitdir_path(content);
        assert_eq!(path, Some(PathBuf::from("/Users/josh/code/repo")));
    }
}
