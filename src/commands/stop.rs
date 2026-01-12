use std::path::Path;

use anyhow::{Context, Result};

use crate::config;
use crate::workspace::resolve::find_workspace_for_path;

pub fn run(start_dir: &Path) -> Result<()> {
    // First check for workspace
    if let Ok(Some(workspace)) = find_workspace_for_path(start_dir) {
        return stop_workspace(&workspace.dir, &workspace.name);
    }

    // Fall back to legacy .hive.yaml
    let config_path = config::find_config(start_dir)?;
    let project_dir = config::project_dir(&config_path);
    let pid_path = project_dir.join(".hive").join("hive.pid");
    let socket_path = project_dir.join(".hive").join("hive.sock");

    stop_by_pid(&pid_path, &socket_path)
}

fn stop_workspace(workspace_dir: &Path, name: &str) -> Result<()> {
    let pid_path = workspace_dir.join("hive.pid");
    let socket_path = workspace_dir.join("hive.sock");

    if !pid_path.exists() && !socket_path.exists() {
        anyhow::bail!("Workspace '{}' is not running", name);
    }

    stop_by_pid(&pid_path, &socket_path)?;
    println!("Stopped workspace '{}'", name);
    Ok(())
}

fn stop_by_pid(pid_path: &Path, socket_path: &Path) -> Result<()> {
    if !pid_path.exists() {
        // No PID file, just clean up socket if it exists
        if socket_path.exists() {
            std::fs::remove_file(socket_path).ok();
        }
        return Ok(());
    }

    let pid = std::fs::read_to_string(pid_path)
        .context("Failed reading hive.pid")?
        .trim()
        .to_string();

    let status = std::process::Command::new("kill")
        .arg(&pid)
        .status()
        .context("Failed running kill")?;

    if !status.success() {
        anyhow::bail!("Failed to stop hive session (pid {})", pid);
    }

    std::fs::remove_file(pid_path).ok();
    if socket_path.exists() {
        std::fs::remove_file(socket_path).ok();
    }
    Ok(())
}
