use std::path::Path;

use anyhow::{Context, Result};

use crate::config;
use crate::projects;
use crate::utils::git;

pub fn run(start_dir: &Path) -> Result<()> {
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
