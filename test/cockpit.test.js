import test from 'node:test';
import assert from 'node:assert/strict';
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
    assert.equal(cockpit.tabs.length, 3, 'Must initialize default tabs');
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
    assert.equal(cockpit.tabs[1].log.hidden, true);

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
