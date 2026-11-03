use thiserror::Error;

#[derive(Error, Debug)]
pub enum PtyError {
    #[error("PTY allocation failed: {0}")]
    Allocation(String),

    #[error("Failed to spawn process in PTY: {0}")]
    Spawn(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Resize error: {0}")]
    Resize(String),

    #[error("Channel error: {0}")]
    Channel(String),

    #[error("Process lifecycle error: {0}")]
    Process(String),

    #[error("Other error: {0}")]
    Other(String),
}
