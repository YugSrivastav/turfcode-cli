use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use uuid::Uuid;

use turf_protocol::intent::{IntentBoardData, TaskCard, TaskStatus};

use crate::error::IntentError;
use crate::lock_manager::normalize_lock_path;

#[derive(Debug, Clone)]
pub enum IntentEvent {
    TaskAdded(TaskCard),
    TaskUpdated(TaskCard),
    TaskRemoved(String),
}

/// Thread-safe storage for the Intent Board with atomic JSON disk persistence.
#[derive(Clone)]
pub struct IntentBoard {
    cwd: PathBuf,
    intent_path: PathBuf,
    tasks: Arc<RwLock<Vec<TaskCard>>>,
    events_tx: broadcast::Sender<IntentEvent>,
}

impl IntentBoard {
    pub fn new(cwd: impl Into<PathBuf>, intent_path: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        let intent_path = intent_path.into();
        let (events_tx, _) = broadcast::channel(128);

        let board = Self {
            cwd,
            intent_path,
            tasks: Arc::new(RwLock::new(Vec::new())),
            events_tx,
        };

        let _ = board.load_from_disk();
        board
    }

    /// Convenience constructor using cwd and `.turf/intent-board.json`.
    pub fn with_cwd(cwd: impl Into<PathBuf>) -> Self {
        let cwd_buf = cwd.into();
        let intent_path = cwd_buf.join(".turf").join("intent-board.json");
        Self::new(cwd_buf, intent_path)
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn intent_path(&self) -> &Path {
        &self.intent_path
    }

    pub fn subscribe(&self) -> broadcast::Receiver<IntentEvent> {
        self.events_tx.subscribe()
    }

    /// Normalizes and cleans task fields.
    fn normalize_task(&self, mut task: TaskCard) -> TaskCard {
        task.title = task.title.trim().to_string();
        if task.title.is_empty() {
            task.title = "Untitled Task".to_string();
        }
        task.assignee = task.assignee.trim().to_string();
        if task.assignee.is_empty() {
            task.assignee = "Unassigned".to_string();
        }

        task.target_files = task
            .target_files
            .into_iter()
            .map(|f| normalize_lock_path(&f, &self.cwd))
            .filter(|f| !f.is_empty())
            .collect();

        task
    }

    /// Atomically persists current task board to disk using temp file rename.
    pub fn save_to_disk(&self) -> Result<(), IntentError> {
        if let Some(parent) = self.intent_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        let tasks_snapshot = self.tasks.read().clone();
        let payload = IntentBoardData {
            version: 1,
            updated_at: Utc::now(),
            tasks: tasks_snapshot,
        };

        let json = serde_json::to_string_pretty(&payload)?;
        let temp_filename = format!(
            "{}.tmp.{}",
            self.intent_path.file_name().and_then(|f| f.to_str()).unwrap_or("intent-board.json"),
            Uuid::new_v4()
        );
        let temp_path = self.intent_path.with_file_name(temp_filename);

        std::fs::write(&temp_path, json)?;
        std::fs::rename(&temp_path, &self.intent_path)?;

        Ok(())
    }

    /// Load existing tasks from disk if `.turf/intent-board.json` exists.
    pub fn load_from_disk(&self) -> Result<Vec<TaskCard>, IntentError> {
        if !self.intent_path.exists() {
            // Also check fallback legacy path .turf/intent.json if present
            let legacy_path = self.cwd.join(".turf").join("intent.json");
            if legacy_path.exists() {
                return self.load_from_path(&legacy_path);
            }
            return Ok(Vec::new());
        }

        self.load_from_path(&self.intent_path.clone())
    }

    fn load_from_path(&self, path: &Path) -> Result<Vec<TaskCard>, IntentError> {
        let raw = std::fs::read_to_string(path)?;
        let data: IntentBoardData = match serde_json::from_str(&raw) {
            Ok(d) => d,
            Err(_) => {
                // Fallback: raw list of tasks
                let tasks: Vec<TaskCard> = serde_json::from_str(&raw)?;
                IntentBoardData {
                    version: 1,
                    updated_at: Utc::now(),
                    tasks,
                }
            }
        };

        let mut tasks_lock = self.tasks.write();
        tasks_lock.clear();
        for task in data.tasks {
            tasks_lock.push(self.normalize_task(task));
        }

        Ok(tasks_lock.clone())
    }

    /// Add a new task card or update if an existing task with same id exists.
    pub async fn add_task(&self, task: TaskCard) -> Result<TaskCard, IntentError> {
        let normalized = self.normalize_task(task);
        {
            let mut tasks = self.tasks.write();
            if let Some(pos) = tasks.iter().position(|t| t.id == normalized.id) {
                tasks[pos] = normalized.clone();
            } else {
                tasks.push(normalized.clone());
            }
        }

        self.save_to_disk()?;
        let _ = self.events_tx.send(IntentEvent::TaskAdded(normalized.clone()));
        Ok(normalized)
    }

    /// Update status of a task by ID.
    pub async fn update_task_status(&self, id: &str, status: TaskStatus) -> Result<Option<TaskCard>, IntentError> {
        let updated = {
            let mut tasks = self.tasks.write();
            if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
                task.status = status;
                Some(task.clone())
            } else {
                None
            }
        };

        if let Some(ref task) = updated {
            self.save_to_disk()?;
            let _ = self.events_tx.send(IntentEvent::TaskUpdated(task.clone()));
        }

        Ok(updated)
    }

    /// Update task details.
    pub async fn update_task<F>(&self, id: &str, update_fn: F) -> Result<Option<TaskCard>, IntentError>
    where
        F: FnOnce(&mut TaskCard),
    {
        let updated = {
            let mut tasks = self.tasks.write();
            if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
                update_fn(task);
                let normalized = self.normalize_task(task.clone());
                *task = normalized.clone();
                Some(normalized)
            } else {
                None
            }
        };

        if let Some(ref task) = updated {
            self.save_to_disk()?;
            let _ = self.events_tx.send(IntentEvent::TaskUpdated(task.clone()));
        }

        Ok(updated)
    }

    /// Remove a task card by ID.
    pub async fn remove_task(&self, id: &str) -> Result<Option<TaskCard>, IntentError> {
        let removed = {
            let mut tasks = self.tasks.write();
            tasks.iter().position(|t| t.id == id).map(|pos| tasks.remove(pos))
        };

        if let Some(ref task) = removed {
            self.save_to_disk()?;
            let _ = self.events_tx.send(IntentEvent::TaskRemoved(task.id.clone()));
        }

        Ok(removed)
    }

    /// Get a task card by ID.
    pub async fn get_task(&self, id: &str) -> Option<TaskCard> {
        let tasks = self.tasks.read();
        tasks.iter().find(|t| t.id == id).cloned()
    }

    /// Get all task cards.
    pub async fn get_tasks(&self) -> Vec<TaskCard> {
        let tasks = self.tasks.read();
        tasks.clone()
    }
}
