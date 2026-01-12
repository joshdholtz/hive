use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HiveConfig {
    pub architect: ArchitectConfig,
    pub workers: WorkersConfig,
    pub session: String,
    pub tasks: TasksConfig,
    pub windows: Vec<WindowConfig>,
    pub setup: Option<Vec<String>>,
    pub messages: Option<MessagesConfig>,
    pub worker_instructions: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ArchitectConfig {
    pub backend: Backend,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WorkersConfig {
    pub backend: Backend,
    /// Skip all permission prompts (Claude: --dangerously-skip-permissions)
    #[serde(default)]
    pub skip_permissions: bool,
    /// Setup commands to run in each worker's directory before starting
    #[serde(default)]
    pub setup: Vec<String>,
    /// Files to symlink from main repo to worktrees (e.g., .env)
    #[serde(default)]
    pub symlink: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Claude,
    Codex,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TasksConfig {
    pub source: TaskSource,
    pub file: Option<String>,
    pub github_org: Option<String>,
    pub github_project: Option<u32>,
    pub github_project_id: Option<String>,
    pub github_status_field_id: Option<String>,
    pub github_lane_field_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum TaskSource {
    Yaml,
    Github,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WindowConfig {
    pub name: String,
    pub layout: Option<String>,
    pub workers: Vec<WorkerConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct WorkerConfig {
    pub id: String,
    pub dir: Option<String>,
    pub lane: Option<String>,
    pub branch: Option<BranchConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BranchConfig {
    pub local: String,
    pub remote: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MessagesConfig {
    pub startup: Option<String>,
    pub nudge: Option<String>,
}

pub fn load_config(path: &Path) -> Result<HiveConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed reading config at {}", path.display()))?;
    let config: HiveConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed parsing YAML at {}", path.display()))?;
    Ok(config)
}

pub fn find_config(start_dir: &Path) -> Result<PathBuf> {
    let mut current = start_dir.to_path_buf();
    loop {
        let candidate = current.join(".hive.yaml");
        if candidate.exists() {
            return Ok(candidate);
        }
        if !current.pop() {
            anyhow::bail!("No .hive.yaml found in current or parent directories");
        }
    }
}
