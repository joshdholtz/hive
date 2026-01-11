use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::config::{
    ArchitectConfig, Backend, HiveConfig, TasksConfig, TaskSource, WindowConfig, WorkerConfig,
    WorkersConfig,
};
use crate::projects;
use crate::tasks::yaml::{LaneTasks, TasksFile, WorkerProtocol};

pub fn run(start_dir: &Path) -> Result<PathBuf> {
    let project_dir = start_dir.to_path_buf();
    let config_path = project_dir.join(".hive.yaml");

    if config_path.exists() {
        return Ok(config_path);
    }

    let mut state = SetupState::new(&project_dir);
    setup_terminal()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    let result = run_wizard(&mut terminal, &mut state, &project_dir, &config_path);

    cleanup_terminal()?;
    result?;

    Ok(config_path)
}

#[derive(Clone, Copy, Debug)]
enum Step {
    Welcome,
    Backends,
    Workers,
    Registry,
    Confirm,
    Done,
}

struct SetupState {
    step: Step,
    architect_backend: Backend,
    workers_backend: Backend,
    worker_count: usize,
    add_to_registry: bool,
    selection: usize,
    project_name: String,
}

impl SetupState {
    fn new(project_dir: &Path) -> Self {
        let project_name = project_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string();
        Self {
            step: Step::Welcome,
            architect_backend: Backend::Claude,
            workers_backend: Backend::Claude,
            worker_count: 2,
            add_to_registry: true,
            selection: 0,
            project_name,
        }
    }
}

fn run_wizard(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    state: &mut SetupState,
    project_dir: &Path,
    config_path: &Path,
) -> Result<()> {
    loop {
        terminal.draw(|frame| render_setup(frame, state))?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if handle_setup_key(state, key, project_dir, config_path)? {
                    break;
                }
            }
        }
    }

    Ok(())
}

fn handle_setup_key(
    state: &mut SetupState,
    key: KeyEvent,
    project_dir: &Path,
    config_path: &Path,
) -> Result<bool> {
    if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
        anyhow::bail!("Setup cancelled");
    }

    match state.step {
        Step::Welcome => {
            if key.code == KeyCode::Enter {
                state.step = Step::Backends;
                state.selection = 0;
            }
        }
        Step::Backends => match key.code {
            KeyCode::Up => state.selection = state.selection.saturating_sub(1),
            KeyCode::Down => state.selection = (state.selection + 1).min(1),
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                if state.selection == 0 {
                    state.architect_backend = toggle_backend(state.architect_backend);
                } else {
                    state.workers_backend = toggle_backend(state.workers_backend);
                }
            }
            KeyCode::Enter => state.step = Step::Workers,
            _ => {}
        },
        Step::Workers => match key.code {
            KeyCode::Left | KeyCode::Char('-') => {
                state.worker_count = state.worker_count.saturating_sub(1).max(1);
            }
            KeyCode::Right | KeyCode::Char('+') => {
                state.worker_count = (state.worker_count + 1).min(8);
            }
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                state.worker_count = c.to_digit(10).unwrap() as usize;
            }
            KeyCode::Enter => state.step = Step::Registry,
            _ => {}
        },
        Step::Registry => match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                state.add_to_registry = !state.add_to_registry;
            }
            KeyCode::Enter => state.step = Step::Confirm,
            _ => {}
        },
        Step::Confirm => {
            if key.code == KeyCode::Enter {
                write_config(project_dir, config_path, state)?;
                write_tasks(project_dir, state)?;
                crate::commands::role::run(project_dir, None).ok();
                if state.add_to_registry {
                    let _ = projects::add_project(project_dir, Some(state.project_name.clone()));
                }
                state.step = Step::Done;
            }
        }
        Step::Done => {
            if key.code == KeyCode::Enter {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn write_config(project_dir: &Path, config_path: &Path, state: &SetupState) -> Result<()> {
    let workers = (1..=state.worker_count)
        .map(|idx| {
            let lane = if state.worker_count == 1 {
                "default".to_string()
            } else {
                format!("lane-{}", idx)
            };
            WorkerConfig {
                id: format!("worker-{}", idx),
                dir: Some(".".to_string()),
                lane: Some(lane),
                branch: None,
            }
        })
        .collect::<Vec<_>>();

    let config = HiveConfig {
        architect: ArchitectConfig {
            backend: state.architect_backend,
        },
        workers: WorkersConfig {
            backend: state.workers_backend,
        },
        session: state.project_name.clone(),
        tasks: TasksConfig {
            source: TaskSource::Yaml,
            file: Some(".hive/tasks.yaml".to_string()),
            github_org: None,
            github_project: None,
            github_project_id: None,
            github_status_field_id: None,
            github_lane_field_id: None,
        },
        windows: vec![WindowConfig {
            name: "main".to_string(),
            layout: Some("even-horizontal".to_string()),
            workers,
        }],
        setup: None,
        messages: None,
        worker_instructions: None,
    };

    let content = serde_yaml::to_string(&config)?;
    std::fs::write(config_path, content)
        .with_context(|| format!("Failed writing {}", config_path.display()))?;

    std::fs::create_dir_all(project_dir.join(".hive"))?;
    Ok(())
}

fn write_tasks(project_dir: &Path, state: &SetupState) -> Result<()> {
    let mut tasks = TasksFile::default();
    tasks.worker_protocol = Some(WorkerProtocol {
        claim: Some("Move the task to in_progress and add claimed_by/claimed_at".to_string()),
        complete: Some("Move the task to done and add summary/files_changed".to_string()),
    });
    tasks.rules = Some(vec![
        "Claim one task at a time".to_string(),
        "Create a PR before starting a new task".to_string(),
    ]);

    for idx in 1..=state.worker_count {
        let lane = if state.worker_count == 1 {
            "default".to_string()
        } else {
            format!("lane-{}", idx)
        };
        tasks.lanes.insert(lane, LaneTasks::default());
    }

    let tasks_path = project_dir.join(".hive").join("tasks.yaml");
    let content = serde_yaml::to_string(&tasks)?;
    std::fs::write(&tasks_path, content)
        .with_context(|| format!("Failed writing {}", tasks_path.display()))?;

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

    let area = centered_rect(70, 70, frame.area());
    let block = Block::default()
        .title("Hive Setup")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    frame.render_widget(block.clone(), area);

    let inner = block.inner(area);
    let text = match state.step {
        Step::Welcome => vec![
            "Welcome to Hive".to_string(),
            "".to_string(),
            "This will create .hive.yaml and .hive/tasks.yaml in this repo.".to_string(),
            "Press Enter to continue.".to_string(),
        ],
        Step::Backends => vec![
            "Choose backends".to_string(),
            "".to_string(),
            format!(
                "{} Architect backend: {:?}",
                if state.selection == 0 { ">" } else { " " },
                state.architect_backend
            ),
            format!(
                "{} Workers backend: {:?}",
                if state.selection == 1 { ">" } else { " " },
                state.workers_backend
            ),
            "".to_string(),
            "Use Up/Down to select, Left/Right to toggle.".to_string(),
            "Press Enter to continue.".to_string(),
        ],
        Step::Workers => vec![
            "Workers".to_string(),
            "".to_string(),
            format!("Worker count: {}", state.worker_count),
            "Use Left/Right or 1-9 to change.".to_string(),
            "Press Enter to continue.".to_string(),
        ],
        Step::Registry => vec![
            "Project registry".to_string(),
            "".to_string(),
            format!(
                "Add this repo to the global project list: {}",
                if state.add_to_registry { "Yes" } else { "No" }
            ),
            "Use Left/Right to toggle.".to_string(),
            "Press Enter to continue.".to_string(),
        ],
        Step::Confirm => vec![
            "Ready to write config".to_string(),
            "".to_string(),
            format!("Project: {}", state.project_name),
            format!("Architect backend: {:?}", state.architect_backend),
            format!("Workers backend: {:?}", state.workers_backend),
            format!("Workers: {}", state.worker_count),
            format!(
                "Add to registry: {}",
                if state.add_to_registry { "Yes" } else { "No" }
            ),
            "".to_string(),
            "Press Enter to create files.".to_string(),
        ],
        Step::Done => vec![
            "Setup complete".to_string(),
            "".to_string(),
            "Press Enter to continue.".to_string(),
        ],
    }
    .join("\n");

    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::White));
    frame.render_widget(paragraph, inner);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: ratatui::layout::Rect) -> ratatui::layout::Rect {
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
