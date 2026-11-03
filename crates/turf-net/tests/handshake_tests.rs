use std::time::Duration;
use turf_net::{crypto, NetMessage, TunnelClient, TunnelServer};

#[tokio::test]
async fn test_encrypted_handshake_roundtrip_success() {
    let mut server = TunnelServer::new(Some("TURF-7A9B".into()))
        .with_host("127.0.0.1")
        .with_port(0);

    let local_addr = server.start().await.expect("Server failed to start");
    let ws_url = format!("ws://{local_addr}");

    let mut client = TunnelClient::new("TURF-7A9B", "Ayush")
        .with_role("Peer")
        .with_auth_token("ephemeral_auth_token_42");

    let peer_id = client
        .connect(&ws_url)
        .await
        .expect("Handshake should succeed with valid credentials");

    assert!(peer_id.starts_with("peer_"));
    assert_eq!(client.peer_id().await, Some(peer_id.clone()));
    assert!(client.is_connected().await);

    // Verify peers list contains this client
    let peers = client.peers().await;
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0].id, peer_id);
    assert_eq!(peers[0].username, "Ayush");

    // Clean shutdown
    client.disconnect().await;
    server.stop().await;
}

#[tokio::test]
async fn test_handshake_rejected_on_room_code_mismatch() {
    let mut server = TunnelServer::new(Some("TURF-7A9B".into()))
        .with_host("127.0.0.1")
        .with_port(0);

    let local_addr = server.start().await.expect("Server failed to start");
    let ws_url = format!("ws://{local_addr}");

    // Client connects with wrong room code
    let mut client = TunnelClient::new("TURF-9999", "Intruder");

    let result = client.connect(&ws_url).await;
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Room code mismatch") || err_msg.contains("Handshake"),
        "Unexpected error: {err_msg}"
    );

    server.stop().await;
}

#[tokio::test]
async fn test_handshake_rejected_on_corrupted_encrypted_payload() {
    use futures_util::{SinkExt, StreamExt};

    let mut server = TunnelServer::new(Some("TURF-7A9B".into()))
        .with_host("127.0.0.1")
        .with_port(0);

    let local_addr = server.start().await.expect("Server failed to start");
    let ws_url = format!("ws://{local_addr}");

    // Connect raw websocket to send tampered handshake payload
    let (ws_stream, _) = tokio_tungstenite::connect_async(&ws_url)
        .await
        .expect("Raw WS connect failed");
    let (mut writer, mut reader) = ws_stream.split();

    let mut encrypted = crypto::encrypt_str("Valid JSON payload", "TURF-7A9B").unwrap();
    // Tamper with authentication tag
    encrypted.tag = "00000000000000000000000000000000".into();

    let bad_handshake = NetMessage::Handshake {
        room_code: "TURF-7A9B".into(),
        encrypted_payload: Some(encrypted),
        username: "Malicious".into(),
        client_version: "0.1.0".into(),
    };

    writer
        .send(bad_handshake.to_ws_message().unwrap())
        .await
        .unwrap();

    let resp = tokio::time::timeout(Duration::from_secs(3), reader.next())
        .await
        .expect("Timeout waiting for response")
        .expect("Stream closed unexpectedly")
        .expect("WS frame error");

    let net_msg = NetMessage::from_ws_message(resp).unwrap();
    match net_msg {
        NetMessage::HandshakeError { message } => {
            assert!(
                message.contains("decryption failed") || message.contains("Decryption"),
                "Expected decryption error, got: {message}"
            );
        }
        other => panic!("Expected HandshakeError, got {:?}", other),
    }

    server.stop().await;
}

#[tokio::test]
async fn test_multiple_clients_distinct_peer_ids() {
    let mut server = TunnelServer::new(Some("TURF-MULTI".into()))
        .with_host("127.0.0.1")
        .with_port(0);

    let local_addr = server.start().await.expect("Server failed to start");
    let ws_url = format!("ws://{local_addr}");

    let mut client1 = TunnelClient::new("TURF-MULTI", "Peer1");
    let peer1_id = client1.connect(&ws_url).await.unwrap();

    let mut client2 = TunnelClient::new("TURF-MULTI", "Peer2");
    let peer2_id = client2.connect(&ws_url).await.unwrap();

    assert_ne!(peer1_id, peer2_id);

    tokio::time::sleep(Duration::from_millis(50)).await;

    let server_peers = server.connected_peers().await;
    assert_eq!(server_peers.len(), 2);

    client1.disconnect().await;
    client2.disconnect().await;
    server.stop().await;
}
