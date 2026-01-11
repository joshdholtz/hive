use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::app::{key_to_bytes, layout_visible_panes};
use crate::app::state::{App, AppWindow, ClientPane};
use crate::config;
use crate::ipc::{decode_server_message, ClientMessage, PaneSize, ServerMessage};
use crate::projects;
use crate::ui;

pub fn run(start_dir: &Path) -> Result<()> {
    let config_path = config::find_config(start_dir)?;
    let project_dir = config::project_dir(&config_path);
    let socket_path = project_dir.join(".hive").join("hive.sock");
    let log_path = project_dir.join(".hive").join("client.log");
    let _ = std::fs::write(&log_path, "");

    let mut conn = ClientConn::connect(socket_path, &log_path)?;

    let mut app = App::new(
        crate::config::Backend::Claude,
        Vec::<ClientPane>::new(),
        Vec::<AppWindow>::new(),
        project_dir.clone(),
    );

    setup_terminal()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(std::io::stdout()))?;

    let result = run_tui(&mut terminal, &mut app, &mut conn, &log_path);

    cleanup_terminal()?;
    result
}

struct ClientConn {
    socket_path: std::path::PathBuf,
    stream: UnixStream,
    read_buf: String,
}

impl ClientConn {
    fn connect(socket_path: std::path::PathBuf, log_path: &std::path::Path) -> Result<Self> {
        let stream = UnixStream::connect(&socket_path)
            .with_context(|| format!("Failed to connect to {}", socket_path.display()))?;
        stream.set_nonblocking(true)?;
        log_line(log_path, "connected");
        Ok(Self {
            socket_path,
            stream,
            read_buf: String::new(),
        })
    }

    fn send(&mut self, message: ClientMessage) -> Result<()> {
        let line = serde_json::to_string(&message)?;
        match writeln!(self.stream, "{}", line) {
            Ok(_) => {
                self.stream.flush()?;
                Ok(())
            }
            Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => {
                self.reconnect()?;
                writeln!(self.stream, "{}", line)?;
                self.stream.flush()?;
                Ok(())
            }
            Err(err) => Err(err.into()),
        }
    }

    fn read_messages(&mut self, log_path: &std::path::Path) -> Result<Vec<ServerMessage>> {
        let mut messages = Vec::new();
        let mut buf = [0u8; 4096];

        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    log_line(log_path, "reader-eof");
                    self.reconnect()?;
                    break;
                }
                Ok(n) => {
                    self.read_buf.push_str(&String::from_utf8_lossy(&buf[..n]));
                    while let Some(pos) = self.read_buf.find('\n') {
                        let line = self.read_buf[..pos].to_string();
                        self.read_buf.drain(..=pos);
                        if let Some(message) = decode_server_message(&line) {
                            messages.push(message);
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    log_line(log_path, "reader-error");
                    self.reconnect()?;
                    break;
                }
            }
        }

        Ok(messages)
    }

    fn reconnect(&mut self) -> Result<()> {
        let stream = UnixStream::connect(&self.socket_path).with_context(|| {
            format!("Failed to reconnect to {}", self.socket_path.display())
        })?;
        stream.set_nonblocking(true)?;
        self.stream = stream;
        self.read_buf.clear();
        Ok(())
    }
}

fn log_line(path: &std::path::Path, line: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", line);
    }
}

fn run_tui(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
    conn: &mut ClientConn,
    log_path: &std::path::Path,
) -> Result<()> {
    let mut last_tick = Instant::now();
    let mut last_sizes: Vec<PaneSize> = Vec::new();
    let mut pending_output: HashMap<String, Vec<u8>> = HashMap::new();

    loop {
        terminal.draw(|frame| ui::render(frame, app))?;

        if !app.panes.is_empty() {
            let area = terminal.size()?;
            let rect = Rect::new(0, 0, area.width, area.height);
            let body = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
                .split(rect)[1];
            let pane_area = if app.sidebar.visible {
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Length(24), Constraint::Min(0)])
                    .split(body)[1]
            } else {
                body
            };
            let layout = crate::ui::layout::calculate_layout(app, pane_area);
            let sizes: Vec<PaneSize> = layout
                .iter()
                .map(|(idx, rect)| PaneSize {
                    pane_id: app.panes[*idx].id.clone(),
                    rows: rect.height.saturating_sub(2).max(1),
                    cols: rect.width.saturating_sub(2).max(1),
                })
                .collect();
            if sizes != last_sizes {
                for (idx, rect) in &layout {
                    let rows = rect.height.saturating_sub(2).max(1);
                    let cols = rect.width.saturating_sub(2).max(1);
                    if let Some(pane) = app.panes.get_mut(*idx) {
                        pane.output_buffer.resize(rows, cols);
                    }
                }
                conn.send(ClientMessage::Resize { panes: sizes.clone() })?;
                last_sizes = sizes;
            }
        }

        for message in conn.read_messages(log_path)? {
            match message {
                ServerMessage::State { state } => {
                    log_line(log_path, "apply-state");
                    app.apply_state(state);
                    for pane in &mut app.panes {
                        if let Some(data) = pending_output.remove(&pane.id) {
                            pane.output_buffer.push_bytes(&data);
                        }
                    }
                }
                ServerMessage::Output { pane_id, data } => {
                    log_line(log_path, &format!("apply-output {}", pane_id));
                    if let Some(pane) = app.panes.iter_mut().find(|p| p.id == pane_id) {
                        pane.output_buffer.push_bytes(&data);
                    } else {
                        pending_output.entry(pane_id).or_default().extend_from_slice(&data);
                    }
                }
                ServerMessage::PaneExited { pane_id } => {
                    log_line(log_path, &format!("pane-exited {}", pane_id));
                    if let Some(pane) = app.panes.iter_mut().find(|p| p.id == pane_id) {
                        pane.output_buffer.push_bytes(b"\n[pane exited]");
                    }
                }
                ServerMessage::Error { message } => {
                    log_line(log_path, "server-error");
                    if let Some(pane) = app.panes.first_mut() {
                        pane.output_buffer.push_bytes(message.as_bytes());
                    }
                }
            }
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if handle_key_event(app, conn, key)? {
                    break;
                }
            }
        }

        if last_tick.elapsed() >= Duration::from_millis(250) {
            last_tick = Instant::now();
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}

fn handle_key_event(app: &mut App, conn: &mut ClientConn, key: KeyEvent) -> Result<bool> {
    if app.show_help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
            app.show_help = false;
        }
        return Ok(false);
    }

    if app.show_projects {
        return handle_projects_key(app, key);
    }

    let visible = layout_visible_panes(app);

    if app.show_palette {
        let items = crate::app::palette::build_items(app);
        let filtered = crate::app::palette::filter_indices(&items, &app.palette_query);
        let max_index = filtered.len().saturating_sub(1);
        if app.palette_selection > max_index {
            app.palette_selection = 0;
        }

        match key.code {
            KeyCode::Esc => app.show_palette = false,
            KeyCode::Up => {
                if app.palette_selection > 0 {
                    app.palette_selection -= 1;
                }
            }
            KeyCode::Down => {
                if app.palette_selection < max_index {
                    app.palette_selection += 1;
                }
            }
            KeyCode::Backspace => {
                app.palette_query.pop();
                app.palette_selection = 0;
            }
            KeyCode::Enter => {
                if let Some(item_idx) = filtered.get(app.palette_selection) {
                    if let Some(item) = items.get(*item_idx) {
                        match item.action.clone() {
                            crate::app::palette::PaletteAction::FocusNext => app.focus_next(&visible),
                            crate::app::palette::PaletteAction::FocusPrev => app.focus_prev(&visible),
                            crate::app::palette::PaletteAction::FocusPane(idx) => {
                                app.focused_pane = idx
                            }
                            crate::app::palette::PaletteAction::ToggleZoom => app.toggle_zoom(),
                            crate::app::palette::PaletteAction::ToggleSidebar => {
                                app.sidebar.visible = !app.sidebar.visible;
                                if !app.sidebar.visible {
                                    app.sidebar.focused = false;
                                }
                            }
                            crate::app::palette::PaletteAction::FocusSidebar => {
                                if app.sidebar.visible {
                                    app.sidebar.focused = true;
                                    app.nav_mode = false;
                                }
                            }
                            crate::app::palette::PaletteAction::ProjectManager => {
                                open_project_manager(app)?;
                            }
                            crate::app::palette::PaletteAction::NudgeAll => {
                                conn.send(ClientMessage::Nudge { worker: None })?;
                            }
                            crate::app::palette::PaletteAction::NudgeFocused => {
                                if let Some(pane) = app.panes.get(app.focused_pane) {
                                    conn.send(ClientMessage::Nudge {
                                        worker: Some(pane.id.clone()),
                                    })?;
                                }
                            }
                            crate::app::palette::PaletteAction::ToggleHelp => {
                                app.show_help = !app.show_help;
                            }
                            crate::app::palette::PaletteAction::Quit => return Ok(true),
                        }
                    }
                }
                app.show_palette = false;
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    app.palette_query.push(c);
                    app.palette_selection = 0;
                }
            }
            _ => {}
        }

        return Ok(false);
    }

    if app.sidebar.focused && app.sidebar.visible {
        match key.code {
            KeyCode::Esc => {
                app.sidebar.focused = false;
            }
            KeyCode::Tab => {
                app.sidebar.focused = false;
            }
            KeyCode::Up | KeyCode::Char('k') => app.sidebar.move_up(&app.panes),
            KeyCode::Down | KeyCode::Char('j') => app.sidebar.move_down(&app.panes),
            KeyCode::Char(' ') => {
                let changes = app.sidebar.toggle_selected(&mut app.panes);
                for (pane_id, visible) in changes {
                    conn.send(ClientMessage::SetVisibility { pane_id, visible })?;
                }
                app.ensure_focus_visible();
            }
            KeyCode::Enter => {
                if let Some(pane_id) = app.sidebar.selected_pane_id() {
                    if let Some(pane) = app.panes.iter_mut().find(|pane| pane.id == pane_id) {
                        pane.visible = true;
                        app.focused_pane = app
                            .panes
                            .iter()
                            .position(|pane| pane.id == pane_id)
                            .unwrap_or(app.focused_pane);
                        conn.send(ClientMessage::SetVisibility {
                            pane_id,
                            visible: true,
                        })?;
                    }
                    app.sidebar.focused = false;
                } else {
                    let changes = app.sidebar.toggle_selected(&mut app.panes);
                    for (pane_id, visible) in changes {
                        conn.send(ClientMessage::SetVisibility { pane_id, visible })?;
                    }
                    app.ensure_focus_visible();
                }
            }
            KeyCode::Left | KeyCode::Char('h') => app.sidebar.collapse_selected(),
            KeyCode::Right | KeyCode::Char('l') => app.sidebar.expand_selected(),
            KeyCode::Char('a') => {
                let changes = app.sidebar.select_all(&mut app.panes);
                for (pane_id, visible) in changes {
                    conn.send(ClientMessage::SetVisibility { pane_id, visible })?;
                }
                app.ensure_focus_visible();
            }
            KeyCode::Char('n') => {
                let changes = app.sidebar.select_none(&mut app.panes);
                for (pane_id, visible) in changes {
                    conn.send(ClientMessage::SetVisibility { pane_id, visible })?;
                }
                app.ensure_focus_visible();
            }
            _ => {}
        }
        return Ok(false);
    }

    if app.nav_mode {
        match key.code {
            KeyCode::Esc => app.nav_mode = false,
            KeyCode::Up | KeyCode::Left => app.focus_prev(&visible),
            KeyCode::Down | KeyCode::Right => app.focus_next(&visible),
            KeyCode::Char('h') => app.focus_prev(&visible),
            KeyCode::Char('j') => app.focus_next(&visible),
            KeyCode::Char('k') => app.focus_prev(&visible),
            KeyCode::Char('l') => app.focus_next(&visible),
            KeyCode::Tab if app.sidebar.visible => {
                app.sidebar.focused = true;
                app.nav_mode = false;
            }
            KeyCode::Char('z') => app.toggle_zoom(),
            KeyCode::Char('?') => app.show_help = !app.show_help,
            KeyCode::Char('n') => {
                conn.send(ClientMessage::Nudge { worker: None })?;
            }
            KeyCode::Char('N') => {
                if let Some(pane) = app.panes.get(app.focused_pane) {
                    conn.send(ClientMessage::Nudge {
                        worker: Some(pane.id.clone()),
                    })?;
                }
            }
            KeyCode::Enter => app.nav_mode = false,
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('d') => {
                conn.send(ClientMessage::Detach)?;
                return Ok(true);
            }
            KeyCode::PageUp => {
                if let Some(pane) = app.panes.get_mut(app.focused_pane) {
                    pane.output_buffer.scroll_up(10);
                }
            }
            KeyCode::PageDown => {
                if let Some(pane) = app.panes.get_mut(app.focused_pane) {
                    pane.output_buffer.scroll_down(10);
                }
            }
            KeyCode::Home => {
                if let Some(pane) = app.panes.get_mut(app.focused_pane) {
                    pane.output_buffer.scroll_to_top();
                }
            }
            KeyCode::End => {
                if let Some(pane) = app.panes.get_mut(app.focused_pane) {
                    pane.output_buffer.scroll_to_bottom();
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g') {
        app.nav_mode = !app.nav_mode;
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
        app.show_palette = true;
        app.palette_query.clear();
        app.palette_selection = 0;
    } else {
        let bytes = key_to_bytes(key);
        if !bytes.is_empty() {
            if let Some(pane) = app.panes.get(app.focused_pane) {
                conn.send(ClientMessage::Input {
                    pane_id: pane.id.clone(),
                    data: bytes,
                })?;
            }
        }
    }

    Ok(false)
}

fn open_project_manager(app: &mut App) -> Result<()> {
    let projects_file = projects::load_projects().unwrap_or_default();
    app.projects = projects_file.projects;
    app.projects_selection = 0;
    app.projects_input.clear();
    app.projects_input_mode = false;
    app.projects_message = None;
    app.show_projects = true;
    Ok(())
}

fn handle_projects_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    if app.projects_input_mode {
        match key.code {
            KeyCode::Esc => {
                app.projects_input_mode = false;
                app.projects_input.clear();
            }
            KeyCode::Backspace => {
                app.projects_input.pop();
            }
            KeyCode::Enter => {
                let input = app.projects_input.trim();
                if !input.is_empty() {
                    let path = std::path::PathBuf::from(input);
                    match projects::add_project(&path, None) {
                        Ok(projects_file) => {
                            app.projects = projects_file.projects;
                            app.projects_message =
                                Some(format!("Added {}", path.display()));
                        }
                        Err(err) => {
                            app.projects_message = Some(format!("Failed: {}", err));
                        }
                    }
                }
                app.projects_input_mode = false;
                app.projects_input.clear();
            }
            KeyCode::Char(c) => {
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    app.projects_input.push(c);
                }
            }
            _ => {}
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Esc => {
            app.show_projects = false;
        }
        KeyCode::Up => {
            if app.projects_selection > 0 {
                app.projects_selection -= 1;
            }
        }
        KeyCode::Down => {
            if app.projects_selection + 1 < app.projects.len() {
                app.projects_selection += 1;
            }
        }
        KeyCode::Char('a') => {
            let path = app.project_dir.clone();
            match projects::add_project(&path, None) {
                Ok(projects_file) => {
                    app.projects = projects_file.projects;
                    app.projects_message = Some("Added current project".to_string());
                }
                Err(err) => {
                    app.projects_message = Some(format!("Failed: {}", err));
                }
            }
        }
        KeyCode::Char('A') => {
            app.projects_input_mode = true;
            app.projects_input.clear();
        }
        KeyCode::Char('d') => {
            if let Some(project) = app.projects.get(app.projects_selection) {
                match projects::remove_project_by_path(&project.path) {
                    Ok(projects_file) => {
                        app.projects = projects_file.projects;
                        if app.projects_selection >= app.projects.len() && !app.projects.is_empty()
                        {
                            app.projects_selection = app.projects.len() - 1;
                        }
                        app.projects_message = Some("Removed project".to_string());
                    }
                    Err(err) => {
                        app.projects_message = Some(format!("Failed: {}", err));
                    }
                }
            }
        }
        _ => {}
    }

    Ok(false)
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
