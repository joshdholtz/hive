use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{self, TaskSource};
use crate::tasks::yaml::{LaneTasks, TasksFile, WorkerProtocol};
use crate::utils::{git, shell};

pub fn run(start_dir: &Path) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    let config = config::load_config(&config_path)?;
    let project_dir = config::project_dir(&config_path);

    let mut issues = Vec::new();
    let mut fixes = Vec::new();

    if let TaskSource::Yaml = config.tasks.source {
        let tasks_path = config::tasks_file_path(&config_path, &config);
        if !tasks_path.exists() {
            issues.push(format!("Missing tasks file: {}", tasks_path.display()));
            if let Err(err) = create_tasks_file(&config, &tasks_path) {
                fixes.push(format!("Failed to create tasks file: {}", err));
            } else {
                fixes.push(format!("Created tasks file: {}", tasks_path.display()));
            }
        }
    }

    let architect = project_dir.join(".hive").join("ARCHITECT.md");
    let mut missing_role = false;
    if !architect.exists() {
        issues.push("Missing .hive/ARCHITECT.md".to_string());
        missing_role = true;
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
                missing_role = true;
            }
        }
    }

    if missing_role {
        if let Err(err) = crate::commands::role::run(&project_dir, None) {
            fixes.push(format!("Failed to regenerate role files: {}", err));
        } else {
            fixes.push("Regenerated role files".to_string());
        }
    }

    if let Err(err) = git::ensure_git_exclude(&project_dir) {
        issues.push(format!("Failed to update git exclude: {}", err));
    } else {
        fixes.push("Ensured .hive/ is in git exclude".to_string());
    }

    let backend_cmd = match config.workers.backend {
        crate::config::Backend::Claude => "claude",
        crate::config::Backend::Codex => "codex",
    };
    if !shell::command_available(backend_cmd) {
        issues.push(format!("Missing required backend command: {}", backend_cmd));
    }

    if issues.is_empty() {
        println!("Hive doctor: no issues found");
    } else {
        println!("Hive doctor found {} issue(s):", issues.len());
        for issue in issues {
            println!("- {}", issue);
        }
    }

    if !fixes.is_empty() {
        println!("\nFixes:");
        for fix in fixes {
            println!("- {}", fix);
        }
    }

    Ok(())
}

fn create_tasks_file(config: &crate::config::HiveConfig, path: &Path) -> Result<()> {
    let mut tasks = TasksFile::default();
    tasks.worker_protocol = Some(WorkerProtocol {
        claim: Some("Move the task to in_progress and add claimed_by/claimed_at".to_string()),
        complete: Some("Move the task to done and add summary/files_changed".to_string()),
    });
    tasks.rules = Some(vec![
        "Claim one task at a time".to_string(),
        "Create a PR before starting a new task".to_string(),
    ]);

    for window in &config.windows {
        for worker in &window.workers {
            let lane = worker.lane.clone().unwrap_or_else(|| worker.id.clone());
            tasks.lanes.entry(lane).or_insert_with(LaneTasks::default);
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_yaml::to_string(&tasks)?;
    std::fs::write(path, content)
        .with_context(|| format!("Failed writing {}", path.display()))?;
    Ok(())
}
