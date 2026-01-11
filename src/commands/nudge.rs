use std::path::Path;

use anyhow::Result;

use crate::app::build_nudge_message;
use crate::config::{self, TaskSource};
use crate::ipc::ClientMessage;
use crate::tasks::{counts_for_lane, load_tasks};

pub fn run(start_dir: &Path, specific_worker: Option<&str>) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    let config = config::load_config(&config_path)?;

    let project_dir = config::project_dir(&config_path);
    let socket_path = project_dir.join(".hive").join("hive.sock");

    if socket_path.exists() {
        let mut stream = std::os::unix::net::UnixStream::connect(&socket_path)?;
        let line = serde_json::to_string(&ClientMessage::Nudge {
            worker: specific_worker.map(|s| s.to_string()),
        })?;
        use std::io::Write;
        writeln!(stream, "{}", line)?;
        println!("Nudge sent to running session.");
        return Ok(());
    }

    if let TaskSource::Yaml = config.tasks.source {
        let tasks_path = config::tasks_file_path(&config_path, &config);
        let tasks = load_tasks(&tasks_path).unwrap_or_default();

        for window in &config.windows {
            for worker in &window.workers {
                if let Some(target) = specific_worker {
                    if worker.id != target {
                        continue;
                    }
                }

                let lane = worker.lane.clone().unwrap_or_else(|| worker.id.clone());
                let counts = counts_for_lane(&tasks, &lane);

                if counts.backlog > 0 && counts.in_progress == 0 {
                    let message = build_nudge_message(&config, &lane, counts.backlog, &worker.branch);
                    println!("[{}] {}", worker.id, message);
                }
            }
        }
    } else {
        println!("GitHub task source nudging not implemented yet.");
    }

    Ok(())
}
