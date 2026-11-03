/**
 * TurfCode CLI - Peer Client Loop & Network Adapter
 *
 * Implements:
 * - Peer client connection via room code [MD-006]
 * - Encrypted join handshake transmitting username, auth token, and session ID [MD-019]
 * - Intent Board, Lock Table, and Dev Port state synchronization
 * - Zero-latency local input buffer and command loop [MD-020], [MD-026]
 * - Dev server reverse port forwarding client [MD-031]
 * - Automatic reconnection with graceful failover [MD-006]
 */

import http from 'http';
import crypto from 'crypto';
import { EventEmitter } from 'events';
import { WebSocket } from 'ws';
import { encryptPayload } from './tunnel.js';

/**
 * LocalInputBuffer
 *
 * Implements local input handling with zero typing latency [MD-020]
 * and per-user local command history [MD-026].
 */
export class LocalInputBuffer {
  /**
   * @param {Object} [options={}]
   * @param {number} [options.maxHistory=100] Max items in local command history
   */
  constructor(options = {}) {
    this.buffer = '';
    this.cursor = 0;
    this.history = [];
    this.historyIndex = -1;
    this.tempInput = '';
    this.maxHistory = options.maxHistory || 100;
  }

  /**
   * Insert text at current cursor position (instant feedback, zero network latency).
   * @param {string} text
   * @returns {string} Current buffer
   */
  insert(text) {
    if (!text) return this.buffer;
    this.buffer = this.buffer.slice(0, this.cursor) + text + this.buffer.slice(this.cursor);
    this.cursor += text.length;
    return this.buffer;
  }

  /**
   * Backspace: delete character before cursor.
   * @returns {string}
   */
  backspace() {
    if (this.cursor <= 0) return this.buffer;
    this.buffer = this.buffer.slice(0, this.cursor - 1) + this.buffer.slice(this.cursor);
    this.cursor--;
    return this.buffer;
  }

  /**
   * Delete character at current cursor position.
   * @returns {string}
   */
  delete() {
    if (this.cursor >= this.buffer.length) return this.buffer;
    this.buffer = this.buffer.slice(0, this.cursor) + this.buffer.slice(this.cursor + 1);
    return this.buffer;
  }

  /**
   * Move cursor left.
   * @returns {number}
   */
  moveLeft() {
    if (this.cursor > 0) this.cursor--;
    return this.cursor;
  }

  /**
   * Move cursor right.
   * @returns {number}
   */
  moveRight() {
    if (this.cursor < this.buffer.length) this.cursor++;
    return this.cursor;
  }

  /**
   * Move cursor to start of buffer.
   * @returns {number}
   */
  moveToStart() {
    this.cursor = 0;
    return this.cursor;
  }

  /**
   * Move cursor to end of buffer.
   * @returns {number}
   */
  moveToEnd() {
    this.cursor = this.buffer.length;
    return this.cursor;
  }

  /**
   * Up arrow: navigate backwards through local command history [MD-026].
   * @returns {string}
   */
  historyUp() {
    if (this.history.length === 0) return this.buffer;
    if (this.historyIndex === -1) {
      this.tempInput = this.buffer;
      this.historyIndex = this.history.length - 1;
    } else if (this.historyIndex > 0) {
      this.historyIndex--;
    }
    this.buffer = this.history[this.historyIndex];
    this.cursor = this.buffer.length;
    return this.buffer;
  }

  /**
   * Down arrow: navigate forwards through local command history [MD-026].
   * @returns {string}
   */
  historyDown() {
    if (this.historyIndex === -1) return this.buffer;
    if (this.historyIndex < this.history.length - 1) {
      this.historyIndex++;
      this.buffer = this.history[this.historyIndex];
    } else {
      this.historyIndex = -1;
      this.buffer = this.tempInput;
    }
    this.cursor = this.buffer.length;
    return this.buffer;
  }

  /**
   * Submit current buffer, record in history, and reset input.
   * @returns {string} Submitted text
   */
  submit() {
    const value = this.buffer;
    if (value.trim().length > 0) {
      // Avoid duplicate consecutive history entries
      if (this.history.length === 0 || this.history[this.history.length - 1] !== value) {
        this.history.push(value);
        if (this.history.length > this.maxHistory) {
          this.history.shift();
        }
      }
    }
    this.buffer = '';
    this.cursor = 0;
    this.historyIndex = -1;
    this.tempInput = '';
    return value;
  }

  /**
   * Clear current buffer.
   */
  clear() {
    this.buffer = '';
    this.cursor = 0;
    this.historyIndex = -1;
    this.tempInput = '';
  }

  /**
   * Get current buffer text.
   * @returns {string}
   */
  getValue() {
    return this.buffer;
  }

  /**
   * Get current cursor position.
   * @returns {number}
   */
  getCursor() {
    return this.cursor;
  }
}

/**
 * TurfClient
 *
 * Peer network client connecting to TurfCode TunnelServer.
 */
export class TurfClient extends EventEmitter {
  /**
   * @param {Object} options
   * @param {string} options.roomCode Room code (e.g. "TURF-7A9B")
   * @param {string} [options.hostUrl] WebSocket server endpoint (e.g. "ws://127.0.0.1:7873")
   * @param {string} [options.username] Peer username
   * @param {string} [options.role='Peer'] Peer role
   * @param {string} [options.authToken] Peer auth token [MD-019]
   * @param {string} [options.sessionId] Peer session ID [MD-019]
   * @param {boolean} [options.autoReconnect=true] Automatic reconnection flag [MD-006]
   * @param {number} [options.reconnectIntervalMs=1000] Reconnect retry backoff
   * @param {number} [options.maxReconnectAttempts=5] Maximum reconnect tries
   */
  constructor(options = {}) {
    super();

    if (!options.roomCode) {
      throw new Error('Room code is required to initialize TurfClient');
    }

    this.roomCode = String(options.roomCode).trim().toUpperCase();
    this.hostUrl = options.hostUrl || process.env.TURF_HOST_URL || 'ws://127.0.0.1:7873';
    this.username = options.username || process.env.USER || 'Peer';
    this.role = options.role || 'Peer';

    // Invariant [MD-019]: Ephemeral credentials for agent quota
    this.authToken = options.authToken || crypto.randomBytes(16).toString('hex');
    this.sessionId = options.sessionId || crypto.randomUUID();

    this.autoReconnect = options.autoReconnect !== false;
    this.reconnectIntervalMs = options.reconnectIntervalMs || 1000;
    this.maxReconnectAttempts = options.maxReconnectAttempts || 5;

    this.ws = null;
    this.peerId = null;
    this.isConnected = false;
    this.isManuallyClosed = false;
    this.reconnectAttempts = 0;
    this.reconnectTimer = null;

    // Local input loop buffer [MD-020]
    this.inputBuffer = new LocalInputBuffer();

    // Synchronized state
    this.tasks = [];
    this.locks = [];
    this.peers = [];
    this.activeDevPorts = [];

    // Pending proxy requests: requestId -> { resolve, reject, timer }
    this.pendingProxyRequests = new Map();

    // Active local reverse proxy servers: port -> http.Server
    this.localProxyServers = new Map();
  }

  /**
   * Connect to host session server and perform encrypted join handshake [MD-019].
   *
   * @param {number} [timeoutMs=8000]
   * @returns {Promise<Object>} Handshake response
   */
  async connect(timeoutMs = 8000) {
    this.isManuallyClosed = false;

    return new Promise((resolve, reject) => {
      let settled = false;

      const timer = setTimeout(() => {
        if (!settled) {
          settled = true;
          if (this.ws) {
            try {
              this.ws.close();
            } catch {}
          }
          reject(new Error(`Connection to ${this.hostUrl} timed out after ${timeoutMs}ms`));
        }
      }, timeoutMs);

      try {
        this.ws = new WebSocket(this.hostUrl);
      } catch (err) {
        clearTimeout(timer);
        return reject(err);
      }

      this.ws.on('open', () => {
        // Step 1: Send encrypted join handshake [MD-019]
        try {
          const payload = {
            username: this.username,
            role: this.role,
            authToken: this.authToken,
            sessionId: this.sessionId
          };

          const encryptedPayload = encryptPayload(payload, this.roomCode);

          this.ws.send(
            JSON.stringify({
              type: 'HANDSHAKE',
              roomCode: this.roomCode,
              encryptedPayload
            })
          );
        } catch (err) {
          clearTimeout(timer);
          settled = true;
          reject(err);
        }
      });

      this.ws.on('message', (raw) => {
        try {
          const msg = JSON.parse(raw.toString('utf8'));

          if (msg.type === 'HANDSHAKE_ACK') {
            if (!settled) {
              settled = true;
              clearTimeout(timer);

              this.isConnected = true;
              this.reconnectAttempts = 0;
              this.peerId = msg.peerId;
              this.tasks = msg.tasks || [];
              this.locks = msg.locks || [];
              this.activeDevPorts = msg.activeDevPorts || [];
              this.peers = msg.peers || [];

              this.emit('connected', {
                peerId: this.peerId,
                roomCode: this.roomCode,
                tasks: this.tasks,
                locks: this.locks,
                activeDevPorts: this.activeDevPorts,
                peers: this.peers
              });

              resolve(msg);
            }
            return;
          }

          if (msg.type === 'HANDSHAKE_ERROR') {
            if (!settled) {
              settled = true;
              clearTimeout(timer);
              reject(new Error(`Handshake rejected: ${msg.message}`));
            }
            return;
          }

          this._handleMessage(msg);
        } catch (err) {
          this.emit('error', err);
        }
      });

      this.ws.on('close', (code, reason) => {
        this.isConnected = false;
        this.emit('disconnected', { code, reason: reason ? reason.toString() : '' });

        if (!settled) {
          settled = true;
          clearTimeout(timer);
          reject(new Error(`Connection closed before handshake completed: code ${code}`));
          return;
        }

        // Automatic reconnection failover [MD-006]
        if (this.autoReconnect && !this.isManuallyClosed) {
          this._attemptReconnect();
        }
      });

      this.ws.on('error', (err) => {
        this.emit('error', err);
        if (!settled) {
          settled = true;
          clearTimeout(timer);
          reject(err);
        }
      });
    });
  }

  /**
   * Handle incoming protocol message from host.
   * @private
   */
  _handleMessage(msg) {
    switch (msg.type) {
      case 'TASKS_SYNC':
        this.tasks = msg.payload?.tasks || msg.tasks || [];
        this.emit('tasks-updated', this.tasks);
        break;

      case 'LOCKS_SYNC':
        this.locks = msg.payload?.locks || msg.locks || [];
        this.emit('locks-updated', this.locks);
        break;

      case 'PEER_JOINED': {
        const peer = msg.payload?.peer || msg.peer;
        if (peer && !this.peers.some((p) => p.id === peer.id)) {
          this.peers.push(peer);
        }
        this.emit('peer-joined', peer);
        this.emit('peers-updated', this.peers);
        break;
      }

      case 'PEER_LEFT': {
        const peerId = msg.payload?.peerId || msg.peerId;
        this.peers = this.peers.filter((p) => p.id !== peerId);
        this.emit('peer-left', { peerId });
        this.emit('peers-updated', this.peers);
        break;
      }

      case 'DEV_PORTS_ACTIVE':
        this.activeDevPorts = msg.payload?.ports || msg.ports || [];
        this.emit('dev-ports-active', this.activeDevPorts);
        break;

      case 'DEV_PROXY_RES': {
        const pending = this.pendingProxyRequests.get(msg.requestId);
        if (pending) {
          clearTimeout(pending.timer);
          this.pendingProxyRequests.delete(msg.requestId);
          if (msg.error) {
            pending.reject(new Error(msg.error));
          } else {
            pending.resolve({
              statusCode: msg.statusCode,
              headers: msg.headers || {},
              body: msg.body,
              isBase64: Boolean(msg.isBase64)
            });
          }
        }
        break;
      }

      case 'PING':
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
          this.ws.send(JSON.stringify({ type: 'PONG', timestamp: Date.now() }));
        }
        break;

      case 'TERMINAL_OUTPUT':
        this.emit('terminal-output', msg.payload || msg);
        break;

      default:
        this.emit('message', msg);
        break;
    }
  }

  /**
   * Reconnection retry loop with exponential backoff [MD-006].
   * @private
   */
  _attemptReconnect() {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      this.emit('reconnect-failed', { attempts: this.reconnectAttempts });
      return;
    }

    this.reconnectAttempts++;
    const delay = this.reconnectIntervalMs * Math.min(this.reconnectAttempts, 5);

    this.emit('reconnecting', { attempt: this.reconnectAttempts, delay });

    this.reconnectTimer = setTimeout(async () => {
      try {
        await this.connect();
        this.emit('reconnected');
      } catch {
        // Will trigger close event and re-enter _attemptReconnect
      }
    }, delay);

    if (this.reconnectTimer.unref) {
      this.reconnectTimer.unref();
    }
  }

  /**
   * Send a JSON protocol message to the host.
   *
   * @param {Object} msg
   */
  sendMessage(msg) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('Cannot send message: TurfClient is not connected');
    }
    this.ws.send(JSON.stringify(msg));
  }

  /**
   * Submit prompt to active agent on host [MD-020].
   *
   * @param {string} promptText
   */
  submitPrompt(promptText) {
    this.sendMessage({
      type: 'PROMPT_SUBMIT',
      peerId: this.peerId,
      username: this.username,
      text: promptText
    });
  }

  /**
   * Add a task to shared Intent Board.
   *
   * @param {Object} task
   */
  addTask(task) {
    this.sendMessage({
      type: 'TASK_ADD',
      task: {
        title: task.title,
        assignee: task.assignee || this.username,
        status: task.status || 'queued',
        targetFiles: task.targetFiles || []
      }
    });
  }

  /**
   * Update task on shared Intent Board.
   *
   * @param {string} taskId
   * @param {Object} updates
   */
  updateTask(taskId, updates) {
    this.sendMessage({
      type: 'TASK_UPDATE',
      taskId,
      updates
    });
  }

  /**
   * Request atomic lock sequence on host [MD-002], [MD-040].
   *
   * @param {string} taskId
   * @param {string[]} files
   */
  acquireLock(taskId, files) {
    this.sendMessage({
      type: 'LOCK_ACQUIRE',
      taskId,
      files,
      ownerId: this.username
    });
  }

  /**
   * Release file locks for a task.
   *
   * @param {string} taskId
   */
  releaseLocks(taskId) {
    this.sendMessage({
      type: 'LOCK_RELEASE',
      taskId
    });
  }

  /**
   * Perform reverse port forwarding request across the tunnel to host dev server [MD-031].
   *
   * @param {Object} options
   * @param {number} options.port Port on host (e.g. 3000)
   * @param {string} [options.method='GET']
   * @param {string} [options.path='/']
   * @param {Object} [options.headers={}]
   * @param {string|null} [options.body=null]
   * @param {number} [options.timeoutMs=5000]
   * @returns {Promise<{ statusCode: number, headers: Object, body: string, isBase64: boolean }>}
   */
  async requestDevProxy({
    port,
    method = 'GET',
    path: reqPath = '/',
    headers = {},
    body = null,
    timeoutMs = 5000
  }) {
    const requestId = `req_${crypto.randomBytes(4).toString('hex')}`;

    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pendingProxyRequests.delete(requestId);
        reject(new Error(`Dev proxy request ${requestId} to port ${port} timed out`));
      }, timeoutMs);

      this.pendingProxyRequests.set(requestId, { resolve, reject, timer });

      try {
        this.sendMessage({
          type: 'DEV_PROXY_REQ',
          requestId,
          port,
          method,
          path: reqPath,
          headers,
          body
        });
      } catch (err) {
        clearTimeout(timer);
        this.pendingProxyRequests.delete(requestId);
        reject(err);
      }
    });
  }

  /**
   * Starts a local reverse proxy server on the peer machine [MD-031].
   * Allows the peer to open http://localhost:<localPort> and preview
   * the host dev server running at targetPort (e.g. 3000).
   *
   * @param {number} targetPort Port on host machine (e.g. 3000)
   * @param {number} [localPort=targetPort] Port to bind on peer machine
   * @returns {Promise<{ port: number, url: string, close: Function }>}
   */
  async startLocalDevProxy(targetPort, localPort = targetPort) {
    if (this.localProxyServers.has(localPort)) {
      const existing = this.localProxyServers.get(localPort);
      return { port: localPort, url: `http://localhost:${localPort}`, close: () => existing.close() };
    }

    return new Promise((resolve, reject) => {
      const server = http.createServer(async (req, res) => {
        const chunks = [];
        req.on('data', (c) => chunks.push(c));
        req.on('end', async () => {
          try {
            const bodyStr = chunks.length > 0 ? Buffer.concat(chunks).toString('utf8') : null;
            const result = await this.requestDevProxy({
              port: targetPort,
              method: req.method,
              path: req.url,
              headers: req.headers,
              body: bodyStr
            });

            res.writeHead(result.statusCode, result.headers);
            if (result.isBase64) {
              res.end(Buffer.from(result.body, 'base64'));
            } else {
              res.end(result.body || '');
            }
          } catch (err) {
            res.writeHead(502, { 'Content-Type': 'text/plain' });
            res.end(`TurfCode Reverse Proxy Error: ${err.message}`);
          }
        });
      });

      server.on('error', (err) => {
        reject(err);
      });

      server.listen(localPort, '127.0.0.1', () => {
        const addr = server.address();
        const boundPort = typeof addr === 'object' && addr !== null ? addr.port : localPort;
        this.localProxyServers.set(boundPort, server);

        resolve({
          port: boundPort,
          url: `http://localhost:${boundPort}`,
          close: () => {
            server.close();
            this.localProxyServers.delete(boundPort);
          }
        });
      });
    });
  }

  /**
   * Handle user typing locally with zero latency [MD-020].
   *
   * @param {string} char
   * @returns {string} Current input buffer
   */
  type(char) {
    return this.inputBuffer.insert(char);
  }

  /**
   * Delete character before cursor locally.
   *
   * @returns {string}
   */
  backspace() {
    return this.inputBuffer.backspace();
  }

  /**
   * Submit local input buffer:
   * Only completed prompts and commands cross the network [MD-020].
   *
   * @returns {string} Submitted command
   */
  submitInput() {
    const text = this.inputBuffer.submit();
    if (!text || text.trim().length === 0) {
      return '';
    }

    const trimmed = text.trim();

    if (trimmed.startsWith('/task add ')) {
      const title = trimmed.substring(10).trim();
      this.addTask({ title });
    } else {
      this.submitPrompt(trimmed);
    }

    return trimmed;
  }

  /**
   * Disconnect client, cancel reconnect loops, and close local proxies.
   */
  disconnect() {
    this.isManuallyClosed = true;

    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    for (const pending of this.pendingProxyRequests.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error('Client disconnected'));
    }
    this.pendingProxyRequests.clear();

    for (const server of this.localProxyServers.values()) {
      try {
        server.close();
      } catch {}
    }
    this.localProxyServers.clear();

    if (this.ws) {
      try {
        this.ws.close(1000, 'Client disconnected');
      } catch {}
      this.ws = null;
    }

    this.isConnected = false;
    this.emit('disconnected', { code: 1000, reason: 'Manual disconnect' });
  }
}
