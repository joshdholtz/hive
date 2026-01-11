use std::path::Path;

use anyhow::Result;

use crate::config;
use crate::server;

pub fn run(start_dir: &Path) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    server::run(&config_path)
}
