import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import path from 'path';
import os from 'os';
import { createCockpit } from '../src/cockpit.js';

test('Cockpit boots in non-interactive environment with 2-section layout', () => {
  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    screenOptions: { dump: false }
  });

  try {
    assert.ok(cockpit.screen, 'Screen must be created');
    assert.equal(cockpit.sidebar.position.width, '30%', 'Sidebar width must be 30% [MD-001]');
    assert.equal(cockpit.terminalPane.position.width, '70%', 'Terminal pane width must be 70% [MD-001]');
    assert.equal(cockpit.isSidebarCollapsed, false, 'Sidebar starts expanded');
    assert.equal(cockpit.tabs.length, 1, 'Must initialize default shell tab without mock tabs');
  } finally {
    cockpit.destroy();
  }
});

test('Cockpit sidebar toggle (Ctrl+B) collapses to 4 cols and expands to 30%', () => {
  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    screenOptions: { dump: false }
  });

  try {
    assert.equal(cockpit.isSidebarCollapsed, false);

    // Collapse
    cockpit.toggleSidebar();
    assert.equal(cockpit.isSidebarCollapsed, true);
    assert.equal(cockpit.sidebar.width, 4, 'Collapsed sidebar should be 4 columns');
    assert.equal(cockpit.terminalPane.left, 4, 'Terminal pane shifts to left 4');
    assert.equal(cockpit.terminalPane.position.width, '100%-4', 'Terminal pane expands to 100%-4');

    // Expand back
    cockpit.toggleSidebar();
    assert.equal(cockpit.isSidebarCollapsed, false);
    assert.equal(cockpit.sidebar.position.width, '30%', 'Sidebar returns to 30%');
    assert.equal(cockpit.terminalPane.position.left, '30%', 'Terminal pane returns to left 30%');
    assert.equal(cockpit.terminalPane.position.width, '70%', 'Terminal pane returns to 70%');
  } finally {
    cockpit.destroy();
  }
});

test('Cockpit Tab focus cycle navigates between Intent Board, Terminal, and Input Box', () => {
  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    screenOptions: { dump: false }
  });

  try {
    // Initial focus index is 2 (input box)
    assert.equal(cockpit.focusIndex, 2);

    // Cycle focus: 2 -> 0 (Sidebar / Intent Board)
    cockpit.cycleFocus();
    assert.equal(cockpit.focusIndex, 0);

    // Cycle focus: 0 -> 1 (Terminal Log)
    cockpit.cycleFocus();
    assert.equal(cockpit.focusIndex, 1);

    // Cycle focus: 1 -> 2 (Input Box)
    cockpit.cycleFocus();
    assert.equal(cockpit.focusIndex, 2);
  } finally {
    cockpit.destroy();
  }
});

test('Cockpit switches tabs (Ctrl+1..9) cleanly', () => {
  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    screenOptions: { dump: false }
  });

  try {
    assert.equal(cockpit.activeTabIndex, 0);
    assert.equal(cockpit.tabs[0].log.hidden, false);

    // Add dynamic agent tabs
    cockpit.addAgentTab({ name: '2: agy (Host)' });
    cockpit.addAgentTab({ name: '3: agy (Peer)' });

    assert.equal(cockpit.tabs[1].log.hidden, true);
    assert.equal(cockpit.tabs[2].log.hidden, true);

    // Switch to tab 1 (Ctrl+2)
    cockpit.switchTab(1);
    assert.equal(cockpit.activeTabIndex, 1);
    assert.equal(cockpit.tabs[0].log.hidden, true);
    assert.equal(cockpit.tabs[1].log.hidden, false);

    // Switch to tab 2 (Ctrl+3)
    cockpit.switchTab(2);
    assert.equal(cockpit.activeTabIndex, 2);
    assert.equal(cockpit.tabs[1].log.hidden, true);
    assert.equal(cockpit.tabs[2].log.hidden, false);
  } finally {
    cockpit.destroy();
  }
});

test('spawnTerminalProcess pipes stdout directly to terminal log without screen.spawn', async () => {
  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    screenOptions: { dump: false }
  });

  try {
    let screenSpawnCalled = false;
    cockpit.screen.spawn = () => {
      screenSpawnCalled = true;
    };

    const activeTab = cockpit.tabs[cockpit.activeTabIndex];
    const logSpy = [];
    const origLog = activeTab.log.log.bind(activeTab.log);
    activeTab.log.log = (msg) => {
      logSpy.push(msg);
      origLog(msg);
    };

    const proc = cockpit.spawnTerminalProcess('node', ['-e', 'console.log("TEST_PIPE_OUTPUT")']);
    assert.ok(proc, 'Process must be returned');
    assert.equal(screenSpawnCalled, false, 'screen.spawn() must NEVER be called');

    await new Promise((resolve) => {
      proc.on('close', resolve);
    });

    assert.equal(screenSpawnCalled, false, 'screen.spawn() must NEVER be called');
    const found = logSpy.some((line) => line && line.includes('TEST_PIPE_OUTPUT'));
    assert.ok(found, `Output must appear in terminal log: ${JSON.stringify(logSpy)}`);
  } finally {
    cockpit.destroy();
  }
});

test('Cockpit integrates IntentBoard and LockManager with live rendering in sidebar', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-cockpit-test-'));
  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    cwd: tmpDir,
    screenOptions: { dump: false }
  });

  try {
    assert.ok(cockpit.intentBoard, 'IntentBoard must be mounted');
    assert.ok(cockpit.lockManager, 'LockManager must be mounted');

    // Acquire lock sequence
    cockpit.acquireLock('Host', '1', ['src/server.js', 'src/router.js']);
    const locksContent = cockpit.locksBox.getContent();
    assert.ok(locksContent.includes('src/server.js'), 'Active lock must render in locksBox');
    assert.ok(locksContent.includes('Host'), 'Holder must render in locksBox');

    // Add a task
    cockpit.addTask({
      id: '99',
      title: 'Build Atomic Lock Engine',
      assignee: 'Host',
      status: 'in_progress',
      targetFiles: ['src/server.js']
    });

    const tasksContent = cockpit.tasksBox.getContent();
    assert.ok(tasksContent.includes('Build Atomic Lock Engine'), 'Task must render in tasksBox');
    assert.ok(tasksContent.includes('[IN PROGRESS]'), 'Status badge must render');
    assert.ok(tasksContent.includes('[LOCKED BY HOST]'), 'Active lock indicator must render on task card');

    // Release lock
    cockpit.releaseLocks('1');
    const updatedLocks = cockpit.locksBox.getContent();
    assert.ok(updatedLocks.includes('(No active file locks - workspace free)'));
  } finally {
    cockpit.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('Cockpit toggles File Tree in Intent Board via [F] / toggleFileTree() [MD-027]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-cockpit-tree-'));
  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    cwd: tmpDir,
    screenOptions: { dump: false }
  });

  try {
    assert.equal(cockpit.isFileTreeVisible, false, 'File tree starts hidden');
    assert.equal(cockpit.fileTreeBox.hidden, true);
    assert.equal(cockpit.tasksBox.hidden, false);

    // Toggle on (press [F])
    cockpit.toggleFileTree();
    assert.equal(cockpit.isFileTreeVisible, true, 'File tree becomes visible');
    assert.equal(cockpit.fileTreeBox.hidden, false);
    assert.equal(cockpit.tasksBox.hidden, true);

    // Toggle off (press [F] again)
    cockpit.toggleFileTree();
    assert.equal(cockpit.isFileTreeVisible, false, 'File tree hides');
    assert.equal(cockpit.fileTreeBox.hidden, true);
    assert.equal(cockpit.tasksBox.hidden, false);
  } finally {
    cockpit.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('Cockpit opens dedicated DiffViewer tab via [D] / openDiffTab() [MD-016]', async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-cockpit-diff-'));
  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    cwd: tmpDir,
    screenOptions: { dump: false }
  });

  try {
    const initialTabsCount = cockpit.tabs.length;
    const diffTab = await cockpit.openDiffTab();

    assert.ok(diffTab, 'Diff tab must be created and returned');
    assert.equal(diffTab.id, 'diff-viewer');
    assert.equal(cockpit.activeTabIndex, cockpit.tabs.indexOf(diffTab));
    assert.equal(cockpit.tabs.length, initialTabsCount + 1);

    // Re-opening diff tab reuses existing tab and refreshes
    const secondCall = await cockpit.openDiffTab();
    assert.equal(secondCall, diffTab);
    assert.equal(cockpit.tabs.length, initialTabsCount + 1);
  } finally {
    cockpit.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('Cockpit triggers audio bell cues on agent completion and lock conflicts [MD-034]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-cockpit-bell-'));
  let bellEvents = [];
  const mockStream = {
    write: (data) => {
      bellEvents.push(data);
    }
  };

  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    cwd: tmpDir,
    screenOptions: { dump: false },
    silent: false
  });
  cockpit.bell.outputStream = mockStream;

  try {
    // 1. Lock conflict triggers bell
    cockpit.acquireLock('Yug', 'task-1', ['file1.js']);
    // Attempt conflicting lock
    cockpit.acquireLock('Ayush', 'task-2', ['file1.js']);

    assert.ok(bellEvents.length >= 1, 'Lock conflict must ring bell');
    assert.equal(bellEvents[0], '\x07');

    // 2. Task completion via intentBoard / agentRunner triggers bell
    cockpit.bell.clearHistory();
    bellEvents = [];

    cockpit.agentRunner.emit('exit', { taskId: 'task-1', code: 0 });
    assert.ok(bellEvents.length >= 1, 'Agent success exit must ring bell');
    assert.equal(bellEvents[0], '\x07');
  } finally {
    cockpit.destroy();
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('Cockpit disables allMotion and rejects stray mouse escape artifacts [MD-043]', () => {
  const cockpit = createCockpit({
    roomCode: 'TURF-TEST',
    screenOptions: { dump: false }
  });

  try {
    assert.equal(cockpit.inputBox.options.mouse, false, 'inputBox must have mouse: false to prevent listener stacking');

    // Simulate stray mouse keypress event
    cockpit.inputBox.clearValue();
    cockpit.inputBox._listener('C', { name: 'mouse', type: 'X10' });
    assert.equal(cockpit.inputBox.value, '', 'Mouse event must not leak into inputBox');

    cockpit.inputBox._listener('\x1b[MCF,', {});
    assert.equal(cockpit.inputBox.value, '', 'ANSI escape sequence must not leak into inputBox');

    // UTF/X10 mouse coordinates (>127 Latin1 range: ¤, ¥, ¦, etc.) [MD-044]
    cockpit.inputBox._listener('¤', {});
    cockpit.inputBox._listener('¥', {});
    assert.equal(cockpit.inputBox.value, '', 'UTF mouse coordinates (¤, ¥) must not leak into inputBox');

    // Legitimate keypress works
    cockpit.inputBox._listener('h', { name: 'h' });
    cockpit.inputBox._listener('i', { name: 'i' });
    assert.equal(cockpit.inputBox.value, 'hi', 'Legitimate character input must work');
  } finally {
    cockpit.destroy();
  }
});
