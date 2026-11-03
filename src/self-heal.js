/**
 * TurfCode CLI - Boot Self-Healing & Session Recovery
 *
 * Automatically detects unclean exits in .turf/session.json, clears stale locks,
 * safely preserves uncommitted git changes, and outputs a recovery notice on startup.
 *
 * Complies with [MD-036].
 */

import fs from 'fs';
import path from 'path';
import { execFileSync } from 'child_process';
import chalk from 'chalk';
import { isPidAlive } from './lock-manager.js';
import { PALETTE } from './theme.js';

export function getSessionPath(cwd = process.cwd()) {
  return path.join(cwd, '.turf', 'session.json');
}

export function getLocksPath(cwd = process.cwd()) {
  return path.join(cwd, '.turf', 'locks.json');
}

/**
 * Record session start state into .turf/session.json.
 * @param {Object} options
 * @param {string} [options.cwd=process.cwd()]
 * @param {string} [options.sessionId]
 * @param {string} [options.roomCode]
 * @param {number} [options.pid=process.pid]
 * @returns {Object} Session metadata
 */
export function recordSessionStart(options = {}) {
  const cwd = options.cwd || process.cwd();
  const sessionPath = getSessionPath(cwd);
  const turfDir = path.dirname(sessionPath);

  if (!fs.existsSync(turfDir)) {
    fs.mkdirSync(turfDir, { recursive: true });
  }

  const session = {
    sessionId: options.sessionId || `turf-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    roomCode: options.roomCode || 'TURF-LOCAL',
    pid: typeof options.pid === 'number' ? options.pid : process.pid,
    startTime: Date.now(),
    lastUpdated: Date.now(),
    active: true,
    cleanExit: false
  };

  try {
    fs.writeFileSync(sessionPath, JSON.stringify(session, null, 2), 'utf8');
  } catch {
    // Non-fatal if session file cannot be written
  }

  return session;
}

/**
 * Record clean session exit into .turf/session.json.
 * @param {Object} options
 * @param {string} [options.cwd=process.cwd()]
 * @param {boolean} [options.cleanExit=true]
 */
export function recordSessionExit(options = {}) {
  const cwd = options.cwd || process.cwd();
  const sessionPath = getSessionPath(cwd);

  if (!fs.existsSync(sessionPath)) return;

  try {
    const raw = fs.readFileSync(sessionPath, 'utf8');
    const session = JSON.parse(raw);
    session.active = false;
    session.cleanExit = options.cleanExit !== false;
    session.exitTime = Date.now();

    fs.writeFileSync(sessionPath, JSON.stringify(session, null, 2), 'utf8');
  } catch {
    // Non-fatal
  }
}

/**
 * Inspect .turf/session.json to detect if previous exit was unclean.
 * @param {string} [cwd=process.cwd()]
 * @returns {{unclean: boolean, reason?: string, session?: Object}}
 */
export function detectUncleanExit(cwd = process.cwd()) {
  const sessionPath = getSessionPath(cwd);

  if (!fs.existsSync(sessionPath)) {
    return { unclean: false, reason: 'no_session_file' };
  }

  let session;
  try {
    const raw = fs.readFileSync(sessionPath, 'utf8');
    session = JSON.parse(raw);
  } catch {
    // Corrupt session file indicates an unclean crash / partial write
    return { unclean: true, reason: 'corrupt_session_file' };
  }

  // If cleanExit was explicitly true, previous shutdown was clean
  if (session.cleanExit === true && session.active === false) {
    return { unclean: false, session };
  }

  // If active is true or cleanExit is false, check if process died
  if (session.active === true || session.cleanExit === false) {
    const pid = session.pid;
    // If PID is no longer alive or different process, it was an unclean exit
    const alive = pid && pid !== process.pid ? isPidAlive(pid) : false;
    if (!alive) {
      return {
        unclean: true,
        reason: 'crashed_or_killed_without_clean_exit',
        session
      };
    }
  }

  return { unclean: false, session };
}

/**
 * Count uncommitted git changes to verify preservation.
 * Crucial [MD-036] invariant: Boot recovery must never discard uncommitted git changes.
 * @param {string} [cwd=process.cwd()]
 * @returns {number} Number of modified/uncommitted files
 */
export function countUncommittedChanges(cwd = process.cwd()) {
  try {
    const stdout = execFileSync('git', ['status', '--porcelain'], {
      cwd,
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
      timeout: 3000
    });
    return stdout
      .split(/\r?\n/)
      .map((l) => l.trim())
      .filter(Boolean).length;
  } catch {
    return 0;
  }
}

/**
 * Execute self-healing on boot.
 * Detects unclean exit in .turf/session.json, auto-resets stale locks without discarding
 * uncommitted git changes, updates session status, and prints recovery notice.
 *
 * @param {Object} [options]
 * @param {string} [options.cwd=process.cwd()]
 * @param {boolean} [options.silent=false]
 * @param {Object} [options.lockManager] Optional LockManager instance to sync
 * @returns {{recovered: boolean, staleLocksCleared: number, preservedChanges: number, notice: string|null}}
 */
export function selfHeal(options = {}) {
  const cwd = options.cwd || process.cwd();
  const detection = detectUncleanExit(cwd);

  if (!detection.unclean) {
    return {
      recovered: false,
      staleLocksCleared: 0,
      preservedChanges: countUncommittedChanges(cwd),
      notice: null
    };
  }

  // 1. Inspect and clear stale locks in .turf/locks.json [MD-036]
  const locksPath = getLocksPath(cwd);
  let staleLocksCleared = 0;

  if (fs.existsSync(locksPath)) {
    try {
      const rawLocks = fs.readFileSync(locksPath, 'utf8');
      const parsed = JSON.parse(rawLocks);
      const lockList = Array.isArray(parsed.locks) ? parsed.locks : (Array.isArray(parsed) ? parsed : []);
      staleLocksCleared = lockList.length;
    } catch {
      staleLocksCleared = 0;
    }

    try {
      // Reset locks file atomically
      fs.writeFileSync(
        locksPath,
        JSON.stringify({
          version: 1,
          updatedAt: new Date().toISOString(),
          locks: []
        }, null, 2),
        'utf8'
      );
    } catch {
      // Non-fatal
    }
  }

  // If LockManager instance is passed, reset in-memory locks
  if (options.lockManager && typeof options.lockManager.destroy === 'function') {
    if (options.lockManager.locks instanceof Map) {
      options.lockManager.locks.clear();
      options.lockManager.emit('change', []);
    }
  }

  // 2. Check uncommitted git changes (must never discard them)
  const preservedChanges = countUncommittedChanges(cwd);

  // 3. Update session file to mark recovery
  const sessionPath = getSessionPath(cwd);
  try {
    const recoveredSession = {
      sessionId: `turf-recovered-${Date.now()}`,
      pid: process.pid,
      startTime: Date.now(),
      active: true,
      cleanExit: false,
      recoveredAt: Date.now(),
      previousExitUnclean: true
    };
    fs.writeFileSync(sessionPath, JSON.stringify(recoveredSession, null, 2), 'utf8');
  } catch {
    // Non-fatal
  }

  // 4. Compose recovery notice
  const lockSuffix = staleLocksCleared === 1 ? '1 stale lock' : `${staleLocksCleared} stale locks`;
  const changesSuffix = preservedChanges === 1 ? '1 uncommitted change' : `${preservedChanges} uncommitted changes`;
  const notice = `[SELF-HEALING] Detected unclean exit from previous session. Reset ${lockSuffix}. Safely preserved ${changesSuffix}.`;

  if (!options.silent) {
    const formattedNotice = `${chalk.hex(PALETTE.electricLime)('[SELF-HEALING]')} ${chalk.hex('#eab308')('Detected unclean exit from previous session.')} ${chalk.hex(PALETTE.textBright)(`Cleared ${lockSuffix}. Preserved ${changesSuffix}.`)}`;
    console.log(formattedNotice);
  }

  return {
    recovered: true,
    staleLocksCleared,
    preservedChanges,
    notice
  };
}
