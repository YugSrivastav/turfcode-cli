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

#[tokio::test]
async fn test_named_session_lifecycle_management() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("session_mgmt.sock");

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

    // 1. Initial list has default session
    let sessions = client.list_sessions().await.expect("list_sessions failed");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].name, "default");

    // 2. Create detached named session "agent-alpha"
    let created = client
        .create_session("agent-alpha")
        .await
        .expect("Failed to create session");
    assert_eq!(created.name, "agent-alpha");
    assert_eq!(created.pane_count, 1);

    // 3. Duplicate creation fails
    let dup_err = client.create_session("agent-alpha").await;
    assert!(dup_err.is_err(), "Duplicate session creation must fail");

    // 4. Create second named session "agent-beta"
    client
        .create_session("agent-beta")
        .await
        .expect("Failed to create agent-beta");

    let sessions_after_create = client.list_sessions().await.expect("list_sessions failed");
    assert_eq!(sessions_after_create.len(), 3);
    assert!(sessions_after_create.iter().any(|s| s.name == "default"));
    assert!(sessions_after_create
        .iter()
        .any(|s| s.name == "agent-alpha"));
    assert!(sessions_after_create.iter().any(|s| s.name == "agent-beta"));

    // 5. Kill session "agent-alpha"
    client
        .kill_session("agent-alpha")
        .await
        .expect("Failed to kill session");

    let sessions_after_kill = client.list_sessions().await.expect("list_sessions failed");
    assert_eq!(sessions_after_kill.len(), 2);
    assert!(!sessions_after_kill.iter().any(|s| s.name == "agent-alpha"));
    assert!(sessions_after_kill.iter().any(|s| s.name == "agent-beta"));

    // 6. Kill non-existent session fails
    let kill_nonexistent = client.kill_session("ghost-session").await;
    assert!(kill_nonexistent.is_err());

    server.shutdown();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_pane_send_keys_and_read_lines_automation() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("pane_automation.sock");

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

    let pane_id = client
        .spawn_pane(1, None, None)
        .await
        .expect("Failed to spawn pane");

    // Send keys with newline
    client
        .send_pane_keys(pane_id, "echo HERDR_AUTOMATION_OK\n")
        .await
        .expect("send_pane_keys failed");

    // Poll read_pane_lines until output is present
    let mut found = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        if let Ok(lines) = client.read_pane_lines(pane_id, Some(5)).await {
            if lines.iter().any(|l| l.contains("HERDR_AUTOMATION_OK")) {
                found = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(found, "Expected HERDR_AUTOMATION_OK in pane lines output");

    // Test read_pane_lines with limited lines
    let single_line = client
        .read_pane_lines(pane_id, Some(1))
        .await
        .expect("read 1 line failed");
    assert!(single_line.len() <= 1);

    // Test special key sequences like Enter
    client
        .send_pane_keys(pane_id, "Enter")
        .await
        .expect("send Enter failed");

    // Test split pane
    let new_pane_id = client
        .split_pane(pane_id, "v")
        .await
        .expect("split_pane failed");
    assert!(new_pane_id > pane_id);

    client.close_pane(pane_id).await.expect("close pane 1");
    client.close_pane(new_pane_id).await.expect("close pane 2");

    server.shutdown();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_kill_session_reaps_child_ptys() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("reap.sock");

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

    let st_initial = client.status().await.expect("status query failed");
    assert_eq!(st_initial.active_panes, 0);

    // Create session (spawns 1 child pane)
    let session = client
        .create_session("reap-target")
        .await
        .expect("create session failed");
    assert_eq!(session.pane_count, 1);

    let st_created = client.status().await.expect("status query failed");
    assert_eq!(st_created.active_panes, 1);

    // Kill session: must terminate child PTY and reap it from active panes
    client
        .kill_session("reap-target")
        .await
        .expect("kill session failed");

    let st_killed = client.status().await.expect("status query failed");
    assert_eq!(
        st_killed.active_panes, 0,
        "Killing session must reap all its child PTYs"
    );

    server.shutdown();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_periodic_foreground_process_sampling_and_status_changed_event() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("process_introspection.sock");

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

    // Spawn a shell pane
    let pane_id = client
        .spawn_pane(1, None, None)
        .await
        .expect("Failed to spawn pane");
    assert!(pane_id > 0);

    // Wait for the background sampler or event loop to detect foreground process and emit PaneStatusChanged
    let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
    let mut status_detected = false;
    let mut detected_cmd = None;

    while tokio::time::Instant::now() < deadline {
        tokio::select! {
            event_res = event_rx.recv() => {
                if let Ok(DaemonEvent::PaneStatusChanged { pane_id: event_pid, foreground_cmd }) = event_res {
                    if event_pid == pane_id && foreground_cmd.is_some() {
                        status_detected = true;
                        detected_cmd = foreground_cmd;
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(20)) => {
                // Also query pane_info directly
                if let Ok(info) = client.pane_info(pane_id).await {
                    if info.foreground_cmd.is_some() {
                        status_detected = true;
                        detected_cmd = info.foreground_cmd;
                        break;
                    }
                }
            }
        }
    }

    #[cfg(unix)]
    {
        assert!(
            status_detected,
            "Daemon should emit PaneStatusChanged or report foreground_cmd in pane_info"
        );
        let cmd = detected_cmd.expect("Expected detected foreground command");
        assert!(cmd.contains("sh") || cmd.contains("bash") || cmd.contains("zsh"));
    }

    // Verify pane_info returns is_alive = true
    let info = client.pane_info(pane_id).await.expect("Failed to query pane info");
    assert_eq!(info.pane_id, pane_id);
    assert_eq!(info.tab_id, 1);
    assert!(info.is_alive);

    server.shutdown();
    let _ = server_task.await;
}
