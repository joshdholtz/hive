use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use notify::{RecursiveMode, Watcher};

use super::yaml::load_tasks;

#[derive(Debug, Clone)]
pub enum NudgeRequest {
    All,
}

fn log_line(path: &Path, line: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{}", line);
    }
}

pub fn spawn_yaml_watcher(
    tasks_file: PathBuf,
    nudge_tx: Sender<NudgeRequest>,
    debounce: Duration,
    settle: Duration,
    log_path: PathBuf,
) -> Result<()> {
    thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(watcher) => watcher,
            Err(e) => {
                log_line(&log_path, &format!("watcher: failed to create: {}", e));
                return;
            }
        };

        if let Err(e) = watcher.watch(&tasks_file, RecursiveMode::NonRecursive) {
            log_line(
                &log_path,
                &format!("watcher: failed to watch {}: {}", tasks_file.display(), e),
            );
            return;
        }

        log_line(
            &log_path,
            &format!("watcher: watching {}", tasks_file.display()),
        );

        let mut last_nudge = Instant::now()
            .checked_sub(debounce * 2)
            .unwrap_or_else(Instant::now);

        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(event) => {
                    log_line(&log_path, &format!("watcher: file event {:?}", event));

                    if last_nudge.elapsed() >= debounce {
                        thread::sleep(settle);

                        // Validate YAML before triggering nudge
                        match load_tasks(&tasks_file) {
                            Ok(_) => {
                                log_line(&log_path, "watcher: yaml valid, sending nudge");
                                last_nudge = Instant::now();
                                if nudge_tx.send(NudgeRequest::All).is_err() {
                                    log_line(&log_path, "watcher: nudge channel closed");
                                    break;
                                }
                            }
                            Err(e) => {
                                log_line(&log_path, &format!("watcher: yaml invalid: {}", e));
                            }
                        }
                    } else {
                        log_line(&log_path, "watcher: debounce, skipping");
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    log_line(&log_path, "watcher: channel disconnected");
                    break;
                }
            }
        }
    });

    Ok(())
}
