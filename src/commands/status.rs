use std::path::Path;

use anyhow::Result;

use crate::config::{self, TaskSource};
use crate::tasks::{counts_for_lane, load_tasks};
use crate::workspace::resolve::find_workspace_for_path;

pub fn run(start_dir: &Path) -> Result<()> {
    // First check for workspace
    if let Ok(Some(workspace)) = find_workspace_for_path(start_dir) {
        return run_workspace_status(&workspace);
    }

    // Fall back to legacy .hive.yaml
    let config_path = config::find_config(start_dir)?;
    let config = config::load_config(&config_path)?;

    let project_dir = config::project_dir(&config_path);
    let socket_path = project_dir.join(".hive").join("hive.sock");
    let status = if socket_path.exists() { "RUNNING" } else { "STOPPED" };

    println!("Session: {}", config.session);
    println!("Backend: {:?}", config.workers.backend);
    println!("Task Source: {:?}", config.tasks.source);
    println!("Status: {}", status);

    if let TaskSource::Yaml = config.tasks.source {
        let tasks_path = config::tasks_file_path(&config_path, &config);
        let tasks = load_tasks(&tasks_path).unwrap_or_default();

        println!("\nWORKER              LANE            BACKLOG     IN_PROGRESS");
        println!("------              ----            -------     -----------");

        for window in &config.windows {
            for worker in &window.workers {
                let lane = worker.lane.clone().unwrap_or_else(|| worker.id.clone());
                let counts = counts_for_lane(&tasks, &lane);
                println!(
                    "{:<18} {:<14} {:<10} {:<11}",
                    worker.id, lane, counts.backlog, counts.in_progress
                );
            }
        }
    } else {
        println!("\nGitHub task source status not implemented yet.");
    }

    Ok(())
}

fn run_workspace_status(workspace: &crate::workspace::resolve::WorkspaceMeta) -> Result<()> {
    let socket_path = workspace.dir.join("hive.sock");
    let status = if socket_path.exists() { "RUNNING" } else { "STOPPED" };

    println!("Workspace: {}", workspace.name);
    println!("Backend: {:?}", workspace.config.workers.backend);
    println!("Status: {}", status);
    println!("Projects: {}", workspace.config.projects.len());
    println!("Total Workers: {}", workspace.config.total_workers());

    println!("\nPROJECT                         WORKERS   LANES");
    println!("-------                         -------   -----");

    for project in &workspace.config.projects {
        let name = project.path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let lanes = project.lanes.join(", ");
        println!("{:<30} {:<8} {}", name, project.workers, lanes);
    }

    Ok(())
}
