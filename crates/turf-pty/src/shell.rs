use crate::config::PtyOptions;
use portable_pty::CommandBuilder;

/// Determine the default shell for the current operating system platform.
/// On Unix/Linux/WSL/macOS: evaluates `$SHELL`, falling back to `bash`.
/// On Windows: defaults to `powershell.exe`.
pub fn default_shell() -> String {
    #[cfg(windows)]
    {
        "powershell.exe".to_string()
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "bash".to_string())
    }
}

/// Build a `portable_pty::CommandBuilder` from `PtyOptions`.
pub fn build_command(options: &PtyOptions) -> CommandBuilder {
    let program = match &options.command {
        Some(cmd) => cmd.clone(),
        None => default_shell(),
    };

    let mut cmd = CommandBuilder::new(program);

    for arg in &options.args {
        cmd.arg(arg);
    }

    // Standard environment configuration for rich ANSI terminal multiplexing
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    for (k, v) in &options.env {
        cmd.env(k, v);
    }

    if let Some(ref cwd) = options.working_dir {
        cmd.cwd(cwd);
    }

    cmd
}
