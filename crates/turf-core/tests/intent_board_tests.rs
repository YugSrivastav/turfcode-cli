use tempfile::tempdir;
use turf_core::IntentBoard;
use turf_protocol::intent::{TaskCard, TaskStatus};

#[tokio::test]
async fn test_intent_board_crud_and_status_transitions() {
    let dir = tempdir().unwrap();
    let board = IntentBoard::with_cwd(dir.path());

    // 1. Add Task (Queued)
    let task = TaskCard::new(
        "1",
        "Implement Lock Engine",
        "Alice",
        TaskStatus::Queued,
        vec!["src/lock.rs".to_string()],
    );
    let added = board.add_task(task).await.unwrap();
    assert_eq!(added.id, "1");
    assert_eq!(added.status, TaskStatus::Queued);

    // 2. Transition to InProgress
    let updated = board
        .update_task_status("1", TaskStatus::InProgress)
        .await
        .unwrap()
        .expect("Task should exist");
    assert_eq!(updated.status, TaskStatus::InProgress);

    // 3. Transition to Blocked
    let blocked = board
        .update_task_status("1", TaskStatus::Blocked)
        .await
        .unwrap()
        .expect("Task should exist");
    assert_eq!(blocked.status, TaskStatus::Blocked);

    // 4. Transition to Done
    let done = board
        .update_task_status("1", TaskStatus::Done)
        .await
        .unwrap()
        .expect("Task should exist");
    assert_eq!(done.status, TaskStatus::Done);

    // 5. Update task details
    board
        .update_task("1", |t| {
            t.assignee = "Bob".to_string();
            t.target_files.push("src/extra.rs".to_string());
        })
        .await
        .unwrap();

    let fetched = board.get_task("1").await.unwrap();
    assert_eq!(fetched.assignee, "Bob");
    assert_eq!(fetched.target_files.len(), 2);

    // 6. Remove Task
    let removed = board.remove_task("1").await.unwrap();
    assert!(removed.is_some());
    assert!(board.get_task("1").await.is_none());
}

#[tokio::test]
async fn test_intent_board_atomic_disk_persistence() {
    let dir = tempdir().unwrap();
    let intent_file = dir.path().join(".turf").join("intent-board.json");

    {
        let board1 = IntentBoard::new(dir.path(), &intent_file);
        let task = TaskCard::new(
            "task-42",
            "Multi-agent coordination",
            "Agent-1",
            TaskStatus::InProgress,
            vec!["crates/turf-core/src/lib.rs".to_string()],
        );
        board1.add_task(task).await.unwrap();
        assert!(intent_file.exists());
    }

    // Verify raw JSON file content
    let raw = std::fs::read_to_string(&intent_file).unwrap();
    assert!(raw.contains("task-42"));
    assert!(raw.contains("in_progress"));

    // Reload in second instance
    {
        let board2 = IntentBoard::new(dir.path(), &intent_file);
        let loaded = board2.get_task("task-42").await.unwrap();
        assert_eq!(loaded.title, "Multi-agent coordination");
        assert_eq!(loaded.status, TaskStatus::InProgress);
    }
}
