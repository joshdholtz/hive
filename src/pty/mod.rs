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
    skip_permissions: bool,
) -> Result<(
    Box<dyn portable_pty::MasterPty + Send>,
    Box<dyn portable_pty::Child + Send>,
    Box<dyn std::io::Write + Send>,
)> {
    let pty_system = native_pty_system();
    // Codex caches terminal dimensions, so start with a larger size
    // to avoid TUI rendering issues when panes are small
    let (rows, cols) = match backend {
        Backend::Codex => (40, 120),
        Backend::Claude => (24, 80),
    };
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("Failed to open PTY")?;

    let cmd = match backend {
        Backend::Claude => {
            let mut cmd = CommandBuilder::new("claude");
            if skip_permissions {
                cmd.arg("--dangerously-skip-permissions");
            }
            cmd.arg(message);
            cmd.cwd(working_dir);
            // Set terminal type and locale for proper unicode rendering
            cmd.env("TERM", "xterm-256color");
            cmd.env("LANG", "en_US.UTF-8");
            cmd.env("LC_ALL", "en_US.UTF-8");
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
            // Set terminal type and locale for proper rendering
            cmd.env("TERM", "xterm-256color");
            cmd.env("LANG", "en_US.UTF-8");
            cmd.env("LC_ALL", "en_US.UTF-8");
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

/// Check if data contains a cursor position query (ESC[6n or ESC[?6n)
pub fn contains_cursor_query(data: &[u8]) -> bool {
    // Look for ESC[6n pattern (cursor position query / DSR)
    // ESC = 0x1b, [ = 0x5b, 6 = 0x36, n = 0x6e
    for window in data.windows(4) {
        if window[0] == 0x1b && window[1] == b'[' && window[2] == b'6' && window[3] == b'n' {
            return true;
        }
    }
    // Also check for ESC[?6n variant
    for window in data.windows(5) {
        if window[0] == 0x1b
            && window[1] == b'['
            && window[2] == b'?'
            && window[3] == b'6'
            && window[4] == b'n'
        {
            return true;
        }
    }
    false
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
