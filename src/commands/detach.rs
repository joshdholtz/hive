use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config;
use crate::ipc::ClientMessage;

pub fn run(start_dir: &Path) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    let project_dir = config::project_dir(&config_path);
    let socket_path = project_dir.join(".hive").join("hive.sock");

    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("Failed to connect to {}", socket_path.display()))?;

    let line = serde_json::to_string(&ClientMessage::Detach)?;
    writeln!(stream, "{}", line)?;

    Ok(())
}
