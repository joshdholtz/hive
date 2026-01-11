use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::app::{build_nudge_message, build_startup_message};
use crate::app::state::{AppWindow, LayoutKind, LayoutMode};
use crate::app::types::PaneType;
use crate::config::{self, HiveConfig, TaskSource};
use crate::ipc::{
    decode_client_message, encode_message, AppState, ClientMessage, PaneInfo, PaneSize, ServerMessage,
    WindowInfo,
};
use crate::pty::{spawn_agent, spawn_reader_thread, Pane, PaneEvent};
use crate::tasks::{counts_for_lane, load_tasks, spawn_yaml_watcher, NudgeRequest};
use crate::utils::{git, shell};

const ARCHITECT_MESSAGE: &str = "Read .hive/ARCHITECT.md. You are the architect - plan tasks but do NOT edit code. Add tasks to the tasks file for workers to pick up.";

pub fn run(config_path: &Path) -> Result<()> {
    let config = config::load_config(config_path)?;
    config::validate(&config)?;
    let project_dir = config::project_dir(config_path);

    if let Some(setup) = &config.setup {
        for command in setup {
            shell::run_shell_command(command, &project_dir)?;
        }
    }

    git::ensure_git_exclude(&project_dir)?;
    std::fs::create_dir_all(project_dir.join(".hive"))?;

    let layout_mode = load_layout_mode(&project_dir).unwrap_or(LayoutMode::Default);

    let (panes, windows) = spawn_panes(&config, &project_dir)?;

    let (event_tx, event_rx) = mpsc::channel::<ServerEvent>();
    let (pane_tx, pane_rx) = mpsc::channel::<PaneEvent>();

    for pane in &panes {
        let reader = pane
            .master
            .try_clone_reader()
            .context("Failed to clone PTY reader")?;
        spawn_reader_thread(pane.id.clone(), reader, pane_tx.clone());
    }

    let (nudge_tx, nudge_rx) = mpsc::channel::<NudgeRequest>();

    let tasks_file = if let TaskSource::Yaml = config.tasks.source {
        let tasks_path = config::tasks_file_path(config_path, &config);
        spawn_yaml_watcher(
            tasks_path.clone(),
            nudge_tx.clone(),
            Duration::from_secs(10),
            Duration::from_secs(5),
        )
        .ok();
        Some(tasks_path)
    } else {
        None
    };

    let socket_path = socket_path(&project_dir);
    prepare_socket(&socket_path)?;

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("Failed to bind {}", socket_path.display()))?;
    listener.set_nonblocking(true)?;

    let log_path = project_dir.join(".hive").join("server.log");
    let _ = std::fs::write(&log_path, ""); // reset log

    let state = ServerState {
        config,
        project_dir,
        panes,
        windows,
        layout_mode,
        task_counts: HashMap::new(),
        tasks_file,
        log_path,
    };

    write_pid(&state.project_dir)?;

    let result = event_loop(state, listener, event_rx, pane_rx, event_tx, nudge_rx);

    cleanup_socket(&socket_path).ok();

    result
}

struct ServerState {
    config: HiveConfig,
    project_dir: PathBuf,
    panes: Vec<Pane>,
    windows: Vec<AppWindow>,
    layout_mode: LayoutMode,
    task_counts: HashMap<String, crate::tasks::TaskCounts>,
    tasks_file: Option<PathBuf>,
    log_path: PathBuf,
}

enum ServerEvent {
    ClientConnected { client_id: usize, sender: Sender<ServerMessage> },
    ClientMessage { client_id: usize, message: ClientMessage },
    ClientDisconnected { client_id: usize },
}

#[derive(Clone)]
struct ClientHandle {
    id: usize,
    sender: Sender<ServerMessage>,
}

fn event_loop(
    mut state: ServerState,
    listener: UnixListener,
    event_rx: Receiver<ServerEvent>,
    pane_rx: Receiver<PaneEvent>,
    event_tx: Sender<ServerEvent>,
    nudge_rx: Receiver<NudgeRequest>,
) -> Result<()> {
    let client_counter = Arc::new(AtomicUsize::new(1));
    let mut clients: Vec<ClientHandle> = Vec::new();

    refresh_task_counts(&mut state).ok();

    let accept_tx = event_tx.clone();
    let accept_counter = client_counter.clone();
    let log_path = state.log_path.clone();
    thread::spawn(move || loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let client_id = accept_counter.fetch_add(1, Ordering::SeqCst);
                match handle_client(stream, client_id, accept_tx.clone(), log_path.clone()) {
                    Ok(sender) => {
                        let _ = accept_tx.send(ServerEvent::ClientConnected { client_id, sender });
                    }
                    Err(err) => {
                        let _ = accept_tx.send(ServerEvent::ClientDisconnected { client_id });
                        eprintln!("client {} error: {}", client_id, err);
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break,
        }
    });

    let mut last_tick = Instant::now();

    loop {
        while let Ok(req) = nudge_rx.try_recv() {
            match req {
                NudgeRequest::All => {
                    refresh_task_counts(&mut state).ok();
                    let _ = nudge_workers(&mut state, None);
                    broadcast_state(&state, &mut clients);
                }
            }
        }

        while let Ok(event) = pane_rx.try_recv() {
            match event {
                PaneEvent::Output { pane_id, data } => {
                    log_line(&state.log_path, &format!("pane-output {} bytes={}", pane_id, data.len()));
                    if let Some(pane) = state.panes.iter_mut().find(|p| p.id == pane_id) {
                        pane.output_buffer.push_bytes(&data);
                        pane.push_history(&data);
                    }
                    broadcast(&mut clients, ServerMessage::Output { pane_id, data });
                }
                PaneEvent::Exited { pane_id } => {
                    log_line(&state.log_path, &format!("pane-exited {}", pane_id));
                    broadcast(&mut clients, ServerMessage::PaneExited { pane_id });
                }
                PaneEvent::Error { pane_id, error } => {
                    log_line(&state.log_path, &format!("pane-error {} {}", pane_id, error));
                    let message = format!("[error] {}", error);
                    broadcast(
                        &mut clients,
                        ServerMessage::Output {
                            pane_id,
                            data: message.into_bytes(),
                        },
                    );
                }
            }
        }

        match event_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(event) => match event {
                ServerEvent::ClientConnected { client_id, sender } => {
                    log_line(&state.log_path, &format!("client-connected {}", client_id));
                    clients.push(ClientHandle { id: client_id, sender });
                    let handle = clients.last().cloned();
                    broadcast_state(&state, &mut clients);
                    if let Some(handle) = handle {
                        send_replay(&state, &handle);
                    }
                }
                ServerEvent::ClientMessage { client_id, message } => {
                    log_line(&state.log_path, &format!("client-message {}", client_id));
                    handle_client_message(&mut state, &mut clients, message);
                }
                ServerEvent::ClientDisconnected { client_id } => {
                    log_line(&state.log_path, &format!("client-disconnected {}", client_id));
                    clients.retain(|client| client.id != client_id);
                }
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if last_tick.elapsed() >= Duration::from_secs(2) {
            last_tick = Instant::now();
        }
    }

    Ok(())
}

fn handle_client(
    stream: UnixStream,
    client_id: usize,
    event_tx: Sender<ServerEvent>,
    log_path: PathBuf,
) -> Result<Sender<ServerMessage>> {
    stream.set_nonblocking(false)?;
    let (reader, mut writer) = stream.try_clone().map(|clone| (clone, stream))?;

    let (tx, rx) = mpsc::channel::<ServerMessage>();
    let log_path_writer = log_path.clone();
    thread::spawn(move || {
        while let Ok(message) = rx.recv() {
            let line = encode_message(&message);
            if writeln!(writer, "{}", line).is_err() {
                log_line(&log_path_writer, "client-write-error");
                break;
            }
        }
        log_line(&log_path_writer, "client-writer-exit");
    });

    let mut reader = BufReader::new(reader);
    let log_path_reader = log_path.clone();
    thread::spawn(move || loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                log_line(&log_path_reader, "client-read-eof");
                let _ = event_tx.send(ServerEvent::ClientDisconnected { client_id });
                break;
            }
            Ok(_) => {
                if let Some(message) = decode_client_message(line.trim()) {
                    let _ = event_tx.send(ServerEvent::ClientMessage { client_id, message });
                }
            }
            Err(_) => {
                log_line(&log_path_reader, "client-read-error");
                let _ = event_tx.send(ServerEvent::ClientDisconnected { client_id });
                break;
            }
        }
    });

    Ok(tx)
}

fn handle_client_message(
    state: &mut ServerState,
    clients: &mut Vec<ClientHandle>,
    message: ClientMessage,
) {
    match message {
        ClientMessage::Input { pane_id, data } => {
            if let Some(pane) = state.panes.iter_mut().find(|p| p.id == pane_id) {
                let _ = crate::pty::send_bytes(&mut pane.writer, &data);
            }
        }
        ClientMessage::Resize { panes } => {
            for pane in panes {
                resize_pane(state, pane);
            }
        }
        ClientMessage::Nudge { worker } => {
            refresh_task_counts(state).ok();
            let _ = nudge_workers(state, worker.as_deref());
            broadcast_state(state, clients);
        }
        ClientMessage::SetVisibility { pane_id, visible } => {
            if let Some(pane) = state.panes.iter_mut().find(|p| p.id == pane_id) {
                pane.visible = visible;
                broadcast_state(state, clients);
            }
        }
        ClientMessage::Layout { mode } => {
            state.layout_mode = mode;
            let _ = write_layout_mode(&state.project_dir, mode);
            broadcast_state(state, clients);
        }
        ClientMessage::Detach => {}
    }
}

fn resize_pane(state: &mut ServerState, pane: PaneSize) {
    if let Some(target) = state.panes.iter_mut().find(|p| p.id == pane.pane_id) {
        let _ = target.master.resize(portable_pty::PtySize {
            rows: pane.rows,
            cols: pane.cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }
}

fn spawn_panes(config: &HiveConfig, project_dir: &Path) -> Result<(Vec<Pane>, Vec<AppWindow>)> {
    let mut panes = Vec::new();
    let mut windows = Vec::new();
    let group_counts = build_group_counts(config, project_dir);

    let (arch_master, arch_child, arch_writer) =
        spawn_agent(config.architect.backend, ARCHITECT_MESSAGE, project_dir)?;

    panes.push(Pane {
        id: "architect".to_string(),
        pane_type: PaneType::Architect,
        master: arch_master,
        child: arch_child,
        writer: arch_writer,
        output_buffer: crate::pty::output::OutputBuffer::new(24, 80, 2000),
        raw_history: std::collections::VecDeque::new(),
        raw_history_max: 200_000,
        lane: None,
        working_dir: project_dir.to_path_buf(),
        branch: None,
        group: None,
        visible: true,
    });

    let architect_idx = 0;
    windows.push(AppWindow {
        name: "Architect".to_string(),
        layout: LayoutKind::EvenHorizontal,
        pane_indices: vec![architect_idx],
    });

    for window in &config.windows {
        let mut pane_indices = Vec::new();
        for worker in &window.workers {
            let lane = worker.lane.clone().unwrap_or_else(|| worker.id.clone());
            let dir = worker.dir.clone().unwrap_or_else(|| ".".to_string());
            let working_dir = project_dir.join(dir);
            let startup_message = build_startup_message(config, &lane);
            let group = group_for_dir(&working_dir, project_dir, &group_counts);

            let (master, child, writer) =
                spawn_agent(config.workers.backend, &startup_message, &working_dir)?;

            let pane = Pane {
                id: worker.id.clone(),
                pane_type: PaneType::Worker { lane: lane.clone() },
                master,
                child,
                writer,
                output_buffer: crate::pty::output::OutputBuffer::new(24, 80, 2000),
                raw_history: std::collections::VecDeque::new(),
                raw_history_max: 200_000,
                lane: Some(lane),
                working_dir,
                branch: worker.branch.clone(),
                group,
                visible: true,
            };

            panes.push(pane);
            pane_indices.push(panes.len() - 1);
        }

        windows.push(AppWindow {
            name: window.name.clone(),
            layout: LayoutKind::from_str(window.layout.as_deref().unwrap_or("even-horizontal")),
            pane_indices,
        });
    }

    Ok((panes, windows))
}

fn build_group_counts(config: &HiveConfig, project_dir: &Path) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for window in &config.windows {
        for worker in &window.workers {
            let dir = worker.dir.clone().unwrap_or_else(|| ".".to_string());
            let working_dir = project_dir.join(dir);
            if let Some(group) = group_name_for_dir(&working_dir, project_dir) {
                *counts.entry(group).or_insert(0) += 1;
            }
        }
    }
    counts
}

fn group_for_dir(
    working_dir: &Path,
    project_dir: &Path,
    group_counts: &HashMap<String, usize>,
) -> Option<String> {
    let name = group_name_for_dir(working_dir, project_dir)?;
    if group_counts.get(&name).copied().unwrap_or(0) > 1 {
        Some(name)
    } else {
        None
    }
}

fn group_name_for_dir(working_dir: &Path, project_dir: &Path) -> Option<String> {
    let rel = working_dir.strip_prefix(project_dir).ok()?;
    let parent = rel.parent()?;
    let name = parent.file_name()?.to_string_lossy().to_string();
    if name == "." || name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn nudge_workers(state: &mut ServerState, specific_worker: Option<&str>) -> Result<Vec<String>> {
    let mut nudged = Vec::new();

    for pane in &mut state.panes {
        let lane = match &pane.pane_type {
            PaneType::Worker { lane } => lane.clone(),
            _ => continue,
        };

        if let Some(target) = specific_worker {
            if pane.id != target {
                continue;
            }
        }

        let counts = state.task_counts.get(&lane).copied().unwrap_or_default();

        if counts.backlog > 0 && counts.in_progress == 0 {
            let message = build_nudge_message(&state.config, &lane, counts.backlog, &pane.branch);
            crate::pty::send_to_pane(&mut pane.writer, &message)?;
            nudged.push(pane.id.clone());
        }
    }

    Ok(nudged)
}

fn refresh_task_counts(state: &mut ServerState) -> Result<()> {
    let Some(tasks_file) = &state.tasks_file else {
        return Ok(());
    };

    let tasks = load_tasks(tasks_file)?;
    let mut counts = HashMap::new();

    for window in &state.config.windows {
        for worker in &window.workers {
            let lane = worker.lane.clone().unwrap_or_else(|| worker.id.clone());
            let lane_counts = counts_for_lane(&tasks, &lane);
            counts.insert(lane, lane_counts);
        }
    }

    state.task_counts = counts;
    Ok(())
}

fn broadcast_state(state: &ServerState, clients: &mut Vec<ClientHandle>) {
    let message = ServerMessage::State { state: build_state(state) };
    broadcast(clients, message);
}

fn broadcast(clients: &mut Vec<ClientHandle>, message: ServerMessage) {
    clients.retain(|client| client.sender.send(message.clone()).is_ok());
}

fn build_state(state: &ServerState) -> AppState {
    let project_name = state
        .project_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("hive")
        .to_string();

    AppState {
        project_name,
        backend: state.config.workers.backend,
        layout_mode: state.layout_mode,
        panes: state
            .panes
            .iter()
            .map(|pane| PaneInfo {
                id: pane.id.clone(),
                pane_type: pane.pane_type.clone(),
                lane: pane.lane.clone(),
                branch: pane.branch.clone(),
                group: pane.group.clone(),
                visible: pane.visible,
            })
            .collect(),
        windows: state
            .windows
            .iter()
            .map(|window| WindowInfo {
                name: window.name.clone(),
                layout: window.layout,
                pane_indices: window.pane_indices.clone(),
            })
            .collect(),
        task_counts: state.task_counts.clone(),
    }
}

fn send_replay(state: &ServerState, client: &ClientHandle) {
    for pane in &state.panes {
        if !pane.raw_history.is_empty() {
            let data: Vec<u8> = pane.raw_history.iter().copied().collect();
            let _ = client.sender.send(ServerMessage::Output {
                pane_id: pane.id.clone(),
                data,
            });
        }
    }
}

fn socket_path(project_dir: &Path) -> PathBuf {
    project_dir.join(".hive").join("hive.sock")
}

fn prepare_socket(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn cleanup_socket(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn write_pid(project_dir: &Path) -> Result<()> {
    let pid_path = project_dir.join(".hive").join("hive.pid");
    std::fs::write(pid_path, std::process::id().to_string())?;
    Ok(())
}

fn load_layout_mode(project_dir: &Path) -> Result<LayoutMode> {
    let path = project_dir.join(".hive").join("layout-mode");
    if !path.exists() {
        return Ok(LayoutMode::Default);
    }
    let content = std::fs::read_to_string(path)?;
    match content.trim() {
        "custom" => Ok(LayoutMode::Custom),
        _ => Ok(LayoutMode::Default),
    }
}

fn write_layout_mode(project_dir: &Path, mode: LayoutMode) -> Result<()> {
    let path = project_dir.join(".hive").join("layout-mode");
    let value = match mode {
        LayoutMode::Default => "default",
        LayoutMode::Custom => "custom",
    };
    std::fs::write(path, value)?;
    Ok(())
}

fn log_line(path: &Path, line: &str) {
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", line);
    }
}
