use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::{broadcast, mpsc, watch, Mutex};
use turf_protocol::ipc::{
    DaemonEvent, DaemonRequest, DaemonResponse, DaemonStatus, IpcMessage,
};
use turf_pty::{PtyOptions, PtyProcessHandle, PtySession, PtyWriter};
use turf_vte::Terminal;

use crate::socket::{bind_listener, recv_ipc_message, send_ipc_message};

/// Descriptor for a multiplexer session managed by the daemon.
#[derive(Debug, Clone)]
pub struct DaemonSession {
    pub id: String,
    pub created_at: Instant,
    pub tab_ids: Vec<usize>,
}

/// A running PTY terminal pane managed by the daemon.
pub struct DaemonPane {
    pub id: usize,
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

        let mut initial_sessions = HashMap::new();
        initial_sessions.insert(
            "default".to_string(),
            DaemonSession {
                id: "default".to_string(),
                created_at: Instant::now(),
                tab_ids: vec![1],
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
                if event_out_tx.send(IpcMessage::Event { event }).await.is_err() {
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
                let mut sessions = self.state.sessions.lock().await;
                sessions.entry(id.clone()).or_insert_with(|| DaemonSession {
                    id: id.clone(),
                    created_at: Instant::now(),
                    tab_ids: vec![1],
                });
                DaemonResponse::SessionAttached { id }
            }
            DaemonRequest::DetachSession { id: _ } => DaemonResponse::Success,
            DaemonRequest::SpawnPane {
                tab_id,
                cwd,
                command,
            } => match self.spawn_pane_internal(tab_id, cwd, command).await {
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
                let tab_id = {
                    let panes = self.state.panes.lock().await;
                    panes.get(&pane_id).map(|p| p.tab_id).unwrap_or(1)
                };
                match self.spawn_pane_internal(tab_id, None, None).await {
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
        tab_id: usize,
        cwd: Option<String>,
        command: Option<String>,
    ) -> anyhow::Result<usize> {
        let pane_id = self.state.next_pane_id.fetch_add(1, Ordering::SeqCst);

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
            tab_id,
            terminal,
            pty_writer: writer,
            pty_handle,
            is_alive,
            exit_code,
            created_at: Instant::now(),
        };

        self.state.panes.lock().await.insert(pane_id, daemon_pane);

        Ok(pane_id)
    }

    /// Gracefully signal daemon shutdown.
    pub fn shutdown(&self) {
        let _ = self.state.shutdown_tx.send(true);
    }
}
