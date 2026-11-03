use std::time::Duration;
use tempfile::tempdir;
use turf_core::LockManager;

#[tokio::test]
async fn test_atomic_sequence_acquisition_success() {
    let dir = tempdir().unwrap();
    let manager = LockManager::with_cwd(dir.path());

    let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
    let res = manager
        .acquire_atomic_sequence("Host", "task-1", &files, 5000)
        .await
        .unwrap();

    assert!(res.is_success());
    assert_eq!(res.locks.len(), 2);

    let active_locks = manager.get_locks().await;
    assert_eq!(active_locks.len(), 2);
    assert!(manager.is_locked("src/main.rs").await.is_some());
    assert!(manager.is_locked("src/lib.rs").await.is_some());
}

#[tokio::test]
async fn test_atomic_collision_prevention_fail_fast_no_partial_locks() {
    let dir = tempdir().unwrap();
    let manager = LockManager::with_cwd(dir.path());

    // Task 1 locks file A and file B
    let files_1 = vec!["src/a.rs".to_string(), "src/b.rs".to_string()];
    let res_1 = manager
        .acquire_atomic_sequence("Agent-1", "task-1", &files_1, 5000)
        .await
        .unwrap();
    assert!(res_1.is_success());

    // Task 2 attempts to lock file B and file C
    // File B is already held by Task 1!
    let files_2 = vec!["src/b.rs".to_string(), "src/c.rs".to_string()];
    let res_2 = manager
        .acquire_atomic_sequence("Agent-2", "task-2", &files_2, 5000)
        .await
        .unwrap();

    // Must fail fast!
    assert!(!res_2.is_success());
    assert_eq!(res_2.conflict_file.as_deref(), Some("src/b.rs"));
    assert_eq!(res_2.held_by.as_deref(), Some("Agent-1"));
    assert_eq!(res_2.task_id.as_deref(), Some("task-1"));

    // CRITICAL: File C must NOT be locked! Zero partial locks allowed ([MD-023], [MD-040]).
    assert!(
        manager.is_locked("src/c.rs").await.is_none(),
        "Partial lock hazard: src/c.rs should not be held!"
    );

    // Release Task 1 locks
    let rel = manager.release_locks("task-1").await.unwrap();
    assert_eq!(rel.released.len(), 2);

    // Now Task 2 can acquire file B and file C atomically
    let res_2_retry = manager
        .acquire_atomic_sequence("Agent-2", "task-2", &files_2, 5000)
        .await
        .unwrap();
    assert!(res_2_retry.is_success());
    assert!(manager.is_locked("src/b.rs").await.is_some());
    assert!(manager.is_locked("src/c.rs").await.is_some());
}

#[tokio::test]
async fn test_heartbeat_renewal_and_ttl_expiration() {
    let dir = tempdir().unwrap();
    let manager = LockManager::with_cwd(dir.path());

    // Acquire lock with short TTL (80ms)
    let files = vec!["src/short_lease.rs".to_string()];
    let res = manager
        .acquire_atomic_sequence("Agent-1", "task-ttl", &files, 80)
        .await
        .unwrap();
    assert!(res.is_success());

    // Send heartbeat at 40ms to keep lease alive
    tokio::time::sleep(Duration::from_millis(40)).await;
    let renewed = manager.heartbeat("task-ttl").await.unwrap();
    assert!(renewed);

    // At 70ms from start (30ms from heartbeat), lock is still valid
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert!(manager.is_locked("src/short_lease.rs").await.is_some());

    // Now wait 120ms without sending heartbeat -> must expire and dissolve!
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(
        manager.is_locked("src/short_lease.rs").await.is_none(),
        "Lock should have expired and dissolved automatically"
    );
}

#[tokio::test]
async fn test_background_tokio_heartbeat_monitor() {
    let dir = tempdir().unwrap();
    let manager = LockManager::with_cwd(dir.path());

    // Start background monitor sweeping every 20ms
    let monitor_handle = manager.start_heartbeat_monitor(Duration::from_millis(20));

    let files = vec!["src/monitor_test.rs".to_string()];
    manager
        .acquire_atomic_sequence("Agent-1", "task-mon", &files, 50)
        .await
        .unwrap();

    assert!(manager.is_locked("src/monitor_test.rs").await.is_some());

    // Wait 100ms for monitor to sweep and dissolve the lock
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Lock map should now be completely empty
    let locks = manager.get_locks().await;
    assert!(
        locks.is_empty(),
        "Background task should have auto-dissolved stale lock"
    );

    monitor_handle.abort();
}

#[tokio::test]
async fn test_process_bound_locks_dissolution_on_process_exit() {
    let dir = tempdir().unwrap();
    let manager = LockManager::with_cwd(dir.path());

    // Spawn a short-lived process (sleep 0.1)
    let mut child = std::process::Command::new("sleep")
        .arg("0.05")
        .spawn()
        .expect("Failed to spawn child process");

    let pid = child.id();

    let files = vec!["src/proc_bound.rs".to_string()];
    // Set a long TTL (10s), but bound to the child process PID
    let res = manager
        .acquire_atomic_sequence_with_pid("Agent-1", "task-proc", &files, 10_000, Some(pid))
        .await
        .unwrap();
    assert!(res.is_success());

    // While process is running, lock is held
    assert!(manager.is_locked("src/proc_bound.rs").await.is_some());

    // Wait for the child process to terminate
    let _ = child.wait().expect("Child process failed to exit");

    // Give kernel a moment to reap the PID
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Lock should now be dissolved because the bound process has exited
    assert!(
        manager.is_locked("src/proc_bound.rs").await.is_none(),
        "Lock bound to terminated process must auto-dissolve!"
    );
}

#[tokio::test]
async fn test_force_unlock_by_host() {
    let dir = tempdir().unwrap();
    let manager = LockManager::with_cwd(dir.path());

    let files = vec!["src/stuck_file.rs".to_string()];
    manager
        .acquire_atomic_sequence("StuckAgent", "task-stuck", &files, 10_000)
        .await
        .unwrap();

    assert!(manager.is_locked("src/stuck_file.rs").await.is_some());

    // Host force-unlocks the file
    let unlock_res = manager.force_unlock("src/stuck_file.rs").await.unwrap();
    assert!(unlock_res.success);
    assert_eq!(unlock_res.released.as_deref(), Some("src/stuck_file.rs"));
    assert_eq!(unlock_res.held_by.as_deref(), Some("StuckAgent"));

    assert!(manager.is_locked("src/stuck_file.rs").await.is_none());
}

#[tokio::test]
async fn test_disk_persistence_and_reload() {
    let dir = tempdir().unwrap();
    let locks_path = dir.path().join(".turf").join("locks.json");

    {
        let manager1 = LockManager::new(dir.path(), &locks_path, 10_000);
        let files = vec!["src/persist.rs".to_string()];
        manager1
            .acquire_atomic_sequence("Host", "task-p", &files, 10_000)
            .await
            .unwrap();
        assert!(locks_path.exists());
    }

    // Create a new manager instance and verify it reloads from disk
    {
        let manager2 = LockManager::new(dir.path(), &locks_path, 10_000);
        let lock = manager2.is_locked("src/persist.rs").await;
        assert!(lock.is_some());
        assert_eq!(lock.unwrap().owner_id, "Host");
    }
}
