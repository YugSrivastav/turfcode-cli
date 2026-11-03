import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import path from 'path';
import os from 'os';
import { IntentBoard, normalizeTaskStatus, formatStatusLabel } from '../src/intent-board.js';

test('normalizeTaskStatus and formatStatusLabel handle various status strings [MD-037]', () => {
  assert.equal(normalizeTaskStatus('queued'), 'queued');
  assert.equal(normalizeTaskStatus('QUEUED'), 'queued');
  assert.equal(normalizeTaskStatus('in_progress'), 'in_progress');
  assert.equal(normalizeTaskStatus('in-progress'), 'in_progress');
  assert.equal(normalizeTaskStatus('in progress'), 'in_progress');
  assert.equal(normalizeTaskStatus('done'), 'done');
  assert.equal(normalizeTaskStatus('COMPLETED'), 'done');
  assert.equal(normalizeTaskStatus('invalid'), 'queued');

  assert.equal(formatStatusLabel('queued'), 'QUEUED');
  assert.equal(formatStatusLabel('in_progress'), 'IN PROGRESS');
  assert.equal(formatStatusLabel('done'), 'DONE');
});

test('IntentBoard: addTask, updateTask, removeTask with atomic persistence [MD-014], [MD-037]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-intent-test-'));
  const intentPath = path.join(tmpDir, '.turf', 'intent.json');
  const board = new IntentBoard({ cwd: tmpDir, intentPath });

  try {
    // 1. Add Task
    const task1 = board.addTask({
      id: '1',
      title: 'Implement Lock Manager',
      assignee: 'Yug',
      status: 'in_progress',
      targetFiles: ['src/lock-manager.js', 'test/lock-manager.test.js']
    });

    assert.equal(task1.id, '1');
    assert.equal(task1.title, 'Implement Lock Manager');
    assert.equal(task1.assignee, 'Yug');
    assert.equal(task1.status, 'in_progress');
    assert.deepEqual(task1.targetFiles, ['src/lock-manager.js', 'test/lock-manager.test.js']);

    // Verify written to disk
    assert.ok(fs.existsSync(intentPath), '.turf/intent.json must exist');
    const diskJson1 = JSON.parse(fs.readFileSync(intentPath, 'utf8'));
    assert.equal(diskJson1.tasks.length, 1);
    assert.equal(diskJson1.tasks[0].title, 'Implement Lock Manager');

    // 2. Add Second Task
    board.addTask({
      id: '2',
      title: 'Peer UI Sync',
      assignee: 'Ayush',
      status: 'queued'
    });
    assert.equal(board.getTasks().length, 2);

    // 3. Update Task
    const updated = board.updateTask('1', { status: 'done' });
    assert.equal(updated.status, 'done');
    assert.equal(board.getTask('1').status, 'done');

    // 4. Remove Task
    const removed = board.removeTask('2');
    assert.equal(removed.id, '2');
    assert.equal(board.getTasks().length, 1);

    // 5. Reload new board instance from disk to verify persistence
    const board2 = new IntentBoard({ cwd: tmpDir, intentPath });
    assert.equal(board2.getTasks().length, 1);
    assert.equal(board2.getTask('1').title, 'Implement Lock Manager');
    assert.equal(board2.getTask('1').status, 'done');
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('IntentBoard: formatTaskCard renders badges and active lock indicators', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-intent-test-'));
  const board = new IntentBoard({ cwd: tmpDir });

  try {
    const task = {
      id: '42',
      title: 'Multiplayer Session Pairing',
      assignee: 'Ayush',
      status: 'in_progress',
      targetFiles: ['src/session.js', 'src/tunnel.js']
    };

    const activeLocks = [
      {
        filePath: 'src/session.js',
        ownerId: 'Ayush',
        taskId: '42'
      }
    ];

    const card = board.formatTaskCard(task, activeLocks);
    assert.ok(card.includes('[IN PROGRESS]'), 'Card must include status badge');
    assert.ok(card.includes('#42: Multiplayer Session Pairing'), 'Card must include task id and title');
    assert.ok(card.includes('↳ Assignee: Ayush'), 'Card must include assignee');
    assert.ok(card.includes('[LOCKED BY AYUSH]'), 'Card must render active lock indicator');
    assert.ok(card.includes('src/session.js'), 'Card must list locked file');
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
