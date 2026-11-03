import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import path from 'path';
import os from 'os';
import { execFileSync } from 'child_process';
import {
  selfHeal,
  detectUncleanExit,
  recordSessionStart,
  recordSessionExit,
  getSessionPath,
  getLocksPath
} from '../src/self-heal.js';
import { AudioBell } from '../src/bell.js';

test('selfHeal: skips recovery when previous exit was clean [MD-036]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-heal-clean-'));

  try {
    recordSessionStart({ cwd: tmpDir, roomCode: 'TURF-TEST' });
    recordSessionExit({ cwd: tmpDir, cleanExit: true });

    const detection = detectUncleanExit(tmpDir);
    assert.equal(detection.unclean, false);

    const result = selfHeal({ cwd: tmpDir, silent: true });
    assert.equal(result.recovered, false);
    assert.equal(result.staleLocksCleared, 0);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('selfHeal: auto-resets stale locks on unclean exit without discarding uncommitted git changes [MD-036]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-heal-unclean-'));

  try {
    // 1. Initialize git repo
    execFileSync('git', ['init', '-b', 'main'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.name', 'Turf Tester'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.email', 'tester@turfcode.dev'], { cwd: tmpDir });

    const fileA = path.join(tmpDir, 'fileA.js');
    fs.writeFileSync(fileA, 'initial commit\n');
    execFileSync('git', ['add', '.'], { cwd: tmpDir });
    execFileSync('git', ['commit', '-m', 'Initial commit'], { cwd: tmpDir });

    // 2. Introduce uncommitted git change
    fs.writeFileSync(fileA, 'uncommitted work in progress - MUST NOT BE DISCARDED!\n');

    // 3. Create simulated unclean session (active: true, cleanExit: false, dead PID)
    const sessionPath = getSessionPath(tmpDir);
    fs.mkdirSync(path.dirname(sessionPath), { recursive: true });
    fs.writeFileSync(
      sessionPath,
      JSON.stringify({
        sessionId: 'crashed-session',
        pid: 9999999, // guaranteed dead pid
        active: true,
        cleanExit: false,
        startTime: Date.now() - 60000
      }, null, 2),
      'utf8'
    );

    // 4. Create stale locks in .turf/locks.json
    const locksPath = getLocksPath(tmpDir);
    fs.writeFileSync(
      locksPath,
      JSON.stringify({
        version: 1,
        locks: [
          { filePath: 'fileA.js', ownerId: 'GhostUser', taskId: 'stale-1' },
          { filePath: 'fileB.js', ownerId: 'GhostUser', taskId: 'stale-2' }
        ]
      }, null, 2),
      'utf8'
    );

    // 5. Run selfHeal
    const result = selfHeal({ cwd: tmpDir, silent: true });

    assert.equal(result.recovered, true, 'Recovery must be executed');
    assert.equal(result.staleLocksCleared, 2, 'Must clear 2 stale locks');
    assert.ok(result.preservedChanges >= 1, 'Must preserve uncommitted changes');
    assert.ok(result.notice.includes('Detected unclean exit'), 'Notice must detail unclean exit');
    assert.ok(result.notice.includes('stale locks'), 'Notice must detail stale locks cleared');

    // Verify .turf/locks.json is reset to empty
    const updatedLocks = JSON.parse(fs.readFileSync(locksPath, 'utf8'));
    assert.equal(updatedLocks.locks.length, 0);

    // CRITICAL [MD-036] INVARIANT: Verify uncommitted changes were NOT discarded!
    const fileAContent = fs.readFileSync(fileA, 'utf8');
    assert.ok(
      fileAContent.includes('uncommitted work in progress - MUST NOT BE DISCARDED!'),
      'Uncommitted git changes must be preserved untouched'
    );
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('AudioBell: emits ANSI \\x07 on task completion and lock conflict, toggleable with --silent [MD-034]', () => {
  let outputBuffer = '';
  const mockStream = {
    write: (data) => {
      outputBuffer += data;
    }
  };

  // 1. Normal mode (audible)
  const bell = new AudioBell({ silent: false, outputStream: mockStream });
  assert.equal(bell.isSilent(), false);

  const ring1 = bell.onTaskCompletion('task-1');
  assert.equal(ring1, true);
  assert.equal(outputBuffer, '\x07');

  const ring2 = bell.onLockConflict('src/auth.js');
  assert.equal(ring2, true);
  assert.equal(outputBuffer, '\x07\x07');

  assert.equal(bell.getHistory().length, 2);

  // 2. Silent mode (muted)
  outputBuffer = '';
  bell.setSilent(true);
  assert.equal(bell.isSilent(), true);

  const silentRing = bell.onTaskCompletion('task-2');
  assert.equal(silentRing, false);
  assert.equal(outputBuffer, '', 'Silent mode must not write \\x07 to output stream');
});
