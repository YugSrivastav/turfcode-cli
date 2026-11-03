use crate::crypto::EncryptedPayload;
use crate::error::NetError;
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use turf_protocol::{FileLock, TaskCard, TaskStatus};

/// Peer information broadcast across connected session peers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: String,
    pub username: String,
    pub role: String,
    pub joined_at: u64,
}

/// Peer credentials transmitted during encrypted join handshake [MD-019].
/// Never persisted to disk; kept strictly in-memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerCredentials {
    pub username: String,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session_id: Option<String>,
}

/// High-level protocol messages conforming to TurfCode network protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NetMessage {
    /// Initial client handshake request with encrypted credentials.
    Handshake {
        room_code: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        encrypted_payload: Option<EncryptedPayload>,
        username: String,
        #[serde(default)]
        client_version: String,
    },

    /// Host acknowledgement of successful handshake with initial state snapshot.
    HandshakeAck {
        success: bool,
        peer_id: String,
        room_code: String,
        tasks: Vec<TaskCard>,
        locks: Vec<FileLock>,
        peers: Vec<PeerInfo>,
        #[serde(default)]
        active_dev_ports: Vec<u16>,
    },

    /// Host rejection of handshake (e.g. room code mismatch, decryption failure).
    HandshakeError { message: String },

    /// Intent board tasks synchronization state delta.
    TasksSync { tasks: Vec<TaskCard> },

    /// Active file locks synchronization state delta.
    LocksSync { locks: Vec<FileLock> },

    /// Connected peers list synchronization state delta.
    PeersSync { peers: Vec<PeerInfo> },

    /// Broadcast notification that a new peer joined.
    PeerJoined { peer: PeerInfo },

    /// Broadcast notification that a peer disconnected.
    PeerLeft { peer_id: String, username: String },

    /// Peer request to add a new task to the shared Intent Board.
    TaskAdd { task: TaskCard },

    /// Host response to task addition.
    TaskAddAck {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        task: Option<TaskCard>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        error: Option<String>,
    },

    /// Peer request to update an existing task on the shared Intent Board.
    TaskUpdate {
        task_id: String,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        status: Option<TaskStatus>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        assignee: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        target_files: Option<Vec<String>>,
    },

    /// Host response to task update.
    TaskUpdateAck {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        task: Option<TaskCard>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        error: Option<String>,
    },

    /// Peer request to atomically acquire a sequence of file locks.
    LockAcquire {
        owner_id: String,
        task_id: String,
        files: Vec<String>,
    },

    /// Host response to lock acquisition request.
    LockAcquireAck {
        success: bool,
        #[serde(default)]
        locked_files: Vec<FileLock>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        conflict: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        error: Option<String>,
    },

    /// Peer request to release locks for a task.
    LockRelease { task_id: String },

    /// Host response to lock release request.
    LockReleaseAck {
        success: bool,
        #[serde(default)]
        released: Vec<FileLock>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        error: Option<String>,
    },

    /// Keep-alive ping message.
    Ping { timestamp: u64 },

    /// Keep-alive pong response.
    Pong { timestamp: u64 },

    /// Interactive terminal input from a peer.
    TerminalInput { peer_id: String, text: String },

    /// Generic broadcast payload.
    Broadcast {
        sub_type: String,
        payload: serde_json::Value,
    },

    /// Generic error notification.
    Error { message: String },
}

impl NetMessage {
    /// Convert `NetMessage` into a tungstenite WebSocket Text message.
    pub fn to_ws_message(&self) -> Result<WsMessage, NetError> {
        let json = serde_json::to_string(self)?;
        Ok(WsMessage::text(json))
    }

    /// Parse a `NetMessage` from a tungstenite WebSocket message.
    pub fn from_ws_message(msg: WsMessage) -> Result<Self, NetError> {
        match msg {
            WsMessage::Text(text) => {
                let parsed: NetMessage = serde_json::from_str(&text)?;
                Ok(parsed)
            }
            WsMessage::Binary(bin) => {
                let parsed: NetMessage = serde_json::from_slice(&bin)?;
                Ok(parsed)
            }
            WsMessage::Ping(payload) => {
                let ts = if payload.len() >= 8 {
                    u64::from_be_bytes(payload[..8].try_into().unwrap_or_default())
                } else {
                    0
                };
                Ok(NetMessage::Ping { timestamp: ts })
            }
            WsMessage::Pong(payload) => {
                let ts = if payload.len() >= 8 {
                    u64::from_be_bytes(payload[..8].try_into().unwrap_or_default())
                } else {
                    0
                };
                Ok(NetMessage::Pong { timestamp: ts })
            }
            WsMessage::Close(_) => Err(NetError::ConnectionClosed),
            WsMessage::Frame(_) => Err(NetError::OperationFailed(
                "Raw WebSocket frames not supported".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handshake_serialization() {
        let msg = NetMessage::Handshake {
            room_code: "TURF-7A9B".into(),
            encrypted_payload: None,
            username: "Ayush".into(),
            client_version: "0.1.0".into(),
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"HANDSHAKE\""));
        assert!(json.contains("\"room_code\":\"TURF-7A9B\""));

        let deserialized: NetMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(msg, deserialized);
    }

    #[test]
    fn test_ws_message_conversion() {
        let msg = NetMessage::Ping {
            timestamp: 123456789,
        };
        let ws_msg = msg.to_ws_message().unwrap();
        let parsed = NetMessage::from_ws_message(ws_msg).unwrap();
        assert_eq!(msg, parsed);
    }
}
