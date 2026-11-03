use std::time::Duration;
use turf_pty::{default_shell, PtyOptions, PtyReadStream, PtySession};

#[test]
fn test_default_shell_resolution() {
    let shell = default_shell();
    assert!(!shell.trim().is_empty(), "Default shell must not be empty");
    #[cfg(unix)]
    {
        assert!(shell.contains("sh") || shell.contains("bash") || shell.contains("zsh"));
    }
    #[cfg(windows)]
    {
        assert_eq!(shell, "powershell.exe");
    }
}

#[test]
fn test_spawn_and_resize() {
    let mut session = PtySession::spawn_shell(24, 80).expect("Failed to spawn PTY session");
    assert_eq!(session.get_size(), (24, 80));

    // Test resize
    session.resize(40, 120).expect("Failed to resize PTY");
    assert_eq!(session.get_size(), (40, 120));

    // Confirm process is alive
    assert!(session.is_alive());

    // Kill process
    session.kill().expect("Failed to kill child process");
}

#[tokio::test]
async fn test_shell_spawn_and_echo_command() {
    // Spawn default shell in PTY
    let mut session = PtySession::spawn_shell(24, 80).expect("Failed to spawn PTY session");

    // Write echo command with newline to PTY
    let token = "TURF_PTY_ECHO_TEST_SUCCESS";
    let echo_cmd = format!("echo {token}\n");
    session
        .write_str(&echo_cmd)
        .expect("Failed to write to PTY");

    // Read PTY output until we encounter the token or timeout after 5 seconds
    let start = std::time::Instant::now();
    let mut received = String::new();
    let mut matched = false;

    while start.elapsed() < Duration::from_secs(5) {
        if let Some(chunk) = session.reader_mut().recv().await {
            let s = String::from_utf8_lossy(&chunk);
            received.push_str(&s);
            if received.contains(token) {
                matched = true;
                break;
            }
        } else {
            break;
        }
    }

    assert!(
        matched,
        "Failed to receive echo token in PTY output within 5s. Received buffer: {:?}",
        received
    );

    let _ = session.kill();
}

#[test]
fn test_sync_read_stream_trait() {
    let opts = PtyOptions::new(24, 80)
        .with_command("echo")
        .with_args(["HELLO_SYNC_STREAM"]);

    let mut session = PtySession::spawn(opts).expect("Failed to spawn echo command");
    let mut output = String::new();

    // Read chunks via trait method
    while let Ok(Some(chunk)) = session.reader_mut().read_chunk() {
        output.push_str(&String::from_utf8_lossy(&chunk));
        if output.contains("HELLO_SYNC_STREAM") {
            break;
        }
    }

    assert!(output.contains("HELLO_SYNC_STREAM"));
}

#[tokio::test]
async fn test_pty_to_vte_streaming_pipeline() {
    let mut session = PtySession::spawn_shell(24, 80).expect("Failed to spawn PTY session");
    let mut term = turf_vte::Terminal::new(24, 80);

    // Send a colorized echo sequence into PTY
    let token = "TURF_VTE_INTEGRATION";
    let cmd = format!("printf '\\033[1;32m{token}\\033[0m\\n'\n");
    session.write_str(&cmd).expect("Failed to write to PTY");

    let start = std::time::Instant::now();
    let mut found = false;

    while start.elapsed() < Duration::from_secs(5) {
        if let Some(chunk) = session.reader_mut().recv().await {
            term.process(&chunk);
            if term.text().contains(token) {
                found = true;
                break;
            }
        } else {
            break;
        }
    }

    assert!(found, "Expected VTE screen buffer to contain {token}");
    let _ = session.kill();
}
