use anyhow::{Context, Result};

use crate::workspace::resolve::find_workspace_by_name;

pub fn run(name: &str, daemon: bool) -> Result<()> {
    let workspace =
        find_workspace_by_name(name)?.with_context(|| format!("Workspace '{}' not found", name))?;

    println!("Opening workspace: {}", workspace.name);
    println!("Location: {}", workspace.dir.display());

    // Start the workspace server
    super::up::run_workspace(&workspace.dir, daemon)
}
