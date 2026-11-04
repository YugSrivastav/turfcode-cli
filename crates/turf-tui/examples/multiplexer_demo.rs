//! Herdr-Grade Ratatui Spatial Multiplexer Demo for TurfCode CLI.
//!
//! Run with:
//! `cargo run -p turf-tui --example multiplexer_demo`

use std::io::stdout;

use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use turf_protocol::intent::{TaskCard, TaskStatus};
use turf_protocol::locks::FileLock;
use turf_tui::{PeerInfo, TerminalCockpit};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = stdout();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let size = terminal.size()?;
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "pilot".to_string());

    // Try to spawn with live POSIX/ConPTY shell, fallback to headless on error
    let mut cockpit = match TerminalCockpit::new_live("TURF-7A9B", "HOST", &username, size.height, size.width) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Notice: Live PTY unavailable ({e}), launching in simulated mode");
            let mut c = TerminalCockpit::new_headless(
                "TURF-7A9B",
                "HOST",
                &username,
                size.height as usize,
                size.width as usize,
            );
            if let Some(tab) = c.tabs.get_mut(c.active_tab_index) {
                if let Some(pane) = tab.pane_manager.active_pane_mut() {
                    pane.terminal.process_str(
                        "\x1b[1;38;2;137;180;250mTurfCode Multiplexer Cockpit (60fps)\x1b[0m\r\n\
                         \x1b[38;2;108;112;134mType ? in prompt or press Alt+Enter to toggle prompt focus.\x1b[0m\r\n\r\n\
                         turfcode-cli$ \x1b[38;2;205;214;244mcargo check --workspace\x1b[0m\r\n\
                            Checking turf-tui v0.1.0\r\n\
                            Finished dev profile [unoptimized + debuginfo] in 0.04s\r\n\r\n\
                         turfcode-cli$ "
                    );
                }
            }
            c
        }
    };

    // Pre-populate sample intents & tasks
    cockpit.sidebar_state.tasks.push(TaskCard::new(
        "task-1",
        "Spatial layout engine",
        &username,
        TaskStatus::Done,
        vec!["crates/turf-tui/src/layout.rs".to_string()],
    ));
    cockpit.sidebar_state.tasks.push(TaskCard::new(
        "task-2",
        "Herdr borderless multiplexer",
        &username,
        TaskStatus::InProgress,
        vec!["crates/turf-tui/src/cockpit.rs".to_string()],
    ));
    cockpit.sidebar_state.tasks.push(TaskCard::new(
        "task-3",
        "Zero-latency PTY frame stream",
        "ayush",
        TaskStatus::Queued,
        vec!["crates/turf-pty".to_string()],
    ));

    // Pre-populate sample locks
    cockpit.sidebar_state.locks.push(FileLock::new(
        "crates/turf-tui/src/cockpit.rs",
        &username,
        "task-2",
        60_000,
        std::process::id().into(),
    ));

    // Pre-populate connected peers
    cockpit.sidebar_state.peers = vec![
        PeerInfo {
            name: username.clone(),
            role: "host".to_string(),
            is_self: true,
            is_online: true,
        },
        PeerInfo {
            name: "ayush".to_string(),
            role: "peer".to_string(),
            is_self: false,
            is_online: true,
        },
    ];

    // Run the 60fps frame blitting event loop
    cockpit.run_event_loop(&mut terminal)?;

    Ok(())
}
