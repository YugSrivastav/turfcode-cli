import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import path from 'path';
import os from 'os';
import { spawn } from 'child_process';
import { LockManager, normalizeLockPath, isPidAlive } from '../src/lock-manager.js';

test('normalizeLockPath handles Windows and POSIX separators and relative paths', () => {
  const cwd = '/test/repo';
  assert.equal(normalizeLockPath('src/models/user.ts', cwd), 'src/models/user.ts');
  assert.equal(normalizeLockPath('./src/models/user.ts', cwd), 'src/models/user.ts');
  assert.equal(normalizeLockPath('src\\models\\user.ts', cwd), 'src/models/user.ts');
});

test('LockManager: acquireAtomicSequence locks all files when free', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-lock-test-'));
  const lockManager = new LockManager({ cwd: tmpDir });

  try {
    const files = ['src/models/user.ts', 'src/routes/auth.ts', 'test/auth.test.ts'];
    const res = lockManager.acquireAtomicSequence('Yug', 'task-1', files);

    assert.equal(res.success, true);
    assert.equal(res.locks.length, 3);
    assert.equal(lockManager.isLocked('src/models/user.ts').ownerId, 'Yug');
    assert.equal(lockManager.isLocked('src/routes/auth.ts').ownerId, 'Yug');
    assert.equal(lockManager.isLocked('test/auth.test.ts').ownerId, 'Yug');

    // Verify persisted to .turf/locks.json
    const locksFile = path.join(tmpDir, '.turf', 'locks.json');
    assert.ok(fs.existsSync(locksFile), 'locks.json must be written');
    const diskContent = JSON.parse(fs.readFileSync(locksFile, 'utf8'));
    assert.equal(diskContent.locks.length, 3);
  } finally {
    lockManager.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('LockManager: fail-fast conflict rejection without partial lock acquisition [MD-023], [MD-040]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-lock-test-'));
  const lockManager = new LockManager({ cwd: tmpDir });

  try {
    // 1. Task 1 locks file B
    const lock1 = lockManager.acquireAtomicSequence('Yug', 'task-1', ['src/routes/auth.ts']);
    assert.equal(lock1.success, true);

    // 2. Task 2 declares sequence [file A, file B, file C]
    // Since file B is held by Task 1, Task 2 must FAIL FAST immediately
    const lock2 = lockManager.acquireAtomicSequence('Ayush', 'task-2', [
      'src/models/user.ts',
      'src/routes/auth.ts',
      'test/auth.test.ts'
    ]);

    assert.equal(lock2.success, false);
    assert.equal(lock2.conflictFile, 'src/routes/auth.ts');
    assert.equal(lock2.heldBy, 'Yug');
    assert.equal(lock2.taskId, 'task-1');

    // 3. CRITICAL: Verify NO PARTIAL LOCKS were acquired for file A or file C!
    assert.equal(lockManager.isLocked('src/models/user.ts'), null, 'File A must NOT be partially locked');
    assert.equal(lockManager.isLocked('test/auth.test.ts'), null, 'File C must NOT be partially locked');
  } finally {
    lockManager.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('LockManager: releaseLocks dissolves all locks for taskId', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-lock-test-'));
  const lockManager = new LockManager({ cwd: tmpDir });

  try {
    lockManager.acquireAtomicSequence('Yug', 'task-1', ['file1.js', 'file2.js']);
    lockManager.acquireAtomicSequence('Ayush', 'task-2', ['file3.js']);

    assert.equal(lockManager.getLocks().length, 3);

    const relRes = lockManager.releaseLocks('task-1');
    assert.equal(relRes.success, true);
    assert.equal(relRes.released.length, 2);

    assert.equal(lockManager.isLocked('file1.js'), null);
    assert.equal(lockManager.isLocked('file2.js'), null);
    assert.ok(lockManager.isLocked('file3.js'), 'file3.js should still be held by task-2');
  } finally {
    lockManager.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('LockManager: heartbeat and TTL auto-dissolution [MD-007]', async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-lock-test-'));
  // Use a fast TTL (50ms) for testing
  const lockManager = new LockManager({ cwd: tmpDir, heartbeatTtlMs: 50 });

  try {
    lockManager.acquireAtomicSequence('Yug', 'task-1', ['stale-file.js']);
    assert.ok(lockManager.isLocked('stale-file.js'), 'File should initially be locked');

    // Heartbeat keeps it alive
    await new Promise((r) => setTimeout(r, 25));
    const renewed = lockManager.heartbeat('task-1');
    assert.equal(renewed, true, 'Heartbeat must successfully renew lease');

    // Wait past TTL without heartbeat
    await new Promise((r) => setTimeout(r, 60));
    assert.equal(lockManager.isLocked('stale-file.js'), null, 'Stale lock must auto-dissolve after TTL');
  } finally {
    lockManager.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('LockManager: auto-dissolves when bound process terminates [MD-007]', async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-lock-test-'));
  const lockManager = new LockManager({ cwd: tmpDir });

  try {
    const child = spawn('node', ['-e', 'setInterval(()=>{}, 1000)']);
    assert.equal(isPidAlive(child.pid), true, 'Spawned process should be alive');

    lockManager.acquireAtomicSequence('Yug', 'task-1', ['proc-file.js'], { pid: child.pid });
    assert.ok(lockManager.isLocked('proc-file.js'), 'File should be locked while process is alive');

    // Terminate process
    child.kill('SIGKILL');
    await new Promise((r) => setTimeout(r, 50));
    assert.equal(isPidAlive(child.pid), false, 'Killed process should no longer be alive');

    // cleanExpiredLocks should immediately dissolve since the process is dead
    const expired = lockManager.cleanExpiredLocks();
    assert.equal(expired.length, 1);
    assert.equal(expired[0].reason, 'process_terminated');
    assert.equal(lockManager.isLocked('proc-file.js'), null);
  } finally {
    lockManager.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('LockManager: forceUnlock manually breaks stuck lock [MD-007]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-lock-test-'));
  const lockManager = new LockManager({ cwd: tmpDir });

  try {
    lockManager.acquireAtomicSequence('Ayush', 'task-99', ['stuck.js']);
    assert.ok(lockManager.isLocked('stuck.js'));

    const forceRes = lockManager.forceUnlock('stuck.js');
    assert.equal(forceRes.success, true);
    assert.equal(forceRes.released, 'stuck.js');
    assert.equal(forceRes.heldBy, 'Ayush');
    assert.equal(lockManager.isLocked('stuck.js'), null);

    const reUnlock = lockManager.forceUnlock('stuck.js');
    assert.equal(reUnlock.success, false);
    assert.equal(reUnlock.reason, 'not_locked');
  } finally {
    lockManager.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
