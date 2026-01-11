use std::collections::HashSet;

use anyhow::{bail, Result};

use super::parser::HiveConfig;

pub fn validate_config(config: &HiveConfig) -> Result<()> {
    if config.session.trim().is_empty() {
        bail!("session must not be empty");
    }

    let mut ids = HashSet::new();
    for window in &config.windows {
        if window.workers.is_empty() {
            bail!("window '{}' has no workers", window.name);
        }
        for worker in &window.workers {
            if worker.id.trim().is_empty() {
                bail!("worker id must not be empty");
            }
            if !ids.insert(worker.id.clone()) {
                bail!("duplicate worker id '{}'", worker.id);
            }
        }
    }

    Ok(())
}
