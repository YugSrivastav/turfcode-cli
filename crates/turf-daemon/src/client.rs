use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use turf_protocol::ipc::{
    DaemonEvent, DaemonRequest, DaemonResponse, DaemonStatus, IpcMessage, PaneInfo, SessionInfo,
};

use crate::socket::{connect_stream, recv_ipc_message, send_ipc_message};

static NEXT_REQ_ID: AtomicU64 = AtomicU64::new(1);

type ResponseSender = oneshot::Sender<Result<DaemonResponse, String>>;
type PendingRequests = Arc<Mutex<HashMap<u64, ResponseSender>>>;

/// IPC client connection handle for communicating with the background TurfCode daemon.
#[derive(Clone)]
pub struct DaemonClient {
    socket_path: PathBuf,
    request_tx: mpsc::Sender<(IpcMessage, ResponseSender)>,
    event_tx: broadcast::Sender<DaemonEvent>,
}

impl DaemonClient {
    /// Connect to a running TurfCode background daemon socket.
    pub async fn connect(socket_path: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let path = socket_path.into();
        let stream = connect_stream(&path).await?;
        let (mut read_half, mut write_half) = tokio::io::split(stream);

        let (request_tx, mut request_rx) = mpsc::channel::<(IpcMessage, ResponseSender)>(128);
        let (event_tx, _) = broadcast::channel::<DaemonEvent>(2048);

        let pending: PendingRequests = Arc::new(Mutex::new(HashMap::new()));

        // Writer task
        let pending_writer = Arc::clone(&pending);
        tokio::spawn(async move {
            while let Some((msg, resp_tx)) = request_rx.recv().await {
                if let IpcMessage::Request { id, .. } = &msg {
                    pending_writer.lock().await.insert(*id, resp_tx);
                    if let Err(e) = send_ipc_message(&mut write_half, &msg).await {
                        if let Some(tx) = pending_writer.lock().await.remove(id) {
                            let _ = tx.send(Err(format!("Failed to send IPC request: {e}")));
                        }
                        break;
                    }
                }
            }
        });

        // Reader task
        let pending_reader = Arc::clone(&pending);
        let event_tx_clone = event_tx.clone();
        tokio::spawn(async move {
            while let Ok(msg) = recv_ipc_message(&mut read_half).await {
                match msg {
                    IpcMessage::Response { id, response } => {
                        if let Some(tx) = pending_reader.lock().await.remove(&id) {
                            let _ = tx.send(Ok(response));
                        }
                    }
                    IpcMessage::Event { event } => {
                        let _ = event_tx_clone.send(event);
                    }
                    _ => {}
                }
            }

            // Connection lost; notify pending callers
            let mut guard = pending_reader.lock().await;
            for (_, tx) in guard.drain() {
                let _ = tx.send(Err("Turf daemon connection closed".to_string()));
            }
        });

        Ok(Self {
            socket_path: path,
            request_tx,
            event_tx,
        })
    }

    /// Retrieve the connected socket path.
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Subscribe to real-time asynchronous daemon events (such as PTY output streams).
    pub fn subscribe(&self) -> broadcast::Receiver<DaemonEvent> {
        self.event_tx.subscribe()
    }

    /// Send an arbitrary RPC request to the daemon and await response.
    pub async fn send_request(&self, request: DaemonRequest) -> anyhow::Result<DaemonResponse> {
        let id = NEXT_REQ_ID.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        let msg = IpcMessage::Request { id, request };

        self.request_tx
            .send((msg, tx))
            .await
            .map_err(|_| anyhow::anyhow!("Daemon client transport closed"))?;

        let response = rx
            .await
            .map_err(|_| anyhow::anyhow!("Daemon request cancelled"))?
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if let DaemonResponse::Error { message } = response {
            anyhow::bail!("{message}");
        }

        Ok(response)
    }

    /// Attempt to send an IPC request synchronously / non-blockingly without waiting for a response.
    pub fn try_send_request(&self, request: DaemonRequest) -> Result<(), String> {
        let id = NEXT_REQ_ID.fetch_add(1, Ordering::SeqCst);
        let (tx, _rx) = oneshot::channel();
        let msg = IpcMessage::Request { id, request };
        self.request_tx
            .try_send((msg, tx))
            .map_err(|e| format!("Failed to send IPC request: {e}"))
    }

    /// Query the daemon health and status (`server.status`).
    pub async fn status(&self) -> anyhow::Result<DaemonStatus> {
        match self.send_request(DaemonRequest::Status).await? {
            DaemonResponse::StatusResult(status) => Ok(status),
            other => anyhow::bail!("Unexpected response to status query: {:?}", other),
        }
    }

    /// Query pane details including alive status and foreground command (`pane.info`).
    pub async fn pane_info(&self, pane_id: usize) -> anyhow::Result<PaneInfo> {
        match self.send_request(DaemonRequest::PaneInfo { pane_id }).await? {
            DaemonResponse::PaneInfoResult(info) => Ok(info),
            other => anyhow::bail!("Unexpected response to pane info query: {:?}", other),
        }
    }

    /// Spawn a new PTY pane (`pane.spawn`).
    pub async fn spawn_pane(
        &self,
        tab_id: usize,
        cwd: Option<String>,
        command: Option<String>,
    ) -> anyhow::Result<usize> {
        match self
            .send_request(DaemonRequest::SpawnPane {
                tab_id,
                cwd,
                command,
            })
            .await?
        {
            DaemonResponse::PaneSpawned { pane_id } => Ok(pane_id),
            other => anyhow::bail!("Unexpected response to spawn pane: {:?}", other),
        }
    }

    /// Write raw byte data to a running pane PTY (`pane.write`).
    pub async fn write_pane(&self, pane_id: usize, data: &[u8]) -> anyhow::Result<()> {
        match self
            .send_request(DaemonRequest::WritePane {
                pane_id,
                data: data.to_vec(),
            })
            .await?
        {
            DaemonResponse::Success => Ok(()),
            other => anyhow::bail!("Unexpected response to write pane: {:?}", other),
        }
    }

    /// Write raw byte data to a running pane PTY non-blockingly (`pane.write`).
    pub fn write_pane_sync(&self, pane_id: usize, data: &[u8]) -> Result<(), String> {
        self.try_send_request(DaemonRequest::WritePane {
            pane_id,
            data: data.to_vec(),
        })
    }

    /// Read the current terminal screen buffer text from a pane (`pane.read`).
    pub async fn read_pane(&self, pane_id: usize) -> anyhow::Result<Vec<u8>> {
        match self
            .send_request(DaemonRequest::ReadPane { pane_id })
            .await?
        {
            DaemonResponse::PaneReadResult { data } => Ok(data),
            other => anyhow::bail!("Unexpected response to read pane: {:?}", other),
        }
    }

    /// Resize a pane (`pane.resize`).
    pub async fn resize_pane(&self, pane_id: usize, cols: u16, rows: u16) -> anyhow::Result<()> {
        match self
            .send_request(DaemonRequest::ResizePane {
                pane_id,
                cols,
                rows,
            })
            .await?
        {
            DaemonResponse::Success => Ok(()),
            other => anyhow::bail!("Unexpected response to resize pane: {:?}", other),
        }
    }

    /// Resize a pane non-blockingly (`pane.resize`).
    pub fn resize_pane_sync(&self, pane_id: usize, cols: u16, rows: u16) -> Result<(), String> {
        self.try_send_request(DaemonRequest::ResizePane {
            pane_id,
            cols,
            rows,
        })
    }

    /// Split a pane (`pane.split`).
    pub async fn split_pane(&self, pane_id: usize, direction: &str) -> anyhow::Result<usize> {
        match self
            .send_request(DaemonRequest::SplitPane {
                pane_id,
                direction: direction.to_string(),
            })
            .await?
        {
            DaemonResponse::PaneSpawned { pane_id: id } => Ok(id),
            DaemonResponse::Success => Ok(pane_id),
            other => anyhow::bail!("Unexpected response to split pane: {:?}", other),
        }
    }

    /// Close and kill a pane process (`pane.close`).
    pub async fn close_pane(&self, pane_id: usize) -> anyhow::Result<()> {
        match self
            .send_request(DaemonRequest::ClosePane { pane_id })
            .await?
        {
            DaemonResponse::Success => Ok(()),
            other => anyhow::bail!("Unexpected response to close pane: {:?}", other),
        }
    }

    /// Attach to a session (`session.attach`). Returns active pane IDs.
    pub async fn attach_session(&self, id: &str) -> anyhow::Result<Vec<usize>> {
        match self
            .send_request(DaemonRequest::AttachSession { id: id.to_string() })
            .await?
        {
            DaemonResponse::SessionAttached { panes, .. } => Ok(panes),
            DaemonResponse::Success => Ok(vec![]),
            other => anyhow::bail!("Unexpected response to attach session: {:?}", other),
        }
    }

    /// Detach from a session (`session.detach`).
    pub async fn detach_session(&self, id: &str) -> anyhow::Result<()> {
        match self
            .send_request(DaemonRequest::DetachSession { id: id.to_string() })
            .await?
        {
            DaemonResponse::Success => Ok(()),
            other => anyhow::bail!("Unexpected response to detach session: {:?}", other),
        }
    }

    /// List active multiplexer sessions (`session.list`).
    pub async fn list_sessions(&self) -> anyhow::Result<Vec<SessionInfo>> {
        match self.send_request(DaemonRequest::ListSessions).await? {
            DaemonResponse::SessionsResult { sessions } => Ok(sessions),
            other => anyhow::bail!("Unexpected response to list sessions: {:?}", other),
        }
    }

    /// Create a new detached named session (`session.create`).
    pub async fn create_session(&self, name: &str) -> anyhow::Result<SessionInfo> {
        match self
            .send_request(DaemonRequest::CreateSession {
                name: name.to_string(),
            })
            .await?
        {
            DaemonResponse::SessionCreated { session } => Ok(session),
            DaemonResponse::Success => Ok(SessionInfo::new(name, name, 0, 0)),
            other => anyhow::bail!("Unexpected response to create session: {:?}", other),
        }
    }

    /// Kill and terminate a named session and reap its child PTYs (`session.kill`).
    pub async fn kill_session(&self, name: &str) -> anyhow::Result<()> {
        match self
            .send_request(DaemonRequest::KillSession {
                name: name.to_string(),
            })
            .await?
        {
            DaemonResponse::Success => Ok(()),
            other => anyhow::bail!("Unexpected response to kill session: {:?}", other),
        }
    }

    /// Send text or keystrokes to a running pane PTY (`pane.send_keys`).
    pub async fn send_pane_keys(&self, pane_id: usize, keys: &str) -> anyhow::Result<()> {
        match self
            .send_request(DaemonRequest::PaneSendKeys {
                pane_id,
                keys: keys.to_string(),
            })
            .await?
        {
            DaemonResponse::Success => Ok(()),
            other => anyhow::bail!("Unexpected response to send pane keys: {:?}", other),
        }
    }

    /// Read terminal text lines from a pane screen buffer (`pane.read_lines`).
    pub async fn read_pane_lines(
        &self,
        pane_id: usize,
        lines: Option<usize>,
    ) -> anyhow::Result<Vec<String>> {
        match self
            .send_request(DaemonRequest::PaneReadLines { pane_id, lines })
            .await?
        {
            DaemonResponse::PaneLinesResult { lines } => Ok(lines),
            DaemonResponse::PaneReadResult { data } => {
                let text = String::from_utf8_lossy(&data);
                let all_lines: Vec<String> = text.lines().map(|s| s.to_string()).collect();
                if let Some(n) = lines {
                    let start = all_lines.len().saturating_sub(n);
                    Ok(all_lines[start..].to_vec())
                } else {
                    Ok(all_lines)
                }
            }
            other => anyhow::bail!("Unexpected response to read pane lines: {:?}", other),
        }
    }

    /// List all known workspaces (`workspace.list`).
    pub async fn list_workspaces(&self) -> anyhow::Result<Vec<String>> {
        match self.send_request(DaemonRequest::ListWorkspaces).await? {
            DaemonResponse::WorkspacesResult { workspaces } => Ok(workspaces),
            other => anyhow::bail!("Unexpected response to list workspaces: {:?}", other),
        }
    }

    /// Request daemon shutdown (`server.shutdown`).
    pub async fn shutdown(&self) -> anyhow::Result<()> {
        match self.send_request(DaemonRequest::Shutdown).await? {
            DaemonResponse::Success => Ok(()),
            other => anyhow::bail!("Unexpected response to shutdown: {:?}", other),
        }
    }
}
