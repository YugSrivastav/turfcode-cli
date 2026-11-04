use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::client::DaemonClient;
use crate::socket::default_socket_path;

/// Probe whether a TurfCode daemon is currently responsive at the given socket path.
pub async fn probe_daemon(socket_path: &Path) -> bool {
    if !socket_path.exists() {
        return false;
    }

    match DaemonClient::connect(socket_path).await {
        Ok(client) => matches!(
            tokio::time::timeout(Duration::from_millis(350), client.status()).await,
            Ok(Ok(_))
        ),
        Err(_) => false,
    }
}

/// Ensure the background TurfCode daemon is running, auto-spawning it if unresponsive.
/// Conforms to [MD-051]:
/// - Probes default socket (`$TURF_SOCKET_PATH` or `~/.turf/turf.sock`)
/// - Spawns detached background daemon if unresponsive
/// - Waits up to 2 seconds for socket readiness
pub async fn ensure_daemon_running() -> anyhow::Result<PathBuf> {
    let socket_path = default_socket_path();

    if probe_daemon(&socket_path).await {
        tracing::debug!(
            "TurfCode daemon is already running at {}",
            socket_path.display()
        );
        return Ok(socket_path);
    }

    // Clean up stale socket file if it exists
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    spawn_detached_daemon(&socket_path)?;

    // Wait up to 2.0 seconds for socket readiness
    let poll_interval = Duration::from_millis(50);
    let attempts = 40; // 40 * 50ms = 2000ms

    for _ in 0..attempts {
        tokio::time::sleep(poll_interval).await;
        if probe_daemon(&socket_path).await {
            tracing::info!(
                "TurfCode daemon auto-spawned and ready at {}",
                socket_path.display()
            );
            return Ok(socket_path);
        }
    }

    anyhow::bail!(
        "Timed out waiting up to 2 seconds for TurfCode daemon socket to become ready at '{}'",
        socket_path.display()
    );
}

/// Helper to spawn the detached background daemon process.
fn spawn_detached_daemon(socket_path: &Path) -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let exe_dir = current_exe.parent().unwrap_or_else(|| Path::new("."));

    // Check custom override, sibling turf-daemon binary, or current_exe with daemon run subcommand
    let (cmd_bin, args) = if let Ok(bin_override) = std::env::var("TURF_DAEMON_BIN") {
        (
            PathBuf::from(bin_override),
            vec![
                "--socket".to_string(),
                socket_path.to_string_lossy().to_string(),
            ],
        )
    } else {
        let sibling_daemon = exe_dir.join("turf-daemon");
        if sibling_daemon.exists() {
            (
                sibling_daemon,
                vec![
                    "--socket".to_string(),
                    socket_path.to_string_lossy().to_string(),
                ],
            )
        } else {
            (
                current_exe,
                vec![
                    "daemon".to_string(),
                    "run".to_string(),
                    "--socket".to_string(),
                    socket_path.to_string_lossy().to_string(),
                ],
            )
        }
    };

    let log_path = socket_path
        .parent()
        .map(|p| p.join("daemon.log"))
        .unwrap_or_else(|| PathBuf::from("daemon.log"));

    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| anyhow::anyhow!("Failed to open daemon log '{}': {e}", log_path.display()))?;

    let err_file = log_file.try_clone()?;

    let mut cmd = std::process::Command::new(&cmd_bin);
    cmd.args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::from(log_file))
        .stderr(std::process::Stdio::from(err_file));

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    cmd.spawn().map_err(|e| {
        anyhow::anyhow!(
            "Failed to spawn daemon binary '{}': {e}",
            cmd_bin.display()
        )
    })?;

    Ok(())
}
