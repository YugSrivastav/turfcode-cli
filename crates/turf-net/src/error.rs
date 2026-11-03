use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("WebSocket error: {0}")]
    WebSocket(Box<tokio_tungstenite::tungstenite::Error>),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Cryptographic error: {0}")]
    Crypto(String),

    #[error("Handshake error: {0}")]
    Handshake(String),

    #[error("Connection closed")]
    ConnectionClosed,

    #[error("Invalid room code: '{0}'")]
    InvalidRoomCode(String),

    #[error("Peer not found: '{0}'")]
    PeerNotFound(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),
}

impl From<tokio_tungstenite::tungstenite::Error> for NetError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::WebSocket(Box::new(err))
    }
}
