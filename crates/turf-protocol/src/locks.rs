use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents an active file lock held by a task or agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileLock {
    pub file_path: String,
    pub owner_id: String,
    pub task_id: String,
    pub timestamp: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub ttl_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pid: Option<u32>,
}

impl FileLock {
    pub fn new(
        file_path: impl Into<String>,
        owner_id: impl Into<String>,
        task_id: impl Into<String>,
        ttl_ms: u64,
        pid: Option<u32>,
    ) -> Self {
        let now = Utc::now();
        Self {
            file_path: file_path.into(),
            owner_id: owner_id.into(),
            task_id: task_id.into(),
            timestamp: now,
            last_heartbeat: now,
            ttl_ms,
            pid,
        }
    }

    /// Check if this lock's lease has expired according to TTL.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        let elapsed_ms = (now - self.last_heartbeat).num_milliseconds();
        elapsed_ms > self.ttl_ms as i64
    }
}

/// Result of an atomic sequence lock acquisition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcquireResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub conflict_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub held_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub error: Option<String>,
    #[serde(default)]
    pub locks: Vec<FileLock>,
}

impl AcquireResult {
    pub fn success(locks: Vec<FileLock>) -> Self {
        Self {
            success: true,
            conflict_file: None,
            held_by: None,
            task_id: None,
            error: None,
            locks,
        }
    }

    pub fn conflict(
        conflict_file: impl Into<String>,
        held_by: impl Into<String>,
        task_id: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            success: false,
            conflict_file: Some(conflict_file.into()),
            held_by: Some(held_by.into()),
            task_id: Some(task_id.into()),
            error: Some(error.into()),
            locks: Vec::new(),
        }
    }

    pub fn is_success(&self) -> bool {
        self.success
    }
}

/// Result of releasing locks for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseResult {
    pub success: bool,
    pub released: Vec<FileLock>,
}

/// Result of host force-unlocking a specific file path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForceUnlockResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub released: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub held_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,
}

/// Persistent JSON format for .turf/locks.json
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockTable {
    pub version: u32,
    pub updated_at: DateTime<Utc>,
    pub locks: Vec<FileLock>,
}

impl Default for LockTable {
    fn default() -> Self {
        Self {
            version: 1,
            updated_at: Utc::now(),
            locks: Vec::new(),
        }
    }
}
