use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
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

use crate::app::state::{App, AppWindow, ClientPane};
use crate::app::{key_to_bytes, layout_visible_panes};
use crate::config;
use crate::ipc::{decode_server_message, ClientMessage, PaneSize, ServerMessage};
use crate::projects;
use crate::pty::output::{filter_alternate_screen, OutputBuffer};
use crate::ui;

pub fn run(start_dir: &Path) -> Result<()> {
    // First check for workspace
    if let Ok(Some(workspace)) = crate::workspace::resolve::find_workspace_for_path(start_dir) {
        return run_workspace(&workspace.dir);
    }

    // Fall back to legacy .hive.yaml
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

/// Attach to a workspace server
pub fn run_workspace(workspace_dir: &Path) -> Result<()> {
    let socket_path = workspace_dir.join("hive.sock");
    let log_path = workspace_dir.join("client.log");
    let _ = std::fs::write(&log_path, "");

    let mut conn = ClientConn::connect(socket_path, &log_path)?;

    let mut app = App::new(
        crate::config::Backend::Claude,
        Vec::<ClientPane>::new(),
        Vec::<AppWindow>::new(),
        workspace_dir.to_path_buf(),
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
        let stream = UnixStream::connect(&self.socket_path)
            .with_context(|| format!("Failed to reconnect to {}", self.socket_path.display()))?;
        stream.set_nonblocking(true)?;
        self.stream = stream;
        self.read_buf.clear();
        Ok(())
    }
}

fn log_line(path: &std::path::Path, line: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
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
    let mut cached_size = (80u16, 24u16); // Fallback size (width, height)

    loop {
        if let Err(e) = terminal.draw(|frame| ui::render(frame, app)) {
            log_line(log_path, &format!("draw error: {}", e));
            // Continue - don't crash on draw errors
        }

        // Calculate workers_per_page based on terminal size
        // Use cached size if query fails (cursor position timeout)
        let (width, height) = match terminal.size() {
            Ok(size) => {
                cached_size = (size.width, size.height);
                (size.width, size.height)
            }
            Err(e) => {
                log_line(log_path, &format!("size error: {}", e));
                cached_size
            }
        };
        let rect = Rect::new(0, 0, width, height);
        let body = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(rect)[1];
        let pane_area = if app.sidebar.visible {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(24), Constraint::Min(0)])
                .split(body)[1]
        } else {
            body
        };

        let has_architect = app
            .panes
            .iter()
            .any(|p| p.visible && matches!(p.pane_type, crate::app::types::PaneType::Architect));
        let workers_per_page =
            crate::ui::layout::calculate_workers_per_page(pane_area, has_architect);

        // Clamp page if terminal resized
        app.clamp_worker_page(workers_per_page);

        // Minimum PTY size - avoid zero dimensions but honor pane bounds
        let min_pty_rows = 2u16;
        let min_pty_cols = 2u16;

        if !app.panes.is_empty() {
            let layout = crate::ui::layout::calculate_layout(app, pane_area, workers_per_page);
            let sizes: Vec<PaneSize> = layout
                .iter()
                .map(|(idx, rect)| PaneSize {
                    pane_id: app.panes[*idx].id.clone(),
                    rows: rect.height.saturating_sub(2).max(min_pty_rows),
                    cols: rect.width.saturating_sub(2).max(min_pty_cols),
                })
                .collect();
            if sizes != last_sizes {
                for (idx, rect) in &layout {
                    let rows = rect.height.saturating_sub(2).max(min_pty_rows);
                    let cols = rect.width.saturating_sub(2).max(min_pty_cols);
                    if let Some(pane) = app.panes.get_mut(*idx) {
                        pane.output_buffer.resize(rows, cols);
                    }
                }
                conn.send(ClientMessage::Resize {
                    panes: sizes.clone(),
                })?;
                last_sizes = sizes;
            }
        }

        for message in conn.read_messages(log_path)? {
            match message {
                ServerMessage::State { state } => {
                    log_line(log_path, "apply-state");
                    app.apply_state(state);

                    // Immediately resize buffers to current terminal size before processing output
                    // This prevents replay from being processed at wrong size (24x80 default)
                    if !app.panes.is_empty() {
                        let layout =
                            crate::ui::layout::calculate_layout(app, pane_area, workers_per_page);
                        for (idx, rect) in &layout {
                            let rows = rect.height.saturating_sub(2).max(min_pty_rows);
                            let cols = rect.width.saturating_sub(2).max(min_pty_cols);
                            if let Some(pane) = app.panes.get_mut(*idx) {
                                pane.output_buffer.resize(rows, cols);
                            }
                        }
                    }

                    for pane in &mut app.panes {
                        if let Some(data) = pending_output.remove(&pane.id) {
                            pane.output_buffer.push_bytes(&data);
                            // Also push to raw history for tmux-style scrollback
                            for byte in &data {
                                pane.raw_history.push_back(*byte);
                            }
                            while pane.raw_history.len() > pane.raw_history_max {
                                pane.raw_history.pop_front();
                            }
                        }
                    }
                }
                ServerMessage::Output { pane_id, data } => {
                    log_line(log_path, &format!("apply-output {}", pane_id));
                    if let Some(pane) = app.panes.iter_mut().find(|p| p.id == pane_id) {
                        pane.output_buffer.push_bytes(&data);
                        // Also push to raw history for tmux-style scrollback
                        for byte in &data {
                            pane.raw_history.push_back(*byte);
                        }
                        while pane.raw_history.len() > pane.raw_history_max {
                            pane.raw_history.pop_front();
                        }
                    } else {
                        pending_output
                            .entry(pane_id)
                            .or_default()
                            .extend_from_slice(&data);
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

        // Handle events with graceful error recovery
        match event::poll(Duration::from_millis(50)) {
            Ok(true) => {
                match event::read() {
                    Ok(Event::Key(key)) => {
                        if handle_key_event(app, conn, key, workers_per_page, pane_area)? {
                            break;
                        }
                    }
                    Ok(_) => {} // Ignore non-key events
                    Err(e) => {
                        log_line(log_path, &format!("event read error: {}", e));
                        // Continue - don't crash on event read errors
                    }
                }
            }
            Ok(false) => {} // No event
            Err(e) => {
                log_line(log_path, &format!("event poll error: {}", e));
                // Continue - don't crash on poll errors
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

fn handle_key_event(
    app: &mut App,
    conn: &mut ClientConn,
    key: KeyEvent,
    workers_per_page: usize,
    pane_area: Rect,
) -> Result<bool> {
    if app.show_help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
            app.show_help = false;
        }
        return Ok(false);
    }

    if app.show_projects {
        return handle_projects_key(app, key);
    }

    if app.show_task_queue {
        return handle_task_queue_key(app, key);
    }

    if app.scroll_mode {
        return handle_scroll_mode_key(app, key);
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
                            crate::app::palette::PaletteAction::FocusNext => {
                                app.focus_next(&visible)
                            }
                            crate::app::palette::PaletteAction::FocusPrev => {
                                app.focus_prev(&visible)
                            }
                            crate::app::palette::PaletteAction::FocusPane(idx) => {
                                app.focused_pane = idx
                            }
                            crate::app::palette::PaletteAction::ToggleZoom => app.toggle_zoom(),
                            crate::app::palette::PaletteAction::ToggleArchitectPosition => {
                                app.toggle_architect_position();
                                conn.send(ClientMessage::SetArchitectLeft {
                                    left: app.architect_left,
                                })?;
                            }
                            crate::app::palette::PaletteAction::ToggleSidebar => {
                                app.sidebar.visible = !app.sidebar.visible;
                                if !app.sidebar.visible {
                                    app.sidebar.focused = false;
                                }
                            }
                            crate::app::palette::PaletteAction::FocusSidebar => {
                                if app.sidebar.visible {
                                    app.sidebar.focused = true;
                                }
                            }
                            crate::app::palette::PaletteAction::ProjectManager => {
                                open_project_manager(app)?;
                            }
                            crate::app::palette::PaletteAction::ToggleTaskQueue => {
                                app.show_task_queue = !app.show_task_queue;
                                app.task_queue_selection = 0;
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
                            crate::app::palette::PaletteAction::Detach => {
                                conn.send(ClientMessage::Detach)?;
                                return Ok(true);
                            }
                            crate::app::palette::PaletteAction::Stop => {
                                conn.send(ClientMessage::Shutdown)?;
                                return Ok(true);
                            }
                        }
                    }
                }
                app.show_palette = false;
            }
            KeyCode::Char(c)
                if c >= '1' && c <= '9' && !key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                // Number shortcuts: 1-9 select items directly
                let idx = (c as usize) - ('1' as usize);
                if let Some(item_idx) = filtered.get(idx) {
                    if let Some(item) = items.get(*item_idx) {
                        match item.action.clone() {
                            crate::app::palette::PaletteAction::FocusNext => {
                                app.focus_next(&visible)
                            }
                            crate::app::palette::PaletteAction::FocusPrev => {
                                app.focus_prev(&visible)
                            }
                            crate::app::palette::PaletteAction::FocusPane(pane_idx) => {
                                app.focused_pane = pane_idx
                            }
                            crate::app::palette::PaletteAction::ToggleZoom => app.toggle_zoom(),
                            crate::app::palette::PaletteAction::ToggleArchitectPosition => {
                                app.toggle_architect_position();
                                conn.send(ClientMessage::SetArchitectLeft {
                                    left: app.architect_left,
                                })?;
                            }
                            crate::app::palette::PaletteAction::ToggleSidebar => {
                                app.sidebar.visible = !app.sidebar.visible;
                                if !app.sidebar.visible {
                                    app.sidebar.focused = false;
                                }
                            }
                            crate::app::palette::PaletteAction::FocusSidebar => {
                                if app.sidebar.visible {
                                    app.sidebar.focused = true;
                                }
                            }
                            crate::app::palette::PaletteAction::ProjectManager => {
                                open_project_manager(app)?;
                            }
                            crate::app::palette::PaletteAction::ToggleTaskQueue => {
                                app.show_task_queue = !app.show_task_queue;
                                app.task_queue_selection = 0;
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
                            crate::app::palette::PaletteAction::Detach => {
                                conn.send(ClientMessage::Detach)?;
                                return Ok(true);
                            }
                            crate::app::palette::PaletteAction::Stop => {
                                conn.send(ClientMessage::Shutdown)?;
                                return Ok(true);
                            }
                        }
                        app.show_palette = false;
                    }
                }
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

    if app.sidebar.focused && app.sidebar.visible && !key.modifiers.contains(KeyModifiers::CONTROL)
    {
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

    // Sidebar reordering with Ctrl+U/D (only when sidebar focused)
    if app.sidebar.focused && app.sidebar.visible {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('u') {
            // Preserve focused pane by ID across reorder
            let focused_id = app.panes.get(app.focused_pane).map(|p| p.id.clone());
            if app.sidebar.reorder_up(&mut app.panes) {
                // Restore focused_pane to point to the same pane by ID
                if let Some(id) = focused_id {
                    if let Some(new_idx) = app.panes.iter().position(|p| p.id == id) {
                        app.focused_pane = new_idx;
                    }
                }
                let pane_ids: Vec<String> = app.panes.iter().map(|p| p.id.clone()).collect();
                conn.send(ClientMessage::ReorderPanes { pane_ids })?;
            }
            return Ok(false);
        } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('d') {
            // Preserve focused pane by ID across reorder
            let focused_id = app.panes.get(app.focused_pane).map(|p| p.id.clone());
            if app.sidebar.reorder_down(&mut app.panes) {
                // Restore focused_pane to point to the same pane by ID
                if let Some(id) = focused_id {
                    if let Some(new_idx) = app.panes.iter().position(|p| p.id == id) {
                        app.focused_pane = new_idx;
                    }
                }
                let pane_ids: Vec<String> = app.panes.iter().map(|p| p.id.clone()).collect();
                conn.send(ClientMessage::ReorderPanes { pane_ids })?;
            }
            return Ok(false);
        }
    }

    // Calculate layout for grid navigation
    let layout = crate::ui::layout::calculate_layout(app, pane_area, workers_per_page);
    let has_architect = app
        .panes
        .iter()
        .any(|p| p.visible && matches!(p.pane_type, crate::app::types::PaneType::Architect));

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('h') {
        // Move left in grid
        navigate_grid(app, &layout, has_architect, -1, 0, workers_per_page);
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('j') {
        // Move down in grid
        navigate_grid(app, &layout, has_architect, 0, 1, workers_per_page);
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
        // Move up in grid
        navigate_grid(app, &layout, has_architect, 0, -1, workers_per_page);
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
        // Move right in grid
        navigate_grid(app, &layout, has_architect, 1, 0, workers_per_page);
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
        // Toggle sidebar (and focus it when opening)
        app.sidebar.visible = !app.sidebar.visible;
        if app.sidebar.visible {
            app.sidebar.focused = true;
        } else {
            app.sidebar.focused = false;
        }
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('d') {
        // Detach from session
        conn.send(ClientMessage::Detach)?;
        return Ok(true);
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('z') {
        // Toggle zoom on focused pane
        app.toggle_zoom();
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
        // Toggle smart mode (only show active panes)
        app.smart_mode = !app.smart_mode;
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
        // Open command palette
        app.show_palette = true;
        app.palette_query.clear();
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('t') {
        // Toggle task queue view
        app.show_task_queue = !app.show_task_queue;
        app.task_queue_selection = 0;
    } else if key.code == KeyCode::Esc
        || (key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('['))
    {
        // Enter scroll mode (like tmux copy mode) - ESC or Ctrl+[
        // Note: Ctrl+[ sends ESC in terminals, so we check for both
        if app.scroll_mode {
            // Already in scroll mode, exit it
            app.scroll_mode = false;
            app.scroll_buffer = None;
            return Ok(false);
        }
        // Build a scrollback buffer from raw history for scroll mode.
        if let Some(pane) = app.panes.get(app.focused_pane) {
            let history: Vec<u8> = pane.raw_history.iter().copied().collect();
            let filtered = filter_alternate_screen(&history);
            let (rows, cols) = pane.output_buffer.size();
            let mut scroll_buf = OutputBuffer::new(rows, cols, 10000);
            scroll_buf.push_bytes(&filtered);
            app.scroll_buffer = Some(scroll_buf);
        } else {
            app.scroll_buffer = None;
        }
        app.scroll_mode = true;
    } else if key.code == KeyCode::PageUp {
        // Scroll up in focused pane
        if let Some(pane) = app.panes.get_mut(app.focused_pane) {
            pane.output_buffer.scroll_up(10);
        }
    } else if key.code == KeyCode::PageDown {
        // Scroll down in focused pane
        if let Some(pane) = app.panes.get_mut(app.focused_pane) {
            pane.output_buffer.scroll_down(10);
        }
    } else if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('d') {
        // Detach from session
        conn.send(ClientMessage::Detach)?;
        return Ok(true);
    } else if key.code == KeyCode::Home && key.modifiers.contains(KeyModifiers::CONTROL) {
        // Scroll to top of focused pane
        if let Some(pane) = app.panes.get_mut(app.focused_pane) {
            pane.output_buffer.scroll_to_top();
        }
    } else if key.code == KeyCode::End && key.modifiers.contains(KeyModifiers::CONTROL) {
        // Scroll to bottom of focused pane
        if let Some(pane) = app.panes.get_mut(app.focused_pane) {
            pane.output_buffer.scroll_to_bottom();
        }
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
                            app.projects_message = Some(format!("Added {}", path.display()));
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

fn handle_task_queue_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    let max_lines = crate::ui::task_queue::count_lines(app);

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.show_task_queue = false;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.task_queue_selection > 0 {
                app.task_queue_selection -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.task_queue_selection + 1 < max_lines {
                app.task_queue_selection += 1;
            }
        }
        KeyCode::Char(' ') => {
            // Toggle expand/collapse if on a lane header
            if let Some(lane) = crate::ui::task_queue::get_selected_lane(app) {
                let expanded = app.task_queue_expanded.get(&lane).copied().unwrap_or(true);
                app.task_queue_expanded.insert(lane, !expanded);
            }
        }
        KeyCode::Enter => {
            // Jump to lane's worker pane if on a lane header
            if let Some(lane) = crate::ui::task_queue::get_selected_lane(app) {
                // Find the pane with this lane
                if let Some((idx, _)) = app
                    .panes
                    .iter()
                    .enumerate()
                    .find(|(_, p)| p.lane.as_deref() == Some(&lane))
                {
                    app.focused_pane = idx;
                    app.show_task_queue = false;
                }
            }
        }
        _ => {}
    }

    Ok(false)
}

fn handle_scroll_mode_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => {
            app.scroll_mode = false;
            app.scroll_buffer = None;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut scroll_buf) = app.scroll_buffer {
                scroll_buf.scroll_up(1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut scroll_buf) = app.scroll_buffer {
                scroll_buf.scroll_down(1);
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Half page up
            if let Some(ref mut scroll_buf) = app.scroll_buffer {
                scroll_buf.scroll_up(15);
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            // Half page down
            if let Some(ref mut scroll_buf) = app.scroll_buffer {
                scroll_buf.scroll_down(15);
            }
        }
        KeyCode::Char('g') => {
            // Scroll to top
            if let Some(ref mut scroll_buf) = app.scroll_buffer {
                scroll_buf.scroll_to_top();
            }
        }
        KeyCode::Char('G') => {
            // Scroll to bottom
            if let Some(ref mut scroll_buf) = app.scroll_buffer {
                scroll_buf.scroll_to_bottom();
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

/// Navigate in the grid with automatic page change when moving off edge
fn navigate_grid(
    app: &mut App,
    layout: &[(usize, Rect)],
    has_architect: bool,
    dx: i32,
    dy: i32,
    workers_per_page: usize,
) {
    use crate::ui::layout::{get_grid_position, get_pane_at_position};

    let Some(pos) = get_grid_position(layout, app.focused_pane, has_architect) else {
        return;
    };

    // Calculate target position
    let new_col = (pos.col as i32 + dx).max(0) as usize;
    let new_row = (pos.row as i32 + dy).max(0) as usize;

    // For vertical movement (j/k), just navigate within the current page
    if dy != 0 {
        // Don't go above row 0
        if dy < 0 && pos.row == 0 {
            return;
        }
        // Don't go below the last row
        if dy > 0 && new_row >= pos.num_rows {
            return;
        }
        // Normal vertical navigation
        if let Some(new_idx) = get_pane_at_position(layout, new_row, pos.col, has_architect) {
            app.focused_pane = new_idx;
        }
        return;
    }

    // For horizontal movement (h/l), check for page changes at edges
    if dx < 0 && pos.col == 0 {
        // Moving left at left edge - change page from any row
        if app.worker_page > 0 {
            app.worker_page -= 1;
            // Focus the last worker on the previous page
            focus_worker_on_page(app, app.worker_page, workers_per_page, true);
        }
        return;
    }

    if dx > 0 && new_col >= pos.num_cols {
        // Moving right at right edge - change page from any row
        let total_pages = app.total_worker_pages(workers_per_page);
        if app.worker_page + 1 < total_pages {
            app.worker_page += 1;
            // Focus the first worker on the new page
            focus_worker_on_page(app, app.worker_page, workers_per_page, false);
        }
        return;
    }

    // Normal horizontal navigation within current page
    if let Some(new_idx) = get_pane_at_position(layout, new_row, new_col, has_architect) {
        app.focused_pane = new_idx;
    }
}

/// Focus a worker on a specific page
fn focus_worker_on_page(app: &mut App, page: usize, workers_per_page: usize, last: bool) {
    // Use the same visual order as the layout calculation
    let workers = crate::ui::layout::get_workers_in_visual_order(app);

    // Get workers on the target page
    let page_start = page * workers_per_page;
    let page_workers: Vec<usize> = workers
        .into_iter()
        .skip(page_start)
        .take(workers_per_page)
        .collect();

    // Focus first or last worker on the page
    if last {
        if let Some(&idx) = page_workers.last() {
            app.focused_pane = idx;
        }
    } else if let Some(&idx) = page_workers.first() {
        app.focused_pane = idx;
    }
}
