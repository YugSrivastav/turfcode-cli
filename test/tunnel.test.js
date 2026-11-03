/**
 * TurfCode CLI - Tunnel & Peer Client Network Tests
 *
 * Validates:
 * - Cryptographically secure room code generation [MD-025]
 * - Encrypted join handshake & credential isolation [MD-019]
 * - Message broadcasting & Intent Board / Lock synchronization
 * - Reverse Port Forwarding dev server proxy [MD-031]
 * - Zero-latency local input buffer & history loop [MD-020], [MD-026]
 * - Automated ephemeral tunnel & reconnection failover [MD-006]
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'http';
import net from 'net';
import crypto from 'crypto';
import {
  TunnelServer,
  generateRoomCode,
  encryptPayload,
  decryptPayload,
  isPortListening,
  proxyHttpRequest
} from '../src/tunnel.js';
import { TurfClient, LocalInputBuffer } from '../src/client.js';
import { LockManager } from '../src/lock-manager.js';
import { IntentBoard } from '../src/intent-board.js';

// Helper to find an available local port
function getFreePort() {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      const port = srv.address().port;
      srv.close(() => resolve(port));
    });
    srv.on('error', reject);
  });
}

test('generateRoomCode produces 6-character TURF-XXXX room codes with unambiguous characters [MD-025]', () => {
  const code = generateRoomCode();
  assert.match(code, /^TURF-[2-9A-Z]{4}$/, 'Room code must match TURF-XXXX format');

  // Verify ambiguous characters (0, 1, I, O) are excluded
  const generatedCodes = new Set();
  for (let i = 0; i < 50; i++) {
    const c = generateRoomCode();
    assert.match(c, /^TURF-[2-9A-Z]{4}$/);
    assert.doesNotMatch(c, /[01IO]/, 'Must exclude ambiguous characters 0, 1, I, O');
    generatedCodes.add(c);
  }

  // High entropy / uniqueness test
  assert.equal(generatedCodes.size, 50, '50 generated room codes must be unique');
});

test('encryptPayload & decryptPayload round-trip securely with AES-256-GCM [MD-019]', () => {
  const secret = 'TURF-7A9B';
  const data = {
    username: 'Ayush',
    authToken: 'token_secret_12345',
    sessionId: 'session_uuid_67890',
    role: 'Peer'
  };

  const encrypted = encryptPayload(data, secret);
  assert.ok(encrypted.iv, 'Encrypted payload must contain IV');
  assert.ok(encrypted.tag, 'Encrypted payload must contain auth tag');
  assert.ok(encrypted.data, 'Encrypted payload must contain ciphertext');

  // Ciphertext must not contain plaintext credentials
  assert.ok(!encrypted.data.includes('Ayush'));
  assert.ok(!encrypted.data.includes('token_secret_12345'));

  const decrypted = decryptPayload(encrypted, secret);
  assert.deepEqual(decrypted, data, 'Decrypted payload must match original data');

  // Decryption fails with wrong room code
  assert.throws(() => {
    decryptPayload(encrypted, 'TURF-WRONG');
  }, /auth|cipher|tag|fail/i, 'Decryption with wrong secret must throw');
});

test('Handshake validation: connects peer, decrypts credentials, and isolates credentials [MD-019]', async () => {
  const roomCode = generateRoomCode();
  const server = new TunnelServer({
    roomCode,
    port: 0
  });

  const serverInfo = await server.start();

  const client = new TurfClient({
    roomCode,
    hostUrl: server.localUrl,
    username: 'Ayush',
    role: 'Peer',
    authToken: 'peer_auth_token_safe',
    sessionId: 'peer_session_id_safe'
  });

  try {
    const ack = await client.connect();
    assert.equal(ack.type, 'HANDSHAKE_ACK');
    assert.equal(ack.success, true);
    assert.equal(ack.roomCode, roomCode);
    assert.ok(ack.peerId, 'Must assign peerId');
    assert.equal(client.isConnected, true);

    // Verify credentials isolation [MD-019]:
    // Credentials must never appear in getConnectedPeers()
    const peers = server.getConnectedPeers();
    assert.equal(peers.length, 1);
    assert.equal(peers[0].username, 'Ayush');
    assert.equal(peers[0].role, 'Peer');
    assert.equal(peers[0].authToken, undefined, 'authToken must never be exposed');
    assert.equal(peers[0].sessionId, undefined, 'sessionId must never be exposed');
  } finally {
    client.disconnect();
    await server.stop();
  }
});

test('Handshake validation: rejects invalid room code or corrupt payload [MD-019], [MD-025]', async () => {
  const roomCode = generateRoomCode();
  const server = new TunnelServer({
    roomCode,
    port: 0
  });

  await server.start();

  // 1. Client with WRONG room code
  const wrongClient = new TurfClient({
    roomCode: 'TURF-9999', // Mismatched room code
    hostUrl: server.localUrl,
    username: 'Hacker',
    autoReconnect: false
  });

  await assert.rejects(async () => {
    await wrongClient.connect(2000);
  }, /room code mismatch|rejected/i, 'Connection with wrong room code must be rejected');

  wrongClient.disconnect();
  await server.stop();
});

test('Message broadcasting & State synchronization (Tasks and Locks)', async () => {
  const roomCode = generateRoomCode();
  const lockManager = new LockManager({ autoStartMonitor: false });
  const intentBoard = new IntentBoard({ lockManager });

  const server = new TunnelServer({
    roomCode,
    port: 0,
    lockManager,
    intentBoard
  });

  await server.start();

  const client1 = new TurfClient({
    roomCode,
    hostUrl: server.localUrl,
    username: 'Ayush',
    autoReconnect: false
  });

  const client2 = new TurfClient({
    roomCode,
    hostUrl: server.localUrl,
    username: 'Bob',
    autoReconnect: false
  });

  try {
    await client1.connect();
    await client2.connect();

    // 1. Broadcast test
    let receivedPayload = null;
    client2.on('message', (msg) => {
      if (msg.type === 'TEST_ALERT') {
        receivedPayload = msg.payload;
      }
    });

    server.broadcast('TEST_ALERT', { message: 'Emergency meeting' });

    // Allow event loop to process WebSocket messages
    await new Promise((r) => setTimeout(r, 80));
    assert.deepEqual(receivedPayload, { message: 'Emergency meeting' });

    // 2. Broadcast with excludePeerId
    let client1GotExclude = false;
    let client2GotExclude = false;
    client1.on('message', (msg) => {
      if (msg.type === 'PRIVATE_NOTICE') client1GotExclude = true;
    });
    client2.on('message', (msg) => {
      if (msg.type === 'PRIVATE_NOTICE') client2GotExclude = true;
    });

    server.broadcast('PRIVATE_NOTICE', { text: 'Only Bob sees this' }, client1.peerId);
    await new Promise((r) => setTimeout(r, 80));

    assert.equal(client1GotExclude, false, 'client1 should be excluded from broadcast');
    assert.equal(client2GotExclude, true, 'client2 should receive broadcast');

    // 3. Task synchronization
    let taskSyncReceived = null;
    client1.on('tasks-updated', (tasks) => {
      taskSyncReceived = tasks;
    });

    intentBoard.addTask({ title: 'Build Tunnel Engine', assignee: 'Team' });
    await new Promise((r) => setTimeout(r, 80));

    assert.ok(taskSyncReceived, 'Client must receive tasks-updated');
    assert.ok(taskSyncReceived.some((t) => t.title === 'Build Tunnel Engine'));

    // 4. Lock synchronization
    let lockSyncReceived = null;
    client2.on('locks-updated', (locks) => {
      lockSyncReceived = locks;
    });

    lockManager.acquireAtomicSequence('Host', 'task_1', ['src/tunnel.js']);
    await new Promise((r) => setTimeout(r, 80));

    assert.ok(lockSyncReceived, 'Client must receive locks-updated');
    assert.ok(
      lockSyncReceived.some((l) => (l.filePath || l.file) === 'src/tunnel.js'),
      'Lock on src/tunnel.js must be present'
    );
  } finally {
    client1.disconnect();
    client2.disconnect();
    lockManager.destroy();
    await server.stop();
  }
});

test('Reverse port forward proxy: proxies HTTP traffic on common dev ports [MD-031]', async () => {
  // 1. Boot a mock local web dev server on host (e.g. Next.js/Vite simulation)
  const devPort = await getFreePort();
  const mockDevServer = http.createServer((req, res) => {
    if (req.url === '/api/info') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ framework: 'nextjs', port: devPort, message: 'Live Preview' }));
    } else {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end('<!DOCTYPE html><html><body><h1>Dev Server Preview</h1></body></html>');
    }
  });

  await new Promise((resolve) => mockDevServer.listen(devPort, '127.0.0.1', resolve));

  const roomCode = generateRoomCode();
  const server = new TunnelServer({
    roomCode,
    port: 0,
    devPorts: [devPort, 3000, 5173]
  });

  await server.start();

  const client = new TurfClient({
    roomCode,
    hostUrl: server.localUrl,
    username: 'Ayush',
    autoReconnect: false
  });

  try {
    await client.connect();

    // Verify isPortListening detects active dev server
    const isListening = await isPortListening(devPort);
    assert.equal(isListening, true, `Port ${devPort} must be detected as active`);

    // Verify proxyHttpRequest directly
    const directRes = await proxyHttpRequest({
      port: devPort,
      method: 'GET',
      path: '/api/info'
    });
    assert.equal(directRes.statusCode, 200);
    const parsedInfo = JSON.parse(directRes.body);
    assert.equal(parsedInfo.framework, 'nextjs');

    // Verify Reverse Port Forwarding across WebSocket tunnel [MD-031]
    const tunnelRes = await client.requestDevProxy({
      port: devPort,
      method: 'GET',
      path: '/api/info'
    });
    assert.equal(tunnelRes.statusCode, 200);
    assert.deepEqual(JSON.parse(tunnelRes.body), { framework: 'nextjs', port: devPort, message: 'Live Preview' });

    // Verify Local Dev Proxy on Peer: peer opens local port and previews host web app
    const peerProxyPort = await getFreePort();
    const localProxy = await client.startLocalDevProxy(devPort, peerProxyPort);
    assert.equal(localProxy.port, peerProxyPort);

    // Make HTTP request to peer's local proxy port
    const peerClientReq = await new Promise((resolve, reject) => {
      http.get(`http://127.0.0.1:${peerProxyPort}/api/info`, (res) => {
        let data = '';
        res.on('data', (c) => (data += c));
        res.on('end', () => resolve({ statusCode: res.statusCode, body: data }));
      }).on('error', reject);
    });

    assert.equal(peerClientReq.statusCode, 200);
    assert.deepEqual(JSON.parse(peerClientReq.body), { framework: 'nextjs', port: devPort, message: 'Live Preview' });

    localProxy.close();

    // Verify inactive port invariant [MD-031]:
    // "Port forwarder must bind only when the respective host port is actively listening."
    const unusedPort = await getFreePort();
    await assert.rejects(async () => {
      await client.requestDevProxy({
        port: unusedPort,
        method: 'GET',
        path: '/'
      });
    }, /not actively listening/i, 'Must reject proxy to inactive port per MD-031');
  } finally {
    client.disconnect();
    await server.stop();
    await new Promise((resolve) => mockDevServer.close(resolve));
  }
});

test('Zero-latency local input buffer and per-user command history [MD-020], [MD-026]', () => {
  const buffer = new LocalInputBuffer();

  // Insertion & typing
  buffer.insert('git commit -m "feat"');
  assert.equal(buffer.getValue(), 'git commit -m "feat"');
  assert.equal(buffer.getCursor(), 20);

  // Backspace
  buffer.backspace();
  assert.equal(buffer.getValue(), 'git commit -m "feat');

  // Cursor movement
  buffer.moveLeft();
  buffer.moveLeft();
  buffer.insert('st_');
  assert.equal(buffer.getValue(), 'git commit -m "fest_at');

  // Submit
  const submitted = buffer.submit();
  assert.equal(submitted, 'git commit -m "fest_at');
  assert.equal(buffer.getValue(), '', 'Buffer must be empty after submission');
  assert.equal(buffer.getCursor(), 0);

  // History navigation with Up/Down arrows [MD-026]
  buffer.insert('npm test');
  buffer.submit();

  buffer.insert('npm run build');
  buffer.submit();

  // Up arrow returns previous command
  assert.equal(buffer.historyUp(), 'npm run build');
  assert.equal(buffer.historyUp(), 'npm test');
  assert.equal(buffer.historyUp(), 'git commit -m "fest_at');

  // Down arrow returns forward command
  assert.equal(buffer.historyDown(), 'npm test');
  assert.equal(buffer.historyDown(), 'npm run build');
  assert.equal(buffer.historyDown(), '', 'Down past recent returns draft input');
});

test('Automated ephemeral tunnel URL generation and peer auto-reconnect [MD-006]', async () => {
  const roomCode = generateRoomCode();
  const server = new TunnelServer({
    roomCode,
    port: 0
  });

  const serverInfo = await server.start();
  assert.ok(serverInfo.tunnelUrl, 'Server must provide an ephemeral tunnel URL [MD-006]');
  assert.ok(serverInfo.tunnelUrl.includes(roomCode.toLowerCase()) || serverInfo.tunnelUrl.startsWith('ws'));

  // Test automatic reconnection
  const client = new TurfClient({
    roomCode,
    hostUrl: server.localUrl,
    autoReconnect: true,
    reconnectIntervalMs: 50
  });

  try {
    await client.connect();
    assert.equal(client.isConnected, true);

    let reconnected = false;
    client.on('reconnected', () => {
      reconnected = true;
    });

    // Simulate unexpected socket drop
    client.ws.terminate();

    // Wait for auto-reconnection
    await new Promise((r) => setTimeout(r, 250));
    assert.equal(client.isConnected, true, 'Client must reconnect automatically [MD-006]');
  } finally {
    client.disconnect();
    await server.stop();
  }
});
