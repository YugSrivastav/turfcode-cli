use crate::client::TunnelClient;
use crate::crypto;
use crate::error::NetError;
use crate::server::TunnelServer;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::env;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use turf_core::{intent_board::IntentBoard, lock_manager::LockManager};
use turf_tui::{PeerInfo, TerminalCockpit};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Banner and splash printing for TurfCode CLI.
pub fn print_splash() {
    println!(
        "\x1b[1;32m
  ╔═════════════════════════════════════════════════════════════════════════╗
  ║   ████████╗██╗   ██╗██████╗ ███████╗ ██████╗ ██████╗ ██████╗ ███████╗   ║
  ║   ╚══██╔══╝██║   ██║██╔══██╗██╔════╝██╔════╝██╔═══██╗██╔══██╗██╔════╝   ║
  ║      ██║   ██║   ██║██████╔╝█████╗  ██║     ██║   ██║██║  ██║█████╗     ║
  ║      ██║   ██║   ██║██╔══██╗██╔══╝  ██║     ██║   ██║██║  ██║██╔══╝     ║
  ║      ██║   ╚██████╔╝██║  ██║██║     ╚██████╗╚██████╔╝██████╔╝███████╗   ║
  ║      ╚═╝    ╚═════╝ ╚═╝  ╚═╝╚═╝      ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝   ║
  ║                      ELECTRIC LIME COCKPIT v{}                       ║
  ╚═════════════════════════════════════════════════════════════════════════╝
\x1b[0m",
        VERSION
    );
}

/// Print CLI help information.
pub fn print_help() {
    println!(
        "\x1b[1;32mTURFCODE CLI\x1b[0m — \x1b[37mReal-time multiplayer AI coding in the terminal\x1b[0m

\x1b[2mUSAGE:\x1b[0m
  \x1b[32mturfcode\x1b[0m                 Open interactive launcher menu (Create or Join a Turf)
  \x1b[32mturf\x1b[0m                     Alias for turfcode
  \x1b[32mturf host [dir]\x1b[0m          Host a shared Turf session in the given directory (default: .)
  \x1b[32mturf join <id> [url]\x1b[0m     Join an active Turf session using room code
  \x1b[32mturf attach [id]\x1b[0m         Attach to an active background daemon session (default: default)
  \x1b[32mturf daemon start\x1b[0m        Start the persistent background daemon
  \x1b[32mturf daemon stop\x1b[0m         Stop the running background daemon
  \x1b[32mturf daemon status\x1b[0m       Show background daemon process and socket status
  \x1b[32mturf --help, -h\x1b[0m          Show this help information
  \x1b[32mturf --version, -v\x1b[0m       Show version
"
    );
}

/// Helper to open the system file manager directly on the target folder.
/// Supports Windows, WSL, macOS, and Linux, gracefully ignoring errors if headless.
pub fn open_file_manager(target_dir: &Path) {
    if env::args().any(|a| a == "--no-cockpit") {
        return;
    }

    let is_wsl = std::env::var_os("WSL_DISTRO_NAME").is_some()
        || Path::new("/proc/sys/fs/binfmt_misc/WSLInterop").exists();

    if cfg!(target_os = "windows") {
        let _ = std::process::Command::new("explorer.exe")
            .arg(target_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    } else if is_wsl {
        let win_path = std::process::Command::new("wslpath")
            .arg("-w")
            .arg(target_dir)
            .output()
            .ok()
            .and_then(|out| {
                if out.status.success() {
                    String::from_utf8(out.stdout).ok().map(|s| s.trim().to_string())
                } else {
                    None
                }
            });
        let target = win_path.unwrap_or_else(|| target_dir.to_string_lossy().to_string());
        let _ = std::process::Command::new("explorer.exe")
            .arg(target)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    } else if cfg!(target_os = "macos") {
        let _ = std::process::Command::new("open")
            .arg(target_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    } else {
        // Native Linux
        let _ = std::process::Command::new("xdg-open")
            .arg(target_dir)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// Normalize workspace folder path.
pub fn normalize_workspace_path(input: &str) -> PathBuf {
    let trimmed = input.trim().trim_matches('"').trim_matches('\'');
    if trimmed.is_empty() || trimmed == "." {
        env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        let path = PathBuf::from(trimmed);
        if path.is_absolute() {
            path
        } else {
            env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }
    }
}

/// Boot the interactive cockpit backed by the background daemon thin client,
/// falling back cleanly to in-process standalone mode if daemon is unavailable.
pub async fn run_interactive_choice(
    session_code: &str,
    role: &str,
    username: &str,
    rows: u16,
    cols: u16,
) -> TerminalCockpit {
    match turf_daemon::ensure_daemon_running().await {
        Ok(sock_path) => {
            match turf_daemon::DaemonClient::connect(&sock_path).await {
                Ok(client) => {
                    match TerminalCockpit::new_daemon(
                        client,
                        session_code,
                        role,
                        username,
                        rows,
                        cols,
                    )
                    .await
                    {
                        Ok(cockpit) => {
                            tracing::info!(
                                "Connected thin-client cockpit to daemon at {}",
                                sock_path.display()
                            );
                            return cockpit;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to initialize daemon cockpit: {}; falling back to standalone",
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to connect to daemon at {}: {}; falling back to standalone",
                        sock_path.display(),
                        e
                    );
                }
            }
        }
        Err(e) => {
            tracing::warn!(
                "Daemon auto-start failed: {}; falling back to standalone",
                e
            );
        }
    }

    // Graceful fallback to standalone in-process mode
    TerminalCockpit::new_standalone(session_code, role, username, rows, cols)
        .unwrap_or_else(|_| {
            TerminalCockpit::new_headless(
                session_code,
                role,
                username,
                rows as usize,
                cols as usize,
            )
        })
}

/// Host a TurfCode session in the target directory.
pub async fn run_host(target_dir: &Path, port: u16) -> Result<(), NetError> {
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir)?;
    }

    let room_code = crypto::generate_room_code();
    let intent_board = Arc::new(IntentBoard::with_cwd(target_dir));
    let lock_manager = Arc::new(LockManager::with_cwd(target_dir));

    let bind_port = if port == 0 { 7873 } else { port };
    let mut server = TunnelServer::new(Some(room_code.clone()))
        .with_host("127.0.0.1")
        .with_port(bind_port)
        .with_intent_board(Arc::clone(&intent_board))
        .with_lock_manager(Arc::clone(&lock_manager));

    let local_addr = match server.start().await {
        Ok(addr) => addr,
        Err(_) if port == 0 => {
            // Fallback to dynamic port if default port 7873 is in use
            let mut fallback = TunnelServer::new(Some(room_code.clone()))
                .with_host("127.0.0.1")
                .with_port(0)
                .with_intent_board(Arc::clone(&intent_board))
                .with_lock_manager(Arc::clone(&lock_manager));
            let addr = fallback.start().await?;
            server = fallback;
            addr
        }
        Err(e) => return Err(e),
    };

    let username = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "Host".to_string());

    let is_terminal = io::stdout().is_terminal() && !env::args().any(|arg| arg == "--no-cockpit");

    if is_terminal {
        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let size = terminal.size()?;

        let mut cockpit = run_interactive_choice(
            &room_code,
            "HOST",
            &username,
            size.height,
            size.width,
        )
        .await;

        let space_name = target_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("turfcode")
            .to_string();
        cockpit.sidebar_state.current_space = space_name;
        cockpit.sidebar_state.peers = vec![PeerInfo {
            name: username.clone(),
            role: "HOST".to_string(),
            is_self: true,
            is_online: true,
        }];
        cockpit.sidebar_state.tasks = intent_board.get_tasks().await;
        cockpit.sidebar_state.locks = lock_manager.get_locks().await;

        let mut event_rx = server.subscribe();

        cockpit
            .run_event_loop_with(&mut terminal, |cp| {
                while let Ok(event) = event_rx.try_recv() {
                    match event {
                        crate::server::ServerEvent::PeerConnected {
                            peer_id: _,
                            username: p_user,
                            role,
                        } => {
                            if let Some(p) = cp.sidebar_state.peers.iter_mut().find(|p| p.name == p_user) {
                                p.is_online = true;
                                p.role = role;
                            } else {
                                cp.sidebar_state.peers.push(PeerInfo {
                                    name: p_user,
                                    role,
                                    is_self: false,
                                    is_online: true,
                                });
                            }
                        }
                        crate::server::ServerEvent::PeerDisconnected {
                            peer_id: _,
                            username: p_user,
                        } => {
                            if let Some(p) = cp.sidebar_state.peers.iter_mut().find(|p| p.name == p_user) {
                                p.is_online = false;
                            }
                        }
                        crate::server::ServerEvent::TasksChanged(tasks) => {
                            cp.sidebar_state.tasks = tasks;
                        }
                        crate::server::ServerEvent::LocksChanged(locks) => {
                            cp.sidebar_state.locks = locks;
                        }
                        _ => {}
                    }
                }
            })?;

        println!("\n  Shutting down TurfCode session...");
        server.stop().await;
        return Ok(());
    }

    // Headless / non-terminal waiting mode (for automated tests or headless environments)
    println!(
        "\n  \x1b[1;32m⚡ TURF CREATED: \x1b[1;33m{}\x1b[0m",
        room_code
    );
    println!(
        "  \x1b[2m📂 Hosting Folder: \x1b[36m{}\x1b[0m",
        target_dir.display()
    );
    println!(
        "  \x1b[2m🌐 Local Endpoint: \x1b[36mws://{}\x1b[0m",
        local_addr
    );
    println!("  \x1b[2m🔗 Share this Session ID with your collaborator to join!\x1b[0m\n");

    let mut event_rx = server.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                crate::server::ServerEvent::PeerConnected {
                    peer_id,
                    username,
                    role,
                } => {
                    println!(
                        "  \x1b[32m✔ Peer joined: \x1b[1m{} ({}) [{}]\x1b[0m",
                        username, role, peer_id
                    );
                }
                crate::server::ServerEvent::PeerDisconnected { peer_id, username } => {
                    println!(
                        "  \x1b[31m✖ Peer disconnected: \x1b[1m{} [{}]\x1b[0m",
                        username, peer_id
                    );
                }
                _ => {}
            }
        }
    });

    // Wait for Ctrl+C to terminate
    tokio::signal::ctrl_c().await.ok();
    println!("\n  Shutting down TurfCode session...");
    server.stop().await;
    Ok(())
}

/// Join an active TurfCode session using a room code.
pub async fn run_join(room_code: &str, ws_url: &str) -> Result<(), NetError> {
    let raw_code = if room_code.trim().is_empty() {
        print!("  \x1b[32mSession ID (e.g. TURF-XXXX): \x1b[0m");
        io::stdout().flush().ok();
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line).ok();
        line.trim().to_string()
    } else {
        room_code.trim().to_string()
    };

    let clean_code = crypto::normalize_room_code(&raw_code);
    let username = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "Peer".into());

    let mut client = TunnelClient::new(&clean_code, &username).with_role("Peer");
    let mut event_rx = client.subscribe();

    let peer_id = client.connect(ws_url).await?;

    let is_terminal = io::stdout().is_terminal() && !env::args().any(|arg| arg == "--no-cockpit");

    if is_terminal {
        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let size = terminal.size()?;

        let mut cockpit = run_interactive_choice(
            &clean_code,
            "PEER",
            &username,
            size.height,
            size.width,
        )
        .await;

        cockpit.sidebar_state.current_space = format!("turf-{clean_code}");
        cockpit.sidebar_state.tasks = client.tasks().await;
        cockpit.sidebar_state.locks = client.locks().await;

        let initial_peers = client.peers().await;
        cockpit.sidebar_state.peers = initial_peers
            .into_iter()
            .map(|p| PeerInfo {
                name: p.username.clone(),
                role: p.role,
                is_self: p.username == username,
                is_online: true,
            })
            .collect();
        if !cockpit.sidebar_state.peers.iter().any(|p| p.is_self) {
            cockpit.sidebar_state.peers.insert(
                0,
                PeerInfo {
                    name: username.clone(),
                    role: "PEER".to_string(),
                    is_self: true,
                    is_online: true,
                },
            );
        }

        cockpit
            .run_event_loop_with(&mut terminal, |cp| {
                while let Ok(event) = event_rx.try_recv() {
                    match event {
                        crate::client::ClientEvent::TasksSync(tasks) => {
                            cp.sidebar_state.tasks = tasks;
                        }
                        crate::client::ClientEvent::LocksSync(locks) => {
                            cp.sidebar_state.locks = locks;
                        }
                        crate::client::ClientEvent::PeersSync(peers) => {
                            cp.sidebar_state.peers = peers
                                .into_iter()
                                .map(|p| PeerInfo {
                                    name: p.username.clone(),
                                    role: p.role,
                                    is_self: p.username == username,
                                    is_online: true,
                                })
                                .collect();
                        }
                        crate::client::ClientEvent::PeerJoined(p) => {
                            if let Some(peer) = cp.sidebar_state.peers.iter_mut().find(|peer| peer.name == p.username) {
                                peer.is_online = true;
                                peer.role = p.role;
                            } else {
                                cp.sidebar_state.peers.push(PeerInfo {
                                    name: p.username.clone(),
                                    role: p.role,
                                    is_self: p.username == username,
                                    is_online: true,
                                });
                            }
                        }
                        crate::client::ClientEvent::PeerLeft {
                            peer_id: _,
                            username: p_name,
                        } => {
                            if let Some(p) = cp.sidebar_state.peers.iter_mut().find(|peer| peer.name == p_name) {
                                p.is_online = false;
                            }
                        }
                        crate::client::ClientEvent::Disconnected => {
                            cp.should_exit = true;
                        }
                        _ => {}
                    }
                }
            })?;

        println!("\n  Leaving Turf session...");
        client.disconnect().await;
        return Ok(());
    }

    // Headless / non-terminal waiting mode
    println!(
        "\n  \x1b[1;32m⚡ JOINING TURF: \x1b[1;33m{}\x1b[0m",
        clean_code
    );
    println!("  \x1b[2m🔗 Connecting to: \x1b[36m{}\x1b[0m...", ws_url);
    println!(
        "  \x1b[1;32m✔ CONNECTED SUCCESSFULLY!\x1b[0m Assigned Peer ID: \x1b[36m{}\x1b[0m\n",
        peer_id
    );

    let initial_tasks = client.tasks().await;
    println!("  📋 Replicated Tasks ({}):", initial_tasks.len());
    for t in &initial_tasks {
        println!("     - [{}] {} ({})", t.status, t.title, t.assignee);
    }

    let initial_peers = client.peers().await;
    println!("  👥 Connected Peers ({}):", initial_peers.len());
    for p in &initial_peers {
        println!("     - {} ({})", p.username, p.role);
    }
    println!("\n  \x1b[2mListening for live state updates (Ctrl+C to leave)...\x1b[0m");

    tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            match event {
                crate::client::ClientEvent::TasksSync(tasks) => {
                    println!(
                        "  \x1b[32m[Sync]\x1b[0m Intent Board updated: {} tasks",
                        tasks.len()
                    );
                }
                crate::client::ClientEvent::LocksSync(locks) => {
                    println!(
                        "  \x1b[33m[Sync]\x1b[0m File locks updated: {} active locks",
                        locks.len()
                    );
                }
                crate::client::ClientEvent::PeerJoined(p) => {
                    println!(
                        "  \x1b[32m✔ New peer joined: {} ({})\x1b[0m",
                        p.username, p.role
                    );
                }
                crate::client::ClientEvent::PeerLeft { username, .. } => {
                    println!("  \x1b[31m✖ Peer left: {}\x1b[0m", username);
                }
                crate::client::ClientEvent::Disconnected => {
                    println!("  \x1b[31m✖ Disconnected from host.\x1b[0m");
                    break;
                }
                _ => {}
            }
        }
    });

    tokio::signal::ctrl_c().await.ok();
    println!("\n  Leaving Turf session...");
    client.disconnect().await;
    Ok(())
}

/// Attach directly to a running or new TurfCode daemon session.
pub async fn run_attach(session_code: &str) -> Result<(), NetError> {
    let clean_code = crypto::normalize_room_code(session_code);
    let username = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "User".into());

    let is_terminal = io::stdout().is_terminal() && !env::args().any(|arg| arg == "--no-cockpit");
    if is_terminal {
        let stdout = io::stdout();
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let size = terminal.size()?;

        let mut cockpit = run_interactive_choice(
            &clean_code,
            "CLIENT",
            &username,
            size.height,
            size.width,
        )
        .await;

        cockpit.sidebar_state.current_space = format!("turf-{clean_code}");
        cockpit.sidebar_state.peers = vec![PeerInfo {
            name: username.clone(),
            role: "CLIENT".to_string(),
            is_self: true,
            is_online: true,
        }];

        cockpit.run_event_loop(&mut terminal)?;

        if cockpit.is_detached {
            println!(
                "\n  \x1b[1;32m✔ Detached cleanly from session {}.\x1b[0m Child processes continue in background.",
                clean_code
            );
            println!("  \x1b[2mTo re-attach, run: turf attach {}\x1b[0m\n", clean_code);
        } else {
            println!("\n  Exited TurfCode session {}.", clean_code);
        }
    } else {
        println!("Headless mode: session {} active in daemon.", clean_code);
    }

    Ok(())
}

/// Interactive launcher prompt conforming to [MD-041].
pub async fn run_interactive_prompt() -> Result<(), NetError> {
    print_splash();

    println!(
        "  \x1b[32m[1]\x1b[0m Create a Turf   \x1b[2m(Host a local folder / live session)\x1b[0m"
    );
    println!("  \x1b[32m[2]\x1b[0m Join a Turf     \x1b[2m(Connect to an active room code)\x1b[0m");
    println!("  \x1b[32m[3]\x1b[0m Attach Session  \x1b[2m(Re-attach to background daemon session)\x1b[0m");
    println!("  \x1b[2m[4] Exit\x1b[0m\n");

    print!("  \x1b[32mSelect option [1-4]: \x1b[0m");
    io::stdout().flush().ok();

    let mut line = String::new();
    let stdin = io::stdin();
    stdin.lock().read_line(&mut line).ok();
    let choice = line.trim();

    match choice {
        "1" => {
            print!("\x1b[2J\x1b[1;1H");
            print_splash();
            print!("  \x1b[32mEnter folder to host [default: .]: \x1b[0m");
            io::stdout().flush().ok();
            let mut dir_line = String::new();
            stdin.lock().read_line(&mut dir_line).ok();
            let target_dir = normalize_workspace_path(&dir_line);
            open_file_manager(&target_dir);
            run_host(&target_dir, 0).await?;
        }
        "2" => {
            print!("\x1b[2J\x1b[1;1H");
            print_splash();
            print!("  \x1b[32mSession ID (e.g. TURF-XXXX): \x1b[0m");
            io::stdout().flush().ok();
            let mut code_line = String::new();
            stdin.lock().read_line(&mut code_line).ok();
            let room_code = code_line.trim();

            run_join(room_code, "ws://127.0.0.1:7873").await?;
        }
        "3" => {
            print!("\x1b[2J\x1b[1;1H");
            print_splash();
            print!("  \x1b[32mSession ID [default: default]: \x1b[0m");
            io::stdout().flush().ok();
            let mut code_line = String::new();
            stdin.lock().read_line(&mut code_line).ok();
            let trimmed = code_line.trim();
            let session_code = if trimmed.is_empty() { "default" } else { trimmed };
            run_attach(session_code).await?;
        }
        _ => {
            println!("  Exiting.");
        }
    }

    Ok(())
}

/// Handle daemon subcommands (`turf daemon start`, `turf daemon stop`, `turf daemon status`, `turf daemon run`).
pub async fn handle_daemon_cli(subargs: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let subcmd = subargs.first().map(|s| s.as_str()).unwrap_or("status");
    match subcmd {
        "start" => {
            let foreground = subargs.iter().any(|a| a == "--foreground" || a == "-f");
            let socket_path = turf_daemon::default_socket_path();
            if foreground {
                println!(
                    "Starting TurfCode daemon in foreground on {}...",
                    socket_path.display()
                );
                let server = turf_daemon::TurfDaemonServer::new(socket_path);
                server.run().await?;
            } else if turf_daemon::probe_daemon(&socket_path).await {
                println!(
                    "\x1b[1;33mTurfCode daemon is already running\x1b[0m at {}",
                    socket_path.display()
                );
            } else {
                turf_daemon::ensure_daemon_running().await?;
                println!(
                    "\x1b[1;32m✔ TurfCode daemon started successfully\x1b[0m at {}",
                    socket_path.display()
                );
            }
        }
        "run" => {
            let mut socket_path = turf_daemon::default_socket_path();
            let mut i = 0;
            while i < subargs.len() {
                if subargs[i] == "--socket" && i + 1 < subargs.len() {
                    socket_path = PathBuf::from(&subargs[i + 1]);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            let server = turf_daemon::TurfDaemonServer::new(socket_path);
            let server_handle = server.clone();
            tokio::spawn(async move {
                tokio::signal::ctrl_c().await.ok();
                server_handle.shutdown();
            });
            server.run().await?;
        }
        "stop" => {
            let socket_path = turf_daemon::default_socket_path();
            match turf_daemon::DaemonClient::connect(&socket_path).await {
                Ok(client) => {
                    client.shutdown().await?;
                    println!("\x1b[1;32m✔ TurfCode daemon stopped successfully.\x1b[0m");
                }
                Err(_) => {
                    println!(
                        "\x1b[1;33mTurfCode daemon is not running\x1b[0m (socket at {}).",
                        socket_path.display()
                    );
                }
            }
        }
        "status" => {
            let socket_path = turf_daemon::default_socket_path();
            match turf_daemon::DaemonClient::connect(&socket_path).await {
                Ok(client) => match client.status().await {
                    Ok(st) => {
                        println!("\x1b[1;32mTurfCode Daemon: RUNNING\x1b[0m");
                        println!("  PID:             {}", st.pid);
                        println!("  Uptime:          {}s", st.uptime_secs);
                        println!("  Active Sessions: {}", st.active_sessions);
                        println!("  Active Panes:    {}", st.active_panes);
                        println!("  Socket:          {}", st.socket_path);
                        println!("  Version:         v{}", st.version);
                    }
                    Err(e) => {
                        println!(
                            "\x1b[1;31mTurfCode Daemon: UNRESPONSIVE\x1b[0m (Error: {})",
                            e
                        );
                    }
                },
                Err(_) => {
                    println!(
                        "\x1b[1;33mTurfCode Daemon: STOPPED\x1b[0m (Socket not responding at {})",
                        socket_path.display()
                    );
                }
            }
        }
        other => {
            eprintln!(
                "\x1b[31mError:\x1b[0m Unknown daemon subcommand '{}'. Valid options: start, stop, status",
                other
            );
            std::process::exit(1);
        }
    }
    Ok(())
}

/// Main CLI entry point.
pub async fn main_cli() -> Result<(), Box<dyn std::error::Error>> {
    let raw_args: Vec<String> = env::args().collect();
    let args: Vec<String> = raw_args
        .into_iter()
        .filter(|a| a != "--no-cockpit")
        .collect();

    if args.len() <= 1 {
        // Auto-detect and connect to daemon, falling back cleanly to in-process mode
        match turf_daemon::ensure_daemon_running().await {
            Ok(sock_path) => {
                tracing::info!("Connected to TurfCode daemon at {}", sock_path.display());
                if let Ok(client) = turf_daemon::DaemonClient::connect(&sock_path).await {
                    if let Ok(status) = client.status().await {
                        if status.active_panes > 0 {
                            tracing::info!(
                                "Active panes ({}) detected in background daemon; re-attaching...",
                                status.active_panes
                            );
                            run_attach("default").await?;
                            return Ok(());
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    "Daemon auto-start unavailable ({}); falling back to standalone in-process mode",
                    e
                );
            }
        }
        run_interactive_prompt().await?;
        return Ok(());
    }

    match args[1].as_str() {
        "--help" | "-h" => {
            print_splash();
            print_help();
        }
        "--version" | "-v" => {
            println!("turfcode v{}", VERSION);
        }
        "attach" => {
            let session_code = args.get(2).map(|s| s.as_str()).unwrap_or("default");
            run_attach(session_code).await?;
        }
        "daemon" => {
            handle_daemon_cli(&args[2..]).await?;
        }
        "host" => {
            let target = if args.len() > 2 {
                normalize_workspace_path(&args[2])
            } else {
                normalize_workspace_path(".")
            };
            open_file_manager(&target);
            run_host(&target, 0).await?;
        }
        "join" => {
            if args.len() < 3 {
                eprintln!(
                    "\x1b[31mError:\x1b[0m Missing room code. Usage: turf join <TURF-XXXX> [url]"
                );
                std::process::exit(1);
            }
            let room_code = &args[2];
            let url = if args.len() > 3 {
                &args[3]
            } else {
                "ws://127.0.0.1:7873"
            };
            run_join(room_code, url).await?;
        }
        other => {
            eprintln!(
                "\x1b[31mError:\x1b[0m Unknown command '{}'. See 'turf --help'.",
                other
            );
            std::process::exit(1);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_print_splash_banner_correctness() {
        let splash = format!(
            "  ╔═════════════════════════════════════════════════════════════════════════╗\n  ║   ████████╗██╗   ██╗██████╗ ███████╗ ██████╗ ██████╗ ██████╗ ███████╗   ║\n  ║   ╚══██╔══╝██║   ██║██╔══██╗██╔════╝██╔════╝██╔═══██╗██╔══██╗██╔════╝   ║\n  ║      ██║   ██║   ██║██████╔╝█████╗  ██║     ██║   ██║██║  ██║█████╗     ║\n  ║      ██║   ██║   ██║██╔══██╗██╔══╝  ██║     ██║   ██║██║  ██║██╔══╝     ║\n  ║      ██║   ╚██████╔╝██║  ██║██║     ╚██████╗╚██████╔╝██████╔╝███████╗   ║\n  ║      ╚═╝    ╚═════╝ ╚═╝  ╚═╝╚═╝      ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝   ║\n  ║                      ELECTRIC LIME COCKPIT v{}                       ║\n  ╚═════════════════════════════════════════════════════════════════════════╝",
            VERSION
        );

        for line in splash.lines() {
            assert_eq!(
                line.chars().count(),
                77,
                "Banner line width must be exactly 77 chars: {}",
                line
            );
        }
    }

    #[test]
    fn test_normalize_workspace_path() {
        let current = env::current_dir().unwrap();
        assert_eq!(normalize_workspace_path(""), current);
        assert_eq!(normalize_workspace_path("."), current);
        assert_eq!(normalize_workspace_path("\"\""), current);
    }

    #[test]
    fn test_open_file_manager_graceful() {
        let temp = tempdir().unwrap();
        open_file_manager(temp.path());
    }

    #[tokio::test]
    async fn test_daemon_cli_status_when_stopped() {
        let res = handle_daemon_cli(&["status".to_string()]).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_daemon_cli_stop_when_stopped() {
        let res = handle_daemon_cli(&["stop".to_string()]).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_run_interactive_choice_fallback() {
        let cockpit = run_interactive_choice("TURF-TEST-CHOICE", "HOST", "tester", 24, 80).await;
        assert_eq!(cockpit.session_code, "TURF-TEST-CHOICE");
        assert_eq!(cockpit.role, "HOST");
        assert_eq!(cockpit.username, "tester");
        assert_eq!(cockpit.tabs.len(), 1);
    }
}
