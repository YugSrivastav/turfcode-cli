pub mod config;
pub mod error;
pub mod session;
pub mod shell;
pub mod stream;
pub mod traits;

#[cfg(unix)]
pub mod unix;

pub use config::PtyOptions;
pub use error::PtyError;
pub use session::{PtyProcessHandle, PtySession};
pub use shell::default_shell;
pub use stream::{PtyReader, PtyWriteCommand, PtyWriter};
pub use traits::{PtyReadStream, PtyWriteStream};

/// Representation of an introspected foreground process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cmdline: String,
}

impl ProcessInfo {
    pub fn new(pid: u32, name: impl Into<String>, cmdline: impl Into<String>) -> Self {
        Self {
            pid,
            name: name.into(),
            cmdline: cmdline.into(),
        }
    }

    /// Extract a concise display command suitable for dynamic tab badges and pane headers.
    /// (e.g. `vim main.rs`, `cargo test`, `git diff`, `bash`)
    pub fn display_cmd(&self) -> String {
        let cmd = self.cmdline.trim();
        if cmd.is_empty() {
            return self.name.clone();
        }

        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return self.name.clone();
        }

        // Clean up first argument (executable path)
        let exe = parts[0].trim_start_matches('-');
        let exe_name = std::path::Path::new(exe)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(exe);

        // Check if it's an interactive shell
        let is_shell = matches!(exe_name, "bash" | "zsh" | "fish" | "sh" | "dash");
        if is_shell {
            // If shell with only flags like `-i`, `--login`, just return shell name
            let has_subcommand = parts[1..].iter().any(|arg| !arg.starts_with('-') && *arg != "-c");
            if !has_subcommand {
                return exe_name.to_string();
            }
        }

        // For non-shells or shells running commands:
        if parts.len() == 1 {
            exe_name.to_string()
        } else {
            // E.g. `vim main.rs`, `cargo test`, `git diff`
            // Take up to 2 extra args, but avoid overly long titles
            let extra_args = parts[1..].iter().take(2).copied().collect::<Vec<_>>().join(" ");
            let formatted = format!("{} {}", exe_name, extra_args);
            if formatted.len() > 32 {
                format!("{}...", &formatted[..29])
            } else {
                formatted
            }
        }
    }
}
