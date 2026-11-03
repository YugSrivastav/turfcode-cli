use std::path::{Path, PathBuf};
use std::process::Command;

use turf_protocol::worktree::{format_co_authors, CoAuthor, SquashMergeResult, WorktreeInfo};

use crate::error::WorktreeError;

#[derive(Debug, Clone, Default)]
pub struct WorktreeOptions {
    pub branch_name: Option<String>,
    pub target_branch: Option<String>,
}

/// Worktree Manager for creating isolated git worktrees and squash-merging with git trailers.
#[derive(Debug, Clone)]
pub struct WorktreeManager {
    repo_root: PathBuf,
    worktrees_dir: PathBuf,
    base_branch: String,
    git_bin: String,
}

impl WorktreeManager {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        let root = repo_root.into();
        let worktrees_dir = root.join(".turf").join("worktrees");
        let git_bin = "git".to_string();

        let mut manager = Self {
            repo_root: root,
            worktrees_dir,
            base_branch: "main".to_string(),
            git_bin,
        };

        // Attempt to auto-detect current branch as base_branch
        if let Ok(branch) = manager.run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None) {
            let b = branch.trim();
            if !b.is_empty() && b != "HEAD" {
                manager.base_branch = b.to_string();
            }
        }

        manager
    }

    pub fn with_worktrees_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.worktrees_dir = dir.into();
        self
    }

    pub fn with_base_branch(mut self, branch: impl Into<String>) -> Self {
        self.base_branch = branch.into();
        self
    }

    pub fn with_git_bin(mut self, bin: impl Into<String>) -> Self {
        self.git_bin = bin.into();
        self
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn worktrees_dir(&self) -> &Path {
        &self.worktrees_dir
    }

    pub fn base_branch(&self) -> &str {
        &self.base_branch
    }

    /// Execute a git command within repoRoot or a custom working directory.
    pub fn run_git(&self, args: &[&str], cwd: Option<&Path>) -> Result<String, WorktreeError> {
        let work_dir = cwd.unwrap_or(&self.repo_root);
        let mut cmd = Command::new(&self.git_bin);
        cmd.args(args);
        cmd.current_dir(work_dir);
        cmd.env("GIT_TERMINAL_PROMPT", "0");

        let output = cmd.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(WorktreeError::GitCommand {
                status: output.status.code(),
                stderr,
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Get resolved filesystem path for a task worktree.
    pub fn get_worktree_path(&self, task_id: &str) -> PathBuf {
        self.worktrees_dir.join(task_id)
    }

    /// Creates an isolated worktree under `.turf/worktrees/<task-id>` with branch `task/<task-id>`.
    pub fn create_task_worktree(
        &self,
        task_id: &str,
        branch_name: Option<&str>,
    ) -> Result<WorktreeInfo, WorktreeError> {
        let id = task_id.trim();
        let branch = match branch_name {
            Some(b) => b.to_string(),
            None => {
                if id.starts_with("task/") || id.starts_with("turf/") {
                    id.to_string()
                } else {
                    format!("task/{id}")
                }
            }
        };

        let worktree_path = self.get_worktree_path(id);

        if !self.worktrees_dir.exists() {
            std::fs::create_dir_all(&self.worktrees_dir)?;
        }

        // If directory already exists and is registered in git worktrees, return existing
        if worktree_path.exists() {
            let active_list = self.list_worktrees().unwrap_or_default();
            let canonical_wt =
                std::fs::canonicalize(&worktree_path).unwrap_or_else(|_| worktree_path.clone());
            if let Some(existing) = active_list.iter().find(|wt| {
                let wt_p = PathBuf::from(&wt.worktree_path);
                let can_p = std::fs::canonicalize(&wt_p).unwrap_or(wt_p);
                can_p == canonical_wt
            }) {
                return Ok(WorktreeInfo {
                    task_id: id.to_string(),
                    branch_name: existing.branch_name.clone(),
                    worktree_path: worktree_path.to_string_lossy().to_string(),
                    commit: existing.commit.clone(),
                });
            } else {
                // Orphaned directory; clean it up
                let _ = std::fs::remove_dir_all(&worktree_path);
            }
        }

        // Check if the branch already exists locally
        let branch_exists = match self.run_git(&["branch", "--list", &branch], None) {
            Ok(output) => !output.trim().is_empty(),
            Err(_) => false,
        };

        let wt_str = worktree_path.to_string_lossy().to_string();

        if branch_exists {
            self.run_git(&["worktree", "add", &wt_str, &branch], None)?;
        } else {
            // Determine commit-ish to branch from
            let start_point = if self
                .run_git(&["rev-parse", "--verify", &self.base_branch], None)
                .is_ok()
            {
                self.base_branch.as_str()
            } else {
                "HEAD"
            };
            self.run_git(
                &["worktree", "add", "-b", &branch, &wt_str, start_point],
                None,
            )?;
        }

        let commit = self
            .run_git(&["rev-parse", "HEAD"], Some(&worktree_path))
            .ok()
            .map(|s| s.trim().to_string());

        Ok(WorktreeInfo {
            task_id: id.to_string(),
            branch_name: branch,
            worktree_path: wt_str,
            commit,
        })
    }

    /// Automated squash-merge into target branch (defaults to main) with Co-authored-by git trailers.
    pub fn merge_task_worktree(
        &self,
        task_id: &str,
        commit_message: &str,
        co_authors: &[CoAuthor],
        options: Option<WorktreeOptions>,
    ) -> Result<SquashMergeResult, WorktreeError> {
        let opts = options.unwrap_or_default();
        let id = task_id.trim();
        let branch = opts.branch_name.unwrap_or_else(|| {
            if id.starts_with("task/") || id.starts_with("turf/") {
                id.to_string()
            } else {
                format!("task/{id}")
            }
        });
        let target_branch = opts
            .target_branch
            .unwrap_or_else(|| self.base_branch.clone());

        // Ensure host checkout is on target_branch
        let current_branch = self
            .run_git(&["rev-parse", "--abbrev-ref", "HEAD"], None)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        if current_branch != target_branch {
            self.run_git(&["checkout", &target_branch], None)?;
        }

        // Perform git squash-merge
        if let Err(err) = self.run_git(&["merge", "--squash", &branch], None) {
            // On conflict or merge failure, abort/reset to keep tree clean
            let _ = self.run_git(&["reset", "--hard", "HEAD"], None);
            let details = match err {
                WorktreeError::GitCommand { stderr, .. } => stderr,
                other => other.to_string(),
            };
            return Err(WorktreeError::MergeConflict {
                branch,
                target_branch,
                details,
            });
        }

        // Check if any changes are staged
        let status = self
            .run_git(&["status", "--porcelain"], None)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();

        if status.is_empty() {
            return Ok(SquashMergeResult {
                success: true,
                commit_hash: None,
                no_changes: true,
                task_id: id.to_string(),
                branch,
                target_branch,
            });
        }

        // Format commit message with Co-authored-by trailers
        let base_msg = if commit_message.trim().is_empty() {
            format!("Auto-squash task #{id}")
        } else {
            commit_message.trim().to_string()
        };

        let trailers = format_co_authors(co_authors);
        let final_message = if trailers.is_empty() {
            base_msg
        } else {
            format!("{}\n\n{}", base_msg, trailers.join("\n"))
        };

        // Commit on host repo with fallback author config if missing
        self.run_git(
            &[
                "-c",
                "user.name=TurfCode Host",
                "-c",
                "user.email=host@turfcode.local",
                "commit",
                "-m",
                &final_message,
            ],
            None,
        )?;

        let commit_hash = self
            .run_git(&["rev-parse", "HEAD"], None)
            .map(|s| s.trim().to_string())
            .ok();

        Ok(SquashMergeResult {
            success: true,
            commit_hash,
            no_changes: false,
            task_id: id.to_string(),
            branch,
            target_branch,
        })
    }

    /// Cleanly prunes and removes the worktree directory and optionally deletes branch.
    pub fn remove_task_worktree(
        &self,
        task_id: &str,
        delete_branch: bool,
    ) -> Result<(), WorktreeError> {
        let id = task_id.trim();
        let branch = if id.starts_with("task/") || id.starts_with("turf/") {
            id.to_string()
        } else {
            format!("task/{id}")
        };
        let worktree_path = self.get_worktree_path(id);
        let wt_str = worktree_path.to_string_lossy().to_string();

        if worktree_path.exists() {
            let _ = self.run_git(&["worktree", "remove", "--force", &wt_str], None);
        }

        let _ = self.run_git(&["worktree", "prune"], None);

        if worktree_path.exists() {
            let _ = std::fs::remove_dir_all(&worktree_path);
        }

        if delete_branch {
            let _ = self.run_git(&["branch", "-D", &branch], None);
        }

        Ok(())
    }

    /// List all registered git worktrees in the repository.
    pub fn list_worktrees(&self) -> Result<Vec<WorktreeInfo>, WorktreeError> {
        let output = self.run_git(&["worktree", "list", "--porcelain"], None)?;
        let mut worktrees = Vec::new();
        let mut current_path: Option<String> = None;
        let mut current_commit: Option<String> = None;
        let mut current_branch: Option<String> = None;

        for line in output.lines() {
            if let Some(path_str) = line.strip_prefix("worktree ") {
                if let Some(path) = current_path.take() {
                    let task_id = Path::new(&path)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    worktrees.push(WorktreeInfo {
                        task_id,
                        branch_name: current_branch.take().unwrap_or_default(),
                        worktree_path: path,
                        commit: current_commit.take(),
                    });
                }
                current_path = Some(path_str.trim().to_string());
            } else if let Some(head) = line.strip_prefix("HEAD ") {
                current_commit = Some(head.trim().to_string());
            } else if let Some(branch) = line.strip_prefix("branch ") {
                let cleaned = branch
                    .trim()
                    .strip_prefix("refs/heads/")
                    .unwrap_or(branch.trim());
                current_branch = Some(cleaned.to_string());
            }
        }

        if let Some(path) = current_path {
            let task_id = Path::new(&path)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("unknown")
                .to_string();
            worktrees.push(WorktreeInfo {
                task_id,
                branch_name: current_branch.unwrap_or_default(),
                worktree_path: path,
                commit: current_commit,
            });
        }

        Ok(worktrees)
    }

    /// Check if a task worktree directory exists.
    pub fn has_worktree(&self, task_id: &str) -> bool {
        self.get_worktree_path(task_id).exists()
    }
}
