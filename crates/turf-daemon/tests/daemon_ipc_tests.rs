use std::time::Duration;
use tempfile::tempdir;
use turf_daemon::{DaemonClient, TurfDaemonServer};
use turf_protocol::ipc::DaemonEvent;

#[tokio::test]
async fn test_socket_bind_and_connection_handshake() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("handshake.sock");

    let server = TurfDaemonServer::new(&sock_path);
    let server_task = tokio::spawn({
        let server = server.clone();
        async move { server.run().await }
    });

    // Wait for server to bind
    for _ in 0..50 {
        if sock_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let client = DaemonClient::connect(&sock_path)
        .await
        .expect("Failed to connect client");

    let status = client.status().await.expect("Failed to query status");
    assert_eq!(status.pid, std::process::id());
    assert_eq!(status.active_sessions, 1);
    assert_eq!(status.active_panes, 0);
    assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(status.socket_path, sock_path.to_string_lossy());

    server.shutdown();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_pane_spawn_and_roundtrip_write_read_stream() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("pane_io.sock");

    let server = TurfDaemonServer::new(&sock_path);
    let server_task = tokio::spawn({
        let server = server.clone();
        async move { server.run().await }
    });

    for _ in 0..50 {
        if sock_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let client = DaemonClient::connect(&sock_path)
        .await
        .expect("Failed to connect client");

    let mut event_rx = client.subscribe();

    let pane_id = client
        .spawn_pane(1, None, None)
        .await
        .expect("Failed to spawn pane");
    assert!(pane_id > 0);

    // Subscribe and stream real-time PTY output
    let payload = b"echo DAEMON_STREAM_VERIFIED\n";
    client
        .write_pane(pane_id, payload)
        .await
        .expect("Failed to write to pane");

    let mut stream_matched = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);

    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            event_res = event_rx.recv() => {
                if let Ok(DaemonEvent::PaneOutput { pane_id: event_pid, bytes }) = event_res {
                    if event_pid == pane_id {
                        let text = String::from_utf8_lossy(&bytes);
                        if text.contains("DAEMON_STREAM_VERIFIED") {
                            stream_matched = true;
                            break;
                        }
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                // Also verify pane screen buffer read
                if let Ok(data) = client.read_pane(pane_id).await {
                    let text = String::from_utf8_lossy(&data);
                    if text.contains("DAEMON_STREAM_VERIFIED") {
                        stream_matched = true;
                        break;
                    }
                }
            }
        }
    }

    assert!(
        stream_matched,
        "Failed to receive echo output through daemon socket"
    );

    // Test resize
    client
        .resize_pane(pane_id, 120, 40)
        .await
        .expect("Failed to resize pane");

    // Test close pane
    client
        .close_pane(pane_id)
        .await
        .expect("Failed to close pane");

    let status = client.status().await.expect("Failed to query status");
    assert_eq!(status.active_panes, 0);

    server.shutdown();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_graceful_client_disconnect_without_killing_child_processes() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("persistence.sock");

    let server = TurfDaemonServer::new(&sock_path);
    let server_task = tokio::spawn({
        let server = server.clone();
        async move { server.run().await }
    });

    for _ in 0..50 {
        if sock_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Connect client 1 and spawn persistent child process
    let client1 = DaemonClient::connect(&sock_path)
        .await
        .expect("Failed to connect client1");

    let pane_id = client1
        .spawn_pane(1, None, Some("sleep 30".to_string()))
        .await
        .expect("Failed to spawn pane with sleep command");

    let status1 = client1.status().await.expect("Failed to query status1");
    assert_eq!(status1.active_panes, 1);

    // Disconnect client 1 completely
    drop(client1);
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Connect independent client 2 to inspect daemon state
    let client2 = DaemonClient::connect(&sock_path)
        .await
        .expect("Failed to connect client2");

    let status2 = client2.status().await.expect("Failed to query status2");
    assert_eq!(
        status2.active_panes, 1,
        "Child process must persist across client disconnects"
    );

    // Cleanup pane using client 2
    client2
        .close_pane(pane_id)
        .await
        .expect("Failed to close pane");

    let status3 = client2.status().await.expect("Failed to query status3");
    assert_eq!(status3.active_panes, 0);

    server.shutdown();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_daemon_status_query_and_clean_shutdown() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("shutdown.sock");

    let server = TurfDaemonServer::new(&sock_path);
    let server_task = tokio::spawn({
        let server = server.clone();
        async move { server.run().await }
    });

    for _ in 0..50 {
        if sock_path.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let client = DaemonClient::connect(&sock_path)
        .await
        .expect("Failed to connect client");

    let status = client.status().await.expect("Failed to get status");
    assert_eq!(status.active_panes, 0);

    // Request daemon shutdown via IPC
    client.shutdown().await.expect("Failed to request shutdown");

    // Wait for server task to finish cleanly
    let server_res = tokio::time::timeout(Duration::from_secs(2), server_task).await;
    assert!(
        server_res.is_ok(),
        "Server should terminate after shutdown request"
    );

    // Verify socket file was cleaned up
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !sock_path.exists(),
        "Socket file should be removed upon clean server exit"
    );
}
