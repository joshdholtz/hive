use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use notify::{RecursiveMode, Watcher};

#[derive(Debug, Clone)]
pub enum NudgeRequest {
    All,
}

pub fn spawn_yaml_watcher(
    tasks_file: PathBuf,
    nudge_tx: Sender<NudgeRequest>,
    debounce: Duration,
    settle: Duration,
) -> Result<()> {
    thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        let mut watcher = match notify::recommended_watcher(tx) {
            Ok(watcher) => watcher,
            Err(_) => return,
        };

        if watcher.watch(&tasks_file, RecursiveMode::NonRecursive).is_err() {
            return;
        }

        let mut last_nudge = Instant::now().checked_sub(debounce * 2).unwrap_or_else(Instant::now);

        loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(_event) => {
                    if last_nudge.elapsed() >= debounce {
                        thread::sleep(settle);
                        last_nudge = Instant::now();
                        if nudge_tx.send(NudgeRequest::All).is_err() {
                            break;
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    Ok(())
}
