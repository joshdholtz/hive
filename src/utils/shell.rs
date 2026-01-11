use std::path::Path;

use anyhow::{Context, Result};

pub fn run_shell_command(command: &str, cwd: &Path) -> Result<()> {
    let status = std::process::Command::new("sh")
        .arg("-lc")
        .arg(command)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("Failed running setup command: {}", command))?;

    if !status.success() {
        anyhow::bail!("Setup command failed: {}", command);
    }

    Ok(())
}

pub fn command_available(command: &str) -> bool {
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    for path in std::env::split_paths(&paths) {
        let full = path.join(command);
        if full.exists() {
            return true;
        }
    }
    false
}
