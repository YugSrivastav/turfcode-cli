use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use turf_protocol::ipc::{decode_payload, encode_frame, IpcMessage};

#[cfg(unix)]
use tokio::net::{UnixListener, UnixStream};

/// Default socket path resolution conforming to [MD-051]:
/// 1. `$TURF_SOCKET_PATH` if set and non-empty.
/// 2. `~/.turf/turf.sock` on Unix.
/// 3. `\\.\pipe\turf` on Windows.
pub fn default_socket_path() -> PathBuf {
    if let Ok(val) = std::env::var("TURF_SOCKET_PATH") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }

    if cfg!(windows) {
        return PathBuf::from(r"\\.\pipe\turf");
    }

    if let Ok(home) = std::env::var("HOME") {
        let trimmed = home.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(".turf").join("turf.sock");
        }
    }

    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        let trimmed = userprofile.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join(".turf").join("turf.sock");
        }
    }

    PathBuf::from(".turf").join("turf.sock")
}

#[cfg(unix)]
pub type SocketListener = UnixListener;
#[cfg(unix)]
pub type SocketStream = UnixStream;

/// Bind a socket listener at the given path, automatically clearing stale socket files if dead.
#[cfg(unix)]
pub async fn bind_listener(path: &Path) -> std::io::Result<SocketListener> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if path.exists() {
        // Probe if existing socket is active
        match UnixStream::connect(path).await {
            Ok(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    format!("Turf daemon socket at '{}' is already in use", path.display()),
                ));
            }
            Err(_) => {
                // Socket file exists but no process is listening; clear stale file
                tracing::debug!("Removing stale daemon socket at {}", path.display());
                let _ = std::fs::remove_file(path);
            }
        }
    }

    UnixListener::bind(path)
}

/// Connect to a running daemon socket stream.
#[cfg(unix)]
pub async fn connect_stream(path: &Path) -> std::io::Result<SocketStream> {
    UnixStream::connect(path).await
}

/// Write a length-prefixed IPC message to an asynchronous stream.
pub async fn send_ipc_message<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    msg: &IpcMessage,
) -> std::io::Result<()> {
    let frame = encode_frame(msg)?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

/// Read a length-prefixed IPC message from an asynchronous stream.
pub async fn recv_ipc_message<R: AsyncReadExt + Unpin>(
    reader: &mut R,
) -> std::io::Result<IpcMessage> {
    let mut len_bytes = [0u8; 4];
    reader.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;
    if len > 32 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "IPC message exceeds maximum frame size (32MB)",
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).await?;
    decode_payload(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use turf_protocol::ipc::{DaemonRequest, DaemonResponse};

    #[tokio::test]
    async fn test_socket_bind_and_connect_roundtrip() {
        let dir = tempdir().unwrap();
        let sock_path = dir.path().join("test_roundtrip.sock");

        let listener = bind_listener(&sock_path).await.unwrap();

        let server_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let msg = recv_ipc_message(&mut stream).await.unwrap();
            match msg {
                IpcMessage::Request { id, request } => {
                    assert_eq!(request, DaemonRequest::Status);
                    let resp = IpcMessage::Response {
                        id,
                        response: DaemonResponse::Success,
                    };
                    send_ipc_message(&mut stream, &resp).await.unwrap();
                }
                _ => panic!("Expected request"),
            }
        });

        let mut client_stream = connect_stream(&sock_path).await.unwrap();
        let req = IpcMessage::Request {
            id: 100,
            request: DaemonRequest::Status,
        };
        send_ipc_message(&mut client_stream, &req).await.unwrap();

        let resp = recv_ipc_message(&mut client_stream).await.unwrap();
        match resp {
            IpcMessage::Response { id, response } => {
                assert_eq!(id, 100);
                assert_eq!(response, DaemonResponse::Success);
            }
            _ => panic!("Expected response"),
        }

        server_task.await.unwrap();
    }

    #[test]
    fn test_default_socket_path_resolution() {
        let orig = std::env::var("TURF_SOCKET_PATH");
        std::env::set_var("TURF_SOCKET_PATH", "/tmp/custom_turf.sock");
        assert_eq!(default_socket_path(), PathBuf::from("/tmp/custom_turf.sock"));
        match orig {
            Ok(v) => std::env::set_var("TURF_SOCKET_PATH", v),
            Err(_) => std::env::remove_var("TURF_SOCKET_PATH"),
        }
    }
}
