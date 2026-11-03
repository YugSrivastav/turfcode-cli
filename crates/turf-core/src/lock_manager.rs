use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use uuid::Uuid;

use turf_protocol::locks::{AcquireResult, FileLock, ForceUnlockResult, LockTable, ReleaseResult};

use crate::error::LockError;

/// Check if a process ID is currently alive on the host operating system.
#[cfg(unix)]
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let res = libc::kill(pid as libc::pid_t, 0);
        if res == 0 {
            true
        } else {
            let err = std::io::Error::last_os_error().raw_os_error();
            // EPERM means process exists but we lack permissions (still alive)
            // ESRCH means process does not exist
            err == Some(libc::EPERM)
        }
    }
}

#[cfg(not(unix))]
pub fn is_pid_alive(_pid: u32) -> bool {
    true
}

/// Normalize file path for lock table indexing.
/// Replaces backslashes, trims leading `./`, and resolves relative to cwd if inside cwd.
pub fn normalize_lock_path(file_path: &str, cwd: &Path) -> String {
    let trimmed = file_path.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let p = Path::new(trimmed);
    let rel_path = if p.is_absolute() {
        if let Ok(rel) = p.strip_prefix(cwd) {
            rel.to_path_buf()
        } else {
            p.to_path_buf()
        }
    } else {
        p.to_path_buf()
    };

    let mut path_str = rel_path.to_string_lossy().replace('\\', "/");
    while let Some(stripped) = path_str.strip_prefix("./") {
        path_str = stripped.to_string();
    }
    path_str
}

#[derive(Debug, Clone)]
pub enum LockEvent {
    Acquired(Vec<FileLock>),
    Released(Vec<FileLock>),
    Expired(Vec<FileLock>),
    ForceUnlocked(String),
}

/// Thread-safe Atomic Multi-File Lock Engine.
#[derive(Clone)]
pub struct LockManager {
    cwd: PathBuf,
    locks_path: PathBuf,
    default_ttl_ms: u64,
    locks: Arc<RwLock<HashMap<String, FileLock>>>,
    events_tx: broadcast::Sender<LockEvent>,
}

impl LockManager {
    /// Create a new LockManager with custom paths and default TTL.
    pub fn new(
        cwd: impl Into<PathBuf>,
        locks_path: impl Into<PathBuf>,
        default_ttl_ms: u64,
    ) -> Self {
        let cwd = cwd.into();
        let locks_path = locks_path.into();
        let (events_tx, _) = broadcast::channel(128);

        let manager = Self {
            cwd,
            locks_path,
            default_ttl_ms,
            locks: Arc::new(RwLock::new(HashMap::new())),
            events_tx,
        };

        let _ = manager.load_from_disk();
        manager
    }

    /// Convenience constructor using cwd and `.turf/locks.json`.
    pub fn with_cwd(cwd: impl Into<PathBuf>) -> Self {
        let cwd_buf = cwd.into();
        let locks_path = cwd_buf.join(".turf").join("locks.json");
        Self::new(cwd_buf, locks_path, 10_000)
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn locks_path(&self) -> &Path {
        &self.locks_path
    }

    pub fn subscribe(&self) -> broadcast::Receiver<LockEvent> {
        self.events_tx.subscribe()
    }

    /// Atomically persists current locks to `.turf/locks.json` using temp file rename.
    fn persist_locks(&self, locks: &HashMap<String, FileLock>) -> Result<(), LockError> {
        if let Some(parent) = self.locks_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let table = LockTable {
            version: 1,
            updated_at: Utc::now(),
            locks: locks.values().cloned().collect(),
        };

        let payload = serde_json::to_string_pretty(&table)?;
        let temp_filename = format!(
            "{}.tmp.{}",
            self.locks_path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("locks.json"),
            Uuid::new_v4()
        );
        let temp_path = self.locks_path.with_file_name(temp_filename);

        std::fs::write(&temp_path, payload)?;
        std::fs::rename(&temp_path, &self.locks_path)?;

        Ok(())
    }

    /// Load existing locks from disk if the file exists.
    pub fn load_from_disk(&self) -> Result<Vec<FileLock>, LockError> {
        if !self.locks_path.exists() {
            return Ok(Vec::new());
        }

        let raw = std::fs::read_to_string(&self.locks_path)?;
        let table: LockTable = match serde_json::from_str(&raw) {
            Ok(t) => t,
            Err(_) => {
                // Try reading as a raw Vec<FileLock> fallback
                let locks: Vec<FileLock> = serde_json::from_str(&raw)?;
                LockTable {
                    version: 1,
                    updated_at: Utc::now(),
                    locks,
                }
            }
        };

        let mut lock_map = self.locks.write();
        lock_map.clear();
        for lock in table.locks {
            let norm = normalize_lock_path(&lock.file_path, &self.cwd);
            if !norm.is_empty() {
                lock_map.insert(norm, lock);
            }
        }

        Ok(lock_map.values().cloned().collect())
    }

    /// Clean any locks where heartbeat has ceased for >ttl_ms or process terminated.
    /// Complies with [MD-007].
    pub fn clean_expired_locks(&self) -> Vec<FileLock> {
        let now = Utc::now();
        let mut expired = Vec::new();

        {
            let mut locks = self.locks.write();
            let mut to_remove = Vec::new();

            for (norm_path, lock) in locks.iter() {
                let is_heartbeat_expired = lock.is_expired(now);
                let is_dead_process = match lock.pid {
                    Some(pid) => !is_pid_alive(pid),
                    None => false,
                };

                if is_heartbeat_expired || is_dead_process {
                    to_remove.push(norm_path.clone());
                    expired.push(lock.clone());
                }
            }

            for key in &to_remove {
                locks.remove(key);
            }

            if !expired.is_empty() {
                let _ = self.persist_locks(&locks);
            }
        }

        if !expired.is_empty() {
            let _ = self.events_tx.send(LockEvent::Expired(expired.clone()));
        }

        expired
    }

    /// Pre-flight atomic sequence file locking.
    ///
    /// Validates that NONE of the declared files in `file_paths` are currently held.
    /// If ANY file is held by a different task, fail-fast immediately returning
    /// conflict info without acquiring partial locks ([MD-023], [MD-040]).
    /// If all are free (or already held by the same task), lock all files atomically.
    pub async fn acquire_atomic_sequence(
        &self,
        owner_id: &str,
        task_id: &str,
        file_paths: &[impl AsRef<str>],
        ttl_ms: u64,
    ) -> Result<AcquireResult, LockError> {
        self.acquire_atomic_sequence_with_pid(owner_id, task_id, file_paths, ttl_ms, None)
            .await
    }

    /// Atomic sequence locking with process ID binding for process-bound auto-dissolution.
    pub async fn acquire_atomic_sequence_with_pid(
        &self,
        owner_id: &str,
        task_id: &str,
        file_paths: &[impl AsRef<str>],
        ttl_ms: u64,
        pid: Option<u32>,
    ) -> Result<AcquireResult, LockError> {
        if file_paths.is_empty() {
            return Ok(AcquireResult::success(Vec::new()));
        }

        // Clean expired locks first before checking availability
        self.clean_expired_locks();

        let normalized_paths: Vec<String> = file_paths
            .iter()
            .map(|fp| normalize_lock_path(fp.as_ref(), &self.cwd))
            .filter(|p| !p.is_empty())
            .collect();

        if normalized_paths.is_empty() {
            return Ok(AcquireResult::success(Vec::new()));
        }

        let effective_ttl = if ttl_ms == 0 {
            self.default_ttl_ms
        } else {
            ttl_ms
        };
        let now = Utc::now();

        let mut locks = self.locks.write();

        // 1. Conflict Check: Verify that NONE of the files are held by another task
        for norm_path in &normalized_paths {
            if let Some(existing) = locks.get(norm_path) {
                // If held by a different task, fail-fast rejection without acquiring ANY partial locks!
                if existing.task_id != task_id {
                    let conflict = AcquireResult::conflict(
                        norm_path.clone(),
                        existing.owner_id.clone(),
                        existing.task_id.clone(),
                        format!(
                            "[LOCK CONFLICT] Cannot acquire lock on {} (Held by {} on Task #{}).",
                            norm_path, existing.owner_id, existing.task_id
                        ),
                    );
                    return Ok(conflict);
                }
            }
        }

        // 2. Atomic Acquisition: All files are free -> acquire all files atomically
        let mut acquired = Vec::new();
        for norm_path in normalized_paths {
            let lock = FileLock {
                file_path: norm_path.clone(),
                owner_id: owner_id.to_string(),
                task_id: task_id.to_string(),
                timestamp: now,
                last_heartbeat: now,
                ttl_ms: effective_ttl,
                pid,
            };
            locks.insert(norm_path, lock.clone());
            acquired.push(lock);
        }

        self.persist_locks(&locks)?;
        let _ = self.events_tx.send(LockEvent::Acquired(acquired.clone()));

        Ok(AcquireResult::success(acquired))
    }

    /// Dissolve all locks associated with a task ID.
    pub async fn release_locks(&self, task_id: &str) -> Result<ReleaseResult, LockError> {
        let mut released = Vec::new();

        {
            let mut locks = self.locks.write();
            let mut to_remove = Vec::new();

            for (norm_path, lock) in locks.iter() {
                if lock.task_id == task_id {
                    to_remove.push(norm_path.clone());
                    released.push(lock.clone());
                }
            }

            for key in &to_remove {
                locks.remove(key);
            }

            if !released.is_empty() {
                self.persist_locks(&locks)?;
            }
        }

        if !released.is_empty() {
            let _ = self.events_tx.send(LockEvent::Released(released.clone()));
        }

        Ok(ReleaseResult {
            success: true,
            released,
        })
    }

    /// Release a single file lock.
    pub async fn release_file(&self, file_path: &str) -> Result<bool, LockError> {
        let norm = normalize_lock_path(file_path, &self.cwd);
        let removed_lock = {
            let mut locks = self.locks.write();
            let removed = locks.remove(&norm);
            if removed.is_some() {
                self.persist_locks(&locks)?;
            }
            removed
        };

        if let Some(lock) = removed_lock {
            let _ = self.events_tx.send(LockEvent::Released(vec![lock]));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Host manual force unlock to break a stuck lock.
    /// Complies with [MD-007].
    pub async fn force_unlock(&self, file_path: &str) -> Result<ForceUnlockResult, LockError> {
        let norm = normalize_lock_path(file_path, &self.cwd);
        let removed_lock = {
            let mut locks = self.locks.write();
            let removed = locks.remove(&norm);
            if removed.is_some() {
                self.persist_locks(&locks)?;
            }
            removed
        };

        if let Some(removed) = removed_lock {
            let _ = self.events_tx.send(LockEvent::ForceUnlocked(norm.clone()));
            Ok(ForceUnlockResult {
                success: true,
                released: Some(norm),
                held_by: Some(removed.owner_id),
                task_id: Some(removed.task_id),
                reason: None,
            })
        } else {
            Ok(ForceUnlockResult {
                success: false,
                released: None,
                held_by: None,
                task_id: None,
                reason: Some("not_locked".to_string()),
            })
        }
    }

    /// Renew lock lease for a task via heartbeat.
    pub async fn heartbeat(&self, task_id: &str) -> Result<bool, LockError> {
        self.heartbeat_with_pid(task_id, None).await
    }

    /// Renew lock lease with optional updated process ID.
    pub async fn heartbeat_with_pid(
        &self,
        task_id: &str,
        pid: Option<u32>,
    ) -> Result<bool, LockError> {
        let now = Utc::now();
        let mut updated = false;

        {
            let mut locks = self.locks.write();
            for lock in locks.values_mut() {
                if lock.task_id == task_id {
                    lock.last_heartbeat = now;
                    if let Some(p) = pid {
                        lock.pid = Some(p);
                    }
                    updated = true;
                }
            }

            if updated {
                self.persist_locks(&locks)?;
            }
        }

        Ok(updated)
    }

    /// Check if a file is currently locked.
    pub async fn is_locked(&self, file_path: &str) -> Option<FileLock> {
        self.clean_expired_locks();
        let norm = normalize_lock_path(file_path, &self.cwd);
        let locks = self.locks.read();
        locks.get(&norm).cloned()
    }

    /// Get all active locks.
    pub async fn get_locks(&self) -> Vec<FileLock> {
        self.clean_expired_locks();
        let locks = self.locks.read();
        locks.values().cloned().collect()
    }

    /// Starts a background Tokio task that periodically sweeps and auto-dissolves
    /// expired locks or locks whose owner process has terminated.
    pub fn start_heartbeat_monitor(&self, interval: Duration) -> tokio::task::JoinHandle<()> {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                manager.clean_expired_locks();
            }
        })
    }
}
