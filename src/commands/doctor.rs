use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{self, TaskSource};
use crate::tasks::yaml::{LaneTasks, TasksFile, WorkerProtocol};
use crate::utils::{git, shell};
use crate::workspace::{find_workspace_for_path, WorkspaceConfig};

pub fn run(start_dir: &Path) -> Result<()> {
    // First try to find a workspace config for this path
    if let Ok(Some(workspace_meta)) = find_workspace_for_path(start_dir) {
        return run_workspace(&workspace_meta.dir);
    }

    // Fall back to single-project config
    let config_path = config::find_config(start_dir)?;
    let config = config::load_config(&config_path)?;
    let project_dir = config::project_dir(&config_path);

    let mut issues = Vec::new();
    let mut fixes = Vec::new();

    if let TaskSource::Yaml = config.tasks.source {
        let tasks_path = config::tasks_file_path(&config_path, &config);
        if !tasks_path.exists() {
            issues.push(format!("Missing tasks file: {}", tasks_path.display()));
            if let Err(err) = create_tasks_file(&config, &tasks_path) {
                fixes.push(format!("Failed to create tasks file: {}", err));
            } else {
                fixes.push(format!("Created tasks file: {}", tasks_path.display()));
            }
        }
    }

    let architect = project_dir.join(".hive").join("ARCHITECT.md");
    let mut missing_role = false;
    let mut outdated_role = false;

    if !architect.exists() {
        issues.push("Missing .hive/ARCHITECT.md".to_string());
        missing_role = true;
    }

    for window in &config.windows {
        for worker in &window.workers {
            let worker_file = project_dir
                .join(".hive")
                .join("workers")
                .join(&worker.id)
                .join("WORKER.md");
            if !worker_file.exists() {
                issues.push(format!(
                    "Missing worker role file: {}",
                    worker_file.display()
                ));
                missing_role = true;
            } else if !outdated_role {
                // Check if role file matches current workflow config
                if let Ok(content) = std::fs::read_to_string(&worker_file) {
                    let has_required_pr = content.contains("(REQUIRED)");
                    let has_when_requested = content.contains("(When Requested)");

                    if config.workflow.auto_create_pr && has_when_requested {
                        issues.push("Worker role files are outdated (workflow.auto_create_pr changed)".to_string());
                        outdated_role = true;
                    } else if !config.workflow.auto_create_pr && has_required_pr {
                        issues.push("Worker role files are outdated (workflow.auto_create_pr changed)".to_string());
                        outdated_role = true;
                    }
                }
            }
        }
    }

    if missing_role || outdated_role {
        if let Err(err) = crate::commands::role::run(&project_dir, None) {
            fixes.push(format!("Failed to regenerate role files: {}", err));
        } else {
            if missing_role {
                fixes.push("Created missing role files".to_string());
            }
            if outdated_role {
                fixes.push("Updated role files with current workflow config".to_string());
            }
        }
    }

    if let Err(err) = git::ensure_git_exclude(&project_dir) {
        issues.push(format!("Failed to update git exclude: {}", err));
    } else {
        fixes.push("Ensured .hive/ is in git exclude".to_string());
    }

    let backend_cmd = match config.workers.backend {
        crate::config::Backend::Claude => "claude",
        crate::config::Backend::Codex => "codex",
    };
    if !shell::command_available(backend_cmd) {
        issues.push(format!("Missing required backend command: {}", backend_cmd));
    }

    if issues.is_empty() {
        println!("Hive doctor: no issues found");
    } else {
        println!("Hive doctor found {} issue(s):", issues.len());
        for issue in issues {
            println!("- {}", issue);
        }
    }

    if !fixes.is_empty() {
        println!("\nFixes:");
        for fix in fixes {
            println!("- {}", fix);
        }
    }

    Ok(())
}

fn run_workspace(workspace_dir: &Path) -> Result<()> {
    let config = WorkspaceConfig::load(workspace_dir)?;

    let mut issues = Vec::new();
    let mut fixes = Vec::new();

    // Check tasks file
    let tasks_path = workspace_dir.join("tasks.yaml");
    if !tasks_path.exists() {
        issues.push(format!("Missing tasks file: {}", tasks_path.display()));
    }

    // Check architect role
    let architect = workspace_dir.join("ARCHITECT.md");
    let mut missing_role = false;
    let mut outdated_role = false;

    if !architect.exists() {
        issues.push("Missing ARCHITECT.md".to_string());
        missing_role = true;
    }

    // Check worker role files in lanes directory
    let lanes_dir = workspace_dir.join("lanes");
    for project in &config.projects {
        for lane in &project.lanes {
            let worker_file = lanes_dir.join(lane).join("WORKER.md");
            if !worker_file.exists() {
                issues.push(format!("Missing lane role file: {}", worker_file.display()));
                missing_role = true;
            } else if !outdated_role {
                // Check if role file matches current workflow config
                if let Ok(content) = std::fs::read_to_string(&worker_file) {
                    let has_required_pr = content.contains("(REQUIRED)");
                    let has_when_requested = content.contains("(When Requested)");

                    if config.workflow.auto_create_pr && has_when_requested {
                        issues.push("Worker role files are outdated (workflow.auto_create_pr changed)".to_string());
                        outdated_role = true;
                    } else if !config.workflow.auto_create_pr && has_required_pr {
                        issues.push("Worker role files are outdated (workflow.auto_create_pr changed)".to_string());
                        outdated_role = true;
                    }
                }
            }
        }
    }

    if missing_role || outdated_role {
        // Regenerate role files using setup's write functions
        if let Err(err) = regenerate_workspace_roles(workspace_dir, &config) {
            fixes.push(format!("Failed to regenerate role files: {}", err));
        } else {
            if missing_role {
                fixes.push("Created missing role files".to_string());
            }
            if outdated_role {
                fixes.push("Updated role files with current workflow config".to_string());
            }
        }
    }

    // Check backend availability
    let backend_cmd = match config.workers.backend {
        crate::config::Backend::Claude => "claude",
        crate::config::Backend::Codex => "codex",
    };
    if !shell::command_available(backend_cmd) {
        issues.push(format!("Missing required backend command: {}", backend_cmd));
    }

    if issues.is_empty() {
        println!("Hive doctor: no issues found");
    } else {
        println!("Hive doctor found {} issue(s):", issues.len());
        for issue in &issues {
            println!("- {}", issue);
        }
    }

    if !fixes.is_empty() {
        println!("\nFixes:");
        for fix in fixes {
            println!("- {}", fix);
        }
    }

    Ok(())
}

fn regenerate_workspace_roles(workspace_dir: &Path, config: &WorkspaceConfig) -> Result<()> {
    use crate::workspace::slug_from_path;

    // Generate ARCHITECT.md
    let mut content = String::new();
    content.push_str("# Architect Role\n\n");
    content.push_str(
        "You are the architect for this workspace. You plan tasks but do NOT write code.\n\n",
    );
    content.push_str("## Projects in this workspace\n\n");

    for project in &config.projects {
        let project_name = project
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");
        content.push_str(&format!(
            "- **{}** - {}\n",
            project_name,
            project.path.display()
        ));
        content.push_str(&format!("  Lanes: {}\n", project.lanes.join(", ")));
    }

    content.push_str("\n## Task Management\n\n");
    content.push_str(&format!(
        "Tasks are stored in: {}/tasks.yaml\n\n",
        workspace_dir.display()
    ));
    content.push_str(
        "Add tasks to the appropriate lane's backlog. Workers will claim and complete them.\n\n",
    );

    content.push_str("### Task Format\n\n");
    content.push_str("```yaml\n<lane-name>:\n  backlog:\n    - id: my-task-id\n      title: Short title for the task\n      description: |\n        Detailed description of what needs to be done.\n      priority: high\n```\n\n");

    content.push_str("### YAML Validation (CRITICAL)\n\n");
    content.push_str("When editing tasks.yaml, you MUST ensure valid YAML:\n");
    content.push_str(
        "- Empty lists MUST use `[]`, never leave blank (e.g., `backlog: []` not `backlog:`)\n",
    );
    content.push_str(&format!(
        "- After editing, validate with: `yq eval '.' {}/tasks.yaml > /dev/null && echo 'Valid' || echo 'Invalid'`\n\n",
        workspace_dir.display()
    ));

    // PR creation guidance for architect
    if !config.workflow.auto_create_pr {
        content.push_str("## Pull Request Guidance\n\n");
        content.push_str("Workers do NOT automatically create PRs after completing tasks.\n");
        content.push_str("If a task requires a PR, **explicitly state it** in the task description:\n\n");
        content.push_str("```yaml\ndescription: |\n  Implement feature X.\n  \n  **Create a PR when complete.**\n```\n\n");
        content.push_str("Only request PRs when the changes should be reviewed or merged to main.\n");
    }

    std::fs::write(workspace_dir.join("ARCHITECT.md"), content)?;

    // Generate worker role files
    let lanes_dir = workspace_dir.join("lanes");
    std::fs::create_dir_all(&lanes_dir)?;

    for project in &config.projects {
        let project_slug = slug_from_path(&project.path);
        let project_name = project
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project");

        for lane in &project.lanes {
            let lane_dir = lanes_dir.join(lane);
            std::fs::create_dir_all(&lane_dir)?;

            let local_prefix = format!("{}-{}/{}", project_slug, lane, lane);
            let remote_prefix = lane.clone();

            let mut content = String::new();
            content.push_str(&format!("# Worker Role: Lane {}\n\n", lane));
            content.push_str(&format!(
                "You are a worker assigned to the **{}** lane.\n\n",
                lane
            ));

            content.push_str("## Your Project\n\n");
            content.push_str(&format!(
                "- {} ({})\n\n",
                project_name,
                project.path.display()
            ));

            content.push_str("## Branch Naming Convention\n\n");
            content.push_str(&format!(
                "- Create local branches with prefix: `{}/`\n",
                local_prefix
            ));
            content.push_str(&format!("- Example: `{}/my-feature`\n", local_prefix));
            content.push_str(&format!(
                "- Push command: `git push origin {}/my-feature:{}/my-feature`\n\n",
                local_prefix, remote_prefix
            ));

            content.push_str("## Task Management\n\n");
            content.push_str(&format!(
                "Tasks file: {}/tasks.yaml\n",
                workspace_dir.display()
            ));
            content.push_str(&format!("Your lane: `{}`\n\n", lane));

            content.push_str("## Workflow\n\n");
            content.push_str("1. Check your lane's backlog for tasks\n");
            content.push_str("2. Claim ONE task by moving it to `in_progress`\n");
            content.push_str("3. Create a branch following the naming convention above\n");
            content.push_str("4. Complete the task\n");
            if config.workflow.auto_create_pr {
                content.push_str("5. Create a PR with your changes\n");
            } else {
                content.push_str("5. Only create a PR if the task or architect requests it\n");
            }
            content.push_str("6. Move task to `done`, then claim the next task\n\n");

            // Uncommitted changes handling
            match config.workflow.uncommitted_changes.as_str() {
                "commit" => {
                    content.push_str("## Before Starting New Work\n\n");
                    content.push_str("If you have uncommitted changes from a previous task, commit them first.\n\n");
                }
                "error" => {
                    content.push_str("## Before Starting New Work\n\n");
                    content.push_str("If you have uncommitted changes from a previous task, STOP and ask the architect for guidance.\n\n");
                }
                _ => {
                    content.push_str("## Before Starting New Work\n\n");
                    content.push_str("If you have uncommitted changes from a previous task, stash them (`git stash`) before starting new work.\n\n");
                }
            }

            if config.workflow.auto_create_pr {
                content.push_str("## Creating a Pull Request (REQUIRED)\n\n");
                content.push_str("After completing a task, you MUST follow these steps:\n");
                content.push_str(&format!(
                    "1. Create a branch: `git checkout -b {}/task-name`\n",
                    local_prefix
                ));
                content.push_str("2. Stage changes: `git add -A`\n");
                content.push_str("3. Commit: `git commit -m \"description of changes\"`\n");
                content.push_str(&format!(
                    "4. Push: `git push origin {}/task-name:{}/task-name`\n",
                    local_prefix, remote_prefix
                ));
                content.push_str("5. Create PR: `gh pr create --fill`\n");
                content.push_str("6. **Verify the PR URL is displayed before stopping**\n\n");
            } else {
                content.push_str("## Creating a Pull Request (When Requested)\n\n");
                content.push_str("If the task or architect requests a PR, follow these steps:\n");
                content.push_str(&format!(
                    "1. Create a branch: `git checkout -b {}/task-name`\n",
                    local_prefix
                ));
                content.push_str("2. Stage changes: `git add -A`\n");
                content.push_str("3. Commit: `git commit -m \"description of changes\"`\n");
                content.push_str(&format!(
                    "4. Push: `git push origin {}/task-name:{}/task-name`\n",
                    local_prefix, remote_prefix
                ));
                content.push_str("5. Create PR: `gh pr create --fill`\n\n");
                content.push_str("## Completing a Task Without PR\n\n");
                content.push_str("If no PR is requested, simply:\n");
                content.push_str("1. Commit your changes to the current branch\n");
                content.push_str("2. Move the task to `done` in tasks.yaml\n\n");
            }

            content.push_str("## When Backlog is Empty\n\n");
            content.push_str("If your lane's backlog is empty, **STOP IMMEDIATELY**.\n");
            content.push_str(&format!(
                "- Report \"No tasks in backlog for lane {}\"\n",
                lane
            ));
            content.push_str("- Do NOT look for other work\n");
            content.push_str("- Do NOT explore the codebase\n");
            content.push_str("- Simply wait for the architect to add tasks\n\n");

            content.push_str("## YAML Validation (CRITICAL)\n\n");
            content.push_str("When editing tasks.yaml, you MUST ensure valid YAML:\n");
            content.push_str("- Empty lists MUST use `[]`, never leave blank (e.g., `backlog: []` not `backlog:`)\n");
            content.push_str(&format!(
                "- After editing, validate with: `yq eval '.' {}/tasks.yaml > /dev/null && echo 'Valid' || echo 'Invalid'`\n",
                workspace_dir.display()
            ));
            content.push_str("- If validation fails, fix the YAML before proceeding\n");

            std::fs::write(lane_dir.join("WORKER.md"), content)?;
        }
    }

    Ok(())
}

fn create_tasks_file(config: &crate::config::HiveConfig, path: &Path) -> Result<()> {
    use crate::tasks::yaml::ProjectEntry;

    let mut tasks = TasksFile::default();
    tasks.worker_protocol = Some(WorkerProtocol {
        claim: Some("Move the task to in_progress and add claimed_by/claimed_at".to_string()),
        complete: Some("Move the task to done and add summary/files_changed".to_string()),
    });
    tasks.rules = Some(vec![
        "Claim one task at a time".to_string(),
        "Create a PR before starting a new task".to_string(),
    ]);

    // Legacy HiveConfig: all lanes are direct (no project nesting)
    for window in &config.windows {
        for worker in &window.workers {
            let lane = worker.lane.clone().unwrap_or_else(|| worker.id.clone());
            tasks
                .projects
                .entry(lane)
                .or_insert_with(|| ProjectEntry::Direct(LaneTasks::default()));
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_yaml::to_string(&tasks)?;
    std::fs::write(path, content).with_context(|| format!("Failed writing {}", path.display()))?;
    Ok(())
}
