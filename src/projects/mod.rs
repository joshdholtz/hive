use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectsFile {
    pub projects: Vec<ProjectEntry>,
}

pub fn load_projects() -> Result<ProjectsFile> {
    let path = projects_path()?;
    if !path.exists() {
        return Ok(ProjectsFile::default());
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("Failed reading {}", path.display()))?;
    let projects = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed parsing {}", path.display()))?;
    Ok(projects)
}

pub fn save_projects(projects: &ProjectsFile) -> Result<()> {
    let path = projects_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_yaml::to_string(projects)?;
    fs::write(&path, content).with_context(|| format!("Failed writing {}", path.display()))?;
    Ok(())
}

pub fn add_project(path: &Path, name: Option<String>) -> Result<ProjectsFile> {
    let canonical = canonicalize_path(path)?;
    let name = name.unwrap_or_else(|| project_name(&canonical));
    let path_str = canonical.to_string_lossy().to_string();

    let mut projects = load_projects()?;
    if let Some(existing) = projects.projects.iter_mut().find(|p| p.path == path_str) {
        existing.name = name;
    } else {
        projects.projects.push(ProjectEntry {
            name,
            path: path_str,
        });
    }
    projects
        .projects
        .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    save_projects(&projects)?;
    Ok(projects)
}

pub fn remove_project(path: &Path) -> Result<ProjectsFile> {
    let canonical = canonicalize_path(path)?;
    let path_str = canonical.to_string_lossy().to_string();
    let mut projects = load_projects()?;
    projects.projects.retain(|p| p.path != path_str);
    save_projects(&projects)?;
    Ok(projects)
}

pub fn remove_project_by_path(path: &str) -> Result<ProjectsFile> {
    let mut projects = load_projects()?;
    projects.projects.retain(|p| p.path != path);
    save_projects(&projects)?;
    Ok(projects)
}

pub fn hive_home() -> Result<PathBuf> {
    if let Ok(custom) = std::env::var("HIVE_HOME") {
        return Ok(PathBuf::from(custom));
    }
    let home = std::env::var("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".hive"))
}

pub fn projects_path() -> Result<PathBuf> {
    Ok(hive_home()?.join("projects.yaml"))
}

fn canonicalize_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        path.canonicalize()
            .with_context(|| format!("Failed to canonicalize {}", path.display()))
    } else {
        Ok(path.to_path_buf())
    }
}

fn project_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project")
        .to_string()
}
