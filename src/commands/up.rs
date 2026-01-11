use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::commands::attach;
use crate::config;

pub fn run(start_dir: &Path, daemon: bool) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    let project_dir = config::project_dir(&config_path);
    let socket_path = project_dir.join(".hive").join("hive.sock");

    if daemon {
        return spawn_server(&config_path);
    }

    if socket_path.exists() {
        if std::os::unix::net::UnixStream::connect(&socket_path).is_err() {
            let _ = std::fs::remove_file(&socket_path);
            spawn_server(&config_path)?;
            wait_for_socket(&socket_path)?;
        }
    } else {
        spawn_server(&config_path)?;
        wait_for_socket(&socket_path)?;
    }

    attach::run(start_dir)
}

fn spawn_server(config_path: &Path) -> Result<()> {
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
