use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{broadcast, mpsc, watch, Mutex};
use turf_protocol::ipc::{
    DaemonEvent, DaemonRequest, DaemonResponse, DaemonStatus, IpcMessage, SessionInfo,
};
use turf_pty::{PtyOptions, PtyProcessHandle, PtySession, PtyWriter};
use turf_vte::Terminal;

use crate::socket::{bind_listener, recv_ipc_message, send_ipc_message};

/// Descriptor for a multiplexer session managed by the daemon.
#[derive(Debug, Clone)]
pub struct DaemonSession {
    pub id: String,
    pub name: String,
    pub created_at: Instant,
    pub created_at_unix: u64,
    pub tab_ids: Vec<usize>,
    pub pane_ids: Vec<usize>,
}

/// Type alias for multiplexer session state.
pub type SessionState = DaemonSession;

/// A running PTY terminal pane managed by the daemon.
pub struct DaemonPane {
    pub id: usize,
    pub session_name: String,
    pub tab_id: usize,
    pub terminal: Arc<Mutex<Terminal>>,
    pub pty_writer: PtyWriter,
    pub pty_handle: Arc<Mutex<PtyProcessHandle>>,
    pub is_alive: Arc<AtomicBool>,
    pub exit_code: Arc<Mutex<Option<i32>>>,
    pub created_at: Instant,
}

struct ServerState {
    socket_path: PathBuf,
    start_time: Instant,
    sessions: Mutex<HashMap<String, DaemonSession>>,
    panes: Mutex<HashMap<usize, DaemonPane>>,
    workspaces: Mutex<Vec<String>>,
    next_pane_id: AtomicUsize,
    next_tab_id: AtomicUsize,
    event_tx: broadcast::Sender<DaemonEvent>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
}

/// Persistent background daemon server running PTY processes and managing multiplexer state.
#[derive(Clone)]
pub struct TurfDaemonServer {
    state: Arc<ServerState>,
}

impl TurfDaemonServer {
    /// Create a new daemon server binding to the specified socket path.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        let (event_tx, _) = broadcast::channel(2048);
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let path = socket_path.into();

        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut initial_sessions = HashMap::new();
        initial_sessions.insert(
            "default".to_string(),
            DaemonSession {
                id: "default".to_string(),
                name: "default".to_string(),
                created_at: Instant::now(),
                created_at_unix: now_unix,
                tab_ids: vec![1],
                pane_ids: Vec::new(),
            },
        );

        let state = ServerState {
            socket_path: path,
            start_time: Instant::now(),
            sessions: Mutex::new(initial_sessions),
            panes: Mutex::new(HashMap::new()),
            workspaces: Mutex::new(vec!["default".to_string()]),
            next_pane_id: AtomicUsize::new(1),
            next_tab_id: AtomicUsize::new(2),
            event_tx,
            shutdown_tx,
            shutdown_rx,
        };

        Self {
            state: Arc::new(state),
        }
    }

    /// Retrieve the configured socket path.
    pub fn socket_path(&self) -> &Path {
        &self.state.socket_path
    }

    /// Event broadcast subscriber for daemon-internal listeners.
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.state.event_tx.subscribe()
    }

    /// Allocate the next tab ID for a multiplexer session.
    pub fn next_tab_id(&self) -> usize {
        self.state.next_tab_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Run the daemon server event and accept loop until shutdown signal.
    pub async fn run(&self) -> anyhow::Result<()> {
        let listener = bind_listener(&self.state.socket_path).await?;
        tracing::info!(
            "TurfCode daemon listening at '{}' (PID: {})",
            self.state.socket_path.display(),
            std::process::id()
        );

        let mut shutdown_rx = self.state.shutdown_rx.clone();

        loop {
            tokio::select! {
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, _)) => {
                            let server_clone = self.clone();
                            tokio::spawn(async move {
                                if let Err(e) = server_clone.handle_client_connection(stream).await {
                                    tracing::debug!("Client connection terminated: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("Daemon listener accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        tracing::info!("TurfCode daemon received shutdown signal, shutting down gracefully.");
                        break;
                    }
                }
            }
        }

        // Clean up socket file on exit
        if self.state.socket_path.exists() {
            let _ = std::fs::remove_file(&self.state.socket_path);
        }

        Ok(())
    }

    /// Handle a single incoming client connection stream.
    async fn handle_client_connection(
        &self,
        stream: crate::socket::SocketStream,
    ) -> std::io::Result<()> {
        let (mut read_half, mut write_half) = tokio::io::split(stream);

        // Client outgoing channel to serialize responses and asynchronous stream events
        let (client_out_tx, mut client_out_rx) = mpsc::channel::<IpcMessage>(512);

        // Writer task sending frames to client socket
        let writer_handle = tokio::spawn(async move {
            while let Some(msg) = client_out_rx.recv().await {
                if send_ipc_message(&mut write_half, &msg).await.is_err() {
                    break;
                }
            }
        });

        // Forward daemon broadcast events to this client
        let mut event_sub = self.state.event_tx.subscribe();
        let event_out_tx = client_out_tx.clone();
        let event_forward_handle = tokio::spawn(async move {
            while let Ok(event) = event_sub.recv().await {
                if event_out_tx
                    .send(IpcMessage::Event { event })
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Main request handling loop
        while let Ok(msg) = recv_ipc_message(&mut read_half).await {
            match msg {
                IpcMessage::Request { id, request } => {
                    let response = self.handle_request(request).await;
                    if client_out_tx
                        .send(IpcMessage::Response { id, response })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                _ => {
                    // Clients only send requests
                }
            }
        }

        // Client disconnected. Abort transport tasks.
        // NOTE: Running child processes and sessions are PRESERVED!
        writer_handle.abort();
        event_forward_handle.abort();
        tracing::debug!("Client disconnected. Preserving active daemon sessions and panes.");

        Ok(())
    }

    /// Dispatch and execute an RPC request from a client.
    pub async fn handle_request(&self, request: DaemonRequest) -> DaemonResponse {
        match request {
            DaemonRequest::Status => {
                let uptime_secs = self.state.start_time.elapsed().as_secs();
                let active_sessions = self.state.sessions.lock().await.len();
                let active_panes = self.state.panes.lock().await.len();
                DaemonResponse::StatusResult(DaemonStatus {
                    pid: std::process::id(),
                    uptime_secs,
                    active_sessions,
                    active_panes,
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    socket_path: self.state.socket_path.to_string_lossy().to_string(),
                })
            }
            DaemonRequest::AttachSession { id } => {
                let now_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let mut sessions = self.state.sessions.lock().await;
                let session = sessions.entry(id.clone()).or_insert_with(|| DaemonSession {
                    id: id.clone(),
                    name: id.clone(),
                    created_at: Instant::now(),
                    created_at_unix: now_unix,
                    tab_ids: vec![1],
                    pane_ids: Vec::new(),
                });
                let panes = self.state.panes.lock().await;
                session.pane_ids.retain(|pid| panes.contains_key(pid));
                if session.pane_ids.is_empty() && !panes.is_empty() {
                    let mut all_pids: Vec<usize> = panes.keys().copied().collect();
                    all_pids.sort_unstable();
                    session.pane_ids = all_pids;
                }
                let active_panes = session.pane_ids.clone();
                DaemonResponse::SessionAttached {
                    id,
                    panes: active_panes,
                }
            }
            DaemonRequest::DetachSession { id: _ } => DaemonResponse::Success,
            DaemonRequest::ListSessions => {
                let sessions = self.state.sessions.lock().await;
                let panes = self.state.panes.lock().await;
                let mut list = Vec::new();
                for (key, sess) in sessions.iter() {
                    let pane_count = panes
                        .values()
                        .filter(|p| {
                            p.session_name == *key
                                || p.session_name == sess.name
                                || sess.pane_ids.contains(&p.id)
                        })
                        .count();
                    list.push(SessionInfo {
                        id: sess.id.clone(),
                        name: sess.name.clone(),
                        pane_count,
                        created_at_unix: sess.created_at_unix,
                    });
                }
                list.sort_by(|a, b| {
                    a.created_at_unix
                        .cmp(&b.created_at_unix)
                        .then_with(|| a.name.cmp(&b.name))
                });
                DaemonResponse::SessionsResult { sessions: list }
            }
            DaemonRequest::CreateSession { name } => {
                let now_unix = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                {
                    let mut sessions = self.state.sessions.lock().await;
                    if sessions.contains_key(&name) {
                        return DaemonResponse::Error {
                            message: format!("Session '{name}' already exists"),
                        };
                    }
                    sessions.insert(
                        name.clone(),
                        DaemonSession {
                            id: name.clone(),
                            name: name.clone(),
                            created_at: Instant::now(),
                            created_at_unix: now_unix,
                            tab_ids: vec![1],
                            pane_ids: Vec::new(),
                        },
                    );
                }

                // Spawn an initial pane for the detached named session
                let spawn_res = self
                    .spawn_pane_internal(Some(name.clone()), 1, None, None)
                    .await;
                let pane_count = match spawn_res {
                    Ok(_) => 1,
                    Err(e) => {
                        tracing::warn!("Failed to spawn initial pane for session {name}: {e}");
                        0
                    }
                };

                let session_info = SessionInfo {
                    id: name.clone(),
                    name,
                    pane_count,
                    created_at_unix: now_unix,
                };
                DaemonResponse::SessionCreated {
                    session: session_info,
                }
            }
            DaemonRequest::KillSession { name } => {
                let mut sessions = self.state.sessions.lock().await;
                let session_key = if sessions.contains_key(&name) {
                    Some(name.clone())
                } else {
                    sessions
                        .iter()
                        .find(|(_, s)| s.name == name || s.id == name)
                        .map(|(k, _)| k.clone())
                };

                if let Some(key) = session_key {
                    let session = sessions.remove(&key).unwrap();
                    drop(sessions);

                    // Terminate and reap all child PTYs belonging to this session
                    let mut panes = self.state.panes.lock().await;
                    let pids_to_kill: Vec<usize> = panes
                        .iter()
                        .filter(|(pid, p)| {
                            session.pane_ids.contains(pid)
                                || p.session_name == key
                                || p.session_name == session.name
                        })
                        .map(|(pid, _)| *pid)
                        .collect();

                    for pid in pids_to_kill {
                        if let Some(pane) = panes.remove(&pid) {
                            let mut handle = pane.pty_handle.lock().await;
                            let _ = handle.kill();
                        }
                    }

                    DaemonResponse::Success
                } else {
                    DaemonResponse::Error {
                        message: format!("Session '{name}' not found"),
                    }
                }
            }
            DaemonRequest::SpawnPane {
                tab_id,
                cwd,
                command,
            } => match self.spawn_pane_internal(None, tab_id, cwd, command).await {
                Ok(pane_id) => DaemonResponse::PaneSpawned { pane_id },
                Err(e) => DaemonResponse::Error {
                    message: format!("Failed to spawn pane: {e}"),
                },
            },
            DaemonRequest::WritePane { pane_id, data } => {
                let panes = self.state.panes.lock().await;
                if let Some(pane) = panes.get(&pane_id) {
                    if let Err(e) = pane.pty_writer.write_bytes_async(&data).await {
                        DaemonResponse::Error {
                            message: format!("PTY write failed: {e}"),
                        }
                    } else {
                        DaemonResponse::Success
                    }
                } else {
                    DaemonResponse::Error {
                        message: format!("Pane ID {pane_id} not found"),
                    }
                }
            }
            DaemonRequest::PaneSendKeys { pane_id, keys } => {
                let panes = self.state.panes.lock().await;
                if let Some(pane) = panes.get(&pane_id) {
                    let data = parse_keys(&keys);
                    if let Err(e) = pane.pty_writer.write_bytes_async(&data).await {
                        DaemonResponse::Error {
                            message: format!("PTY write failed: {e}"),
                        }
                    } else {
                        DaemonResponse::Success
                    }
                } else {
                    DaemonResponse::Error {
                        message: format!("Pane ID {pane_id} not found"),
                    }
                }
            }
            DaemonRequest::ReadPane { pane_id } => {
                let panes = self.state.panes.lock().await;
                if let Some(pane) = panes.get(&pane_id) {
                    let term = pane.terminal.lock().await;
                    let text = term.text();
                    DaemonResponse::PaneReadResult {
                        data: text.into_bytes(),
                    }
                } else {
                    DaemonResponse::Error {
                        message: format!("Pane ID {pane_id} not found"),
                    }
                }
            }
            DaemonRequest::PaneReadLines { pane_id, lines } => {
                let panes = self.state.panes.lock().await;
                if let Some(pane) = panes.get(&pane_id) {
                    let term = pane.terminal.lock().await;
                    let mut row_lines: Vec<String> =
                        (0..term.rows()).map(|y| term.row_text_trimmed(y)).collect();

                    while let Some(last) = row_lines.last() {
                        if last.is_empty() {
                            row_lines.pop();
                        } else {
                            break;
                        }
                    }

                    let selected = if let Some(n) = lines {
                        let start = row_lines.len().saturating_sub(n);
                        row_lines[start..].to_vec()
                    } else {
                        row_lines
                    };

                    DaemonResponse::PaneLinesResult { lines: selected }
                } else {
                    DaemonResponse::Error {
                        message: format!("Pane ID {pane_id} not found"),
                    }
                }
            }
            DaemonRequest::ResizePane {
                pane_id,
                cols,
                rows,
            } => {
                let panes = self.state.panes.lock().await;
                if let Some(pane) = panes.get(&pane_id) {
                    let mut handle = pane.pty_handle.lock().await;
                    let _ = handle.resize(rows, cols);
                    let mut term = pane.terminal.lock().await;
                    term.resize(rows as usize, cols as usize);
                    DaemonResponse::Success
                } else {
                    DaemonResponse::Error {
                        message: format!("Pane ID {pane_id} not found"),
                    }
                }
            }
            DaemonRequest::SplitPane {
                pane_id,
                direction: _,
            } => {
                let (session_name, tab_id) = {
                    let panes = self.state.panes.lock().await;
                    panes
                        .get(&pane_id)
                        .map(|p| (Some(p.session_name.clone()), p.tab_id))
                        .unwrap_or((None, 1))
                };
                match self
                    .spawn_pane_internal(session_name, tab_id, None, None)
                    .await
                {
                    Ok(new_id) => DaemonResponse::PaneSpawned { pane_id: new_id },
                    Err(e) => DaemonResponse::Error {
                        message: format!("Failed to split pane: {e}"),
                    },
                }
            }
            DaemonRequest::ClosePane { pane_id } => {
                let mut panes = self.state.panes.lock().await;
                if let Some(pane) = panes.remove(&pane_id) {
                    let mut handle = pane.pty_handle.lock().await;
                    let _ = handle.kill();
                    let mut sessions = self.state.sessions.lock().await;
                    if let Some(sess) = sessions.get_mut(&pane.session_name) {
                        sess.pane_ids.retain(|&id| id != pane_id);
                    }
                    for session in sessions.values_mut() {
                        session.pane_ids.retain(|&id| id != pane_id);
                    }
                    DaemonResponse::Success
                } else {
                    DaemonResponse::Error {
                        message: format!("Pane ID {pane_id} not found"),
                    }
                }
            }
            DaemonRequest::ListWorkspaces => {
                let workspaces = self.state.workspaces.lock().await.clone();
                DaemonResponse::WorkspacesResult { workspaces }
            }
            DaemonRequest::Shutdown => {
                tracing::info!("Received Shutdown request via IPC.");
                let _ = self.state.shutdown_tx.send(true);
                DaemonResponse::Success
            }
        }
    }

    /// Internal helper to spawn a child PTY and link it with the background terminal emulator.
    async fn spawn_pane_internal(
        &self,
        session_name: Option<String>,
        tab_id: usize,
        cwd: Option<String>,
        command: Option<String>,
    ) -> anyhow::Result<usize> {
        let pane_id = self.state.next_pane_id.fetch_add(1, Ordering::SeqCst);
        let s_name = session_name.unwrap_or_else(|| "default".to_string());

        let mut options = PtyOptions::new(24, 80);
        if let Some(dir) = cwd {
            options.working_dir = Some(PathBuf::from(dir));
        }
        if let Some(cmd) = command {
            // If command contains whitespace, split or pass to shell
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if !parts.is_empty() {
                options = options.with_command(parts[0]);
                if parts.len() > 1 {
                    options = options.with_args(parts[1..].iter().map(|s| s.to_string()));
                }
            }
        }

        let pty_session = PtySession::spawn(options)?;
        let (mut reader, writer, handle) = pty_session.split();

        let terminal = Arc::new(Mutex::new(Terminal::new(24, 80)));
        let pty_handle = Arc::new(Mutex::new(handle));
        let is_alive = Arc::new(AtomicBool::new(true));
        let exit_code = Arc::new(Mutex::new(None));

        // Background task to read PTY bytes, feed into Terminal, and broadcast output events
        let term_clone = Arc::clone(&terminal);
        let event_tx_clone = self.state.event_tx.clone();
        let is_alive_clone = Arc::clone(&is_alive);
        let exit_code_clone = Arc::clone(&exit_code);
        let handle_clone = Arc::clone(&pty_handle);

        tokio::spawn(async move {
            while let Some(chunk) = reader.recv().await {
                {
                    let mut term = term_clone.lock().await;
                    term.process(&chunk);
                }
                let _ = event_tx_clone.send(DaemonEvent::PaneOutput {
                    pane_id,
                    bytes: chunk,
                });
            }

            // PTY stream ended; process finished
            is_alive_clone.store(false, Ordering::SeqCst);
            let code = {
                let mut h = handle_clone.lock().await;
                match h.wait() {
                    Ok(st) => Some(st.exit_code() as i32),
                    Err(_) => None,
                }
            };
            *exit_code_clone.lock().await = code;
            let _ = event_tx_clone.send(DaemonEvent::PaneExited {
                pane_id,
                exit_code: code,
            });
        });

        let daemon_pane = DaemonPane {
            id: pane_id,
            session_name: s_name.clone(),
            tab_id,
            terminal,
            pty_writer: writer,
            pty_handle,
            is_alive,
            exit_code,
            created_at: Instant::now(),
        };

        self.state.panes.lock().await.insert(pane_id, daemon_pane);

        // Update session's pane_ids if session exists
        {
            let mut sessions = self.state.sessions.lock().await;
            if let Some(session) = sessions.get_mut(&s_name) {
                if !session.pane_ids.contains(&pane_id) {
                    session.pane_ids.push(pane_id);
                }
            }
        }

        Ok(pane_id)
    }

    /// Gracefully signal daemon shutdown.
    pub fn shutdown(&self) {
        let _ = self.state.shutdown_tx.send(true);
    }
}

/// Parse user-supplied keys and shorthand key sequences into terminal input bytes.
pub fn parse_keys(input: &str) -> Vec<u8> {
    match input {
        "Enter" | "ENTER" | "Return" | "RETURN" => vec![b'\n'],
        "Space" | "SPACE" => vec![b' '],
        "Tab" | "TAB" => vec![b'\t'],
        "Backspace" | "BACKSPACE" => vec![0x7f],
        "Escape" | "Esc" | "ESC" => vec![0x1b],
        "Up" | "UP" => vec![0x1b, b'[', b'A'],
        "Down" | "DOWN" => vec![0x1b, b'[', b'B'],
        "Right" | "RIGHT" => vec![0x1b, b'[', b'C'],
        "Left" | "LEFT" => vec![0x1b, b'[', b'D'],
        "C-c" | "Ctrl-C" | "ctrl-c" => vec![0x03],
        "C-d" | "Ctrl-D" | "ctrl-d" => vec![0x04],
        "C-z" | "Ctrl-Z" | "ctrl-z" => vec![0x1a],
        _ => {
            let mut bytes = Vec::new();
            let mut chars = input.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    match chars.peek() {
                        Some('n') => {
                            chars.next();
                            bytes.push(b'\n');
                        }
                        Some('r') => {
                            chars.next();
                            bytes.push(b'\r');
                        }
                        Some('t') => {
                            chars.next();
                            bytes.push(b'\t');
                        }
                        Some('e') => {
                            chars.next();
                            bytes.push(0x1b);
                        }
                        Some('\\') => {
                            chars.next();
                            bytes.push(b'\\');
                        }
                        _ => {
                            bytes.push(b'\\');
                        }
                    }
                } else {
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    bytes.extend_from_slice(s.as_bytes());
                }
            }
            bytes
        }
    }
}
