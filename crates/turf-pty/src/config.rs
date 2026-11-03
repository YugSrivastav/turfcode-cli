use std::collections::HashMap;
use std::path::PathBuf;

/// Configuration options for allocating a PTY and spawning a child process.
#[derive(Debug, Clone)]
pub struct PtyOptions {
    pub rows: u16,
    pub cols: u16,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_dir: Option<PathBuf>,
}

impl Default for PtyOptions {
    fn default() -> Self {
        Self {
            rows: 24,
            cols: 80,
            command: None,
            args: Vec::new(),
            env: HashMap::new(),
            working_dir: None,
        }
    }
}

impl PtyOptions {
    /// Create options with specified initial terminal dimensions.
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            ..Default::default()
        }
    }

    /// Set an explicit command executable to run.
    pub fn with_command<S: Into<String>>(mut self, command: S) -> Self {
        self.command = Some(command.into());
        self
    }

    /// Set command line arguments.
    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    /// Insert an environment variable.
    pub fn with_env<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set the working directory for the spawned process.
    pub fn with_working_dir<P: Into<PathBuf>>(mut self, dir: P) -> Self {
        self.working_dir = Some(dir.into());
        self
    }
}
