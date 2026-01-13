use std::fs::File;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
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
    let config_path = workspace_dir.join("workspace.yaml");
    let log_path = workspace_dir.join("hive.log");

    spawn_daemonized_server(&exe, &config_path, &log_path)
}

fn spawn_legacy_server(config_path: &Path) -> Result<()> {
    let exe = std::env::current_exe().context("Failed to locate hive binary")?;
    let project_dir = config::project_dir(config_path);
    let log_path = project_dir.join(".hive").join("hive.log");

    // Ensure .hive directory exists
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    spawn_daemonized_server(&exe, config_path, &log_path)
}

/// Spawn the server as a daemon that survives SSH logout
fn spawn_daemonized_server(exe: &Path, config_path: &Path, log_path: &Path) -> Result<()> {
    // Open log file for stdout/stderr
    let log_file = File::create(log_path)
        .with_context(|| format!("Failed to create log file: {}", log_path.display()))?;
    let log_file_err = log_file.try_clone()?;

    // Spawn with setsid to create new session (detaches from controlling terminal)
    // This allows the server to survive SSH logout
    unsafe {
        Command::new(exe)
            .arg("serve")
            .arg(config_path)
            .stdin(Stdio::null())
            .stdout(log_file)
            .stderr(log_file_err)
            .pre_exec(|| {
                // Create a new session - this detaches from the controlling terminal
                // The process becomes a session leader and won't receive SIGHUP
                // when the original terminal closes
                nix::unistd::setsid().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
                Ok(())
            })
            .spawn()
            .context("Failed to spawn hive server")?;
    }

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
