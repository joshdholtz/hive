use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

pub fn git_common_dir(repo_dir: &Path) -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(repo_dir)
        .output()
        .context("Failed to run git rev-parse")?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = if Path::new(&path).is_absolute() {
        PathBuf::from(path)
    } else {
        repo_dir.join(path)
    };

    Ok(path)
}

pub fn ensure_git_exclude(repo_dir: &Path) -> Result<()> {
    let git_dir = git_common_dir(repo_dir)?;
    let exclude_path = git_dir.join("info").join("exclude");

    let mut content = if exclude_path.exists() {
        fs::read_to_string(&exclude_path).unwrap_or_default()
    } else {
        String::new()
    };

    if !content.lines().any(|line| line.trim() == ".hive/") {
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str(".hive/\n");
        fs::write(&exclude_path, content)
            .with_context(|| format!("Failed writing {}", exclude_path.display()))?;
    }

    Ok(())
}

pub fn remove_git_exclude(repo_dir: &Path) -> Result<()> {
    let git_dir = git_common_dir(repo_dir)?;
    let exclude_path = git_dir.join("info").join("exclude");
    if !exclude_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&exclude_path).unwrap_or_default();
    let filtered: Vec<&str> = content
        .lines()
        .filter(|line| line.trim() != ".hive/")
        .collect();
    let mut new_content = filtered.join("\n");
    if !new_content.is_empty() {
        new_content.push('\n');
    }
    fs::write(&exclude_path, new_content)
        .with_context(|| format!("Failed writing {}", exclude_path.display()))?;
    Ok(())
}
