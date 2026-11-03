use crate::crypto;
use crate::error::NetError;
use crate::protocol::{NetMessage, PeerCredentials, PeerInfo};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, watch, RwLock};
use turf_protocol::{FileLock, LockTable, TaskCard, TaskStatus};

/// Events emitted by `TunnelClient` to notify subscribers of state replication updates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientEvent {
    Connected {
        peer_id: String,
    },
    TasksSync(Vec<TaskCard>),
    LocksSync(Vec<FileLock>),
    PeersSync(Vec<PeerInfo>),
    PeerJoined(PeerInfo),
    PeerLeft {
        peer_id: String,
        username: String,
    },
    TaskAddAck {
        success: bool,
        task: Option<TaskCard>,
        error: Option<String>,
    },
    TaskUpdateAck {
        success: bool,
        task: Option<TaskCard>,
        error: Option<String>,
    },
    LockAcquireAck {
        success: bool,
        locked_files: Vec<FileLock>,
        conflict: Option<String>,
        error: Option<String>,
    },
    LockReleaseAck {
        success: bool,
        released: Vec<FileLock>,
        error: Option<String>,
    },
    TerminalInput {
        peer_id: String,
        text: String,
    },
    Broadcast {
        sub_type: String,
        payload: serde_json::Value,
    },
    Disconnected,
}

/// WebSocket tunnel client connecting to a TurfCode session, performing encrypted handshake,
/// and automatically synchronizing Intent Board tasks, file locks, and peer lists.
pub struct TunnelClient {
    room_code: String,
    username: String,
    role: String,
    auth_token: Option<String>,
    peer_id: Arc<RwLock<Option<String>>>,
    replicated_tasks: Arc<RwLock<Vec<TaskCard>>>,
    replicated_locks: Arc<RwLock<Vec<FileLock>>>,
    connected_peers: Arc<RwLock<Vec<PeerInfo>>>,
    outgoing_tx: Arc<RwLock<Option<mpsc::UnboundedSender<NetMessage>>>>,
    event_tx: broadcast::Sender<ClientEvent>,
    shutdown_tx: Arc<RwLock<Option<watch::Sender<bool>>>>,
    worker_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
    is_connected: Arc<RwLock<bool>>,
}

impl TunnelClient {
    /// Create a new `TunnelClient` with the target room code and username.
    pub fn new(room_code: impl Into<String>, username: impl Into<String>) -> Self {
        let (event_tx, _) = broadcast::channel(128);
        Self {
            room_code: crypto::normalize_room_code(&room_code.into()),
            username: username.into(),
            role: "Peer".into(),
            auth_token: None,
            peer_id: Arc::new(RwLock::new(None)),
            replicated_tasks: Arc::new(RwLock::new(Vec::new())),
            replicated_locks: Arc::new(RwLock::new(Vec::new())),
            connected_peers: Arc::new(RwLock::new(Vec::new())),
            outgoing_tx: Arc::new(RwLock::new(None)),
            event_tx,
            shutdown_tx: Arc::new(RwLock::new(None)),
            worker_task: Arc::new(RwLock::new(None)),
            is_connected: Arc::new(RwLock::new(false)),
        }
    }

    /// Set peer role (e.g. "Peer" or "Agent").
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = role.into();
        self
    }

    /// Set peer auth token for encrypted credential exchange [MD-019].
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Get assigned peer ID once connected.
    pub async fn peer_id(&self) -> Option<String> {
        self.peer_id.read().await.clone()
    }

    /// Get target room code.
    pub fn room_code(&self) -> &str {
        &self.room_code
    }

    /// Get peer username.
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Check if client is currently connected.
    pub async fn is_connected(&self) -> bool {
        *self.is_connected.read().await
    }

    /// Subscribe to live state updates and network events.
    pub fn subscribe(&self) -> broadcast::Receiver<ClientEvent> {
        self.event_tx.subscribe()
    }

    /// Get current replicated tasks snapshot.
    pub async fn tasks(&self) -> Vec<TaskCard> {
        self.replicated_tasks.read().await.clone()
    }

    /// Get current replicated locks snapshot.
    pub async fn locks(&self) -> Vec<FileLock> {
        self.replicated_locks.read().await.clone()
    }

    /// Get current active locks packaged as a `LockTable`.
    pub async fn lock_table(&self) -> LockTable {
        let current_locks = self.locks().await;
        LockTable {
            version: 1,
            updated_at: Utc::now(),
            locks: current_locks,
        }
    }

    /// Get current connected peers snapshot.
    pub async fn peers(&self) -> Vec<PeerInfo> {
        self.connected_peers.read().await.clone()
    }

    /// Connect to the WebSocket endpoint, execute encrypted handshake, and start background state sync.
    pub async fn connect(&mut self, ws_url: &str) -> Result<String, NetError> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url).await?;

        let (mut ws_writer, mut ws_reader) = ws_stream.split();

        // Prepare credentials and perform AES-256-GCM authenticated encryption [MD-019]
        let creds = PeerCredentials {
            username: self.username.clone(),
            role: self.role.clone(),
            auth_token: self.auth_token.clone(),
            session_id: None,
        };

        let encrypted_payload = crypto::encrypt_json(&creds, &self.room_code)?;

        // Send HANDSHAKE request
        let handshake_req = NetMessage::Handshake {
            room_code: self.room_code.clone(),
            encrypted_payload: Some(encrypted_payload),
            username: self.username.clone(),
            client_version: "0.1.0".into(),
        };

        ws_writer.send(handshake_req.to_ws_message()?).await?;

        // Wait for HANDSHAKE_ACK or HANDSHAKE_ERROR with 10s timeout
        let ack_msg = match tokio::time::timeout(Duration::from_secs(10), ws_reader.next()).await {
            Ok(Some(Ok(msg))) => NetMessage::from_ws_message(msg)?,
            Ok(Some(Err(e))) => return Err(e.into()),
            Ok(None) => return Err(NetError::ConnectionClosed),
            Err(_) => {
                return Err(NetError::Handshake(
                    "Handshake response timed out after 10s".into(),
                ))
            }
        };

        let assigned_peer_id = match ack_msg {
            NetMessage::HandshakeAck {
                success,
                peer_id,
                tasks,
                locks,
                peers,
                ..
            } => {
                if !success {
                    return Err(NetError::Handshake("Handshake rejected by host".into()));
                }
                *self.peer_id.write().await = Some(peer_id.clone());
                *self.replicated_tasks.write().await = tasks;
                *self.replicated_locks.write().await = locks;
                *self.connected_peers.write().await = peers;
                *self.is_connected.write().await = true;
                peer_id
            }
            NetMessage::HandshakeError { message } => {
                return Err(NetError::Handshake(message));
            }
            other => {
                return Err(NetError::Handshake(format!(
                    "Unexpected response to handshake: {:?}",
                    other
                )));
            }
        };

        let (outgoing_tx, mut outgoing_rx) = mpsc::unbounded_channel::<NetMessage>();
        *self.outgoing_tx.write().await = Some(outgoing_tx.clone());

        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        *self.shutdown_tx.write().await = Some(shutdown_tx);

        let replicated_tasks = Arc::clone(&self.replicated_tasks);
        let replicated_locks = Arc::clone(&self.replicated_locks);
        let connected_peers = Arc::clone(&self.connected_peers);
        let event_tx = self.event_tx.clone();
        let is_connected = Arc::clone(&self.is_connected);

        let _ = event_tx.send(ClientEvent::Connected {
            peer_id: assigned_peer_id.clone(),
        });

        // Spawn background worker for duplex communication
        let worker_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    outgoing = outgoing_rx.recv() => {
                        match outgoing {
                            Some(msg) => {
                                if let Ok(ws_msg) = msg.to_ws_message() {
                                    if let Err(err) = ws_writer.send(ws_msg).await {
                                        tracing::debug!("Client write error: {err}");
                                        break;
                                    }
                                }
                            }
                            None => break,
                        }
                    }
                    incoming = ws_reader.next() => {
                        match incoming {
                            Some(Ok(raw_msg)) => {
                                match NetMessage::from_ws_message(raw_msg) {
                                    Ok(net_msg) => {
                                        match net_msg {
                                            NetMessage::TasksSync { tasks } => {
                                                *replicated_tasks.write().await = tasks.clone();
                                                let _ = event_tx.send(ClientEvent::TasksSync(tasks));
                                            }
                                            NetMessage::LocksSync { locks } => {
                                                *replicated_locks.write().await = locks.clone();
                                                let _ = event_tx.send(ClientEvent::LocksSync(locks));
                                            }
                                            NetMessage::PeersSync { peers } => {
                                                *connected_peers.write().await = peers.clone();
                                                let _ = event_tx.send(ClientEvent::PeersSync(peers));
                                            }
                                            NetMessage::PeerJoined { peer } => {
                                                {
                                                    let mut peers_lock = connected_peers.write().await;
                                                    if !peers_lock.iter().any(|p| p.id == peer.id) {
                                                        peers_lock.push(peer.clone());
                                                    }
                                                }
                                                let _ = event_tx.send(ClientEvent::PeerJoined(peer));
                                            }
                                            NetMessage::PeerLeft { peer_id, username } => {
                                                {
                                                    let mut peers_lock = connected_peers.write().await;
                                                    peers_lock.retain(|p| p.id != peer_id);
                                                }
                                                let _ = event_tx.send(ClientEvent::PeerLeft { peer_id, username });
                                            }
                                            NetMessage::TaskAddAck { success, task, error } => {
                                                let _ = event_tx.send(ClientEvent::TaskAddAck { success, task, error });
                                            }
                                            NetMessage::TaskUpdateAck { success, task, error } => {
                                                let _ = event_tx.send(ClientEvent::TaskUpdateAck { success, task, error });
                                            }
                                            NetMessage::LockAcquireAck {
                                                success,
                                                locked_files,
                                                conflict,
                                                error,
                                            } => {
                                                let _ = event_tx.send(ClientEvent::LockAcquireAck {
                                                    success,
                                                    locked_files,
                                                    conflict,
                                                    error,
                                                });
                                            }
                                            NetMessage::LockReleaseAck {
                                                success,
                                                released,
                                                error,
                                            } => {
                                                let _ = event_tx.send(ClientEvent::LockReleaseAck {
                                                    success,
                                                    released,
                                                    error,
                                                });
                                            }
                                            NetMessage::Ping { timestamp } => {
                                                let _ = outgoing_tx.send(NetMessage::Pong { timestamp });
                                            }
                                            NetMessage::TerminalInput { peer_id, text } => {
                                                let _ = event_tx.send(ClientEvent::TerminalInput { peer_id, text });
                                            }
                                            NetMessage::Broadcast { sub_type, payload } => {
                                                let _ = event_tx.send(ClientEvent::Broadcast { sub_type, payload });
                                            }
                                            _ => {}
                                        }
                                    }
                                    Err(NetError::ConnectionClosed) => break,
                                    Err(err) => {
                                        tracing::debug!("Malformed incoming message: {err}");
                                    }
                                }
                            }
                            Some(Err(err)) => {
                                tracing::debug!("Client read error: {err}");
                                break;
                            }
                            None => break,
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }

            *is_connected.write().await = false;
            let _ = event_tx.send(ClientEvent::Disconnected);
        });

        *self.worker_task.write().await = Some(worker_handle);
        Ok(assigned_peer_id)
    }

    /// Send a protocol message to the host.
    pub async fn send_message(&self, msg: NetMessage) -> Result<(), NetError> {
        let outgoing_lock = self.outgoing_tx.read().await;
        if let Some(ref tx) = *outgoing_lock {
            tx.send(msg).map_err(|_| NetError::ConnectionClosed)?;
            Ok(())
        } else {
            Err(NetError::ConnectionClosed)
        }
    }

    /// Request host to add a task to the shared Intent Board.
    pub async fn add_task(&self, task: TaskCard) -> Result<(), NetError> {
        self.send_message(NetMessage::TaskAdd { task }).await
    }

    /// Request host to update a task status on the shared Intent Board.
    pub async fn update_task_status(
        &self,
        task_id: &str,
        status: TaskStatus,
    ) -> Result<(), NetError> {
        self.send_message(NetMessage::TaskUpdate {
            task_id: task_id.into(),
            status: Some(status),
            title: None,
            assignee: None,
            target_files: None,
        })
        .await
    }

    /// Request host to atomically acquire a sequence of file locks.
    pub async fn acquire_locks(&self, task_id: &str, files: Vec<String>) -> Result<(), NetError> {
        let owner_id = self.username.clone();
        self.send_message(NetMessage::LockAcquire {
            owner_id,
            task_id: task_id.into(),
            files,
        })
        .await
    }

    /// Request host to release file locks held for a task.
    pub async fn release_locks(&self, task_id: &str) -> Result<(), NetError> {
        self.send_message(NetMessage::LockRelease {
            task_id: task_id.into(),
        })
        .await
    }

    /// Send interactive terminal input text to the host.
    pub async fn send_terminal_input(&self, text: &str) -> Result<(), NetError> {
        let peer_id = self.peer_id().await.unwrap_or_else(|| "unknown".into());
        self.send_message(NetMessage::TerminalInput {
            peer_id,
            text: text.into(),
        })
        .await
    }

    /// Disconnect from the session and abort background sync workers.
    pub async fn disconnect(&self) {
        if let Some(shutdown) = self.shutdown_tx.write().await.take() {
            let _ = shutdown.send(true);
        }
        if let Some(task) = self.worker_task.write().await.take() {
            task.abort();
        }
        *self.is_connected.write().await = false;
        *self.outgoing_tx.write().await = None;
    }
}

impl Drop for TunnelClient {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown_tx.try_write().ok().and_then(|mut g| g.take()) {
            let _ = shutdown.send(true);
        }
        if let Some(task) = self.worker_task.try_write().ok().and_then(|mut g| g.take()) {
            task.abort();
        }
    }
}
