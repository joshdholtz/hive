use std::path::Path;

use anyhow::{Context, Result};

use crate::config;

pub fn run(start_dir: &Path) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    let project_dir = config::project_dir(&config_path);
    let pid_path = project_dir.join(".hive").join("hive.pid");
    let socket_path = project_dir.join(".hive").join("hive.sock");

    if !pid_path.exists() {
        anyhow::bail!("No running hive session found (missing .hive/hive.pid)");
    }

    let pid = std::fs::read_to_string(&pid_path)
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
