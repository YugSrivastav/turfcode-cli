use std::os::unix::io::RawFd;
use std::path::Path;

use crate::ProcessInfo;

/// Query the foreground process of the pseudo-terminal via `tcgetpgrp` and read its process info.
pub fn get_foreground_process(master_fd: RawFd) -> Option<ProcessInfo> {
    let pgrp = unsafe { libc::tcgetpgrp(master_fd) };
    if pgrp <= 0 {
        return None;
    }

    // Try reading /proc/<pid>/comm and /proc/<pid>/cmdline on Linux
    let comm = read_proc_comm(pgrp);
    let cmdline = read_proc_cmdline(pgrp);

    // If neither exists and /proc exists, the process likely exited
    if comm.is_none() && cmdline.is_none() {
        if Path::new("/proc").exists() {
            return None;
        }
        // Fallback for non-Linux Unix (e.g. macOS / BSD where /proc is not mounted)
        return Some(ProcessInfo {
            pid: pgrp as u32,
            name: format!("process-{}", pgrp),
            cmdline: String::new(),
        });
    }

    let name = comm.unwrap_or_else(|| {
        cmdline
            .as_ref()
            .and_then(|c| {
                c.split_whitespace().next().map(|cmd0| {
                    Path::new(cmd0)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(cmd0)
                        .to_string()
                })
            })
            .unwrap_or_else(|| format!("process-{}", pgrp))
    });

    let cmdline_str = cmdline.unwrap_or_else(|| name.clone());

    Some(ProcessInfo {
        pid: pgrp as u32,
        name,
        cmdline: cmdline_str,
    })
}

/// Read the command name from `/proc/<pid>/comm`.
pub fn read_proc_comm(pid: i32) -> Option<String> {
    let path = format!("/proc/{}/comm", pid);
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

/// Read the command line and arguments from `/proc/<pid>/cmdline`.
pub fn read_proc_cmdline(pid: i32) -> Option<String> {
    let path = format!("/proc/{}/cmdline", pid);
    let bytes = std::fs::read(path).ok()?;
    if bytes.is_empty() {
        return None;
    }
    let parts: Vec<&str> = bytes
        .split(|&b| b == 0)
        .filter_map(|slice| std::str::from_utf8(slice).ok())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_self_proc_introspection() {
        let pid = std::process::id() as i32;
        let comm = read_proc_comm(pid);
        assert!(comm.is_some(), "Should be able to read /proc/self/comm on Linux");
        let cmdline = read_proc_cmdline(pid);
        assert!(cmdline.is_some(), "Should be able to read /proc/self/cmdline on Linux");
    }
}
