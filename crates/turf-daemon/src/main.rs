use std::path::PathBuf;
use turf_daemon::{default_socket_path, TurfDaemonServer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mut socket_path = default_socket_path();

    let mut i = 1;
    while i < args.len() {
        if args[i] == "--socket" && i + 1 < args.len() {
            socket_path = PathBuf::from(&args[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }

    let server = TurfDaemonServer::new(socket_path);

    let server_handle = server.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!("Received Ctrl+C, shutting down TurfCode daemon...");
        server_handle.shutdown();
    });

    server.run().await?;
    Ok(())
}
