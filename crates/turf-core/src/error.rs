use thiserror::Error;

#[derive(Error, Debug)]
pub enum LockError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Lock conflict on '{conflict_file}' held by '{held_by}' (task #{task_id})")]
    Conflict {
        conflict_file: String,
        held_by: String,
        task_id: String,
        message: String,
    },

    #[error("Lock error: {0}")]
    Custom(String),
}

#[derive(Error, Debug)]
pub enum IntentError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Task not found with id: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),
}

#[derive(Error, Debug)]
pub enum WorktreeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Git execution failed (exit code {status:?}): {stderr}")]
    GitCommand { status: Option<i32>, stderr: String },

    #[error("Merge conflict merging '{branch}' into '{target_branch}': {details}")]
    MergeConflict {
        branch: String,
        target_branch: String,
        details: String,
    },

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Worktree error: {0}")]
    Custom(String),
}
