use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config;
use crate::ipc::ClientMessage;
use crate::workspace::resolve::find_workspace_for_path;

pub fn run(start_dir: &Path) -> Result<()> {
    // First check for workspace
    let socket_path = if let Ok(Some(workspace)) = find_workspace_for_path(start_dir) {
        workspace.dir.join("hive.sock")
    } else {
        // Fall back to legacy .hive.yaml
        let config_path = config::find_config(start_dir)?;
        let project_dir = config::project_dir(&config_path);
        project_dir.join(".hive").join("hive.sock")
    };

    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("Failed to connect to {}", socket_path.display()))?;

    let line = serde_json::to_string(&ClientMessage::Detach)?;
    writeln!(stream, "{}", line)?;

    println!("Detached from hive session");
    Ok(())
}
