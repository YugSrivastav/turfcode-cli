/**
 * TurfCode CLI - Atomic Multi-File Lock Manager
 *
 * Implements coarse-grained task scope file locking, process-bound heartbeat
 * leases, atomic multi-file sequence locking, fail-fast conflict detection,
 * and host force-unlock.
 *
 * Complies with [MD-002], [MD-007], [MD-023], [MD-040].
 */

import fs from 'fs';
import path from 'path';
import { EventEmitter } from 'events';

/**
 * Check if a process ID is currently alive on the host.
 * @param {number|null} pid
 * @returns {boolean}
 */
export function isPidAlive(pid) {
  if (!pid || typeof pid !== 'number') return true;
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    // EPERM means process exists but we lack permissions (still alive)
    // ESRCH means process does not exist
    return err.code === 'EPERM';
  }
}

/**
 * Normalize file path for lock table indexing.
 * Normalizes slashes, trims leading './', resolves relative to cwd if inside cwd.
 * @param {string} filePath
 * @param {string} cwd
 * @returns {string}
 */
export function normalizeLockPath(filePath, cwd = process.cwd()) {
  if (!filePath || typeof filePath !== 'string') return '';
  const trimmed = filePath.trim();
  let relPath = trimmed;
  if (path.isAbsolute(trimmed)) {
    relPath = path.relative(cwd, trimmed);
  }
  return relPath.replace(/\\/g, '/').replace(/^\.\//, '');
}

export class LockManager extends EventEmitter {
  /**
   * @param {Object} options
   * @param {string} [options.cwd] Base directory for the repository / locks
   * @param {string} [options.locksPath] Path to .turf/locks.json
   * @param {number} [options.heartbeatTtlMs] TTL for heartbeat lease (default 10000ms / 10s per [MD-007])
   * @param {Array} [options.initialLocks] Initial lock list
   * @param {boolean} [options.autoStartMonitor] Whether to automatically start background cleanup timer
   */
  constructor(options = {}) {
    super();
    this.cwd = options.cwd || process.cwd();
    this.locksPath = options.locksPath || path.join(this.cwd, '.turf', 'locks.json');
    this.heartbeatTtlMs = typeof options.heartbeatTtlMs === 'number' ? options.heartbeatTtlMs : 10000;
    
    // In-memory lock map: normalizedFilePath -> LockObject
    this.locks = new Map();
    this.monitorInterval = null;

    if (Array.isArray(options.initialLocks)) {
      for (const lock of options.initialLocks) {
        const norm = normalizeLockPath(lock.filePath || lock.file, this.cwd);
        if (norm) {
          this.locks.set(norm, {
            filePath: norm,
            ownerId: lock.ownerId || lock.holder || 'unknown',
            taskId: String(lock.taskId || lock.task || 'default'),
            timestamp: lock.timestamp || Date.now(),
            lastHeartbeat: lock.lastHeartbeat || Date.now(),
            pid: typeof lock.pid === 'number' ? lock.pid : null
          });
        }
      }
    } else {
      this.load();
    }

    if (options.autoStartMonitor) {
      this.startHeartbeatMonitor();
    }
  }

  /**
   * Start periodic background sweep of expired locks.
   * @param {number} [intervalMs=1000]
   */
  startHeartbeatMonitor(intervalMs = 1000) {
    if (this.monitorInterval) return;
    this.monitorInterval = setInterval(() => {
      this.cleanExpiredLocks();
    }, intervalMs);
    if (this.monitorInterval.unref) {
      this.monitorInterval.unref();
    }
  }

  /**
   * Stop background heartbeat monitor.
   */
  stopHeartbeatMonitor() {
    if (this.monitorInterval) {
      clearInterval(this.monitorInterval);
      this.monitorInterval = null;
    }
  }

  /**
   * Atomically save locks to .turf/locks.json
   */
  _persist() {
    try {
      const dir = path.dirname(this.locksPath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }

      const lockList = Array.from(this.locks.values());
      const payload = JSON.stringify({
        version: 1,
        updatedAt: Date.now(),
        locks: lockList
      }, null, 2);

      const tempFile = `${this.locksPath}.tmp.${Date.now()}.${Math.random().toString(36).slice(2)}`;
      fs.writeFileSync(tempFile, payload, 'utf8');
      fs.renameSync(tempFile, this.locksPath);
    } catch (err) {
      // In tests or read-only workspaces, log error but do not crash in-memory state
      this.emit('error', err);
    }
  }

  /**
   * Load existing locks from .turf/locks.json
   */
  load() {
    try {
      if (fs.existsSync(this.locksPath)) {
        const raw = fs.readFileSync(this.locksPath, 'utf8');
        const parsed = JSON.parse(raw);
        const lockList = Array.isArray(parsed) ? parsed : (Array.isArray(parsed.locks) ? parsed.locks : []);
        this.locks.clear();
        for (const item of lockList) {
          const norm = normalizeLockPath(item.filePath || item.file, this.cwd);
          if (norm) {
            this.locks.set(norm, {
              filePath: norm,
              ownerId: item.ownerId || item.holder || 'unknown',
              taskId: String(item.taskId || item.task || 'default'),
              timestamp: item.timestamp || Date.now(),
              lastHeartbeat: item.lastHeartbeat || Date.now(),
              pid: typeof item.pid === 'number' ? item.pid : null
            });
          }
        }
        this.cleanExpiredLocks();
      }
    } catch (err) {
      this.emit('error', err);
    }
  }

  /**
   * Clean any locks where heartbeat has ceased for >10 seconds or process terminated.
   * Complies with [MD-007].
   * @returns {Array} List of dissolved locks
   */
  cleanExpiredLocks() {
    const now = Date.now();
    const expired = [];

    for (const [normPath, lock] of this.locks.entries()) {
      const isHeartbeatExpired = (now - lock.lastHeartbeat) > this.heartbeatTtlMs;
      const isDeadProcess = lock.pid !== null && !isPidAlive(lock.pid);

      if (isHeartbeatExpired || isDeadProcess) {
        expired.push({
          ...lock,
          reason: isDeadProcess ? 'process_terminated' : 'heartbeat_timeout'
        });
        this.locks.delete(normPath);
      }
    }

    if (expired.length > 0) {
      this._persist();
      this.emit('expired', expired);
      this.emit('change', this.getLocks());
    }

    return expired;
  }

  /**
   * Pre-flight atomic sequence file locking.
   *
   * Must validate that NONE of the declared files in filePaths are currently held.
   * If ANY file is held, fail-fast immediately returning { success: false, conflictFile, heldBy }
   * without acquiring partial locks ([MD-023], [MD-040]).
   * If all are free, lock all files atomically with timestamp and agent PID/owner ID.
   *
   * @param {string} ownerId User or Agent identifier (e.g. 'Host', 'Ayush')
   * @param {string|number} taskId Task identifier
   * @param {string[]} filePaths Declared sequence of files to modify
   * @param {Object} [options]
   * @param {number|null} [options.pid] Process ID bound to the lock lease
   * @returns {{ success: boolean, conflictFile?: string, heldBy?: string, locks?: Array, error?: string }}
   */
  acquireAtomicSequence(ownerId, taskId, filePaths = [], options = {}) {
    if (!Array.isArray(filePaths) || filePaths.length === 0) {
      return { success: true, locks: [] };
    }

    // Clean expired locks first before checking availability
    this.cleanExpiredLocks();

    const normalizedPaths = [];
    for (const fp of filePaths) {
      const norm = normalizeLockPath(fp, this.cwd);
      if (norm) normalizedPaths.push(norm);
    }

    const tId = String(taskId);
    const pid = typeof options.pid === 'number' ? options.pid : null;
    const now = Date.now();

    // 1. Conflict Check: Verify that NONE of the files are held
    for (const normPath of normalizedPaths) {
      const existing = this.locks.get(normPath);
      if (existing) {
        // If already held by a DIFFERENT task, immediate fail-fast rejection
        if (existing.taskId !== tId) {
          const conflict = {
            success: false,
            conflictFile: normPath,
            heldBy: existing.ownerId,
            taskId: existing.taskId,
            error: `[LOCK CONFLICT] Cannot acquire lock on ${normPath} (Held by ${existing.ownerId} on Task #${existing.taskId}).`
          };
          this.emit('conflict', conflict);
          return conflict;
        }
      }
    }

    // 2. Atomic Acquisition: All free -> acquire all files atomically without partial-state hazards
    const acquiredLocks = [];
    for (const normPath of normalizedPaths) {
      const lock = {
        filePath: normPath,
        ownerId: String(ownerId),
        taskId: tId,
        timestamp: now,
        lastHeartbeat: now,
        pid
      };
      this.locks.set(normPath, lock);
      acquiredLocks.push(lock);
    }

    this._persist();
    this.emit('change', this.getLocks());

    return {
      success: true,
      locks: acquiredLocks
    };
  }

  /**
   * Dissolve all locks associated with a task ID.
   * @param {string|number} taskId
   * @returns {{ success: boolean, released: Array }}
   */
  releaseLocks(taskId) {
    const tId = String(taskId);
    const released = [];

    for (const [normPath, lock] of this.locks.entries()) {
      if (lock.taskId === tId) {
        released.push(lock);
        this.locks.delete(normPath);
      }
    }

    if (released.length > 0) {
      this._persist();
      this.emit('change', this.getLocks());
    }

    return {
      success: true,
      released
    };
  }

  /**
   * Release a single file lock.
   * @param {string} filePath
   * @returns {boolean} True if released
   */
  releaseFile(filePath) {
    const norm = normalizeLockPath(filePath, this.cwd);
    if (this.locks.has(norm)) {
      this.locks.delete(norm);
      this._persist();
      this.emit('change', this.getLocks());
      return true;
    }
    return false;
  }

  /**
   * Send heartbeat to renew lock lease for a task.
   * Updates lastHeartbeat for all locks belonging to taskId.
   * @param {string|number} taskId
   * @param {Object} [options]
   * @param {number} [options.pid]
   * @returns {boolean} True if locks exist and were renewed
   */
  heartbeat(taskId, options = {}) {
    const tId = String(taskId);
    const now = Date.now();
    let updated = false;

    for (const lock of this.locks.values()) {
      if (lock.taskId === tId) {
        lock.lastHeartbeat = now;
        if (typeof options.pid === 'number') {
          lock.pid = options.pid;
        }
        updated = true;
      }
    }

    if (updated) {
      this._persist();
    }

    return updated;
  }

  /**
   * Host manual force unlock to break a stuck lock.
   * Complies with [MD-007].
   * @param {string} filePath
   * @returns {{ success: boolean, released?: string, reason?: string }}
   */
  forceUnlock(filePath) {
    const norm = normalizeLockPath(filePath, this.cwd);
    if (this.locks.has(norm)) {
      const removed = this.locks.get(norm);
      this.locks.delete(norm);
      this._persist();
      this.emit('change', this.getLocks());
      return {
        success: true,
        released: norm,
        heldBy: removed.ownerId,
        taskId: removed.taskId
      };
    }

    return {
      success: false,
      reason: 'not_locked',
      filePath: norm
    };
  }

  /**
   * Check if a file is currently locked.
   * @param {string} filePath
   * @returns {Object|null}
   */
  isLocked(filePath) {
    this.cleanExpiredLocks();
    const norm = normalizeLockPath(filePath, this.cwd);
    return this.locks.get(norm) || null;
  }

  /**
   * Get all active locks as an array.
   * @returns {Array}
   */
  getLocks() {
    this.cleanExpiredLocks();
    return Array.from(this.locks.values());
  }

  /**
   * Destroy and clean up resources.
   */
  destroy() {
    this.stopHeartbeatMonitor();
    this.removeAllListeners();
  }
}
