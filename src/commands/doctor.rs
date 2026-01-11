use std::path::Path;

use anyhow::Result;

use crate::config::{self, TaskSource};
use crate::utils::git;

pub fn run(start_dir: &Path) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    let config = config::load_config(&config_path)?;
    let project_dir = config::project_dir(&config_path);

    let mut issues = Vec::new();

    if let TaskSource::Yaml = config.tasks.source {
        let tasks_path = config::tasks_file_path(&config_path, &config);
        if !tasks_path.exists() {
            issues.push(format!("Missing tasks file: {}", tasks_path.display()));
        }
    }

    let architect = project_dir.join(".hive").join("ARCHITECT.md");
    if !architect.exists() {
        issues.push("Missing .hive/ARCHITECT.md".to_string());
    }

    for window in &config.windows {
        for worker in &window.workers {
            let worker_file = project_dir
                .join(".hive")
                .join("workers")
                .join(&worker.id)
                .join("WORKER.md");
            if !worker_file.exists() {
                issues.push(format!("Missing worker role file: {}", worker_file.display()));
            }
        }
    }

    if let Err(err) = git::ensure_git_exclude(&project_dir) {
        issues.push(format!("Failed to update git exclude: {}", err));
    }

    if issues.is_empty() {
        println!("Hive doctor: no issues found");
    } else {
        println!("Hive doctor found {} issue(s):", issues.len());
        for issue in issues {
            println!("- {}", issue);
        }
    }

    Ok(())
}
