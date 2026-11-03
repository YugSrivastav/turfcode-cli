use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use turf_core::{intent_board::IntentBoard, lock_manager::LockManager};
use turf_net::{ClientEvent, TunnelClient, TunnelServer};
use turf_protocol::{TaskCard, TaskStatus};

#[tokio::test]
async fn test_live_state_delta_replication_host_and_peers() {
    let mut server = TunnelServer::new(Some("TURF-SYNC".into()))
        .with_host("127.0.0.1")
        .with_port(0);

    let local_addr = server.start().await.expect("Server failed to start");
    let ws_url = format!("ws://{local_addr}");

    // Connect Client 1 (Ayush)
    let mut client1 = TunnelClient::new("TURF-SYNC", "Ayush");
    let _sub1 = client1.subscribe();
    let peer1_id = client1
        .connect(&ws_url)
        .await
        .expect("Client 1 connect failed");

    // Connect Client 2 (Agent)
    let mut client2 = TunnelClient::new("TURF-SYNC", "Agent-1").with_role("Agent");
    let mut sub2 = client2.subscribe();
    let _peer2_id = client2
        .connect(&ws_url)
        .await
        .expect("Client 2 connect failed");

    // Client 1 should observe Client 2 joining via event
    tokio::time::sleep(Duration::from_millis(60)).await;
    let peers1 = client1.peers().await;
    assert_eq!(peers1.len(), 2);

    // 1. Host creates a task on Intent Board
    let task1 = TaskCard::new(
        "task-101",
        "Implement P2P tunnel",
        "Ayush",
        TaskStatus::InProgress,
        vec!["tunnel.rs".into()],
    );
    server
        .add_task(task1.clone())
        .await
        .expect("Host add_task failed");

    // Wait for delta replication
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Both clients should have replicated task-101
    let tasks_c1 = client1.tasks().await;
    assert_eq!(tasks_c1.len(), 1);
    assert_eq!(tasks_c1[0].id, "task-101");
    assert_eq!(tasks_c1[0].title, "Implement P2P tunnel");
    assert_eq!(tasks_c1[0].status, TaskStatus::InProgress);

    let tasks_c2 = client2.tasks().await;
    assert_eq!(tasks_c2.len(), 1);
    assert_eq!(tasks_c2[0].id, "task-101");

    // 2. Client 1 adds a new task
    let task2 = TaskCard::new(
        "task-102",
        "Write integration tests",
        "Agent-1",
        TaskStatus::Queued,
        vec!["tests.rs".into()],
    );
    client1
        .add_task(task2.clone())
        .await
        .expect("Client add_task failed");

    tokio::time::sleep(Duration::from_millis(60)).await;

    // Verify task2 is replicated on host, client 1, and client 2
    let host_tasks = server.tasks().await;
    assert_eq!(host_tasks.len(), 2);

    let c2_tasks_after = client2.tasks().await;
    assert_eq!(c2_tasks_after.len(), 2);
    assert!(c2_tasks_after.iter().any(|t| t.id == "task-102"));

    // 3. Client 2 updates task1 status to Done
    client2
        .update_task_status("task-101", TaskStatus::Done)
        .await
        .expect("Client 2 update_task_status failed");

    tokio::time::sleep(Duration::from_millis(60)).await;

    let c1_tasks_updated = client1.tasks().await;
    let task1_on_c1 = c1_tasks_updated
        .iter()
        .find(|t| t.id == "task-101")
        .unwrap();
    assert_eq!(task1_on_c1.status, TaskStatus::Done);

    // 4. Client 1 acquires locks on files
    client1
        .acquire_locks(
            "task-101",
            vec!["src/crypto.rs".into(), "src/protocol.rs".into()],
        )
        .await
        .expect("Client 1 acquire_locks failed");

    tokio::time::sleep(Duration::from_millis(60)).await;

    let locks_c2 = client2.locks().await;
    assert_eq!(locks_c2.len(), 2);
    assert!(locks_c2.iter().any(|l| l.file_path == "src/crypto.rs"));
    assert!(locks_c2.iter().any(|l| l.file_path == "src/protocol.rs"));

    // 5. Client 2 attempts conflicting lock acquisition on src/crypto.rs
    client2
        .acquire_locks("task-102", vec!["src/crypto.rs".into()])
        .await
        .expect("Client 2 acquire_locks message failed");

    // Listen for LockAcquireAck on sub2
    let mut got_conflict_ack = false;
    for _ in 0..10 {
        if let Ok(ClientEvent::LockAcquireAck {
            success, conflict, ..
        }) = sub2.try_recv()
        {
            if !success && conflict.is_some() {
                got_conflict_ack = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(got_conflict_ack, "Expected lock conflict ACK on client 2");

    // 6. Client 1 releases locks
    client1
        .release_locks("task-101")
        .await
        .expect("Client 1 release_locks failed");

    tokio::time::sleep(Duration::from_millis(60)).await;

    let locks_c2_after = client2.locks().await;
    assert!(
        locks_c2_after.is_empty(),
        "Locks should be released across peers"
    );

    // 7. Client 1 disconnects -> Client 2 observes PeerLeft
    client1.disconnect().await;
    tokio::time::sleep(Duration::from_millis(60)).await;

    let c2_peers_after = client2.peers().await;
    assert_eq!(c2_peers_after.len(), 1);
    assert!(!c2_peers_after.iter().any(|p| p.id == peer1_id));

    client2.disconnect().await;
    server.stop().await;
}

#[tokio::test]
async fn test_tunnel_server_with_turf_core_components() {
    let tmp = tempdir().unwrap();
    let cwd = tmp.path().to_path_buf();

    let intent_board = Arc::new(IntentBoard::with_cwd(&cwd));
    let lock_manager = Arc::new(LockManager::with_cwd(&cwd));

    // Pre-populate intent board and locks
    let initial_task = TaskCard::new(
        "task-core-1",
        "Setup core engine",
        "Host",
        TaskStatus::Done,
        vec!["Cargo.toml".into()],
    );
    intent_board.add_task(initial_task).await.unwrap();

    let mut server = TunnelServer::new(Some("TURF-CORE".into()))
        .with_host("127.0.0.1")
        .with_port(0)
        .with_intent_board(intent_board.clone())
        .with_lock_manager(lock_manager.clone());

    let local_addr = server.start().await.expect("Server start failed");
    let ws_url = format!("ws://{local_addr}");

    let mut client = TunnelClient::new("TURF-CORE", "Collaborator");
    client
        .connect(&ws_url)
        .await
        .expect("Client connect failed");

    // Initial snapshot should contain pre-populated task from turf-core IntentBoard
    tokio::time::sleep(Duration::from_millis(60)).await;
    let client_tasks = client.tasks().await;
    assert_eq!(client_tasks.len(), 1);
    assert_eq!(client_tasks[0].id, "task-core-1");

    // Peer adds task -> verify persisted on disk via IntentBoard
    let new_task = TaskCard::new(
        "task-core-2",
        "Build WebSocket client",
        "Collaborator",
        TaskStatus::InProgress,
        vec!["client.rs".into()],
    );
    client.add_task(new_task).await.unwrap();

    tokio::time::sleep(Duration::from_millis(80)).await;

    let disk_tasks = intent_board.load_from_disk().unwrap();
    assert_eq!(disk_tasks.len(), 2);
    assert!(disk_tasks.iter().any(|t| t.id == "task-core-2"));

    client.disconnect().await;
    server.stop().await;
}
