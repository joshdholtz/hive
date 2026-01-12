use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::commands::{attach, setup};
use crate::config;
use crate::workspace::resolve::find_workspace_for_path;

pub fn run(start_dir: &Path, daemon: bool) -> Result<()> {
    // First, check if we're in a workspace project
    if let Ok(Some(workspace)) = find_workspace_for_path(start_dir) {
        println!("Found workspace: {}", workspace.name);
        return run_workspace(&workspace.dir, daemon);
    }

    // Check for legacy .hive.yaml config
    if let Ok(config_path) = config::find_config(start_dir) {
        return run_legacy(&config_path, daemon);
    }

    // No workspace or config found, run setup wizard
    let workspace_dir = setup::run(start_dir)?;
    run_workspace(&workspace_dir, daemon)
}

/// Run a workspace from its directory
pub fn run_workspace(workspace_dir: &Path, daemon: bool) -> Result<()> {
    let socket_path = workspace_dir.join("hive.sock");

    if daemon {
        return spawn_workspace_server(workspace_dir);
    }

    if socket_path.exists() {
        if std::os::unix::net::UnixStream::connect(&socket_path).is_err() {
            let _ = std::fs::remove_file(&socket_path);
            spawn_workspace_server(workspace_dir)?;
            wait_for_socket(&socket_path)?;
        }
    } else {
        spawn_workspace_server(workspace_dir)?;
        wait_for_socket(&socket_path)?;
    }

    attach::run_workspace(workspace_dir)
}

/// Run with legacy .hive.yaml configuration
fn run_legacy(config_path: &Path, daemon: bool) -> Result<()> {
    let project_dir = config::project_dir(config_path);
    let socket_path = project_dir.join(".hive").join("hive.sock");

    if daemon {
        return spawn_legacy_server(config_path);
    }

    if socket_path.exists() {
        if std::os::unix::net::UnixStream::connect(&socket_path).is_err() {
            let _ = std::fs::remove_file(&socket_path);
            spawn_legacy_server(config_path)?;
            wait_for_socket(&socket_path)?;
        }
    } else {
        spawn_legacy_server(config_path)?;
        wait_for_socket(&socket_path)?;
    }

    attach::run(&project_dir)
}

fn spawn_workspace_server(workspace_dir: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("Failed to locate hive binary")?;

    // For workspaces, we'll use a different serve mode
    // For now, we need to create a compatible config path
    let config_path = workspace_dir.join("workspace.yaml");

    Command::new(exe)
        .arg("serve")
        .arg(&config_path)
        .spawn()
        .context("Failed to spawn hive server")?;
    Ok(())
}

fn spawn_legacy_server(config_path: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("Failed to locate hive binary")?;

    Command::new(exe)
        .arg("serve")
        .arg(config_path)
        .spawn()
        .context("Failed to spawn hive server")?;
    Ok(())
}

fn wait_for_socket(path: &Path) -> Result<()> {
    let start = Instant::now();
    while !path.exists() {
        if start.elapsed() > Duration::from_secs(5) {
            anyhow::bail!("Timed out waiting for hive server socket");
        }
        thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
