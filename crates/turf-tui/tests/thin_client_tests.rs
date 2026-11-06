use std::time::Duration;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tempfile::tempdir;
use turf_daemon::{DaemonClient, TurfDaemonServer};
use turf_tui::{FocusArea, TerminalCockpit};

#[tokio::test]
async fn test_thin_client_pty_streaming_and_vte_blitting() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("streaming.sock");

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

    let mut cockpit = TerminalCockpit::new_daemon(
        client.clone(),
        "TURF-STREAM",
        "HOST",
        "tester",
        24,
        80,
    )
    .await
    .expect("Failed to create daemon cockpit");

    let status = client.status().await.expect("Failed to get status");
    assert_eq!(status.active_panes, 1);

    // Focus is on terminal by default
    assert_eq!(cockpit.focus, FocusArea::Terminal);

    // Send command to pane through write_pty
    let tab = &cockpit.tabs[cockpit.active_tab_index];
    let pane = tab.pane_manager.active_pane().expect("active pane exists");
    pane.write_pty(b"echo THIN_CLIENT_BLIT_OK\n");

    // Wait and poll daemon events
    let mut matched = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        cockpit.poll_daemon_events();
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().expect("active pane exists");
        let text = pane.terminal.text();
        if text.contains("THIN_CLIENT_BLIT_OK") {
            matched = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(matched, "PTY streaming chunk must blit into VTE terminal buffer");

    server.shutdown();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_thin_client_roundtrip_user_input() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("roundtrip.sock");

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

    let mut cockpit = TerminalCockpit::new_daemon(
        client.clone(),
        "TURF-INPUT",
        "HOST",
        "tester",
        24,
        80,
    )
    .await
    .expect("Failed to create daemon cockpit");

    assert_eq!(cockpit.focus, FocusArea::Terminal);

    // Simulate user typing `echo RT_INPUT_TEST\n` key by key
    for c in "echo RT_INPUT_TEST".chars() {
        cockpit.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    cockpit.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    let mut matched = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        cockpit.poll_daemon_events();
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().expect("active pane exists");
        let text = pane.terminal.text();
        if text.contains("RT_INPUT_TEST") {
            matched = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    assert!(matched, "Keystrokes typed in terminal focus must forward to daemon and echo in VTE");

    server.shutdown();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_clean_client_detach_and_reconnection() {
    let dir = tempdir().expect("Failed to create tempdir");
    let sock_path = dir.path().join("detach.sock");

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

    let client1 = DaemonClient::connect(&sock_path)
        .await
        .expect("Failed to connect client1");

    let mut cockpit1 = TerminalCockpit::new_daemon(
        client1.clone(),
        "TURF-DETACH",
        "HOST",
        "tester",
        24,
        80,
    )
    .await
    .expect("Failed to create daemon cockpit");

    // Send a command into the pane
    let tab = &cockpit1.tabs[cockpit1.active_tab_index];
    let pane = tab.pane_manager.active_pane().expect("active pane exists");
    pane.write_pty(b"echo DETACHED_SURVIVOR\n");

    // Wait for output to arrive in buffer
    let mut matched = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        cockpit1.poll_daemon_events();
        let tab = &cockpit1.tabs[cockpit1.active_tab_index];
        let pane = tab.pane_manager.active_pane().expect("active pane exists");
        let text = pane.terminal.text();
        if text.contains("DETACHED_SURVIVOR") {
            matched = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(matched);

    // User presses Alt+D to detach
    assert!(!cockpit1.should_exit);
    assert!(!cockpit1.is_detached);
    let handled = cockpit1.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::ALT));
    assert!(handled);
    assert!(cockpit1.should_exit);
    assert!(cockpit1.is_detached);

    // Drop client 1
    drop(cockpit1);
    drop(client1);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify daemon is still alive and pane process still exists
    let client2 = DaemonClient::connect(&sock_path)
        .await
        .expect("Failed to connect client2");
    let status = client2.status().await.expect("Failed to query status");
    assert_eq!(status.active_panes, 1, "Child process must persist after client detach");

    // Reconnection: connect a brand new cockpit to the same session
    let mut cockpit2 = TerminalCockpit::new_daemon(
        client2.clone(),
        "TURF-DETACH",
        "HOST",
        "tester",
        24,
        80,
    )
    .await
    .expect("Failed to reconnect daemon cockpit");

    // Verify cockpit2 re-attached to the existing pane and restored screen buffer
    let tab2 = &cockpit2.tabs[cockpit2.active_tab_index];
    let pane2 = tab2.pane_manager.active_pane().expect("active pane exists");
    let text2 = pane2.terminal.text();
    assert!(
        text2.contains("DETACHED_SURVIVOR"),
        "Reconnected cockpit must restore screen buffer from daemon"
    );

    // Keystrokes in reconnected cockpit continue working
    pane2.write_pty(b"echo RECONNECTED_ACTIVE\n");
    let mut reconnected_matched = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < deadline {
        cockpit2.poll_daemon_events();
        let tab = &cockpit2.tabs[cockpit2.active_tab_index];
        let pane = tab.pane_manager.active_pane().expect("active pane exists");
        let text = pane.terminal.text();
        if text.contains("RECONNECTED_ACTIVE") {
            reconnected_matched = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(reconnected_matched, "Reconnected pane must continue streaming output");

    server.shutdown();
    let _ = server_task.await;
}

#[tokio::test]
async fn test_standalone_fallback_when_daemon_unavailable() {
    let bad_path = std::path::PathBuf::from("/tmp/nonexistent_turf_socket_999999.sock");
    let client_res = DaemonClient::connect(&bad_path).await;
    assert!(client_res.is_err());

    // In-process standalone fallback succeeds
    let standalone = TerminalCockpit::new_standalone(
        "TURF-FALLBACK",
        "HOST",
        "tester",
        24,
        80,
    );
    assert!(standalone.is_ok());
    let cockpit = standalone.unwrap();
    assert_eq!(cockpit.tabs.len(), 1);
    assert_eq!(cockpit.session_code, "TURF-FALLBACK");
}
