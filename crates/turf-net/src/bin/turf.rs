//! TurfCode primary command line interface.
//!
//! Supports interactive cockpit sessions, daemon lifecycle management, multi-session management
//! (`turf list`, `turf attach <name>`, `turf session ...`), and programmatic CLI pane automation
//! (`turf pane read`, `turf pane send-keys`, `turf pane split`).

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    turf_net::cli::main_cli().await
}
