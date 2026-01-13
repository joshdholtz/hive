use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::config::{ArchitectConfig, Backend, WorkersConfig};
use crate::tasks::yaml::{LaneTasks, TasksFile, WorkerProtocol};
use crate::workspace::resolve::{create_workspace_dir, find_workspace_for_path};
use crate::workspace::{
    create_worktrees_with_symlinks, slug_from_path, WorkspaceConfig, WorkspaceProject,
};

/// Run the workspace setup wizard
pub fn run(start_dir: &Path) -> Result<PathBuf> {
    // Check if this directory is already part of a workspace
    if let Ok(Some(existing)) = find_workspace_for_path(start_dir) {
        return Ok(existing.dir);
    }

    let mut state = SetupState::new(start_dir);
    setup_terminal()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    let result = run_wizard(&mut terminal, &mut state, start_dir);

    cleanup_terminal()?;
    result
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    Welcome,
    ScanProjects,
    SelectProjects, // Select projects AND worker count per project
    NameLanes,      // Name lanes only for projects with 2+ workers
    Backends,
    SymlinkFiles, // Select files to symlink to worktrees
    Confirm,
    Creating,
    Done,
}

/// A discovered git repository
#[derive(Debug, Clone)]
struct DiscoveredProject {
    name: String,
    path: PathBuf,
    selected: bool,
    /// Number of workers (1-4)
    workers: usize,
    /// Lane names (filled in for multi-worker projects, auto-set for single-worker)
    lanes: Vec<String>,
}

impl DiscoveredProject {
    fn new(name: String, path: PathBuf) -> Self {
        Self {
            name: name.clone(),
            path,
            selected: false,
            workers: 1,
            lanes: vec![name], // Default lane name = project name
        }
    }

    /// Returns true if this project needs lane naming (2+ workers)
    fn needs_lane_naming(&self) -> bool {
        self.workers > 1
    }
}

/// A file that can be symlinked to worktrees
#[derive(Debug, Clone)]
struct SymlinkCandidate {
    path: String,
    selected: bool,
}

struct SetupState {
    step: Step,
    workspace_name: String,
    start_dir: PathBuf,
    discovered_projects: Vec<DiscoveredProject>,
    /// Cursor for SelectProjects step (index into discovered_projects)
    select_cursor: usize,
    /// Which selected project we're configuring (0-based index into selected projects)
    config_project_index: usize,
    /// Cursor for lane selection within current project
    lane_cursor: usize,
    editing_lane: bool,
    lane_input: String,
    architect_backend: Backend,
    workers_backend: Backend,
    backend_selection: usize,
    /// Discovered files that can be symlinked
    symlink_candidates: Vec<SymlinkCandidate>,
    symlink_cursor: usize,
    /// Final list of files to symlink
    symlink_files: Vec<String>,
    error_message: Option<String>,
}

impl SetupState {
    fn new(start_dir: &Path) -> Self {
        let workspace_name = start_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace")
            .to_string();

        Self {
            step: Step::Welcome,
            workspace_name,
            start_dir: start_dir.to_path_buf(),
            discovered_projects: Vec::new(),
            select_cursor: 0,
            config_project_index: 0,
            lane_cursor: 0,
            editing_lane: false,
            lane_input: String::new(),
            architect_backend: Backend::Claude,
            workers_backend: Backend::Claude,
            backend_selection: 0,
            symlink_candidates: Vec::new(),
            symlink_cursor: 0,
            symlink_files: Vec::new(),
            error_message: None,
        }
    }

    /// Scan selected projects for files that should be symlinked
    fn scan_symlink_candidates(&mut self) {
        let mut candidates = std::collections::HashSet::new();
        let patterns = [
            ".env",
            ".env.local",
            ".env.development",
            ".env.production",
            ".env.test",
        ];

        for project in self.discovered_projects.iter().filter(|p| p.selected) {
            for pattern in &patterns {
                let path = project.path.join(pattern);
                if path.exists() {
                    candidates.insert(pattern.to_string());
                }
            }
        }

        self.symlink_candidates = candidates
            .into_iter()
            .map(|path| SymlinkCandidate {
                path,
                selected: true,
            }) // Default selected
            .collect();
        self.symlink_candidates.sort_by(|a, b| a.path.cmp(&b.path));
    }

    /// Check if any selected project will have worktrees (needs symlinks)
    fn has_worktrees(&self) -> bool {
        self.discovered_projects
            .iter()
            .filter(|p| p.selected)
            .any(|p| p.workers > 1)
    }

    fn selected_projects(&self) -> Vec<&DiscoveredProject> {
        self.discovered_projects
            .iter()
            .filter(|p| p.selected)
            .collect()
    }

    fn projects_needing_lanes_count(&self) -> usize {
        self.discovered_projects
            .iter()
            .filter(|p| p.selected && p.needs_lane_naming())
            .count()
    }

    fn total_workers(&self) -> usize {
        self.discovered_projects
            .iter()
            .filter(|p| p.selected)
            .map(|p| p.workers)
            .sum()
    }

    /// Get the current project needing lane naming (immutable)
    fn current_lane_project(&self) -> Option<&DiscoveredProject> {
        self.discovered_projects
            .iter()
            .filter(|p| p.selected && p.needs_lane_naming())
            .nth(self.config_project_index)
    }

    /// Get the current project needing lane naming (mutable)
    fn current_lane_project_mut(&mut self) -> Option<&mut DiscoveredProject> {
        let idx = self.config_project_index;
        self.discovered_projects
            .iter_mut()
            .filter(|p| p.selected && p.needs_lane_naming())
            .nth(idx)
    }
}

fn run_wizard(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut SetupState,
    start_dir: &Path,
) -> Result<PathBuf> {
    loop {
        terminal.draw(|frame| render_setup(frame, state))?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match handle_setup_key(state, key, start_dir)? {
                    KeyResult::Continue => {}
                    KeyResult::Done(path) => return Ok(path),
                    KeyResult::Cancelled => anyhow::bail!("Setup cancelled"),
                }
            }
        }
    }
}

enum KeyResult {
    Continue,
    Done(PathBuf),
    Cancelled,
}

fn handle_setup_key(state: &mut SetupState, key: KeyEvent, start_dir: &Path) -> Result<KeyResult> {
    // Clear error on any key
    state.error_message = None;

    // Handle lane editing mode separately
    if state.editing_lane {
        return handle_lane_editing(state, key);
    }

    if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
        return Ok(KeyResult::Cancelled);
    }

    match state.step {
        Step::Welcome => {
            if key.code == KeyCode::Enter {
                state.step = Step::ScanProjects;
                // Scan for projects
                state.discovered_projects = scan_for_projects(start_dir);
                if state.discovered_projects.is_empty() {
                    // No projects found, add current directory as a project
                    let mut project = DiscoveredProject::new(
                        state.workspace_name.clone(),
                        start_dir.to_path_buf(),
                    );
                    project.selected = true;
                    state.discovered_projects.push(project);
                }
                state.step = Step::SelectProjects;
            }
        }

        Step::ScanProjects => {
            // This step is automatic, transition happens in Welcome
        }

        Step::SelectProjects => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if state.select_cursor > 0 {
                    state.select_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if state.select_cursor < state.discovered_projects.len().saturating_sub(1) {
                    state.select_cursor += 1;
                }
            }
            KeyCode::Char(' ') => {
                if let Some(project) = state.discovered_projects.get_mut(state.select_cursor) {
                    project.selected = !project.selected;
                }
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('+') => {
                // Increase worker count (also selects the project)
                if let Some(project) = state.discovered_projects.get_mut(state.select_cursor) {
                    project.selected = true;
                    if project.workers < 4 {
                        project.workers += 1;
                    }
                }
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('-') => {
                // Decrease worker count (min 1)
                if let Some(project) = state.discovered_projects.get_mut(state.select_cursor) {
                    if project.workers > 1 {
                        project.workers -= 1;
                    }
                }
            }
            KeyCode::Char('a') => {
                for project in &mut state.discovered_projects {
                    project.selected = true;
                }
            }
            KeyCode::Char('n') => {
                for project in &mut state.discovered_projects {
                    project.selected = false;
                }
            }
            KeyCode::Enter => {
                if state.selected_projects().is_empty() {
                    state.error_message = Some("Select at least one project".to_string());
                } else {
                    // Initialize lanes for all selected projects
                    for project in &mut state.discovered_projects {
                        if project.selected {
                            if project.workers == 1 {
                                // Single worker: lane name = project name
                                project.lanes = vec![project.name.clone()];
                            } else {
                                // Multiple workers: need lane naming, start with numbered defaults
                                project.lanes = (1..=project.workers)
                                    .map(|i| format!("lane-{}", i))
                                    .collect();
                            }
                        }
                    }

                    // Skip to Backends if no projects need lane naming
                    if state.projects_needing_lanes_count() > 0 {
                        state.step = Step::NameLanes;
                        state.config_project_index = 0;
                        state.lane_cursor = 0;
                    } else {
                        state.step = Step::Backends;
                    }
                }
            }
            _ => {}
        },

        Step::NameLanes => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if state.lane_cursor > 0 {
                    state.lane_cursor -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let lane_count = state
                    .current_lane_project()
                    .map(|p| p.lanes.len())
                    .unwrap_or(0);
                if state.lane_cursor < lane_count.saturating_sub(1) {
                    state.lane_cursor += 1;
                }
            }
            KeyCode::Enter => {
                // Edit selected lane
                let lane_cursor = state.lane_cursor;
                if let Some(project) = state.current_lane_project() {
                    if lane_cursor < project.lanes.len() {
                        state.lane_input = project.lanes[lane_cursor].clone();
                        state.editing_lane = true;
                    }
                }
            }
            KeyCode::Tab => {
                // Next project or move to Backends
                let total = state.projects_needing_lanes_count();
                if state.config_project_index + 1 < total {
                    state.config_project_index += 1;
                    state.lane_cursor = 0;
                } else {
                    state.step = Step::Backends;
                }
            }
            KeyCode::BackTab => {
                // Previous project
                if state.config_project_index > 0 {
                    state.config_project_index -= 1;
                    state.lane_cursor = 0;
                }
            }
            _ => {}
        },

        Step::Backends => match key.code {
            KeyCode::Up => state.backend_selection = state.backend_selection.saturating_sub(1),
            KeyCode::Down => state.backend_selection = (state.backend_selection + 1).min(1),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                if state.backend_selection == 0 {
                    state.architect_backend = toggle_backend(state.architect_backend);
                } else {
                    state.workers_backend = toggle_backend(state.workers_backend);
                }
            }
            KeyCode::Enter => {
                // Only show symlink step if we have worktrees
                if state.has_worktrees() {
                    state.scan_symlink_candidates();
                    state.step = Step::SymlinkFiles;
                } else {
                    state.step = Step::Confirm;
                }
            }
            _ => {}
        },

        Step::SymlinkFiles => match key.code {
            KeyCode::Up => {
                state.symlink_cursor = state.symlink_cursor.saturating_sub(1);
            }
            KeyCode::Down => {
                if !state.symlink_candidates.is_empty() {
                    state.symlink_cursor =
                        (state.symlink_cursor + 1).min(state.symlink_candidates.len() - 1);
                }
            }
            KeyCode::Char(' ') => {
                if let Some(candidate) = state.symlink_candidates.get_mut(state.symlink_cursor) {
                    candidate.selected = !candidate.selected;
                }
            }
            KeyCode::Enter => {
                // Collect selected files
                state.symlink_files = state
                    .symlink_candidates
                    .iter()
                    .filter(|c| c.selected)
                    .map(|c| c.path.clone())
                    .collect();
                state.step = Step::Confirm;
            }
            _ => {}
        },

        Step::Confirm => {
            if key.code == KeyCode::Enter {
                state.step = Step::Creating;
                // Create the workspace
                match create_workspace(state) {
                    Ok(_workspace_dir) => {
                        state.step = Step::Done;
                    }
                    Err(e) => {
                        state.error_message = Some(format!("Failed: {}", e));
                        state.step = Step::Confirm;
                    }
                }
            }
        }

        Step::Creating => {
            // Wait for creation to complete
        }

        Step::Done => {
            if key.code == KeyCode::Enter {
                let workspace_dir =
                    crate::workspace::resolve::workspace_dir(&state.workspace_name)?;
                return Ok(KeyResult::Done(workspace_dir));
            }
        }
    }

    Ok(KeyResult::Continue)
}

fn handle_lane_editing(state: &mut SetupState, key: KeyEvent) -> Result<KeyResult> {
    match key.code {
        KeyCode::Esc => {
            state.editing_lane = false;
            state.lane_input.clear();
        }
        KeyCode::Enter => {
            let input = state.lane_input.trim().to_lowercase().replace(' ', "-");
            if !input.is_empty() {
                let lane_cursor = state.lane_cursor;
                if let Some(project) = state.current_lane_project_mut() {
                    if lane_cursor < project.lanes.len() {
                        project.lanes[lane_cursor] = input;
                    }
                }
            }
            state.editing_lane = false;
            state.lane_input.clear();
        }
        KeyCode::Backspace => {
            state.lane_input.pop();
        }
        KeyCode::Char(c) => {
            // Only allow alphanumeric and dashes
            if c.is_alphanumeric() || c == '-' || c == '_' || c == ' ' {
                state.lane_input.push(c);
            }
        }
        _ => {}
    }
    Ok(KeyResult::Continue)
}

/// Scan a directory for git repositories (recursively, up to a few levels deep)
fn scan_for_projects(dir: &Path) -> Vec<DiscoveredProject> {
    const MAX_SCAN_DEPTH: usize = 3;
    const SKIP_DIRS: &[&str] = &["node_modules", "target", "venv", ".hive"];

    fn is_hidden(path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            name.starts_with('.')
        } else {
            false
        }
    }

    fn is_git_repo(path: &Path) -> bool {
        path.join(".git").exists()
    }

    fn should_skip(path: &Path) -> bool {
        if is_hidden(path) {
            return true;
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            SKIP_DIRS.contains(&name)
        } else {
            false
        }
    }

    fn collect_projects(path: &Path, depth: usize, projects: &mut Vec<DiscoveredProject>) {
        if is_git_repo(path) {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project")
                .to_string();
            let mut project = DiscoveredProject::new(name, path.to_path_buf());
            project.selected = true;
            projects.push(project);
            return;
        }

        if depth >= MAX_SCAN_DEPTH {
            return;
        }

        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let sub_path = entry.path();
                if !sub_path.is_dir() || should_skip(&sub_path) {
                    continue;
                }

                collect_projects(&sub_path, depth + 1, projects);
            }
        }
    }

    let mut projects = Vec::new();
    collect_projects(dir, 0, &mut projects);

    if projects.is_empty() {
        // No repositories found, default to current directory (matching previous behavior)
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();
        let mut project = DiscoveredProject::new(name, dir.to_path_buf());
        project.selected = true;
        projects.push(project);
    } else {
        projects.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    projects
}

/// Create the workspace with all configuration
fn create_workspace(state: &SetupState) -> Result<PathBuf> {
    let workspace_dir = create_workspace_dir(&state.workspace_name)?;

    // Build workspace config
    let mut config = WorkspaceConfig {
        name: state.workspace_name.clone(),
        root: Some(state.start_dir.clone()),
        projects: Vec::new(),
        architect: ArchitectConfig {
            backend: state.architect_backend,
        },
        workers: WorkersConfig {
            backend: state.workers_backend,
            skip_permissions: false,
            setup: Vec::new(),
            symlink: state.symlink_files.clone(),
        },
        layout: crate::workspace::config::LayoutConfig::default(),
    };

    // Add selected projects with their lanes
    for project in state.discovered_projects.iter().filter(|p| p.selected) {
        config.projects.push(WorkspaceProject {
            path: project.path.clone(),
            workers: project.workers,
            lanes: project.lanes.clone(),
        });
    }

    // Save config
    config.save(&workspace_dir)?;

    // Create worktrees for projects with multiple workers
    for project in &config.projects {
        if project.workers > 1 {
            create_worktrees_with_symlinks(&workspace_dir, project, &config.workers.symlink)?;
        }
    }

    // Create tasks file
    write_tasks(&workspace_dir, &config)?;

    // Create architect role
    write_architect_role(&workspace_dir, &config)?;

    // Create lane role files
    write_lane_roles(&workspace_dir, &config)?;

    Ok(workspace_dir)
}

fn write_tasks(workspace_dir: &Path, config: &WorkspaceConfig) -> Result<()> {
    use crate::tasks::yaml::ProjectEntry;
    use crate::workspace::config::slug_from_path;

    let mut tasks = TasksFile::default();
    tasks.worker_protocol = Some(WorkerProtocol {
        claim: Some("Move the task to in_progress and add claimed_by/claimed_at".to_string()),
        complete: Some("Move the task to done and add summary/files_changed".to_string()),
    });
    tasks.rules = Some(vec![
        "Claim one task at a time".to_string(),
        "Create a PR before starting a new task".to_string(),
    ]);

    // Create project entries - nested for multi-lane, direct for single-lane
    for project in &config.projects {
        let project_slug = slug_from_path(&project.path);

        if project.lanes.len() > 1 {
            // Multi-lane project: nested structure
            let mut lanes = std::collections::HashMap::new();
            for lane in &project.lanes {
                lanes.insert(lane.clone(), LaneTasks::default());
            }
            tasks
                .projects
                .insert(project_slug, ProjectEntry::Nested(lanes));
        } else if let Some(lane) = project.lanes.first() {
            // Single-lane project: direct structure (use lane name as key)
            tasks
                .projects
                .insert(lane.clone(), ProjectEntry::Direct(LaneTasks::default()));
        }
    }

    let tasks_path = workspace_dir.join("tasks.yaml");
    let content = serde_yaml::to_string(&tasks)?;
    std::fs::write(&tasks_path, content)
        .with_context(|| format!("Failed writing {}", tasks_path.display()))?;

    Ok(())
}

fn write_architect_role(workspace_dir: &Path, config: &WorkspaceConfig) -> Result<()> {
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
        "- After editing, validate with: `yq eval '.' {}/tasks.yaml > /dev/null && echo 'Valid' || echo 'Invalid'`\n",
        workspace_dir.display()
    ));
    content.push_str("- If validation fails, fix the YAML before proceeding\n");

    let role_path = workspace_dir.join("ARCHITECT.md");
    std::fs::write(&role_path, content)
        .with_context(|| format!("Failed writing {}", role_path.display()))?;

    Ok(())
}

fn write_lane_roles(workspace_dir: &Path, config: &WorkspaceConfig) -> Result<()> {
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

            // Branch naming
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
            content.push_str("5. Create a PR with your changes\n");
            content.push_str("6. Move task to `done`, then claim the next task\n\n");

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

            let role_path = lane_dir.join("WORKER.md");
            std::fs::write(&role_path, content)
                .with_context(|| format!("Failed writing {}", role_path.display()))?;
        }
    }

    Ok(())
}

fn toggle_backend(current: Backend) -> Backend {
    match current {
        Backend::Claude => Backend::Codex,
        Backend::Codex => Backend::Claude,
    }
}

fn setup_terminal() -> Result<()> {
    terminal::enable_raw_mode()?;
    execute!(std::io::stdout(), EnterAlternateScreen, cursor::Show)?;
    Ok(())
}

fn cleanup_terminal() -> Result<()> {
    terminal::disable_raw_mode()?;
    execute!(std::io::stdout(), cursor::Show, LeaveAlternateScreen)?;
    Ok(())
}

fn render_setup(frame: &mut ratatui::Frame, state: &SetupState) {
    use ratatui::style::{Color, Style};
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let area = centered_rect(80, 80, frame.area());
    let block = Block::default()
        .title("Hive Workspace Setup")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);

    let text = match state.step {
        Step::Welcome => {
            vec![
                "Welcome to Hive Workspace Setup".to_string(),
                "".to_string(),
                format!("Directory: {}", state.start_dir.display()),
                format!("Workspace name: {}", state.workspace_name),
                "".to_string(),
                "This will create a workspace in ~/.hive/workspaces/".to_string(),
                "".to_string(),
                "Press Enter to scan for projects...".to_string(),
            ]
        }

        Step::ScanProjects => {
            vec!["Scanning for projects...".to_string()]
        }

        Step::SelectProjects => {
            let mut lines = vec![
                "Select projects and worker count".to_string(),
                "".to_string(),
                format!("Found {} project(s):", state.discovered_projects.len()),
                "".to_string(),
            ];

            for (i, project) in state.discovered_projects.iter().enumerate() {
                let cursor = if i == state.select_cursor { ">" } else { " " };
                let selected = if project.selected { "[x]" } else { "[ ]" };
                let workers_display = if project.workers > 1 {
                    format!(" ({} workers)", project.workers)
                } else {
                    String::new()
                };
                lines.push(format!(
                    "{} {} {}{}",
                    cursor, selected, project.name, workers_display
                ));
            }

            lines.push("".to_string());
            lines.push("Space: toggle | +/-: workers | a: all | Enter: continue".to_string());

            if let Some(ref err) = state.error_message {
                lines.push("".to_string());
                lines.push(format!("Error: {}", err));
            }

            lines
        }

        Step::NameLanes => {
            let total = state.projects_needing_lanes_count();
            let current_idx = state.config_project_index + 1;

            if let Some(project) = state.current_lane_project() {
                let mut lines = vec![
                    format!(
                        "Name lanes for: {}                         [{} of {}]",
                        project.name, current_idx, total
                    ),
                    "".to_string(),
                    format!(
                        "This project has {} workers. Name each lane:",
                        project.workers
                    ),
                    "".to_string(),
                ];

                for (i, lane) in project.lanes.iter().enumerate() {
                    let marker = if i == state.lane_cursor { ">" } else { " " };
                    lines.push(format!("  {} Lane {}: {}", marker, i + 1, lane));
                }

                lines.push("".to_string());

                if state.editing_lane {
                    lines.push(format!("Lane name: {}_", state.lane_input));
                    lines.push("".to_string());
                    lines.push("Enter: save | Esc: cancel".to_string());
                } else {
                    lines.push("Enter: rename | Tab: next project | Shift+Tab: prev".to_string());
                }

                lines
            } else {
                vec!["No project to configure".to_string()]
            }
        }

        Step::Backends => {
            vec![
                "Choose AI backends".to_string(),
                "".to_string(),
                format!(
                    "{} Architect backend: {:?}",
                    if state.backend_selection == 0 {
                        ">"
                    } else {
                        " "
                    },
                    state.architect_backend
                ),
                format!(
                    "{} Workers backend: {:?}",
                    if state.backend_selection == 1 {
                        ">"
                    } else {
                        " "
                    },
                    state.workers_backend
                ),
                "".to_string(),
                "Up/Down: select | Left/Right: toggle | Enter: continue".to_string(),
            ]
        }

        Step::SymlinkFiles => {
            let mut lines = vec![
                "Symlink files to worktrees".to_string(),
                "".to_string(),
                "These files exist in your projects but won't be in worktrees.".to_string(),
                "Select which files to symlink:".to_string(),
                "".to_string(),
            ];

            if state.symlink_candidates.is_empty() {
                lines.push("  (No .env files found)".to_string());
            } else {
                for (i, candidate) in state.symlink_candidates.iter().enumerate() {
                    let cursor = if i == state.symlink_cursor { ">" } else { " " };
                    let check = if candidate.selected { "[x]" } else { "[ ]" };
                    lines.push(format!("{} {} {}", cursor, check, candidate.path));
                }
            }

            lines.push("".to_string());
            lines.push("Up/Down: select | Space: toggle | Enter: continue".to_string());
            lines
        }

        Step::Confirm => {
            let mut lines = vec![
                "Ready to create workspace".to_string(),
                "".to_string(),
                format!("Workspace: {}", state.workspace_name),
                format!("Location: ~/.hive/workspaces/{}/", state.workspace_name),
                "".to_string(),
                "Projects:".to_string(),
            ];

            for project in state.discovered_projects.iter().filter(|p| p.selected) {
                if project.workers == 1 {
                    lines.push(format!("  {} (1 worker)", project.name));
                } else {
                    lines.push(format!(
                        "  {} ({} workers: {})",
                        project.name,
                        project.workers,
                        project.lanes.join(", ")
                    ));
                }
            }

            lines.push("".to_string());
            lines.push(format!("Architect: {:?}", state.architect_backend));
            lines.push(format!("Workers backend: {:?}", state.workers_backend));
            lines.push(format!("Total workers: {}", state.total_workers()));
            lines.push("".to_string());
            lines.push("Press Enter to create workspace...".to_string());

            if let Some(ref err) = state.error_message {
                lines.push("".to_string());
                lines.push(format!("Error: {}", err));
            }

            lines
        }

        Step::Creating => {
            vec![
                "Creating workspace...".to_string(),
                "".to_string(),
                "Creating directories...".to_string(),
                "Creating worktrees...".to_string(),
                "Writing configuration...".to_string(),
            ]
        }

        Step::Done => {
            vec![
                "Workspace created successfully!".to_string(),
                "".to_string(),
                format!("Location: ~/.hive/workspaces/{}/", state.workspace_name),
                "".to_string(),
                "You can now run 'hive' from any project directory".to_string(),
                format!("or 'hive open {}' from anywhere.", state.workspace_name),
                "".to_string(),
                "Press Enter to start hive...".to_string(),
            ]
        }
    }
    .join("\n");

    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    frame.render_widget(paragraph, inner);
}

fn centered_rect(
    percent_x: u16,
    percent_y: u16,
    r: ratatui::layout::Rect,
) -> ratatui::layout::Rect {
    use ratatui::layout::{Constraint, Direction, Layout};
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
