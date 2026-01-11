pub mod output;
pub mod pane;

use std::io::Read;
use std::path::Path;
use std::thread;

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

use crate::config::Backend;

pub use crate::app::types::PaneType;
pub use pane::Pane;

#[derive(Debug)]
pub enum PaneEvent {
    Output { pane_id: String, data: Vec<u8> },
    Exited { pane_id: String },
    Error { pane_id: String, error: String },
}

pub fn spawn_agent(
    backend: Backend,
    message: &str,
    working_dir: &Path,
) -> Result<(
    Box<dyn portable_pty::MasterPty + Send>,
    Box<dyn portable_pty::Child + Send>,
    Box<dyn std::io::Write + Send>,
)> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("Failed to open PTY")?;

    let cmd = match backend {
        Backend::Claude => {
            let mut cmd = CommandBuilder::new("claude");
            cmd.arg(message);
            cmd.cwd(working_dir);
            cmd
        }
        Backend::Codex => {
            let mut cmd = CommandBuilder::new("env");
            cmd.args([
                "-u",
                "CODEX_SANDBOX",
                "-u",
                "CODEX_SANDBOX_NETWORK_DISABLED",
                "codex",
                "--sandbox",
                "danger-full-access",
                "--ask-for-approval",
                "never",
                message,
            ]);
            cmd.cwd(working_dir);
            cmd
        }
    };

    let child = pair
        .slave
        .spawn_command(cmd)
        .context("Failed to spawn agent command")?;

    let writer = pair
        .master
        .take_writer()
        .context("Failed to take PTY writer")?;
    Ok((pair.master, child, writer))
}

pub fn spawn_reader_thread(
    pane_id: String,
    mut reader: Box<dyn Read + Send>,
    tx: std::sync::mpsc::Sender<PaneEvent>,
) {
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if tx
                        .send(PaneEvent::Output {
                            pane_id: pane_id.clone(),
                            data: buf[..n].to_vec(),
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                Err(err) => {
                    let _ = tx.send(PaneEvent::Error {
                        pane_id: pane_id.clone(),
                        error: err.to_string(),
                    });
                    break;
                }
            }
        }
        let _ = tx.send(PaneEvent::Exited { pane_id });
    });
}

pub fn send_to_pane(writer: &mut dyn std::io::Write, message: &str) -> Result<()> {
    writeln!(writer, "{}", message).context("Failed writing to PTY")?;
    Ok(())
}

pub fn send_bytes(writer: &mut dyn std::io::Write, bytes: &[u8]) -> Result<()> {
    writer.write_all(bytes).context("Failed writing to PTY")?;
    writer.flush().ok();
    Ok(())
}
