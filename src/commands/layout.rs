use std::io::Write;
use std::path::Path;

use anyhow::Result;

use crate::config;
use crate::ipc::ClientMessage;

pub fn run(start_dir: &Path, mode: &str) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    let project_dir = config::project_dir(&config_path);
    let layout_path = project_dir.join(".hive").join("layout-mode");

    let mode = match mode {
        "default" => "default",
        "custom" => "custom",
        _ => anyhow::bail!("Unknown layout mode '{}'. Use default or custom.", mode),
    };

    std::fs::create_dir_all(project_dir.join(".hive"))?;
    std::fs::write(&layout_path, mode)?;

    let socket_path = project_dir.join(".hive").join("hive.sock");
    if socket_path.exists() {
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(&socket_path) {
            let layout = if mode == "custom" {
                crate::app::state::LayoutMode::Custom
            } else {
                crate::app::state::LayoutMode::Default
            };
            if let Ok(line) = serde_json::to_string(&ClientMessage::Layout { mode: layout }) {
                let _ = writeln!(stream, "{}", line);
            }
        }
    }

    println!("Layout mode set to {}", mode);
    Ok(())
}
