use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;

use portable_pty::{Child, MasterPty};

use crate::app::types::PaneType;
use crate::config::BranchConfig;

use super::output::OutputBuffer;

pub struct Pane {
    pub id: String,
    pub pane_type: PaneType,
    pub master: Box<dyn MasterPty + Send>,
    pub child: Box<dyn Child + Send>,
    pub writer: Box<dyn Write + Send>,
    pub output_buffer: OutputBuffer,
    pub raw_history: VecDeque<u8>,
    pub raw_history_max: usize,
    pub lane: Option<String>,
    pub working_dir: PathBuf,
    pub branch: Option<BranchConfig>,
}

impl Pane {
    pub fn push_history(&mut self, data: &[u8]) {
        for byte in data {
            self.raw_history.push_back(*byte);
        }
        while self.raw_history.len() > self.raw_history_max {
            self.raw_history.pop_front();
        }
    }
}
