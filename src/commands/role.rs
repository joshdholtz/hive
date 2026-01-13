use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::config::{self, TaskSource};

pub fn run(start_dir: &Path, specific_worker: Option<&str>) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    let config = config::load_config(&config_path)?;
    let project_dir = config::project_dir(&config_path);

    let tasks_source = config.tasks.source.clone();
    let tasks_file = config::tasks_file_path(&config_path, &config);

    for window in &config.windows {
        for worker in &window.workers {
            if let Some(target) = specific_worker {
                if worker.id != target {
                    continue;
                }
            }

            let lane = worker.lane.clone().unwrap_or_else(|| worker.id.clone());
            let dir = worker.dir.clone().unwrap_or_else(|| ".".to_string());
            let worker_dir = project_dir.join(&dir);
            let role_dir = project_dir.join(".hive").join("workers").join(&worker.id);
            let role_file = role_dir.join("WORKER.md");

            let custom_content = extract_custom_content(&role_file);

            fs::create_dir_all(&role_dir)?;
            let mut content = String::new();

            content.push_str(&format!("# Worker Role: {}\n\n", worker.id));
            content.push_str(&format!(
                "You are a background worker assigned to lane **{}**.\n\n",
                lane
            ));
            content.push_str("## General Behavior\n");
            content.push_str("1. Check your task backlog and claim ONE task at a time\n");
            content.push_str("2. Implement the task completely\n");
            if config.workflow.auto_create_pr {
                content.push_str("3. **CRITICAL: You MUST create a Pull Request before stopping or claiming another task**\n");
                content.push_str("4. Do NOT stop working until you see a PR URL displayed\n\n");
            } else {
                content.push_str("3. Only create a PR if the task description or architect specifically requests it\n");
                content.push_str("4. If no PR is needed, commit your changes and move the task to done\n\n");
            }

            // Uncommitted changes handling
            match config.workflow.uncommitted_changes.as_str() {
                "commit" => {
                    content.push_str("## Before Starting New Work\n");
                    content.push_str("If you have uncommitted changes from a previous task, commit them first.\n\n");
                }
                "error" => {
                    content.push_str("## Before Starting New Work\n");
                    content.push_str("If you have uncommitted changes from a previous task, STOP and ask the architect for guidance.\n\n");
                }
                _ => {
                    // "stash" is default - don't add explicit instruction, just handle it
                    content.push_str("## Before Starting New Work\n");
                    content.push_str("If you have uncommitted changes from a previous task, stash them (`git stash`) before starting new work.\n\n");
                }
            }
            content.push_str("## When Backlog is Empty\n");
            content.push_str("If your lane's backlog is empty, **STOP IMMEDIATELY**.\n");
            content.push_str(&format!(
                "- Report \"No tasks in backlog for lane {}\"\n",
                lane
            ));
            content.push_str("- Do NOT look for other work\n");
            content.push_str("- Do NOT explore the codebase\n");
            content.push_str("- Do NOT make suggestions\n");
            content.push_str("- Simply wait for the architect to add tasks\n\n");
            if config.workflow.auto_create_pr {
                content.push_str("## Creating a Pull Request (REQUIRED)\n");
                content.push_str("After completing a task, you MUST follow these steps:\n");
                content.push_str("1. Create a branch: `git checkout -b <branch-name>`\n");
                content.push_str("2. Stage changes: `git add -A`\n");
                content.push_str("3. Commit: `git commit -m \"description of changes\"`\n");
                content.push_str("4. Push: `git push -u origin <branch-name>`\n");
                content.push_str("5. Create PR: `gh pr create --fill` or `gh pr create --title \"...\" --body \"...\"`\n");
                content.push_str("6. **Verify the PR URL is displayed before stopping**\n\n");
            } else {
                content.push_str("## Creating a Pull Request (When Requested)\n");
                content.push_str("If the task or architect requests a PR, follow these steps:\n");
                content.push_str("1. Create a branch: `git checkout -b <branch-name>`\n");
                content.push_str("2. Stage changes: `git add -A`\n");
                content.push_str("3. Commit: `git commit -m \"description of changes\"`\n");
                content.push_str("4. Push: `git push -u origin <branch-name>`\n");
                content.push_str("5. Create PR: `gh pr create --fill` or `gh pr create --title \"...\" --body \"...\"`\n\n");
                content.push_str("## Completing a Task Without PR\n");
                content.push_str("If no PR is requested, simply:\n");
                content.push_str("1. Commit your changes to the current branch\n");
                content.push_str("2. Move the task to `done` in tasks.yaml\n\n");
            }

            if let Some(branch) = &worker.branch {
                content.push_str("## Branch Naming Convention\n");
                content.push_str(&format!(
                    "- Create local branches with prefix: `{}/`\n",
                    branch.local
                ));
                content.push_str(&format!("- Example: `{}/my-feature`\n", branch.local));
                content.push_str(&format!(
                    "- Push command: `git push origin {}/my-feature:{}/my-feature`\n\n",
                    branch.local, branch.remote
                ));
            }

            match tasks_source {
                TaskSource::Github => {
                    if let Some(project) = config.tasks.github_project {
                        content.push_str("## Task Source\n");
                        content.push_str(&format!(
                            "Tasks are managed in GitHub Project #{}.\n",
                            project
                        ));
                        content.push_str("- View your lane's backlog in the project board\n");
                        content.push_str("- Move tasks to \"In Progress\" when you start\n");
                        content.push_str("- Move tasks to \"Done\" when PR is merged\n\n");
                    }
                }
                TaskSource::Yaml => {
                    let rel_tasks = relative_tasks_path(&worker_dir, &tasks_file);
                    content.push_str("## Task Source\n");
                    content.push_str(&format!(
                        "Tasks are managed in `{}` (relative to your working directory).\n",
                        rel_tasks.display()
                    ));
                    content.push_str(&format!("- Your lane: `{}`\n", lane));
                    content.push_str("- Check the `backlog` section for pending tasks\n");
                    content.push_str("- Move tasks to `in_progress` when you start\n");
                    content.push_str("- Move tasks to `done` when complete\n\n");
                    content.push_str("## YAML Validation (CRITICAL)\n");
                    content.push_str("When editing tasks.yaml, you MUST ensure valid YAML:\n");
                    content.push_str("- Empty lists MUST use `[]`, never leave blank (e.g., `backlog: []` not `backlog:`)\n");
                    content.push_str("- After editing, validate with: `yq eval '.' tasks.yaml > /dev/null && echo 'Valid' || echo 'Invalid'`\n");
                    content.push_str("- If validation fails, fix the YAML before proceeding\n\n");
                }
            }

            if let Some(instructions) = &config.worker_instructions {
                if !instructions.trim().is_empty() {
                    content.push_str("## Additional Instructions (from .hive.yaml)\n");
                    content.push_str(instructions.trim());
                    content.push_str("\n\n");
                }
            }

            content.push_str("---\n## Project-Specific Instructions\n");
            content.push_str("<!-- Add your custom instructions below this line -->\n");
            if let Some(custom) = custom_content {
                if !custom.trim().is_empty() {
                    content.push_str(custom.trim());
                }
            }
            content.push_str("\n");

            fs::write(&role_file, content)
                .with_context(|| format!("Failed writing {}", role_file.display()))?;
        }
    }

    generate_architect_role(&config, &project_dir, &tasks_file)?;

    Ok(())
}

fn extract_custom_content(role_file: &Path) -> Option<String> {
    let content = fs::read_to_string(role_file).ok()?;
    let marker = "## Project-Specific Instructions";
    let idx = content.find(marker)?;
    let after = &content[idx + marker.len()..];
    Some(after.lines().skip(1).collect::<Vec<_>>().join("\n"))
}

fn generate_architect_role(
    config: &crate::config::HiveConfig,
    project_dir: &Path,
    tasks_file: &Path,
) -> Result<()> {
    let role_dir = project_dir.join(".hive");
    fs::create_dir_all(&role_dir)?;
    let role_file = role_dir.join("ARCHITECT.md");

    let mut lanes = Vec::new();
    for window in &config.windows {
        for worker in &window.workers {
            let lane = worker.lane.clone().unwrap_or_else(|| worker.id.clone());
            lanes.push(format!("- **{}** → lane: `{}`", worker.id, lane));
        }
    }

    let mut content = String::new();
    content.push_str("# Architect Role\n\n");
    content.push_str("You are the **architect** - you plan work but do NOT write code.\n\n");
    content.push_str("## Core Principles\n\n");
    content.push_str("1. **Planning only** - You do NOT edit code or make commits\n");
    content.push_str("2. **Research first** - Explore the codebase before proposing tasks\n");
    content.push_str(
        "3. **Get confirmation** - List task titles and wait for user approval before adding\n",
    );
    content.push_str("4. **One task at a time per worker** - Don't overload the backlog\n\n");
    content.push_str("## Your Responsibilities\n\n");
    content
        .push_str("- Convert user intent into well-scoped tasks with clear acceptance criteria\n");
    content.push_str("- Place tasks in the correct lane for the appropriate worker\n");
    content.push_str("- Ask clarifying questions instead of guessing\n");
    content.push_str("- Monitor worker progress and unblock them when needed\n\n");
    content.push_str("## Available Workers\n");
    content.push_str(&lanes.join("\n"));
    content.push_str("\n\n");
    content.push_str("## Task Structure\n\n");
    content.push_str("Each task should include:\n");
    content.push_str("- **id**: Unique identifier (kebab-case)\n");
    content.push_str("- **description**: What needs to be implemented\n");
    content.push_str("- **acceptance**: List of criteria for completion\n\n");

    match config.tasks.source {
        TaskSource::Github => {
            if let Some(project) = config.tasks.github_project {
                content.push_str("## Task Management\n\n");
                content.push_str(&format!(
                    "Tasks are managed in **GitHub Project #{}**.\n\n",
                    project
                ));
                content.push_str("Use the GitHub Project board to:\n");
                content.push_str("- Add new tasks to the appropriate lane's backlog\n");
                content.push_str("- Monitor task status (Backlog → In Progress → Done)\n");
                content.push_str("- Review completed work\n\n");
            }
        }
        TaskSource::Yaml => {
            content.push_str("## Task Management\n\n");
            content.push_str(&format!(
                "Tasks are managed in `{}`.\n\n",
                tasks_file.display()
            ));
            content.push_str("### Adding a Task\n\n");
            content.push_str("```yaml\n<lane-name>:\n  backlog:\n    - id: my-task-id\n      title: Short title for the task\n      description: |\n        Detailed description of what needs to be done.\n      priority: high\n      acceptance:\n        - First acceptance criterion\n        - Second acceptance criterion\n```\n\n");
            content.push_str("### Task Lifecycle\n\n");
            content.push_str("1. **backlog** - Tasks waiting to be claimed\n");
            content
                .push_str("2. **in_progress** - Worker is actively working (max 1 per worker)\n");
            content.push_str("3. **done** - Completed with summary\n\n");
            content.push_str("### YAML Validation (CRITICAL)\n\n");
            content.push_str("When editing tasks.yaml, you MUST ensure valid YAML:\n");
            content.push_str("- Empty lists MUST use `[]`, never leave blank (e.g., `backlog: []` not `backlog:`)\n");
            content.push_str("- After editing, validate with: `yq eval '.' <tasks-file> > /dev/null && echo 'Valid' || echo 'Invalid'`\n");
            content.push_str("- If validation fails, fix the YAML before proceeding\n\n");
        }
    }

    // PR creation guidance for architect
    if !config.workflow.auto_create_pr {
        content.push_str("## Pull Request Guidance\n\n");
        content.push_str("Workers do NOT automatically create PRs after completing tasks.\n");
        content.push_str("If a task requires a PR, **explicitly state it** in the task description:\n\n");
        content.push_str("```yaml\ndescription: |\n  Implement feature X.\n  \n  **Create a PR when complete.**\n```\n\n");
        content.push_str("Only request PRs when the changes should be reviewed or merged to main.\n\n");
    }

    content.push_str("---\n## Project-Specific Instructions\n");
    content.push_str("<!-- Add your custom instructions below this line -->\n\n");

    fs::write(&role_file, content)
        .with_context(|| format!("Failed writing {}", role_file.display()))?;

    Ok(())
}

fn relative_tasks_path(worker_dir: &Path, tasks_file: &Path) -> PathBuf {
    if let Some(relative) = pathdiff::diff_paths(tasks_file, worker_dir) {
        if relative.as_os_str().is_empty() {
            PathBuf::from(".hive/tasks.yaml")
        } else {
            relative
        }
    } else {
        tasks_file.to_path_buf()
    }
}
