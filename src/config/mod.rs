pub mod parser;
pub mod validation;

use std::path::{Path, PathBuf};

use anyhow::Result;

pub use parser::{
    load_config, find_config, ArchitectConfig, Backend, BranchConfig, HiveConfig, MessagesConfig,
    TaskSource, TasksConfig, WindowConfig, WorkerConfig, WorkersConfig,
};

pub fn project_dir(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn tasks_file_path(config_path: &Path, config: &parser::HiveConfig) -> PathBuf {
    let base = project_dir(config_path);
    let tasks_file = config
        .tasks
        .file
        .as_deref()
        .unwrap_or(".hive/tasks.yaml");
    base.join(tasks_file)
}

pub fn validate(config: &parser::HiveConfig) -> Result<()> {
    validation::validate_config(config)
}
