pub mod autodetect;
pub mod client;
pub mod server;
pub mod socket;

pub use autodetect::{ensure_daemon_running, probe_daemon};
pub use client::DaemonClient;
pub use server::{parse_keys, DaemonPane, DaemonSession, SessionState, TurfDaemonServer};
pub use socket::{
    bind_listener, connect_stream, default_socket_path, SocketListener, SocketStream,
};
pub use turf_protocol::ipc::SessionInfo;
