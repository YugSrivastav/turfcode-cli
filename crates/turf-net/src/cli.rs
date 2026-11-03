use crate::client::TunnelClient;
use crate::crypto;
use crate::error::NetError;
use crate::server::TunnelServer;
use std::env;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use turf_core::{intent_board::IntentBoard, lock_manager::LockManager};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Banner and splash printing for TurfCode CLI.
pub fn print_splash() {
    println!(
        "\x1b[1;32m
  ╔═══════════════════════════════════════════════════════════════════╗
  ║   ████████╗██╗   ██╗██████╗ ███████╗ ██████╗ ██████╗ ██████╗    ║
  ║   ╚══██╔══╝██║   ██║██╔══██╗██╔════╝██╔════╝██╔═══██╗██╔══██╗   ║
  ║      ██║   ██║   ██║██████╔╝█████╗  ██║     ██║   ██║██║  ██║   ║
  ║      ██║   ██║   ██║██╔══██╗██╔══╝  ██║     ██║   ██║██║  ██║   ║
  ║      ██║   ╚██████╔╝██║  ██║██║     ╚██████╗╚██████╔╝██████╔╝   ║
  ║      ╚═╝    ╚═════╝ ╚═╝  ╚═╝╚═╝      ╚═════╝ ╚═════╝ ╚═════╝    ║
  ║                   ELECTRIC LIME COCKPIT v{}                    ║
  ╚═══════════════════════════════════════════════════════════════════╝
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
  \x1b[32mturf --help, -h\x1b[0m          Show this help information
  \x1b[32mturf --version, -v\x1b[0m       Show version
"
    );
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

/// Host a TurfCode session in the target directory.
pub async fn run_host(target_dir: &Path, port: u16) -> Result<(), NetError> {
    if !target_dir.exists() {
        std::fs::create_dir_all(target_dir)?;
    }

    let room_code = crypto::generate_room_code();
    let intent_board = Arc::new(IntentBoard::with_cwd(target_dir));
    let lock_manager = Arc::new(LockManager::with_cwd(target_dir));

    let mut server = TunnelServer::new(Some(room_code.clone()))
        .with_host("127.0.0.1")
        .with_port(port)
        .with_intent_board(intent_board)
        .with_lock_manager(lock_manager);

    let local_addr = server.start().await?;

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
    let clean_code = crypto::normalize_room_code(room_code);
    println!(
        "\n  \x1b[1;32m⚡ JOINING TURF: \x1b[1;33m{}\x1b[0m",
        clean_code
    );
    println!("  \x1b[2m🔗 Connecting to: \x1b[36m{}\x1b[0m...", ws_url);

    let username = env::var("USER")
        .or_else(|_| env::var("USERNAME"))
        .unwrap_or_else(|_| "Peer".into());

    let mut client = TunnelClient::new(&clean_code, &username).with_role("Peer");
    let mut event_rx = client.subscribe();

    let peer_id = client.connect(ws_url).await?;

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

/// Interactive launcher prompt conforming to [MD-041].
pub async fn run_interactive_prompt() -> Result<(), NetError> {
    print_splash();

    println!(
        "  \x1b[32m[1]\x1b[0m Create a Turf   \x1b[2m(Host a local folder / live session)\x1b[0m"
    );
    println!("  \x1b[32m[2]\x1b[0m Join a Turf     \x1b[2m(Connect to an active room code)\x1b[0m");
    println!("  \x1b[2m[3] Exit\x1b[0m\n");

    print!("  \x1b[32mSelect option [1-3]: \x1b[0m");
    io::stdout().flush().ok();

    let mut line = String::new();
    let stdin = io::stdin();
    stdin.lock().read_line(&mut line).ok();
    let choice = line.trim();

    match choice {
        "1" => {
            print!("  \x1b[32mEnter folder to host [default: .]: \x1b[0m");
            io::stdout().flush().ok();
            let mut dir_line = String::new();
            stdin.lock().read_line(&mut dir_line).ok();
            let target_dir = normalize_workspace_path(&dir_line);
            run_host(&target_dir, 0).await?;
        }
        "2" => {
            print!("  \x1b[32mEnter 6-character room code (e.g. TURF-7A9B): \x1b[0m");
            io::stdout().flush().ok();
            let mut code_line = String::new();
            stdin.lock().read_line(&mut code_line).ok();
            let room_code = code_line.trim();

            print!("  \x1b[32mEnter host WebSocket URL [default: ws://127.0.0.1:7873]: \x1b[0m");
            io::stdout().flush().ok();
            let mut url_line = String::new();
            stdin.lock().read_line(&mut url_line).ok();
            let url = if url_line.trim().is_empty() {
                "ws://127.0.0.1:7873"
            } else {
                url_line.trim()
            };

            run_join(room_code, url).await?;
        }
        _ => {
            println!("  Exiting.");
        }
    }

    Ok(())
}

/// Main CLI entry point.
pub async fn main_cli() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() <= 1 {
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
        "host" => {
            let target = if args.len() > 2 {
                normalize_workspace_path(&args[2])
            } else {
                normalize_workspace_path(".")
            };
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
