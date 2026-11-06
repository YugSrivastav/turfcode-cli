use serde::{Deserialize, Serialize};

/// Daemon runtime and health status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub pid: u32,
    pub uptime_secs: u64,
    pub active_sessions: usize,
    pub active_panes: usize,
    pub version: String,
    pub socket_path: String,
}

/// Request commands dispatched to the TurfCode background daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonRequest {
    /// Query background daemon health and status (`server.status`).
    Status,
    /// Attach interactive client to a session (`session.attach`).
    AttachSession { id: String },
    /// Detach interactive client from a session without killing processes (`session.detach`).
    DetachSession { id: String },
    /// Spawn a new PTY pane inside a tab (`pane.spawn`).
    SpawnPane {
        tab_id: usize,
        #[serde(default)]
        cwd: Option<String>,
        #[serde(default)]
        command: Option<String>,
    },
    /// Write raw input bytes to a running PTY pane (`pane.write`).
    WritePane { pane_id: usize, data: Vec<u8> },
    /// Read the current terminal screen buffer text of a pane (`pane.read`).
    ReadPane { pane_id: usize },
    /// Resize a pane's PTY and screen buffer (`pane.resize`).
    ResizePane {
        pane_id: usize,
        cols: u16,
        rows: u16,
    },
    /// Split a pane vertically or horizontally (`pane.split`).
    SplitPane {
        pane_id: usize,
        direction: String,
    },
    /// Terminate and close a pane (`pane.close`).
    ClosePane { pane_id: usize },
    /// List all known workspaces (`workspace.list`).
    ListWorkspaces,
    /// Gracefully shutdown the daemon process (`server.shutdown`).
    Shutdown,
}

impl DaemonRequest {
    /// Return the canonical JSON-RPC method name for this request.
    pub fn method_name(&self) -> &'static str {
        match self {
            Self::Status => "server.status",
            Self::AttachSession { .. } => "session.attach",
            Self::DetachSession { .. } => "session.detach",
            Self::SpawnPane { .. } => "pane.spawn",
            Self::WritePane { .. } => "pane.write",
            Self::ReadPane { .. } => "pane.read",
            Self::ResizePane { .. } => "pane.resize",
            Self::SplitPane { .. } => "pane.split",
            Self::ClosePane { .. } => "pane.close",
            Self::ListWorkspaces => "workspace.list",
            Self::Shutdown => "server.shutdown",
        }
    }
}

/// Responses returned by the TurfCode background daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonResponse {
    /// Result of a status query.
    StatusResult(DaemonStatus),
    /// Acknowledgement of a newly spawned pane.
    PaneSpawned { pane_id: usize },
    /// Buffer content from a pane read.
    PaneReadResult { data: Vec<u8> },
    /// Generic success acknowledgement.
    Success,
    /// Error notification with description.
    Error { message: String },
    /// List of workspaces.
    WorkspacesResult { workspaces: Vec<String> },
    /// Confirmation of session attachment.
    SessionAttached {
        id: String,
        #[serde(default)]
        panes: Vec<usize>,
    },
}

impl DaemonResponse {
    /// Check if this response indicates success.
    pub fn is_success(&self) -> bool {
        !matches!(self, DaemonResponse::Error { .. })
    }

    /// Retrieve error message if response is an error.
    pub fn error_message(&self) -> Option<&str> {
        match self {
            DaemonResponse::Error { message } => Some(message.as_str()),
            _ => None,
        }
    }
}

/// Asynchronous stream events broadcast by the background daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DaemonEvent {
    /// Streaming output chunk from a running PTY.
    PaneOutput { pane_id: usize, bytes: Vec<u8> },
    /// Notification that a pane process exited.
    PaneExited {
        pane_id: usize,
        exit_code: Option<i32>,
    },
    /// State change notification across sessions or workspaces.
    StateDelta { description: String },
}

/// Top-level IPC wire envelope for length-prefixed framing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum IpcMessage {
    /// RPC Request from client to server.
    Request { id: u64, request: DaemonRequest },
    /// RPC Response from server to client.
    Response { id: u64, response: DaemonResponse },
    /// Unsolicited server event (PTY output, process exits, state changes).
    Event { event: DaemonEvent },
}

/// Encode an IPC message into a 4-byte length-prefixed frame.
pub fn encode_frame(msg: &IpcMessage) -> Result<Vec<u8>, std::io::Error> {
    let payload = serde_json::to_vec(msg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let len = payload.len() as u32;
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

/// Decode an IPC message from an un-framed JSON payload slice.
pub fn decode_payload(payload: &[u8]) -> Result<IpcMessage, std::io::Error> {
    serde_json::from_slice(payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Decode a length-prefixed frame from a continuous byte buffer.
/// Returns `Ok(Some((message, consumed_bytes)))` if a complete frame is available.
/// Returns `Ok(None)` if more bytes are needed.
pub fn decode_frame(buf: &[u8]) -> Result<Option<(IpcMessage, usize)>, std::io::Error> {
    if buf.len() < 4 {
        return Ok(None);
    }
    let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if len > 32 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "IPC frame length exceeds maximum allowed limit (32MB)",
        ));
    }
    let total_len = 4 + len;
    if buf.len() < total_len {
        return Ok(None);
    }
    let msg = decode_payload(&buf[4..total_len])?;
    Ok(Some((msg, total_len)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_message_framing_roundtrip() {
        let req = IpcMessage::Request {
            id: 42,
            request: DaemonRequest::SpawnPane {
                tab_id: 1,
                cwd: Some("/tmp".to_string()),
                command: Some("sh".to_string()),
            },
        };

        let encoded = encode_frame(&req).expect("Failed to encode frame");
        assert!(encoded.len() > 4);

        let decoded = decode_frame(&encoded)
            .expect("Failed to decode frame")
            .expect("Expected complete frame");

        assert_eq!(decoded.0, req);
        assert_eq!(decoded.1, encoded.len());
    }

    #[test]
    fn test_event_framing_roundtrip() {
        let event_msg = IpcMessage::Event {
            event: DaemonEvent::PaneOutput {
                pane_id: 7,
                bytes: b"Hello Turf Daemon\r\n".to_vec(),
            },
        };

        let encoded = encode_frame(&event_msg).unwrap();
        let (decoded, consumed) = decode_frame(&encoded).unwrap().unwrap();
        assert_eq!(decoded, event_msg);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_daemon_status_response_roundtrip() {
        let resp = IpcMessage::Response {
            id: 1,
            response: DaemonResponse::StatusResult(DaemonStatus {
                pid: 1234,
                uptime_secs: 120,
                active_sessions: 2,
                active_panes: 4,
                version: "0.1.0".to_string(),
                socket_path: "/tmp/turf.sock".to_string(),
            }),
        };

        let encoded = encode_frame(&resp).unwrap();
        let (decoded, _) = decode_frame(&encoded).unwrap().unwrap();
        assert_eq!(decoded, resp);
    }

    #[test]
    fn test_partial_frame_returns_none() {
        let msg = IpcMessage::Request {
            id: 1,
            request: DaemonRequest::Status,
        };
        let encoded = encode_frame(&msg).unwrap();
        assert_eq!(decode_frame(&encoded[..3]).unwrap(), None);
        assert_eq!(decode_frame(&encoded[..encoded.len() - 1]).unwrap(), None);
    }

    #[test]
    fn test_session_attached_response_with_panes_roundtrip() {
        let resp = IpcMessage::Response {
            id: 2,
            response: DaemonResponse::SessionAttached {
                id: "TURF-1234".to_string(),
                panes: vec![1, 2, 3],
            },
        };
        let encoded = encode_frame(&resp).unwrap();
        let (decoded, _) = decode_frame(&encoded).unwrap().unwrap();
        assert_eq!(decoded, resp);

        // Verify backward-compatibility: missing "panes" field decodes to empty vec
        let json_without_panes = r#"{"kind":"Response","id":3,"response":{"type":"SessionAttached","id":"TURF-5678"}}"#;
        let decoded_msg: IpcMessage = serde_json::from_str(json_without_panes).unwrap();
        if let IpcMessage::Response { response: DaemonResponse::SessionAttached { id, panes }, .. } = decoded_msg {
            assert_eq!(id, "TURF-5678");
            assert!(panes.is_empty());
        } else {
            panic!("Unexpected decoded message variant");
        }
    }
}
