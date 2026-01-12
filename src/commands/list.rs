use anyhow::Result;

use crate::workspace::resolve::list_workspaces;

pub fn run() -> Result<()> {
    let workspaces = list_workspaces()?;

    if workspaces.is_empty() {
        println!("No workspaces found.");
        println!();
        println!("Create one by running 'hive' in a directory with projects.");
        return Ok(());
    }

    println!("Workspaces:");
    println!();

    for ws in workspaces {
        let status = if ws.is_running() {
            " [running]"
        } else {
            ""
        };

        let project_count = ws.config.projects.len();
        let total_workers = ws.config.total_workers();

        println!(
            "  {} - {} project(s), {} worker(s){}",
            ws.name, project_count, total_workers, status
        );
        println!("    {}", ws.dir.display());
    }

    Ok(())
}
