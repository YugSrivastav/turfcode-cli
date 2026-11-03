use std::path::Path;
use std::process::Command;
use tempfile::tempdir;
use turf_core::WorktreeManager;
use turf_protocol::worktree::CoAuthor;

fn setup_test_git_repo(path: &Path) {
    let run = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(path)
            .output()
            .expect("Git command failed");
        assert!(
            output.status.success(),
            "Git {:?} failed: {:?}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    };

    run(&["init", "-b", "main"]);
    run(&["config", "user.name", "Test User"]);
    run(&["config", "user.email", "test@turfcode.local"]);
    run(&["config", "commit.gpgsign", "false"]);

    // Create initial commit on main
    std::fs::write(path.join(".gitignore"), ".turf/\n").unwrap();
    std::fs::write(path.join("README.md"), "# TurfCode Test\n").unwrap();
    run(&["add", ".gitignore", "README.md"]);
    run(&["commit", "-m", "Initial commit"]);
}

#[test]
fn test_worktree_creation_isolation() {
    let dir = tempdir().unwrap();
    setup_test_git_repo(dir.path());

    let manager = WorktreeManager::new(dir.path()).with_base_branch("main");

    // Create worktree for task "101"
    let info = manager.create_task_worktree("101", None).unwrap();
    assert_eq!(info.task_id, "101");
    assert_eq!(info.branch_name, "task/101");
    assert!(Path::new(&info.worktree_path).exists());
    assert!(manager.has_worktree("101"));

    // Verify it is listed in git worktrees
    let active = manager.list_worktrees().unwrap();
    assert!(active.iter().any(|wt| wt.branch_name == "task/101"));
}

#[test]
fn test_worktree_automated_squash_merge_with_coauthors() {
    let dir = tempdir().unwrap();
    setup_test_git_repo(dir.path());

    let manager = WorktreeManager::new(dir.path()).with_base_branch("main");

    // 1. Create worktree
    let info = manager.create_task_worktree("102", None).unwrap();
    let wt_path = Path::new(&info.worktree_path);

    // 2. Make commits inside the task worktree
    let feature_file = wt_path.join("feature.txt");
    std::fs::write(&feature_file, "New Agent Feature\n").unwrap();

    let run_in_wt = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(wt_path)
            .output()
            .expect("Git in worktree failed");
        assert!(output.status.success());
    };
    run_in_wt(&["add", "feature.txt"]);
    run_in_wt(&["commit", "-m", "feat: implement feature"]);

    // 3. Automated squash-merge into main with Co-authored-by trailers
    let co_authors = vec![
        CoAuthor::new("Ayush", "ayush@turfcode.local"),
        CoAuthor::new("Antigravity", "agy@turfcode.local"),
    ];

    let merge_res = manager
        .merge_task_worktree("102", "feat(agent): implement new agent feature", &co_authors, None)
        .unwrap();

    assert!(merge_res.success);
    assert!(!merge_res.no_changes);
    assert!(merge_res.commit_hash.is_some());

    // 4. Verify commit on main contains git trailers
    let log_output = Command::new("git")
        .args(["log", "-1", "--format=%B"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let commit_body = String::from_utf8_lossy(&log_output.stdout);

    assert!(commit_body.contains("feat(agent): implement new agent feature"));
    assert!(commit_body.contains("Co-authored-by: Ayush <ayush@turfcode.local>"));
    assert!(commit_body.contains("Co-authored-by: Antigravity <agy@turfcode.local>"));

    // File should now be present on main
    assert!(dir.path().join("feature.txt").exists());
}

#[test]
fn test_worktree_pruning_and_cleanup() {
    let dir = tempdir().unwrap();
    setup_test_git_repo(dir.path());

    let manager = WorktreeManager::new(dir.path()).with_base_branch("main");

    let info = manager.create_task_worktree("103", None).unwrap();
    let wt_path = Path::new(&info.worktree_path);
    assert!(wt_path.exists());

    // Cleanly prune and remove worktree
    manager.remove_task_worktree("103", true).unwrap();

    assert!(!wt_path.exists());
    assert!(!manager.has_worktree("103"));

    // Verify branch was deleted
    let branch_check = Command::new("git")
        .args(["branch", "--list", "task/103"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&branch_check.stdout).trim().is_empty());
}

#[test]
fn test_worktree_merge_conflict_clean_abort() {
    let dir = tempdir().unwrap();
    setup_test_git_repo(dir.path());

    let manager = WorktreeManager::new(dir.path()).with_base_branch("main");

    // Create task worktree
    let info = manager.create_task_worktree("conflict-task", None).unwrap();
    let wt_path = Path::new(&info.worktree_path);

    // Edit conflict.txt in worktree
    std::fs::write(wt_path.join("conflict.txt"), "Task version\n").unwrap();
    let run_in_wt = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(wt_path)
            .output()
            .expect("Git in worktree failed");
        assert!(output.status.success());
    };
    run_in_wt(&["add", "conflict.txt"]);
    run_in_wt(&["commit", "-m", "task commit"]);

    // Edit conflict.txt in main with conflicting content
    std::fs::write(dir.path().join("conflict.txt"), "Main version\n").unwrap();
    let run_in_main = |args: &[&str]| {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .expect("Git in main failed");
        assert!(output.status.success());
    };
    run_in_main(&["add", "conflict.txt"]);
    run_in_main(&["commit", "-m", "main conflicting commit"]);

    // Attempt squash merge: must detect conflict and abort cleanly
    let merge_res = manager.merge_task_worktree("conflict-task", "Merge conflict", &[], None);
    assert!(merge_res.is_err());

    // Verify working tree on main is reset and clean
    let status_output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&status_output.stdout).trim().is_empty());
}
