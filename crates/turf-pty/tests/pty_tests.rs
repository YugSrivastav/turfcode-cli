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

#[test]
fn test_foreground_process_introspection_on_spawned_child() {
    let opts = PtyOptions::new(24, 80)
        .with_command("sleep")
        .with_args(["5"]);

    let session = PtySession::spawn(opts).expect("Failed to spawn sleep command in PTY");

    #[cfg(unix)]
    {
        // Wait briefly for child to initialize and become the foreground process group
        let start = std::time::Instant::now();
        let mut proc_opt = None;
        while start.elapsed() < Duration::from_secs(2) {
            if let Some(p) = session.foreground_process() {
                if p.name == "sleep" {
                    proc_opt = Some(p);
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let proc = proc_opt.expect("Expected foreground process info for sleep");
        assert!(proc.pid > 0, "Foreground process PID must be positive");
        assert_eq!(proc.name, "sleep");
        assert!(proc.cmdline.contains("sleep 5"));
        assert_eq!(proc.display_cmd(), "sleep 5");

        // Verify master_raw_fd access
        let (reader, writer, handle) = session.split();
        let raw_fd = handle.master_raw_fd().expect("Raw master FD should be present on Unix");
        let direct_proc = turf_pty::unix::get_foreground_process(raw_fd).expect("Direct fd introspection failed");
        assert_eq!(direct_proc.name, "sleep");
        assert_eq!(direct_proc.display_cmd(), "sleep 5");
        drop(reader);
        drop(writer);
        let mut handle = handle;
        let _ = handle.kill();
    }
    #[cfg(not(unix))]
    {
        let mut session = session;
        let _ = session.kill();
    }
}

#[tokio::test]
async fn test_foreground_process_on_idle_shell_and_active_command() {
    let mut session = PtySession::spawn_shell(24, 80).expect("Failed to spawn shell PTY");

    #[cfg(unix)]
    {
        // 1. Initially on idle shell, foreground process is the shell itself (bash, zsh, sh, etc.)
        let start = std::time::Instant::now();
        let mut idle_detected = false;
        while start.elapsed() < Duration::from_secs(3) {
            if let Some(proc) = session.foreground_process() {
                let display = proc.display_cmd();
                if display.contains("sh") || display.contains("bash") || display.contains("zsh") {
                    idle_detected = true;
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(idle_detected, "Idle shell should be detected as foreground process");

        // 2. Launch an active command in the shell (`sleep 10`)
        session.write_str("sleep 10\n").expect("Failed to write to PTY");

        let start = std::time::Instant::now();
        let mut sleep_detected = false;
        while start.elapsed() < Duration::from_secs(3) {
            if let Some(proc) = session.foreground_process() {
                if proc.name == "sleep" || proc.display_cmd().starts_with("sleep") {
                    sleep_detected = true;
                    assert_eq!(proc.display_cmd(), "sleep 10");
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(sleep_detected, "Active foreground process 'sleep 10' should be detected");
    }

    let _ = session.kill();
}

#[test]
fn test_process_info_display_cmd_formatting() {
    use turf_pty::ProcessInfo;

    // 1. Active editor command with file argument
    let vim_proc = ProcessInfo::new(101, "vim", "vim main.rs");
    assert_eq!(vim_proc.display_cmd(), "vim main.rs");

    let vim_path_proc = ProcessInfo::new(102, "vim", "/usr/bin/vim src/main.rs");
    assert_eq!(vim_path_proc.display_cmd(), "vim src/main.rs");

    // 2. Build and tool commands
    let cargo_proc = ProcessInfo::new(201, "cargo", "/home/user/.cargo/bin/cargo test --workspace");
    assert_eq!(cargo_proc.display_cmd(), "cargo test --workspace");

    let git_proc = ProcessInfo::new(202, "git", "git diff HEAD~1");
    assert_eq!(git_proc.display_cmd(), "git diff HEAD~1");

    let py_proc = ProcessInfo::new(203, "python3", "python3 server.py");
    assert_eq!(py_proc.display_cmd(), "python3 server.py");

    let npm_proc = ProcessInfo::new(204, "npm", "npm run dev");
    assert_eq!(npm_proc.display_cmd(), "npm run dev");

    // 3. Idle shells
    let bash_proc = ProcessInfo::new(301, "bash", "-bash");
    assert_eq!(bash_proc.display_cmd(), "bash");

    let zsh_proc = ProcessInfo::new(302, "zsh", "/usr/bin/zsh -i");
    assert_eq!(zsh_proc.display_cmd(), "zsh");

    let fish_proc = ProcessInfo::new(303, "fish", "/usr/bin/fish");
    assert_eq!(fish_proc.display_cmd(), "fish");

    // 4. Fallback to name if cmdline is empty
    let empty_cmd = ProcessInfo::new(401, "custom_worker", "");
    assert_eq!(empty_cmd.display_cmd(), "custom_worker");
}
