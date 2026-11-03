/**
 * TurfCode CLI - Agent Runner & Credential Passthrough Engine
 *
 * Manages parallel autonomous agent subprocesses (Antigravity `agy`) with:
 * - Direct binary execution via child_process.spawn ([MD-029])
 * - Multi-agent concurrency without cross-talk ([MD-003])
 * - Peer credential passthrough: GEMINI_API_KEY / AGY_AUTH_TOKEN & AGY_SESSION_ID ([MD-010], [MD-019])
 * - 30ms throttled frame buffering to eliminate TUI redraw flickering ([MD-021])
 * - Owner-only floor control: signal dispatch rejection for unauthorized peers ([MD-013])
 * - Process pause/resume on peer disconnect via SIGSTOP / SIGCONT ([MD-024])
 */

import { spawn } from 'child_process';
import { EventEmitter } from 'events';
import path from 'path';

export class AgentRunner extends EventEmitter {
  /**
   * @param {Object} [options]
   * @param {string} [options.agyBinary='agy'] Path or name of the agy binary
   * @param {string} [options.defaultCwd=process.cwd()] Default working directory
   * @param {number} [options.frameIntervalMs=30] Throttled frame buffer interval in ms ([MD-021])
   * @param {Object} [options.intentBoard] Optional IntentBoard instance for state synchronization
   */
  constructor(options = {}) {
    super();
    this.agyBinary = options.agyBinary || 'agy';
    this.defaultCwd = options.defaultCwd || process.cwd();
    this.frameIntervalMs = typeof options.frameIntervalMs === 'number' ? options.frameIntervalMs : 30;
    this.intentBoard = options.intentBoard || null;

    /** @type {Map<string, Object>} */
    this.agents = new Map();
  }

  /**
   * Spawn an agy agent directly as a subprocess with injected credentials.
   *
   * @param {Object} options
   * @param {string|number} options.taskId Unique task ID
   * @param {string} [options.prompt] Prompt string passed to agy (-p <prompt>)
   * @param {string} [options.ownerId='host'] Invoking user ID (enforces floor control)
   * @param {string} [options.authToken] Peer auth token injected as GEMINI_API_KEY & AGY_AUTH_TOKEN
   * @param {string} [options.sessionId] Peer session ID injected as AGY_SESSION_ID
   * @param {string} [options.worktreePath] Path to isolated git worktree directory ([MD-022])
   * @param {string} [options.agyBinary] Override binary for this agent run
   * @param {string[]} [options.args] Custom argument array (defaults to ['-p', prompt])
   * @param {Object} [options.env] Additional environment variables
   * @returns {Object} Agent record
   */
  spawnAgent(options = {}) {
    const {
      taskId: rawTaskId,
      prompt = '',
      ownerId = 'host',
      authToken,
      sessionId,
      worktreePath,
      agyBinary: customBinary,
      args: customArgs,
      env: customEnv = {}
    } = options;

    if (!rawTaskId) {
      throw new Error('AgentRunner.spawnAgent: taskId is required');
    }
    const taskId = String(rawTaskId);

    // Check if task is already running
    const existing = this.agents.get(taskId);
    if (existing && (existing.status === 'RUNNING' || existing.status === 'PAUSED')) {
      throw new Error(`Agent for task #${taskId} is already running`);
    }

    const binary = customBinary || this.agyBinary;
    const cwd = worktreePath ? path.resolve(worktreePath) : this.defaultCwd;

    // Determine arguments: direct binary execution [MD-029]
    let args;
    if (Array.isArray(customArgs)) {
      args = customArgs;
    } else if (prompt) {
      args = ['-p', prompt];
    } else {
      args = [];
    }

    // Build subprocess environment with isolated credentials [MD-010], [MD-019]
    const procEnv = {
      ...process.env,
      ...customEnv,
      TURF_TASK_ID: taskId,
      TURF_OWNER_ID: String(ownerId)
    };

    if (authToken) {
      procEnv.GEMINI_API_KEY = authToken;
      procEnv.AGY_AUTH_TOKEN = authToken;
    }
    if (sessionId) {
      procEnv.AGY_SESSION_ID = sessionId;
    }

    const isWin = process.platform === 'win32';

    let proc;
    try {
      proc = spawn(binary, args, {
        cwd,
        env: procEnv,
        detached: !isWin
      });
    } catch (err) {
      this._emitError(taskId, err);
      throw err;
    }

    // 30ms Throttled Frame Buffer state [MD-021]
    let buffer = '';
    let frameTimer = null;

    const flushBuffer = () => {
      if (frameTimer) {
        clearTimeout(frameTimer);
        frameTimer = null;
      }
      if (buffer.length > 0) {
        const delta = buffer;
        buffer = '';
        agentRecord.outputHistory += delta;
        this.emit('output', { taskId, text: delta, delta, stream: 'stdout' });
      }
    };

    const appendChunk = (chunk) => {
      buffer += chunk.toString();
      if (!frameTimer) {
        frameTimer = setTimeout(flushBuffer, this.frameIntervalMs);
      }
    };

    const agentRecord = {
      taskId,
      ownerId: String(ownerId),
      prompt,
      binary,
      args,
      worktreePath: cwd,
      pid: proc.pid,
      status: 'RUNNING',
      exitCode: null,
      signal: null,
      spawnedAt: Date.now(),
      outputHistory: '',
      process: proc,
      flushBuffer,
      terminate: (requesterId, signal) => this.terminateAgent(taskId, requesterId, signal),
      pause: () => this.pauseAgent(taskId),
      resume: () => this.resumeAgent(taskId)
    };

    this.agents.set(taskId, agentRecord);

    if (proc.stdout) {
      proc.stdout.on('data', (data) => {
        appendChunk(data);
      });
    }

    if (proc.stderr) {
      proc.stderr.on('data', (data) => {
        appendChunk(data);
      });
    }

    proc.on('close', (code, signal) => {
      // Auto-flush frame buffer immediately on process exit [MD-021]
      flushBuffer();

      if (agentRecord.status !== 'TERMINATED') {
        agentRecord.status = code === 0 ? 'COMPLETED' : 'EXITED';
      }
      agentRecord.exitCode = code;
      agentRecord.signal = signal;

      this.emit('exit', { taskId, code, signal });
    });

    proc.on('error', (err) => {
      flushBuffer();
      agentRecord.status = 'ERROR';
      this._emitError(taskId, err);
    });

    this.emit('spawn', {
      taskId,
      ownerId: String(ownerId),
      prompt,
      pid: proc.pid,
      worktreePath: cwd
    });

    return agentRecord;
  }

  /**
   * Terminate an agent process with owner-only floor control ([MD-013]).
   *
   * @param {string|number} taskId Target task ID
   * @param {string} requesterId User requesting the termination
   * @param {string} [signal='SIGTERM'] Signal to send
   * @returns {{ success: boolean, taskId: string, requesterId: string }}
   */
  terminateAgent(taskId, requesterId, signal = 'SIGTERM') {
    const tId = String(taskId);
    const agent = this.agents.get(tId);

    if (!agent) {
      throw new Error(`Agent for task #${tId} not found`);
    }

    // Enforce owner-only floor control [MD-013]
    if (agent.ownerId && requesterId !== agent.ownerId) {
      const err = new Error(
        `[UNAUTHORIZED] Requester '${requesterId}' is not the owner of task #${tId} (owner: '${agent.ownerId}') [MD-013]`
      );
      err.code = 'ERR_UNAUTHORIZED';
      err.ownerId = agent.ownerId;
      err.requesterId = requesterId;
      throw err;
    }

    // Auto-flush frame buffer on interruption [MD-021]
    if (typeof agent.flushBuffer === 'function') {
      agent.flushBuffer();
    }

    agent.status = 'TERMINATED';
    const proc = agent.process;

    if (proc && !proc.killed && proc.exitCode === null) {
      try {
        if (process.platform === 'win32') {
          proc.kill(signal);
        } else {
          try {
            process.kill(-proc.pid, signal);
          } catch {
            proc.kill(signal);
          }
        }
      } catch {
        // Subprocess may have already exited
      }
    }

    this.emit('terminate', { taskId: tId, requesterId, signal });
    return { success: true, taskId: tId, requesterId };
  }

  /**
   * Pause an agent on peer disconnect via SIGSTOP ([MD-024]).
   *
   * @param {string|number} taskId
   * @returns {{ success: boolean, taskId: string, status: string }}
   */
  pauseAgent(taskId) {
    const tId = String(taskId);
    const agent = this.agents.get(tId);

    if (!agent) {
      throw new Error(`Agent for task #${tId} not found`);
    }

    if (agent.status === 'PAUSED') {
      return { success: true, taskId: tId, status: 'PAUSED' };
    }

    // Flush pending frame buffer before pausing
    if (typeof agent.flushBuffer === 'function') {
      agent.flushBuffer();
    }

    const proc = agent.process;
    if (proc && !proc.killed && proc.exitCode === null) {
      try {
        if (process.platform !== 'win32') {
          try {
            process.kill(-proc.pid, 'SIGSTOP');
          } catch {
            proc.kill('SIGSTOP');
          }
        }
      } catch {
        // Ignore signaling errors if process already exited
      }
    }

    agent.status = 'PAUSED';

    // Broadcast to IntentBoard if integrated
    if (this.intentBoard && typeof this.intentBoard.updateTask === 'function') {
      try {
        this.intentBoard.updateTask(tId, { status: 'in_progress' });
      } catch {}
    }

    this.emit('pause', { taskId: tId, status: 'PAUSED' });
    return { success: true, taskId: tId, status: 'PAUSED' };
  }

  /**
   * Resume a paused agent via SIGCONT ([MD-024]).
   *
   * @param {string|number} taskId
   * @returns {{ success: boolean, taskId: string, status: string }}
   */
  resumeAgent(taskId) {
    const tId = String(taskId);
    const agent = this.agents.get(tId);

    if (!agent) {
      throw new Error(`Agent for task #${tId} not found`);
    }

    if (agent.status !== 'PAUSED') {
      return { success: true, taskId: tId, status: agent.status };
    }

    const proc = agent.process;
    if (proc && !proc.killed && proc.exitCode === null) {
      try {
        if (process.platform !== 'win32') {
          try {
            process.kill(-proc.pid, 'SIGCONT');
          } catch {
            proc.kill('SIGCONT');
          }
        }
      } catch {
        // Ignore signaling errors if process already exited
      }
    }

    agent.status = 'RUNNING';
    this.emit('resume', { taskId: tId, status: 'RUNNING' });
    return { success: true, taskId: tId, status: 'RUNNING' };
  }

  /**
   * Immediately flush the frame buffer for an agent.
   *
   * @param {string|number} taskId
   */
  flushAgent(taskId) {
    const tId = String(taskId);
    const agent = this.agents.get(tId);
    if (agent && typeof agent.flushBuffer === 'function') {
      agent.flushBuffer();
    }
  }

  /**
   * Get an agent record by task ID.
   *
   * @param {string|number} taskId
   * @returns {Object|null}
   */
  getAgent(taskId) {
    return this.agents.get(String(taskId)) || null;
  }

  /**
   * Check if an agent exists for a task ID.
   *
   * @param {string|number} taskId
   * @returns {boolean}
   */
  hasAgent(taskId) {
    return this.agents.has(String(taskId));
  }

  /**
   * List all tracked agents.
   *
   * @returns {Object[]}
   */
  listAgents() {
    return Array.from(this.agents.values());
  }

  /**
   * List currently active (RUNNING or PAUSED) agents.
   *
   * @returns {Object[]}
   */
  getActiveAgents() {
    return Array.from(this.agents.values()).filter(
      (a) => a.status === 'RUNNING' || a.status === 'PAUSED'
    );
  }

  /**
   * Safe error emitter that does not throw unhandled exception if no listeners exist.
   *
   * @private
   */
  _emitError(taskId, error) {
    if (this.listenerCount('error') > 0) {
      this.emit('error', { taskId, error });
    }
  }

  /**
   * Cleanly terminate all managed subprocesses and clear state.
   */
  destroy() {
    for (const [taskId, agent] of this.agents.entries()) {
      if (typeof agent.flushBuffer === 'function') {
        agent.flushBuffer();
      }
      const proc = agent.process;
      if (proc && !proc.killed && proc.exitCode === null) {
        try {
          if (process.platform === 'win32') {
            proc.kill('SIGKILL');
          } else {
            try {
              process.kill(-proc.pid, 'SIGKILL');
            } catch {
              proc.kill('SIGKILL');
            }
          }
        } catch {}
      }
      agent.status = 'TERMINATED';
    }
    this.agents.clear();
    this.removeAllListeners();
  }
}
