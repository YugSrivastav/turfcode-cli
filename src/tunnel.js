/**
 * TurfCode CLI - Ephemeral Tunneling & Session Server
 *
 * Implements:
 * - WebSocket session server (ws)
 * - Cryptographically secure 6-character room codes (TURF-XXXX) [MD-025]
 * - Ephemeral HTTPS/WSS tunnel integration [MD-006]
 * - Peer connection management, heartbeats, and room broadcast
 * - Reverse Port Forwarding for common dev ports (3000, 5173, 8080) [MD-031]
 * - Encrypted credential exchange [MD-019]
 * - Text-only WebSocket diff / state streaming [MD-035]
 */

import http from 'http';
import net from 'net';
import crypto from 'crypto';
import { EventEmitter } from 'events';
import { WebSocketServer, WebSocket } from 'ws';

/**
 * Common development server ports to monitor for reverse port forwarding [MD-031].
 */
export const COMMON_DEV_PORTS = [3000, 5173, 8080];

/**
 * Binary file extensions to filter out from diff/file streaming [MD-035].
 */
export const BINARY_EXTENSIONS = new Set([
  '.png', '.jpg', '.jpeg', '.gif', '.ico', '.webp', '.svg',
  '.exe', '.bin', '.tar', '.gz', '.zip', '.7z',
  '.wasm', '.node', '.dylib', '.so', '.dll',
  '.mp4', '.mp3', '.pdf', '.woff', '.woff2', '.ttf', '.eot'
]);

/**
 * Max payload size for text streaming (1MB) [MD-035].
 */
export const MAX_STREAMING_BYTES = 1024 * 1024;

/**
 * Generate cryptographically secure 6-character room code in TURF-XXXX format [MD-025].
 * Uses unambiguous alphanumeric characters (excludes 0, 1, I, O).
 *
 * @param {number} [codeLength=4]
 * @returns {string} E.g. "TURF-7A9B"
 */
export function generateRoomCode(codeLength = 4) {
  const chars = '23456789ABCDEFGHJKLMNPQRSTUVWXYZ';
  let code = '';
  const bytes = crypto.randomBytes(codeLength);
  for (let i = 0; i < codeLength; i++) {
    code += chars[bytes[i] % chars.length];
  }
  return `TURF-${code}`;
}

/**
 * Derive 32-byte AES-256 key from a secret string (e.g. room code).
 *
 * @param {string} secret
 * @returns {Buffer}
 */
export function deriveKey(secret) {
  return crypto.createHash('sha256').update(String(secret).trim().toUpperCase()).digest();
}

/**
 * Encrypt payload using AES-256-GCM [MD-019].
 *
 * @param {any} data
 * @param {string} secret
 * @returns {{ iv: string, tag: string, data: string }}
 */
export function encryptPayload(data, secret) {
  const key = deriveKey(secret);
  const iv = crypto.randomBytes(12);
  const cipher = crypto.createCipheriv('aes-256-gcm', key, iv);
  const plaintext = typeof data === 'string' ? data : JSON.stringify(data);
  let ciphertext = cipher.update(plaintext, 'utf8', 'hex');
  ciphertext += cipher.final('hex');
  const tag = cipher.getAuthTag().toString('hex');

  return {
    iv: iv.toString('hex'),
    tag,
    data: ciphertext
  };
}

/**
 * Decrypt payload using AES-256-GCM [MD-019].
 *
 * @param {{ iv: string, tag: string, data: string }} encryptedObj
 * @param {string} secret
 * @returns {any}
 */
export function decryptPayload(encryptedObj, secret) {
  if (!encryptedObj || !encryptedObj.iv || !encryptedObj.tag || !encryptedObj.data) {
    throw new Error('Invalid encrypted payload structure');
  }

  const key = deriveKey(secret);
  const iv = Buffer.from(encryptedObj.iv, 'hex');
  const tag = Buffer.from(encryptedObj.tag, 'hex');
  const decipher = crypto.createDecipheriv('aes-256-gcm', key, iv);
  decipher.setAuthTag(tag);

  let decrypted = decipher.update(encryptedObj.data, 'hex', 'utf8');
  decrypted += decipher.final('utf8');

  try {
    return JSON.parse(decrypted);
  } catch {
    return decrypted;
  }
}

/**
 * Check if a TCP port is actively listening on localhost [MD-031].
 *
 * @param {number} port
 * @param {string} [host='127.0.0.1']
 * @param {number} [timeoutMs=300]
 * @returns {Promise<boolean>}
 */
export function isPortListening(port, host = '127.0.0.1', timeoutMs = 300) {
  return new Promise((resolve) => {
    const socket = new net.Socket();
    let settled = false;

    const cleanup = () => {
      if (!settled) {
        settled = true;
        socket.destroy();
      }
    };

    socket.setTimeout(timeoutMs);

    socket.once('connect', () => {
      cleanup();
      resolve(true);
    });

    socket.once('timeout', () => {
      cleanup();
      resolve(false);
    });

    socket.once('error', () => {
      cleanup();
      resolve(false);
    });

    try {
      socket.connect(port, host);
    } catch {
      cleanup();
      resolve(false);
    }
  });
}

/**
 * Proxies an HTTP request to a local port [MD-031].
 *
 * @param {Object} options
 * @param {number} options.port
 * @param {string} [options.method='GET']
 * @param {string} [options.path='/']
 * @param {Object} [options.headers={}]
 * @param {string|Buffer|null} [options.body=null]
 * @param {number} [options.timeoutMs=5000]
 * @returns {Promise<{ statusCode: number, headers: Object, body: string, isBase64: boolean }>}
 */
export async function proxyHttpRequest({
  port,
  method = 'GET',
  path: reqPath = '/',
  headers = {},
  body = null,
  timeoutMs = 5000
}) {
  // Invariant [MD-031]: Check active listening before proxying
  const listening = await isPortListening(port);
  if (!listening) {
    throw new Error(`Port ${port} is not actively listening on host [MD-031]`);
  }

  return new Promise((resolve, reject) => {
    const cleanHeaders = { ...headers };
    delete cleanHeaders.host; // Allow Node http to set correct host header

    const req = http.request(
      {
        hostname: '127.0.0.1',
        port,
        path: reqPath,
        method,
        headers: cleanHeaders,
        timeout: timeoutMs
      },
      (res) => {
        const chunks = [];
        res.on('data', (chunk) => chunks.push(chunk));
        res.on('end', () => {
          const bodyBuffer = Buffer.concat(chunks);
          const contentType = res.headers['content-type'] || '';
          const isText =
            contentType.includes('text') ||
            contentType.includes('json') ||
            contentType.includes('javascript') ||
            contentType.includes('html') ||
            contentType.includes('xml') ||
            contentType.includes('css');

          resolve({
            statusCode: res.statusCode || 200,
            headers: res.headers,
            body: isText ? bodyBuffer.toString('utf8') : bodyBuffer.toString('base64'),
            isBase64: !isText
          });
        });
      }
    );

    req.on('timeout', () => {
      req.destroy();
      reject(new Error(`Proxy request to port ${port} timed out after ${timeoutMs}ms`));
    });

    req.on('error', (err) => {
      reject(err);
    });

    if (body) {
      if (typeof body === 'string') {
        req.write(body);
      } else if (Buffer.isBuffer(body)) {
        req.write(body);
      }
    }

    req.end();
  });
}

/**
 * TunnelServer
 *
 * Central coordinator on the Host machine for peer connections,
 * encrypted handshakes, Intent Board & Lock synchronizations,
 * and dev port forwarding.
 */
export class TunnelServer extends EventEmitter {
  /**
   * @param {Object} [options={}]
   * @param {number} [options.port=0] Local port to bind (0 for ephemeral)
   * @param {string} [options.host='0.0.0.0'] Local host address
   * @param {string} [options.roomCode] Room code (auto-generated if omitted)
   * @param {string} [options.cwd=process.cwd()] Working directory
   * @param {Object} [options.intentBoard] IntentBoard instance
   * @param {Object} [options.lockManager] LockManager instance
   * @param {number[]} [options.devPorts=COMMON_DEV_PORTS] Ports to monitor for dev preview [MD-031]
   * @param {number} [options.heartbeatIntervalMs=5000] Interval for ping/pong heartbeats
   * @param {Function} [options.tunnelProvider] Custom tunnel creation hook
   */
  constructor(options = {}) {
    super();
    this.port = options.port || 0;
    this.host = options.host || '0.0.0.0';
    this.roomCode = options.roomCode || generateRoomCode();
    this.cwd = options.cwd || process.cwd();
    this.intentBoard = options.intentBoard || null;
    this.lockManager = options.lockManager || null;
    this.devPorts = options.devPorts || [...COMMON_DEV_PORTS];
    this.heartbeatIntervalMs = options.heartbeatIntervalMs || 5000;
    this.tunnelProvider = options.tunnelProvider || null;

    this.server = null;
    this.wss = null;
    this.tunnelUrl = null;
    this.localUrl = null;
    this.isListening = false;

    /**
     * Map of peerId -> { id, ws, username, role, credentials, joinedAt, isAlive, authenticated }
     * Note [MD-019]: In-memory only, never persisted.
     */
    this.peers = new Map();

    this.heartbeatTimer = null;
    this.portCheckTimer = null;
    this.activeDevPorts = [];

    // State change listeners
    this._onTasksChanged = () => this._broadcastTasksSync();
    this._onLocksChanged = () => this._broadcastLocksSync();

    if (this.intentBoard && typeof this.intentBoard.on === 'function') {
      this.intentBoard.on('change', this._onTasksChanged);
    }
    if (this.lockManager && typeof this.lockManager.on === 'function') {
      this.lockManager.on('change', this._onLocksChanged);
    }
  }

  /**
   * Start the HTTP and WebSocket session server.
   *
   * @returns {Promise<{ port: number, roomCode: string, tunnelUrl: string, localUrl: string }>}
   */
  async start() {
    if (this.isListening) {
      return {
        port: this.port,
        roomCode: this.roomCode,
        tunnelUrl: this.tunnelUrl,
        localUrl: this.localUrl
      };
    }

    return new Promise((resolve, reject) => {
      this.server = http.createServer((req, res) => {
        // Simple health/probe endpoint
        if (req.url === '/health' || req.url === '/') {
          res.writeHead(200, { 'Content-Type': 'application/json' });
          res.end(
            JSON.stringify({
              turfcode: true,
              roomCode: this.roomCode,
              peers: this.peers.size,
              status: 'online'
            })
          );
          return;
        }

        res.writeHead(404);
        res.end();
      });

      this.wss = new WebSocketServer({ server: this.server });

      this.server.on('error', (err) => {
        this.emit('error', err);
        reject(err);
      });

      this.server.listen(this.port, this.host, async () => {
        const addr = this.server.address();
        this.port = typeof addr === 'object' && addr !== null ? addr.port : this.port;
        this.isListening = true;
        this.localUrl = `ws://127.0.0.1:${this.port}`;

        // Initialize automated ephemeral tunnel [MD-006]
        await this._initEphemeralTunnel();

        // Setup WebSocket connection handling
        this._setupWebSocketServer();

        // Start heartbeat monitor
        this._startHeartbeat();

        // Start dev port monitor [MD-031]
        await this.refreshActiveDevPorts();
        this._startDevPortMonitor();

        const info = {
          port: this.port,
          roomCode: this.roomCode,
          tunnelUrl: this.tunnelUrl,
          localUrl: this.localUrl
        };

        this.emit('started', info);
        resolve(info);
      });
    });
  }

  /**
   * Initialize automated ephemeral tunnel URL [MD-006].
   * @private
   */
  async _initEphemeralTunnel() {
    if (typeof this.tunnelProvider === 'function') {
      try {
        this.tunnelUrl = await this.tunnelProvider(this.port, this.roomCode);
        return;
      } catch (err) {
        this.emit('warn', `Custom tunnel provider failed: ${err.message}`);
      }
    }

    if (process.env.TURF_TUNNEL_URL) {
      this.tunnelUrl = process.env.TURF_TUNNEL_URL;
      return;
    }

    // Default ephemeral tunnel address representation
    this.tunnelUrl = `wss://${this.roomCode.toLowerCase()}.turfcode.live:${this.port}`;
  }

  /**
   * Configure WebSocket server connection lifecycle & message dispatch.
   * @private
   */
  _setupWebSocketServer() {
    this.wss.on('connection', (ws, req) => {
      ws.isAlive = true;
      let peerId = null;

      ws.on('pong', () => {
        ws.isAlive = true;
      });

      // Handshake timeout: Peer must send valid handshake within 10 seconds
      const handshakeTimeout = setTimeout(() => {
        if (!peerId) {
          ws.send(
            JSON.stringify({
              type: 'HANDSHAKE_ERROR',
              message: 'Handshake timeout: Authentication payload not received within 10s'
            })
          );
          ws.close(4008, 'Handshake timeout');
        }
      }, 10000);

      ws.on('message', async (raw) => {
        try {
          const msg = JSON.parse(raw.toString('utf8'));
          await this._handleMessage(ws, msg, (assignedId) => {
            peerId = assignedId;
            clearTimeout(handshakeTimeout);
          });
        } catch (err) {
          ws.send(
            JSON.stringify({
              type: 'ERROR',
              message: `Message handling error: ${err.message}`
            })
          );
        }
      });

      ws.on('close', () => {
        clearTimeout(handshakeTimeout);
        if (peerId) {
          this._handlePeerDisconnect(peerId);
        }
      });

      ws.on('error', (err) => {
        clearTimeout(handshakeTimeout);
        if (peerId) {
          this._handlePeerDisconnect(peerId);
        }
      });
    });
  }

  /**
   * Dispatch incoming WebSocket message.
   * @private
   */
  async _handleMessage(ws, msg, onHandshakeSuccess) {
    switch (msg.type) {
      case 'HANDSHAKE':
        await this._handleHandshake(ws, msg, onHandshakeSuccess);
        break;

      case 'PING':
        ws.send(JSON.stringify({ type: 'PONG', timestamp: Date.now() }));
        break;

      case 'TASK_ADD':
        this._handleTaskAdd(ws, msg);
        break;

      case 'TASK_UPDATE':
        this._handleTaskUpdate(ws, msg);
        break;

      case 'LOCK_ACQUIRE':
        await this._handleLockAcquire(ws, msg);
        break;

      case 'LOCK_RELEASE':
        this._handleLockRelease(ws, msg);
        break;

      case 'DEV_PROXY_REQ':
        await this._handleDevProxyRequest(ws, msg);
        break;

      case 'PROMPT_SUBMIT':
      case 'TERMINAL_INPUT':
        this.emit('peer-input', {
          peerId: msg.peerId,
          text: msg.text,
          command: msg.command
        });
        break;

      case 'BROADCAST':
        if (msg.payload) {
          this.broadcast(msg.subType || 'BROADCAST', msg.payload, msg.peerId);
        }
        break;

      default:
        this.emit('message', msg);
        break;
    }
  }

  /**
   * Process encrypted join handshake [MD-019], [MD-025].
   * @private
   */
  async _handleHandshake(ws, msg, onSuccess) {
    const { roomCode, encryptedPayload } = msg;

    // Validate room code match [MD-025]
    const cleanReceived = String(roomCode || '').trim().toUpperCase();
    const cleanExpected = this.roomCode.trim().toUpperCase();

    if (!cleanReceived || cleanReceived !== cleanExpected) {
      ws.send(
        JSON.stringify({
          type: 'HANDSHAKE_ERROR',
          message: `Room code mismatch: expected ${this.roomCode}, got ${cleanReceived}`
        })
      );
      ws.close(4001, 'Invalid room code');
      return;
    }

    // Decrypt credentials payload using roomCode as symmetric key [MD-019]
    let credentials = {};
    try {
      if (encryptedPayload) {
        credentials = decryptPayload(encryptedPayload, this.roomCode);
      }
    } catch (err) {
      ws.send(
        JSON.stringify({
          type: 'HANDSHAKE_ERROR',
          message: `Handshake decryption failed: ${err.message}`
        })
      );
      ws.close(4002, 'Decryption failure');
      return;
    }

    const username = credentials.username || msg.username || 'Peer';
    const role = credentials.role || 'Peer';
    const peerId = `peer_${crypto.randomBytes(4).toString('hex')}`;

    // Invariant [MD-019]: Store credentials strictly in-memory on the connection.
    // NEVER log or write credentials to persistent session files.
    const peerRecord = {
      id: peerId,
      ws,
      username,
      role,
      credentials: {
        authToken: credentials.authToken || null,
        sessionId: credentials.sessionId || null
      },
      authenticated: true,
      joinedAt: Date.now(),
      isAlive: true
    };

    this.peers.set(peerId, peerRecord);
    onSuccess(peerId);

    // Initial state snapshot
    const initialTasks = this.intentBoard ? this.intentBoard.getTasks() : [];
    const initialLocks = this.lockManager ? this.lockManager.getLocks() : [];
    const activeDevPorts = await this.getActiveDevPorts();
    const connectedPeers = this.getConnectedPeers();

    ws.send(
      JSON.stringify({
        type: 'HANDSHAKE_ACK',
        success: true,
        peerId,
        roomCode: this.roomCode,
        tasks: initialTasks,
        locks: initialLocks,
        activeDevPorts,
        peers: connectedPeers
      })
    );

    // Broadcast to other peers that a new peer joined (excluding credentials!)
    this.broadcast(
      'PEER_JOINED',
      {
        peer: {
          id: peerId,
          username,
          role,
          joinedAt: peerRecord.joinedAt
        }
      },
      peerId
    );

    this.emit('peer-joined', { peerId, username, role });
  }

  /**
   * Handle peer disconnect.
   * @private
   */
  _handlePeerDisconnect(peerId) {
    const peer = this.peers.get(peerId);
    if (!peer) return;

    this.peers.delete(peerId);

    this.broadcast('PEER_LEFT', { peerId, username: peer.username });
    this.emit('peer-left', { peerId, username: peer.username });
  }

  /**
   * Handle task addition request from peer.
   * @private
   */
  _handleTaskAdd(ws, msg) {
    if (!this.intentBoard) return;
    try {
      const added = this.intentBoard.addTask(msg.task || {});
      this._broadcastTasksSync();
      ws.send(JSON.stringify({ type: 'TASK_ADD_ACK', success: true, task: added }));
    } catch (err) {
      ws.send(JSON.stringify({ type: 'TASK_ADD_ACK', success: false, error: err.message }));
    }
  }

  /**
   * Handle task update request from peer.
   * @private
   */
  _handleTaskUpdate(ws, msg) {
    if (!this.intentBoard) return;
    try {
      const updated = this.intentBoard.updateTask(msg.taskId, msg.updates || {});
      this._broadcastTasksSync();
      ws.send(JSON.stringify({ type: 'TASK_UPDATE_ACK', success: true, task: updated }));
    } catch (err) {
      ws.send(JSON.stringify({ type: 'TASK_UPDATE_ACK', success: false, error: err.message }));
    }
  }

  /**
   * Handle lock acquire request from peer.
   * @private
   */
  async _handleLockAcquire(ws, msg) {
    if (!this.lockManager) return;
    try {
      const { taskId, files, ownerId } = msg;
      const result = this.lockManager.acquireAtomicSequence(ownerId || 'Peer', taskId, files || []);
      this._broadcastLocksSync();
      ws.send(
        JSON.stringify({
          type: 'LOCK_ACQUIRE_ACK',
          success: result.success,
          lockedFiles: result.lockedFiles,
          conflict: result.conflict || null
        })
      );
    } catch (err) {
      ws.send(
        JSON.stringify({
          type: 'LOCK_ACQUIRE_ACK',
          success: false,
          error: err.message
        })
      );
    }
  }

  /**
   * Handle lock release request from peer.
   * @private
   */
  _handleLockRelease(ws, msg) {
    if (!this.lockManager) return;
    try {
      const released = this.lockManager.releaseLocks(msg.taskId);
      this._broadcastLocksSync();
      ws.send(JSON.stringify({ type: 'LOCK_RELEASE_ACK', success: true, released }));
    } catch (err) {
      ws.send(JSON.stringify({ type: 'LOCK_RELEASE_ACK', success: false, error: err.message }));
    }
  }

  /**
   * Handle reverse port forward proxy request [MD-031].
   * @private
   */
  async _handleDevProxyRequest(ws, msg) {
    const { requestId, port, method, path: reqPath, headers, body } = msg;

    try {
      // Invariant [MD-031]: Check active listening before proxying
      const listening = await this.isPortListening(port);
      if (!listening) {
        ws.send(
          JSON.stringify({
            type: 'DEV_PROXY_RES',
            requestId,
            statusCode: 502,
            error: `Port ${port} is not actively listening on host [MD-031]`
          })
        );
        return;
      }

      const response = await this.proxyDevRequest({
        port,
        method: method || 'GET',
        path: reqPath || '/',
        headers: headers || {},
        body: body || null
      });

      ws.send(
        JSON.stringify({
          type: 'DEV_PROXY_RES',
          requestId,
          statusCode: response.statusCode,
          headers: response.headers,
          body: response.body,
          isBase64: response.isBase64
        })
      );
    } catch (err) {
      ws.send(
        JSON.stringify({
          type: 'DEV_PROXY_RES',
          requestId,
          statusCode: 502,
          error: err.message
        })
      );
    }
  }

  /**
   * Check if a specific port is actively listening.
   *
   * @param {number} port
   * @returns {Promise<boolean>}
   */
  async isPortListening(port) {
    return isPortListening(port, '127.0.0.1');
  }

  /**
   * Query all common dev ports and return list of actively listening ports [MD-031].
   *
   * @returns {Promise<number[]>}
   */
  async getActiveDevPorts() {
    const active = [];
    for (const port of this.devPorts) {
      if (await this.isPortListening(port)) {
        active.push(port);
      }
    }
    return active;
  }

  /**
   * Refresh active dev ports and broadcast changes to peers if updated.
   *
   * @returns {Promise<number[]>}
   */
  async refreshActiveDevPorts() {
    const current = await this.getActiveDevPorts();
    const prev = this.activeDevPorts;

    const changed =
      current.length !== prev.length ||
      current.some((p, i) => p !== prev[i]);

    if (changed) {
      this.activeDevPorts = current;
      this.broadcast('DEV_PORTS_ACTIVE', { ports: current });
      this.emit('dev-ports-changed', current);
    }

    return current;
  }

  /**
   * Proxy an HTTP request to a local dev server port [MD-031].
   *
   * @param {Object} options
   * @returns {Promise<Object>}
   */
  async proxyDevRequest(options) {
    return proxyHttpRequest(options);
  }

  /**
   * Broadcast tasks synchronization state to all connected peers.
   * @private
   */
  _broadcastTasksSync() {
    if (!this.intentBoard) return;
    const tasks = this.intentBoard.getTasks();
    this.broadcast('TASKS_SYNC', { tasks });
  }

  /**
   * Broadcast locks synchronization state to all connected peers.
   * @private
   */
  _broadcastLocksSync() {
    if (!this.lockManager) return;
    const locks = this.lockManager.getLocks();
    this.broadcast('LOCKS_SYNC', { locks });
  }

  /**
   * Broadcast a message to all authenticated connected peers.
   *
   * @param {string} type Message type
   * @param {any} payload Message data
   * @param {string|null} [excludePeerId=null] Optional peer to exclude
   */
  broadcast(type, payload, excludePeerId = null) {
    const messageStr = JSON.stringify({
      type,
      payload,
      timestamp: Date.now()
    });

    for (const [id, peer] of this.peers.entries()) {
      if (excludePeerId && id === excludePeerId) {
        continue;
      }
      if (peer.authenticated && peer.ws.readyState === WebSocket.OPEN) {
        try {
          peer.ws.send(messageStr);
        } catch {
          // Ignore transient send errors, heartbeat will clean up dead socket
        }
      }
    }

    this.emit('broadcast', { type, payload, excludePeerId });
  }

  /**
   * Get list of connected peers without leaking credentials [MD-019].
   *
   * @returns {Array<{ id: string, username: string, role: string, joinedAt: number }>}
   */
  getConnectedPeers() {
    const list = [];
    for (const peer of this.peers.values()) {
      if (peer.authenticated) {
        list.push({
          id: peer.id,
          username: peer.username,
          role: peer.role,
          joinedAt: peer.joinedAt
        });
      }
    }
    return list;
  }

  /**
   * Heartbeat monitor: pings peers and cleans up dead connections.
   * @private
   */
  _startHeartbeat() {
    if (this.heartbeatTimer) clearInterval(this.heartbeatTimer);

    this.heartbeatTimer = setInterval(() => {
      for (const [id, peer] of this.peers.entries()) {
        if (!peer.isAlive) {
          try {
            peer.ws.terminate();
          } catch {}
          this._handlePeerDisconnect(id);
          continue;
        }

        peer.isAlive = false;
        try {
          peer.ws.ping();
        } catch {
          this._handlePeerDisconnect(id);
        }
      }
    }, this.heartbeatIntervalMs);

    if (this.heartbeatTimer.unref) {
      this.heartbeatTimer.unref();
    }
  }

  /**
   * Periodic dev port monitor: detects when dev servers boot/terminate [MD-031].
   * @private
   */
  _startDevPortMonitor() {
    if (this.portCheckTimer) clearInterval(this.portCheckTimer);

    this.portCheckTimer = setInterval(async () => {
      try {
        await this.refreshActiveDevPorts();
      } catch {}
    }, 3000);

    if (this.portCheckTimer.unref) {
      this.portCheckTimer.unref();
    }
  }

  /**
   * Stop the session server and clean up all listeners and sockets.
   *
   * @returns {Promise<void>}
   */
  async stop() {
    if (!this.isListening) return;

    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
    if (this.portCheckTimer) {
      clearInterval(this.portCheckTimer);
      this.portCheckTimer = null;
    }

    if (this.intentBoard && typeof this.intentBoard.removeListener === 'function') {
      this.intentBoard.removeListener('change', this._onTasksChanged);
    }
    if (this.lockManager && typeof this.lockManager.removeListener === 'function') {
      this.lockManager.removeListener('change', this._onLocksChanged);
    }

    // Close all connected peer websockets
    for (const peer of this.peers.values()) {
      try {
        peer.ws.close(1000, 'Server stopping');
      } catch {}
    }
    this.peers.clear();

    // Close WebSocket server
    if (this.wss) {
      await new Promise((resolve) => this.wss.close(resolve));
      this.wss = null;
    }

    // Close HTTP server
    if (this.server) {
      await new Promise((resolve) => this.server.close(resolve));
      this.server = null;
    }

    this.isListening = false;
    this.emit('stopped');
  }
}
