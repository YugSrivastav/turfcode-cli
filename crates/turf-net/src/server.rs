use crate::crypto;
use crate::error::NetError;
use crate::protocol::{NetMessage, PeerCredentials, PeerInfo};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, watch, RwLock};
use tokio_tungstenite::WebSocketStream;
use turf_core::{intent_board::IntentBoard, lock_manager::LockManager};
use turf_protocol::{AcquireResult, FileLock, LockTable, ReleaseResult, TaskCard, TaskStatus};

/// Internal representation of a connected authenticated peer on the host.
pub struct ServerPeer {
    pub id: String,
    pub username: String,
    pub role: String,
    pub joined_at: u64,
    pub tx: mpsc::UnboundedSender<NetMessage>,
}

struct ConnectionContext {
    stream: TcpStream,
    _peer_addr: SocketAddr,
    expected_room_code: String,
    intent_board: Option<Arc<IntentBoard>>,
    lock_manager: Option<Arc<LockManager>>,
    tasks: Arc<RwLock<Vec<TaskCard>>>,
    locks: Arc<RwLock<Vec<FileLock>>>,
    peers: Arc<RwLock<HashMap<String, ServerPeer>>>,
    event_tx: broadcast::Sender<ServerEvent>,
}

/// Events emitted by the `TunnelServer`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    PeerConnected {
        peer_id: String,
        username: String,
        role: String,
    },
    PeerDisconnected {
        peer_id: String,
        username: String,
    },
    TasksChanged(Vec<TaskCard>),
    LocksChanged(Vec<FileLock>),
    PeersChanged(Vec<PeerInfo>),
    TerminalInput {
        peer_id: String,
        text: String,
    },
    Broadcast {
        sub_type: String,
        payload: serde_json::Value,
    },
}

/// WebSocket tunnel server binding on a local port and coordinating
/// encrypted peer handshakes, Intent Board tasks, file locks, and state broadcasting.
pub struct TunnelServer {
    room_code: String,
    host: String,
    port: u16,
    local_addr: Arc<RwLock<Option<SocketAddr>>>,
    intent_board: Option<Arc<IntentBoard>>,
    lock_manager: Option<Arc<LockManager>>,
    tasks: Arc<RwLock<Vec<TaskCard>>>,
    locks: Arc<RwLock<Vec<FileLock>>>,
    peers: Arc<RwLock<HashMap<String, ServerPeer>>>,
    event_tx: broadcast::Sender<ServerEvent>,
    shutdown_tx: Arc<RwLock<Option<watch::Sender<bool>>>>,
    server_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

impl TunnelServer {
    /// Create a new `TunnelServer` with an optional room code.
    /// If no room code is provided, generates a cryptographically secure `TURF-XXXX` code.
    pub fn new(room_code: Option<String>) -> Self {
        let code = room_code.unwrap_or_else(crypto::generate_room_code);
        let (event_tx, _) = broadcast::channel(128);

        Self {
            room_code: crypto::normalize_room_code(&code),
            host: "127.0.0.1".into(),
            port: 0,
            local_addr: Arc::new(RwLock::new(None)),
            intent_board: None,
            lock_manager: None,
            tasks: Arc::new(RwLock::new(Vec::new())),
            locks: Arc::new(RwLock::new(Vec::new())),
            peers: Arc::new(RwLock::new(HashMap::new())),
            event_tx,
            shutdown_tx: Arc::new(RwLock::new(None)),
            server_task: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the binding host (e.g. "127.0.0.1" or "0.0.0.0").
    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.host = host.into();
        self
    }

    /// Set the binding port (0 for an ephemeral port).
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Attach an `IntentBoard` instance for persistent task management.
    pub fn with_intent_board(mut self, board: Arc<IntentBoard>) -> Self {
        self.intent_board = Some(board);
        self
    }

    /// Attach a `LockManager` instance for atomic multi-file locking.
    pub fn with_lock_manager(mut self, manager: Arc<LockManager>) -> Self {
        self.lock_manager = Some(manager);
        self
    }

    /// Get the normalized 6-character room code (e.g. `TURF-7A9B`).
    pub fn room_code(&self) -> &str {
        &self.room_code
    }

    /// Get the bound local socket address if running.
    pub async fn local_addr(&self) -> Option<SocketAddr> {
        *self.local_addr.read().await
    }

    /// Get the local WebSocket endpoint URL (e.g. `ws://127.0.0.1:port`).
    pub async fn ws_url(&self) -> Option<String> {
        self.local_addr().await.map(|addr| format!("ws://{addr}"))
    }

    /// Subscribe to server lifecycle and state change events.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.event_tx.subscribe()
    }

    /// Start the WebSocket server and begin accepting connections.
    pub async fn start(&mut self) -> Result<SocketAddr, NetError> {
        let bind_addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&bind_addr).await?;
        let local_addr = listener.local_addr()?;
        *self.local_addr.write().await = Some(local_addr);

        // Preload tasks and locks from attached core components if available
        if let Some(ib) = &self.intent_board {
            let loaded = ib.get_tasks().await;
            *self.tasks.write().await = loaded;
        }
        if let Some(lm) = &self.lock_manager {
            let loaded = lm.get_locks().await;
            *self.locks.write().await = loaded;
        }

        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        *self.shutdown_tx.write().await = Some(shutdown_tx);

        let room_code = self.room_code.clone();
        let intent_board = self.intent_board.clone();
        let lock_manager = self.lock_manager.clone();
        let tasks = Arc::clone(&self.tasks);
        let locks = Arc::clone(&self.locks);
        let peers = Arc::clone(&self.peers);
        let event_tx = self.event_tx.clone();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_res = listener.accept() => {
                        match accept_res {
                            Ok((stream, peer_addr)) => {
                                let ctx = ConnectionContext {
                                    stream,
                                    _peer_addr: peer_addr,
                                    expected_room_code: room_code.clone(),
                                    intent_board: intent_board.clone(),
                                    lock_manager: lock_manager.clone(),
                                    tasks: Arc::clone(&tasks),
                                    locks: Arc::clone(&locks),
                                    peers: Arc::clone(&peers),
                                    event_tx: event_tx.clone(),
                                };

                                tokio::spawn(async move {
                                    if let Err(err) = Self::handle_connection(ctx).await {
                                        tracing::debug!("Peer connection error: {err}");
                                    }
                                });
                            }
                            Err(err) => {
                                tracing::warn!("TCP accept error: {err}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                }
            }
        });

        *self.server_task.write().await = Some(handle);
        Ok(local_addr)
    }

    /// Handle an individual incoming TCP connection and perform encrypted handshake.
    async fn handle_connection(ctx: ConnectionContext) -> Result<(), NetError> {
        let ConnectionContext {
            stream,
            _peer_addr,
            expected_room_code,
            intent_board,
            lock_manager,
            tasks,
            locks,
            peers,
            event_tx,
        } = ctx;

        let mut ws_stream: WebSocketStream<TcpStream> =
            tokio_tungstenite::accept_async(stream).await?;

        // Handshake timeout: 10 seconds to send a valid encrypted handshake
        let handshake_msg = match tokio::time::timeout(Duration::from_secs(10), ws_stream.next())
            .await
        {
            Ok(Some(Ok(msg))) => NetMessage::from_ws_message(msg)?,
            Ok(Some(Err(e))) => return Err(e.into()),
            Ok(None) => return Err(NetError::ConnectionClosed),
            Err(_) => {
                let err_msg = NetMessage::HandshakeError {
                    message: "Handshake timeout: Authentication payload not received within 10s"
                        .into(),
                };
                let _ = ws_stream.send(err_msg.to_ws_message()?).await;
                let _ = ws_stream.close(None).await;
                return Err(NetError::Handshake("Handshake timeout".into()));
            }
        };

        let (received_room_code, encrypted_payload, fallback_username) = match handshake_msg {
            NetMessage::Handshake {
                room_code,
                encrypted_payload,
                username,
                ..
            } => (room_code, encrypted_payload, username),
            other => {
                let err_msg = NetMessage::HandshakeError {
                    message: format!("Expected HANDSHAKE message, received {:?}", other),
                };
                let _ = ws_stream.send(err_msg.to_ws_message()?).await;
                let _ = ws_stream.close(None).await;
                return Err(NetError::Handshake("Invalid initial message type".into()));
            }
        };

        // Room code validation
        if crypto::normalize_room_code(&received_room_code) != expected_room_code {
            let err_msg = NetMessage::HandshakeError {
                message: format!(
                    "Room code mismatch: expected {}, got {}",
                    expected_room_code, received_room_code
                ),
            };
            let _ = ws_stream.send(err_msg.to_ws_message()?).await;
            let _ = ws_stream.close(None).await;
            return Err(NetError::Handshake("Room code mismatch".into()));
        }

        // Authenticated decryption of encrypted credentials payload [MD-019]
        let credentials = if let Some(payload) = encrypted_payload {
            match crypto::decrypt_json::<PeerCredentials>(&payload, &expected_room_code) {
                Ok(creds) => creds,
                Err(err) => {
                    let err_msg = NetMessage::HandshakeError {
                        message: format!("Handshake payload decryption failed: {err}"),
                    };
                    let _ = ws_stream.send(err_msg.to_ws_message()?).await;
                    let _ = ws_stream.close(None).await;
                    return Err(NetError::Handshake(format!("Decryption failure: {err}")));
                }
            }
        } else {
            PeerCredentials {
                username: fallback_username,
                role: "Peer".into(),
                auth_token: None,
                session_id: None,
            }
        };

        let peer_id = format!("peer_{}", &uuid::Uuid::new_v4().to_string()[..8]);
        let now_millis = Utc::now().timestamp_millis() as u64;

        let peer_info = PeerInfo {
            id: peer_id.clone(),
            username: credentials.username.clone(),
            role: credentials.role.clone(),
            joined_at: now_millis,
        };

        let (peer_tx, mut peer_rx) = mpsc::unbounded_channel::<NetMessage>();

        // Register peer in server peer list
        {
            let mut peers_lock = peers.write().await;
            peers_lock.insert(
                peer_id.clone(),
                ServerPeer {
                    id: peer_id.clone(),
                    username: credentials.username.clone(),
                    role: credentials.role.clone(),
                    joined_at: now_millis,
                    tx: peer_tx.clone(),
                },
            );
        }

        // Snapshot current state for handshake response
        let current_tasks = tasks.read().await.clone();
        let current_locks = locks.read().await.clone();
        let connected_peer_infos: Vec<PeerInfo> = {
            let peers_lock = peers.read().await;
            peers_lock
                .values()
                .map(|p| PeerInfo {
                    id: p.id.clone(),
                    username: p.username.clone(),
                    role: p.role.clone(),
                    joined_at: p.joined_at,
                })
                .collect()
        };

        // Send HandshakeAck to newly joined peer
        let ack = NetMessage::HandshakeAck {
            success: true,
            peer_id: peer_id.clone(),
            room_code: expected_room_code.clone(),
            tasks: current_tasks,
            locks: current_locks,
            peers: connected_peer_infos.clone(),
            active_dev_ports: vec![],
        };
        ws_stream.send(ack.to_ws_message()?).await?;

        // Broadcast PEER_JOINED to other connected peers
        Self::broadcast_to_peers(
            &peers,
            NetMessage::PeerJoined {
                peer: peer_info.clone(),
            },
            Some(&peer_id),
        )
        .await;

        let _ = event_tx.send(ServerEvent::PeerConnected {
            peer_id: peer_id.clone(),
            username: credentials.username.clone(),
            role: credentials.role.clone(),
        });
        let _ = event_tx.send(ServerEvent::PeersChanged(connected_peer_infos));

        // Split WebSocket stream for concurrent full-duplex reading and writing
        let (mut ws_writer, mut ws_reader) = ws_stream.split();

        // Outgoing write loop
        let peer_id_clone = peer_id.clone();
        let write_task = tokio::spawn(async move {
            while let Some(msg) = peer_rx.recv().await {
                match msg.to_ws_message() {
                    Ok(ws_msg) => {
                        if let Err(err) = ws_writer.send(ws_msg).await {
                            tracing::debug!("Failed to send to peer {peer_id_clone}: {err}");
                            break;
                        }
                    }
                    Err(err) => {
                        tracing::error!(
                            "Failed to serialize message for peer {peer_id_clone}: {err}"
                        );
                    }
                }
            }
        });

        // Incoming read loop
        let peer_id_for_read = peer_id.clone();
        let peer_username_for_read = credentials.username.clone();

        while let Some(msg_result) = ws_reader.next().await {
            match msg_result {
                Ok(raw_msg) => {
                    let net_msg = match NetMessage::from_ws_message(raw_msg) {
                        Ok(m) => m,
                        Err(NetError::ConnectionClosed) => break,
                        Err(err) => {
                            tracing::debug!("Malformed WS message from {peer_id_for_read}: {err}");
                            continue;
                        }
                    };

                    match net_msg {
                        NetMessage::Ping { timestamp } => {
                            let _ = peer_tx.send(NetMessage::Pong { timestamp });
                        }
                        NetMessage::TaskAdd { task } => {
                            let added_task = if let Some(ib) = &intent_board {
                                match ib.add_task(task.clone()).await {
                                    Ok(t) => t,
                                    Err(_) => task,
                                }
                            } else {
                                task
                            };

                            let updated_tasks = {
                                let mut tasks_lock = tasks.write().await;
                                tasks_lock.retain(|t| t.id != added_task.id);
                                tasks_lock.push(added_task.clone());
                                tasks_lock.clone()
                            };

                            let _ = peer_tx.send(NetMessage::TaskAddAck {
                                success: true,
                                task: Some(added_task.clone()),
                                error: None,
                            });

                            Self::broadcast_to_peers(
                                &peers,
                                NetMessage::TasksSync {
                                    tasks: updated_tasks.clone(),
                                },
                                None,
                            )
                            .await;

                            let _ = event_tx.send(ServerEvent::TasksChanged(updated_tasks));
                        }
                        NetMessage::TaskUpdate {
                            task_id,
                            status,
                            title,
                            assignee,
                            target_files,
                        } => {
                            let updated_tasks = {
                                let mut tasks_lock = tasks.write().await;
                                let mut target_task = None;
                                for t in tasks_lock.iter_mut() {
                                    if t.id == task_id {
                                        if let Some(s) = status {
                                            t.status = s;
                                        }
                                        if let Some(ref tit) = title {
                                            t.title = tit.clone();
                                        }
                                        if let Some(ref a) = assignee {
                                            t.assignee = a.clone();
                                        }
                                        if let Some(ref tf) = target_files {
                                            t.target_files = tf.clone();
                                        }
                                        target_task = Some(t.clone());
                                        break;
                                    }
                                }
                                target_task
                            };

                            if let Some(ib) = &intent_board {
                                if let Some(ref s) = status {
                                    let _ = ib.update_task_status(&task_id, *s).await;
                                }
                            }

                            let all_tasks = tasks.read().await.clone();
                            let _ = peer_tx.send(NetMessage::TaskUpdateAck {
                                success: updated_tasks.is_some(),
                                task: updated_tasks,
                                error: None,
                            });

                            Self::broadcast_to_peers(
                                &peers,
                                NetMessage::TasksSync {
                                    tasks: all_tasks.clone(),
                                },
                                None,
                            )
                            .await;

                            let _ = event_tx.send(ServerEvent::TasksChanged(all_tasks));
                        }
                        NetMessage::LockAcquire {
                            owner_id,
                            task_id,
                            files,
                        } => {
                            let acquire_res = if let Some(lm) = &lock_manager {
                                match lm
                                    .acquire_atomic_sequence(&owner_id, &task_id, &files, 30_000)
                                    .await
                                {
                                    Ok(res) => res,
                                    Err(err) => {
                                        AcquireResult::conflict("", "", "", err.to_string())
                                    }
                                }
                            } else {
                                // In-memory atomic sequence acquisition
                                let mut locks_lock = locks.write().await;
                                let now = Utc::now();
                                locks_lock.retain(|l| !l.is_expired(now));

                                let mut conflict = None;
                                for f in &files {
                                    if let Some(existing) =
                                        locks_lock.iter().find(|l| l.file_path == *f)
                                    {
                                        conflict = Some(AcquireResult::conflict(
                                            f.clone(),
                                            existing.owner_id.clone(),
                                            existing.task_id.clone(),
                                            format!(
                                                "File '{f}' is already locked by {}",
                                                existing.owner_id
                                            ),
                                        ));
                                        break;
                                    }
                                }

                                if let Some(conf) = conflict {
                                    conf
                                } else {
                                    let mut new_locks = Vec::new();
                                    for f in files {
                                        let lock =
                                            FileLock::new(f, &owner_id, &task_id, 30_000, None);
                                        locks_lock.push(lock.clone());
                                        new_locks.push(lock);
                                    }
                                    AcquireResult::success(new_locks)
                                }
                            };

                            let current_locks = if let Some(lm) = &lock_manager {
                                lm.get_locks().await
                            } else {
                                locks.read().await.clone()
                            };

                            let _ = peer_tx.send(NetMessage::LockAcquireAck {
                                success: acquire_res.is_success(),
                                locked_files: acquire_res.locks.clone(),
                                conflict: acquire_res.conflict_file.clone(),
                                error: acquire_res.error.clone(),
                            });

                            if acquire_res.is_success() {
                                Self::broadcast_to_peers(
                                    &peers,
                                    NetMessage::LocksSync {
                                        locks: current_locks.clone(),
                                    },
                                    None,
                                )
                                .await;
                                let _ = event_tx.send(ServerEvent::LocksChanged(current_locks));
                            }
                        }
                        NetMessage::LockRelease { task_id } => {
                            let release_res = if let Some(lm) = &lock_manager {
                                match lm.release_locks(&task_id).await {
                                    Ok(res) => res,
                                    Err(_) => ReleaseResult {
                                        success: false,
                                        released: vec![],
                                    },
                                }
                            } else {
                                let mut locks_lock = locks.write().await;
                                let mut released = Vec::new();
                                locks_lock.retain(|l| {
                                    if l.task_id == task_id {
                                        released.push(l.clone());
                                        false
                                    } else {
                                        true
                                    }
                                });
                                ReleaseResult {
                                    success: true,
                                    released,
                                }
                            };

                            let current_locks = if let Some(lm) = &lock_manager {
                                lm.get_locks().await
                            } else {
                                locks.read().await.clone()
                            };

                            let _ = peer_tx.send(NetMessage::LockReleaseAck {
                                success: release_res.success,
                                released: release_res.released,
                                error: None,
                            });

                            Self::broadcast_to_peers(
                                &peers,
                                NetMessage::LocksSync {
                                    locks: current_locks.clone(),
                                },
                                None,
                            )
                            .await;
                            let _ = event_tx.send(ServerEvent::LocksChanged(current_locks));
                        }
                        NetMessage::TerminalInput { text, .. } => {
                            let _ = event_tx.send(ServerEvent::TerminalInput {
                                peer_id: peer_id_for_read.clone(),
                                text,
                            });
                        }
                        NetMessage::Broadcast { sub_type, payload } => {
                            Self::broadcast_to_peers(
                                &peers,
                                NetMessage::Broadcast {
                                    sub_type: sub_type.clone(),
                                    payload: payload.clone(),
                                },
                                Some(&peer_id_for_read),
                            )
                            .await;
                            let _ = event_tx.send(ServerEvent::Broadcast { sub_type, payload });
                        }
                        _ => {}
                    }
                }
                Err(err) => {
                    tracing::debug!("WebSocket read error from {peer_id_for_read}: {err}");
                    break;
                }
            }
        }

        // Clean up disconnected peer
        write_task.abort();
        {
            let mut peers_lock = peers.write().await;
            peers_lock.remove(&peer_id_for_read);
        }

        let remaining_peers: Vec<PeerInfo> = {
            let peers_lock = peers.read().await;
            peers_lock
                .values()
                .map(|p| PeerInfo {
                    id: p.id.clone(),
                    username: p.username.clone(),
                    role: p.role.clone(),
                    joined_at: p.joined_at,
                })
                .collect()
        };

        Self::broadcast_to_peers(
            &peers,
            NetMessage::PeerLeft {
                peer_id: peer_id_for_read.clone(),
                username: peer_username_for_read.clone(),
            },
            None,
        )
        .await;

        let _ = event_tx.send(ServerEvent::PeerDisconnected {
            peer_id: peer_id_for_read,
            username: peer_username_for_read,
        });
        let _ = event_tx.send(ServerEvent::PeersChanged(remaining_peers));

        Ok(())
    }

    /// Helper to broadcast a message to connected peers with optional exclude peer id.
    async fn broadcast_to_peers(
        peers: &Arc<RwLock<HashMap<String, ServerPeer>>>,
        msg: NetMessage,
        exclude_peer_id: Option<&str>,
    ) {
        let peers_lock = peers.read().await;
        for (id, peer) in peers_lock.iter() {
            if let Some(exclude) = exclude_peer_id {
                if id == exclude {
                    continue;
                }
            }
            let _ = peer.tx.send(msg.clone());
        }
    }

    /// Broadcast task updates to all connected peers.
    pub async fn broadcast_tasks(&self, tasks: Vec<TaskCard>) {
        *self.tasks.write().await = tasks.clone();
        Self::broadcast_to_peers(
            &self.peers,
            NetMessage::TasksSync {
                tasks: tasks.clone(),
            },
            None,
        )
        .await;
        let _ = self.event_tx.send(ServerEvent::TasksChanged(tasks));
    }

    /// Broadcast active file locks to all connected peers.
    pub async fn broadcast_locks(&self, locks: Vec<FileLock>) {
        *self.locks.write().await = locks.clone();
        Self::broadcast_to_peers(
            &self.peers,
            NetMessage::LocksSync {
                locks: locks.clone(),
            },
            None,
        )
        .await;
        let _ = self.event_tx.send(ServerEvent::LocksChanged(locks));
    }

    /// Broadcast connected peer list to all connected peers.
    pub async fn broadcast_peers(&self) {
        let peer_infos = self.connected_peers().await;
        Self::broadcast_to_peers(
            &self.peers,
            NetMessage::PeersSync {
                peers: peer_infos.clone(),
            },
            None,
        )
        .await;
        let _ = self.event_tx.send(ServerEvent::PeersChanged(peer_infos));
    }

    /// Broadcast an arbitrary network message to all peers.
    pub async fn broadcast_message(&self, msg: NetMessage, exclude_peer: Option<&str>) {
        Self::broadcast_to_peers(&self.peers, msg, exclude_peer).await;
    }

    /// Get list of connected peers without leaking credentials [MD-019].
    pub async fn connected_peers(&self) -> Vec<PeerInfo> {
        let peers_lock = self.peers.read().await;
        peers_lock
            .values()
            .map(|p| PeerInfo {
                id: p.id.clone(),
                username: p.username.clone(),
                role: p.role.clone(),
                joined_at: p.joined_at,
            })
            .collect()
    }

    /// Add a task locally from host and broadcast state delta to all peers.
    pub async fn add_task(&self, task: TaskCard) -> Result<TaskCard, NetError> {
        let added = if let Some(ib) = &self.intent_board {
            ib.add_task(task)
                .await
                .map_err(|e| NetError::OperationFailed(e.to_string()))?
        } else {
            task
        };

        let mut tasks_lock = self.tasks.write().await;
        tasks_lock.retain(|t| t.id != added.id);
        tasks_lock.push(added.clone());
        let current_tasks = tasks_lock.clone();
        drop(tasks_lock);

        self.broadcast_tasks(current_tasks).await;
        Ok(added)
    }

    /// Update a task status locally from host and broadcast state delta to all peers.
    pub async fn update_task_status(
        &self,
        task_id: &str,
        status: TaskStatus,
    ) -> Result<Option<TaskCard>, NetError> {
        if let Some(ib) = &self.intent_board {
            let _ = ib
                .update_task_status(task_id, status)
                .await
                .map_err(|e| NetError::OperationFailed(e.to_string()))?;
        }

        let mut tasks_lock = self.tasks.write().await;
        let mut updated = None;
        for t in tasks_lock.iter_mut() {
            if t.id == task_id {
                t.status = status;
                updated = Some(t.clone());
                break;
            }
        }
        let current_tasks = tasks_lock.clone();
        drop(tasks_lock);

        if updated.is_some() {
            self.broadcast_tasks(current_tasks).await;
        }
        Ok(updated)
    }

    /// Acquire locks locally from host and broadcast state delta to all peers.
    pub async fn acquire_locks(
        &self,
        owner_id: &str,
        task_id: &str,
        files: Vec<String>,
    ) -> Result<AcquireResult, NetError> {
        let res = if let Some(lm) = &self.lock_manager {
            lm.acquire_atomic_sequence(owner_id, task_id, &files, 30_000)
                .await
                .map_err(|e| NetError::OperationFailed(e.to_string()))?
        } else {
            let mut locks_lock = self.locks.write().await;
            let now = Utc::now();
            locks_lock.retain(|l| !l.is_expired(now));

            let mut conflict = None;
            for f in &files {
                if let Some(existing) = locks_lock.iter().find(|l| l.file_path == *f) {
                    conflict = Some(AcquireResult::conflict(
                        f.clone(),
                        existing.owner_id.clone(),
                        existing.task_id.clone(),
                        format!("File '{f}' is already locked by {}", existing.owner_id),
                    ));
                    break;
                }
            }

            if let Some(conf) = conflict {
                conf
            } else {
                let mut new_locks = Vec::new();
                for f in files {
                    let lock = FileLock::new(f, owner_id, task_id, 30_000, None);
                    locks_lock.push(lock.clone());
                    new_locks.push(lock);
                }
                AcquireResult::success(new_locks)
            }
        };

        if res.is_success() {
            let current_locks = if let Some(lm) = &self.lock_manager {
                lm.get_locks().await
            } else {
                self.locks.read().await.clone()
            };
            self.broadcast_locks(current_locks).await;
        }

        Ok(res)
    }

    /// Release locks locally from host and broadcast state delta to all peers.
    pub async fn release_locks(&self, task_id: &str) -> Result<ReleaseResult, NetError> {
        let res = if let Some(lm) = &self.lock_manager {
            lm.release_locks(task_id)
                .await
                .map_err(|e| NetError::OperationFailed(e.to_string()))?
        } else {
            let mut locks_lock = self.locks.write().await;
            let mut released = Vec::new();
            locks_lock.retain(|l| {
                if l.task_id == task_id {
                    released.push(l.clone());
                    false
                } else {
                    true
                }
            });
            ReleaseResult {
                success: true,
                released,
            }
        };

        let current_locks = if let Some(lm) = &self.lock_manager {
            lm.get_locks().await
        } else {
            self.locks.read().await.clone()
        };
        self.broadcast_locks(current_locks).await;

        Ok(res)
    }

    /// Get snapshot of current tasks.
    pub async fn tasks(&self) -> Vec<TaskCard> {
        if let Some(ib) = &self.intent_board {
            ib.get_tasks().await
        } else {
            self.tasks.read().await.clone()
        }
    }

    /// Get snapshot of current file locks.
    pub async fn locks(&self) -> Vec<FileLock> {
        if let Some(lm) = &self.lock_manager {
            lm.get_locks().await
        } else {
            self.locks.read().await.clone()
        }
    }

    /// Get active locks packaged as a `LockTable`.
    pub async fn lock_table(&self) -> LockTable {
        let current_locks = self.locks().await;
        LockTable {
            version: 1,
            updated_at: Utc::now(),
            locks: current_locks,
        }
    }

    /// Stop the server and terminate background accept tasks.
    pub async fn stop(&self) {
        if let Some(shutdown) = self.shutdown_tx.write().await.take() {
            let _ = shutdown.send(true);
        }
        if let Some(task) = self.server_task.write().await.take() {
            task.abort();
        }
    }
}

impl Drop for TunnelServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self
            .shutdown_tx
            .try_write()
            .ok()
            .and_then(|mut guard| guard.take())
        {
            let _ = shutdown.send(true);
        }
        if let Some(task) = self
            .server_task
            .try_write()
            .ok()
            .and_then(|mut guard| guard.take())
        {
            task.abort();
        }
    }
}
