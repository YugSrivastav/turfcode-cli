/**
 * TurfCode CLI - Electric Lime Cockpit
 * 
 * 2-section Blessed TUI cockpit layout:
 * - 30% Intent Board (Tasks, File Locks, Active Peers, Collapsible)
 * - 70% Terminal & Agent Pane (Multi-tab, Output Log, Subprocess Streaming, Input Box)
 * 
 * Complies with [MD-001], [MD-004], [MD-028], [MD-032], [MD-038].
 */

import blessed from 'blessed';
import { spawn } from 'child_process';
import path from 'path';
import { PALETTE, TAGS, BLESSED_STYLES } from './theme.js';
import { LockManager } from './lock-manager.js';
import { IntentBoard } from './intent-board.js';
import { AgentRunner } from './agent-runner.js';
import { DiffViewer } from './diff-viewer.js';
import { FileTree } from './file-tree.js';
import { AudioBell } from './bell.js';
import { selfHeal, recordSessionStart, recordSessionExit } from './self-heal.js';

export function createCockpit(options = {}) {
  const {
    mode = 'host',
    roomCode = 'TURF-DEMO',
    cwd = process.cwd(),
    userName = mode === 'host' ? 'Host' : 'Peer',
    role = mode === 'host' ? 'Host' : 'Peer',
    initialTasks = [],
    initialLocks = [],
    initialPeers = [],
    screenOptions = {},
    silent = false,
    lockManager: customLockManager,
    intentBoard: customIntentBoard,
    agentRunner: customAgentRunner,
    tunnelServer = null,
    tunnelUrl = null,
    client = null,
    bell: customBell,
    diffViewer: customDiffViewer,
    fileTree: customFileTree
  } = options;

  let currentDir = cwd;
  let isSidebarCollapsed = false;
  let isFileTreeVisible = false;
  let focusIndex = 2; // 0: Sidebar, 1: Terminal Log, 2: Input Box (default focused)
  let activeTabIndex = 0;

  // Boot Self-Healing [MD-036]
  const recoveryResult = selfHeal({
    cwd: currentDir,
    silent: silent,
    lockManager: customLockManager
  });
  recordSessionStart({ cwd: currentDir, roomCode });

  // Initialize Audio Bell [MD-034]
  const bell = customBell || new AudioBell({ silent });

  // Initialize Lock Manager & Intent Board
  const lockManager = customLockManager || new LockManager({
    cwd: currentDir,
    initialLocks: initialLocks.length > 0 ? initialLocks : undefined,
    autoStartMonitor: true
  });

  // Listen for lock conflicts on LockManager and trigger bell [MD-034]
  lockManager.on('conflict', (data) => {
    bell.onLockConflict(data);
  });

  // Initialize File Tree & Diff Viewer [MD-016], [MD-027]
  const fileTree = customFileTree || new FileTree({
    cwd: currentDir,
    lockManager,
    format: 'tags'
  });

  const diffViewer = customDiffViewer || new DiffViewer({
    cwd: currentDir,
    format: 'tags'
  });

  const defaultInitialTasks = initialTasks.length > 0 ? initialTasks : [];

  const intentBoard = customIntentBoard || new IntentBoard({
    cwd: currentDir,
    lockManager,
    initialTasks: defaultInitialTasks
  });

  const agentRunner = customAgentRunner || new AgentRunner({
    defaultCwd: currentDir,
    intentBoard
  });

  const peers = initialPeers.length > 0 ? [...initialPeers] : [
    { name: userName, role: role, isSelf: true }
  ];

  const displayDir = path.basename(currentDir) || currentDir;

  // 1. Create Blessed Screen
  const screen = blessed.screen({
    smartCSR: true,
    title: `TurfCode Cockpit [${roomCode}]`,
    dockBorders: true,
    useBCE: true,
    fullUnicode: true,
    style: {
      bg: PALETTE.bg,
      fg: PALETTE.textBright
    },
    ...screenOptions
  });

  // 2. Top Header Bar (Herdr-style unified status strip)
  const header = blessed.box({
    parent: screen,
    ...BLESSED_STYLES.header,
    content: ` {bold}{#39ff14-fg}TURFCODE{/#39ff14-fg}{/bold}  {#eab308-fg}${roomCode}{/#eab308-fg}  {#4a7c4a-fg}•{/#4a7c4a-fg}  {#f0fff0-fg}${mode.toUpperCase()}{/#f0fff0-fg}  {#4a7c4a-fg}•{/#4a7c4a-fg}  {#06b6d4-fg}${userName}{/#06b6d4-fg}  {#4a7c4a-fg}•{/#4a7c4a-fg}  {#4a7c4a-fg}${displayDir}{/#4a7c4a-fg}`
  });

  // 3. Left Sidebar (Herdr-style unified column)
  const sidebar = blessed.box({
    parent: screen,
    top: 1,
    left: 0,
    width: '30%',
    height: '100%-2',
    border: { type: 'line', fg: PALETTE.mutedBorder },
    style: {
      bg: PALETTE.bg,
      border: { fg: PALETTE.mutedBorder }
    }
  });

  // 3-spaces. Spaces Container (Workspace/repo overview, Herdr-style)
  const spacesBox = blessed.box({
    parent: sidebar,
    top: 0,
    left: 1,
    width: '100%-2',
    height: 3,
    tags: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg
    }
  });

  function renderSpaces() {
    spacesBox.setContent(`{bold}{#4a7c4a-fg}SPACES{/#4a7c4a-fg}{/bold}\n {#39ff14-fg}○{/#39ff14-fg} {#f0fff0-fg}${displayDir}{/#f0fff0-fg}\n   {#4a7c4a-fg}main  clean{/#4a7c4a-fg}`);
  }
  renderSpaces();

  // 3a. Tasks Container (MD-037 4-field schema, Herdr-style borderless section)
  const tasksBox = blessed.box({
    parent: sidebar,
    top: 4,
    left: 1,
    width: '100%-2',
    height: '42%',
    tags: true,
    scrollable: true,
    alwaysScroll: true,
    mouse: true,
    keys: true,
    vi: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg
    }
  });

  function renderTasks() {
    const currentTasks = intentBoard.getTasks();
    const currentLocks = lockManager.getLocks();
    const headerLine = `{bold}{#4a7c4a-fg}INTENTS & TASKS{/#4a7c4a-fg}{/bold}`;
    if (!currentTasks || currentTasks.length === 0) {
      tasksBox.setContent(`${headerLine}\n ${TAGS.dim('(No tasks in intent queue)')}`);
      return;
    }
    const lines = currentTasks.map(t => intentBoard.formatTaskCard(t, currentLocks));
    tasksBox.setContent(`${headerLine}\n${lines.join('\n\n')}`);
  }

  // 3b. File Lock Table (MD-002, MD-040, Herdr-style borderless section)
  const locksBox = blessed.box({
    parent: sidebar,
    top: '48%',
    left: 1,
    width: '100%-2',
    height: '24%',
    tags: true,
    scrollable: true,
    mouse: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg
    }
  });

  function renderLocks() {
    const currentLocks = lockManager.getLocks();
    const headerLine = `{bold}{#4a7c4a-fg}ACTIVE FILE LOCKS{/#4a7c4a-fg}{/bold}`;
    if (!currentLocks || currentLocks.length === 0) {
      locksBox.setContent(`${headerLine}\n ${TAGS.dim('(No active file locks - workspace free)')}`);
      return;
    }
    const lines = currentLocks.map(l => {
      const holder = l.ownerId || l.holder || 'unknown';
      const fPath = l.filePath || l.file;
      return ` {#ef4444-fg}🔒 ${fPath}{/#ef4444-fg} ${TAGS.dim(`[Held by ${holder}]`)}`;
    });
    locksBox.setContent(`${headerLine}\n${lines.join('\n')}`);
  }

  // 3b-alt. Interactive File Tree with Padlock Badges [MD-027]
  const fileTreeBox = blessed.box({
    parent: sidebar,
    top: 0,
    left: 1,
    width: '100%-2',
    height: '100%-2',
    tags: true,
    scrollable: true,
    alwaysScroll: true,
    mouse: true,
    keys: true,
    vi: true,
    hidden: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg
    }
  });

  function renderFileTree() {
    fileTree.renderToBox(fileTreeBox);
  }

  function toggleFileTree() {
    isFileTreeVisible = !isFileTreeVisible;
    if (isFileTreeVisible) {
      spacesBox.hide();
      tasksBox.hide();
      locksBox.hide();
      peersBox.hide();
      fileTreeBox.show();
      renderFileTree();
      if (focusIndex === 0) {
        fileTreeBox.focus();
      }
    } else {
      fileTreeBox.hide();
      spacesBox.show();
      tasksBox.show();
      locksBox.show();
      peersBox.show();
      renderTasks();
      renderLocks();
      if (focusIndex === 0) {
        tasksBox.focus();
      }
    }
    screen.render();
  }

  // Live synchronizer between state and TUI
  intentBoard.on('change', () => {
    if (isFileTreeVisible) {
      renderFileTree();
    } else {
      renderTasks();
      renderLocks();
    }
    screen.render();
  });

  lockManager.on('change', () => {
    if (isFileTreeVisible) {
      renderFileTree();
    } else {
      renderLocks();
      renderTasks();
    }
    screen.render();
  });

  // 3c. Connected Peers Container (Herdr-style borderless section)
  const peersBox = blessed.box({
    parent: sidebar,
    top: '74%',
    left: 1,
    width: '100%-2',
    height: '100%-74%-2',
    tags: true,
    scrollable: true,
    mouse: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg
    }
  });

  function renderPeers() {
    const headerLine = `{bold}{#4a7c4a-fg}CONNECTED PEERS{/#4a7c4a-fg}{/bold}`;
    if (!peers || peers.length === 0) {
      peersBox.setContent(`${headerLine}\n ${TAGS.dim('(Solo session)')}`);
      return;
    }
    const lines = peers.map(p => {
      const dot = TAGS.lime('●');
      const selfLabel = p.isSelf ? ` ${TAGS.limeBold('(You)')}` : '';
      return ` ${dot} {#f0fff0-fg}${p.name}{/#f0fff0-fg}${selfLabel} ${TAGS.cyan(`[${p.role}]`)}`;
    });
    peersBox.setContent(`${headerLine}\n${lines.join('\n')}`);
  }

  // Wire TunnelServer events (Host mode)
  if (tunnelServer) {
    tunnelServer.on('peer-joined', ({ peerId, username: pName, role: pRole }) => {
      if (!peers.some(p => p.id === peerId)) {
        peers.push({ id: peerId, name: pName, role: pRole || 'Peer', isSelf: false });
        renderPeers();
        screen.render();
      }
    });

    tunnelServer.on('peer-left', ({ peerId }) => {
      const idx = peers.findIndex(p => p.id === peerId);
      if (idx !== -1) {
        peers.splice(idx, 1);
        renderPeers();
        screen.render();
      }
    });
  }

  // Wire TurfClient events (Peer mode)
  if (client) {
    client.on('tasks-updated', (syncedTasks) => {
      if (intentBoard) {
        intentBoard.tasks = syncedTasks;
      }
      renderTasks();
      screen.render();
    });

    client.on('locks-updated', (syncedLocks) => {
      if (lockManager) {
        lockManager.locks = syncedLocks;
      }
      renderLocks();
      screen.render();
    });

    client.on('peers-updated', (syncedPeers) => {
      peers.length = 0;
      peers.push({ name: userName, role: role, isSelf: true });
      for (const p of syncedPeers) {
        if (p.username !== userName && !peers.some(existing => existing.id === p.id)) {
          peers.push({ id: p.id, name: p.username, role: p.role || 'Peer', isSelf: false });
        }
      }
      renderPeers();
      screen.render();
    });
  }

  // 3d. Sidebar Collapse Button (Minimal bottom toggle)
  const collapseBtn = blessed.button({
    parent: sidebar,
    bottom: 0,
    left: 1,
    width: '100%-2',
    height: 1,
    content: `{#4a7c4a-fg}« Collapse Sidebar [Ctrl+B]{/#4a7c4a-fg}`,
    tags: true,
    mouse: true,
    style: {
      fg: PALETTE.textMuted,
      bg: PALETTE.bg,
      hover: { fg: PALETTE.electricLime }
    }
  });

  // 4. Right Container (70% Terminal & Agent Pane)
  const terminalPane = blessed.box({
    parent: screen,
    top: 1,
    left: '30%',
    width: '70%',
    height: '100%-2',
    border: { type: 'line', fg: PALETTE.mutedBorder },
    tags: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg,
      border: { fg: PALETTE.mutedBorder }
    }
  });

  // 4a. Tab Bar (Herdr-style pill tab row)
  const tabBar = blessed.box({
    parent: terminalPane,
    top: 0,
    left: 0,
    width: '100%-2',
    height: 1,
    tags: true,
    mouse: true,
    style: {
      fg: PALETTE.textMuted,
      bg: PALETTE.bg
    }
  });

  // Define Tabs: 1: Shell (mock tabs 2 & 3 removed)
  const tabs = [
    {
      id: 'shell',
      name: '1: Shell',
      log: null,
      activeProc: null,
      history: []
    }
  ];

  // Create Log widgets for each Tab inside terminalPane
  tabs.forEach((tab, index) => {
    tab.log = blessed.log({
      parent: terminalPane,
      top: 1,
      left: 0,
      width: '100%-2',
      height: '100%-3',
      scrollable: true,
      alwaysScroll: true,
      tags: true,
      keys: true,
      vi: true,
      mouse: true,
      hidden: index !== 0,
      style: {
        fg: PALETTE.textBright,
        bg: PALETTE.bg
      }
    });
  });

  function renderTabs() {
    const tabParts = tabs.map((t, idx) => {
      const active = idx === activeTabIndex;
      if (active) {
        return `{bold}{#39ff14-bg}{#081008-fg} ${t.name} {/#081008-fg}{/#39ff14-bg}{/bold}`;
      } else {
        return `{#4a7c4a-fg} ${t.name} {/#4a7c4a-fg}`;
      }
    });
    tabParts.push(`{#2a5a2a-fg} + {/#2a5a2a-fg}`);
    tabBar.setContent(` ${tabParts.join(' ')}`);
  }

  function printWelcomeBanner(log) {
    log.log(`{bold}{#39ff14-fg}⚡ TURFCODE ELECTRIC LIME COCKPIT v0.1.0{/#39ff14-fg}{/bold} — {#22c55e-fg}READY{/#22c55e-fg}`);
    log.log(`{#4a7c4a-fg}Session:{/#4a7c4a-fg} {#eab308-fg}${roomCode}{/#eab308-fg}  {#4a7c4a-fg}• Mode:{/#4a7c4a-fg} {#39ff14-fg}${mode.toUpperCase()}{/#39ff14-fg}  {#4a7c4a-fg}• Workspace:{/#4a7c4a-fg} {#f0fff0-fg}${displayDir}{/#f0fff0-fg}`);
    log.log(`{#4a7c4a-fg}Multi-Agent Terminal: Parallel execution without screen wipe{/#4a7c4a-fg}`);
    log.log(`{#4a7c4a-fg}Shortcuts: [Tab] Focus • [D] Diff • [F] Files • [Ctrl+B] Sidebar{/#4a7c4a-fg}\n`);
    if (recoveryResult && recoveryResult.recovered) {
      log.log(`${TAGS.limeBold('[SELF-HEALING]')} ${TAGS.yellow(recoveryResult.notice)}\n`);
    }
  }

  let diffTab = null;

  /**
   * Open or switch to dedicated Diff Viewer tab in the right pane [MD-016]
   * @returns {Promise<Object>} Diff tab object
   */
  async function openDiffTab() {
    let existingIndex = tabs.findIndex(t => t.id === 'diff-viewer');
    if (existingIndex === -1) {
      const log = blessed.log({
        parent: terminalPane,
        top: 1,
        left: 0,
        width: '100%-2',
        height: '100%-3',
        scrollable: true,
        alwaysScroll: true,
        tags: true,
        keys: true,
        vi: true,
        mouse: true,
        hidden: true,
        style: {
          fg: PALETTE.textBright,
          bg: PALETTE.bg
        }
      });

      diffTab = {
        id: 'diff-viewer',
        name: 'D: Diff',
        log,
        activeProc: null,
        history: []
      };

      tabs.push(diffTab);
      existingIndex = tabs.length - 1;
      renderTabs();
    } else {
      diffTab = tabs[existingIndex];
    }

    switchTab(existingIndex);

    diffTab.log.setContent('');
    diffTab.log.log(`{#06b6d4-fg}=== TURFCODE LIVE GIT DIFF INSPECTION [MD-016] ==={/#06b6d4-fg}`);
    diffTab.log.log(`${TAGS.dim('Additions: {#39ff14-fg}+green{/#39ff14-fg} │ Deletions: {#ff3333-fg}-red{/#ff3333-fg} │ Headers: {#06b6d4-fg}cyan{/#06b6d4-fg}')}\n`);

    try {
      const res = await diffViewer.getUncommittedDiff();
      diffTab.log.log(res.colorized);
    } catch (err) {
      diffTab.log.log(`${TAGS.red(`[DIFF ERROR] ${err.message}`)}`);
    }

    screen.render();
    return diffTab;
  }

  /**
   * Add dynamic tab in the right pane for an active agent (Ctrl+1..9)
   * @param {Object} options
   * @param {string|number} [options.taskId]
   * @param {string} [options.name]
   * @param {string} [options.ownerId]
   * @returns {Object} Tab object
   */
  function addAgentTab(options = {}) {
    const taskId = options.taskId ? String(options.taskId) : null;
    if (taskId) {
      const existing = tabs.find(t => t.taskId === taskId || t.id === `agent-${taskId}`);
      if (existing) return existing;
    }

    if (tabs.length >= 9) {
      return getActiveTab();
    }

    const tabNumber = tabs.length + 1;
    const tabName = options.name || `${tabNumber}: agy (#${taskId || tabNumber})`;
    const log = blessed.log({
      parent: terminalPane,
      top: 1,
      left: 0,
      width: '100%-2',
      height: '100%-3',
      scrollable: true,
      alwaysScroll: true,
      tags: true,
      keys: true,
      vi: true,
      mouse: true,
      hidden: true,
      style: {
        fg: PALETTE.textBright,
        bg: PALETTE.bg
      }
    });

    const newTab = {
      id: taskId ? `agent-${taskId}` : `agent-${tabNumber}`,
      taskId,
      ownerId: options.ownerId || 'unknown',
      name: tabName,
      log,
      activeProc: null,
      history: []
    };

    tabs.push(newTab);
    printWelcomeBanner(log);
    renderTabs();
    screen.render();
    return newTab;
  }

  // Connect AgentRunner events (output, exit, error, spawn) to agent tab / active tab log box
  agentRunner.on('spawn', (agent) => {
    if (agent && agent.taskId) {
      addAgentTab({
        taskId: agent.taskId,
        ownerId: agent.ownerId,
        name: `${tabs.length + 1}: agy (#${agent.taskId})`
      });
    }
  });

  agentRunner.on('output', (event) => {
    const text = typeof event === 'string' ? event : (event && (event.text || event.delta) ? (event.text || event.delta) : '');
    const taskId = event && event.taskId ? String(event.taskId) : null;
    const targetTab = (taskId && tabs.find(t => t.taskId === taskId || t.id === `agent-${taskId}`)) || getActiveTab();
    if (targetTab && targetTab.log && text) {
      const lines = text.split(/\r?\n/);
      for (const line of lines) {
        if (line.length > 0) {
          targetTab.log.log(line);
        }
      }
      screen.render();
    }
  });

  agentRunner.on('exit', (event) => {
    const { taskId, code, signal } = event || {};
    const tId = taskId ? String(taskId) : null;
    const targetTab = (tId && tabs.find(t => t.taskId === tId || t.id === `agent-${tId}`)) || getActiveTab();
    if (targetTab && targetTab.log) {
      if (code === 0) {
        bell.onTaskCompletion(tId);
        targetTab.log.log(`${TAGS.dim('[AGENT]')} Task #${tId || 'proc'} ${TAGS.lime('completed successfully')}`);
      } else {
        targetTab.log.log(`${TAGS.dim('[AGENT]')} Task #${tId || 'proc'} ${TAGS.red(`exited with code ${code}${signal ? ` (${signal})` : ''}`)}`);
      }
      screen.render();
    }
  });

  agentRunner.on('error', (event) => {
    const { taskId, error } = event || {};
    const tId = taskId ? String(taskId) : null;
    const errMessage = error && error.message ? error.message : (error || 'Unknown error');
    const targetTab = (tId && tabs.find(t => t.taskId === tId || t.id === `agent-${tId}`)) || getActiveTab();
    if (targetTab && targetTab.log) {
      targetTab.log.log(`${TAGS.red(`[AGENT ERROR] Task #${tId || 'proc'}: ${errMessage}`)}`);
      screen.render();
    }
  });

  // 4b. Command / Prompt Input Box (Herdr-style minimal prompt line)
  const inputContainer = blessed.box({
    parent: terminalPane,
    bottom: 0,
    left: 0,
    width: '100%-2',
    height: 1,
    tags: true,
    mouse: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg
    }
  });

  const promptPrefix = blessed.text({
    parent: inputContainer,
    top: 0,
    left: 1,
    tags: true,
    content: `{bold}{#39ff14-fg}turf >{/#39ff14-fg}{/bold} `
  });

  const inputBox = blessed.textbox({
    parent: inputContainer,
    top: 0,
    left: 8,
    width: '100%-10',
    height: 1,
    keys: true,
    mouse: true,
    inputOnFocus: true,
    style: {
      fg: PALETTE.electricLime,
      bg: PALETTE.bg
    }
  });

  // 5. Bottom Footer Bar (Clean, understated keybinding reference)
  const footer = blessed.box({
    parent: screen,
    ...BLESSED_STYLES.footer,
    content: ` {#39ff14-fg}[Tab]{/#39ff14-fg} {#4a7c4a-fg}Focus{/#4a7c4a-fg}   {#39ff14-fg}[D]{/#39ff14-fg} {#4a7c4a-fg}Diff{/#4a7c4a-fg}   {#39ff14-fg}[F]{/#39ff14-fg} {#4a7c4a-fg}Files{/#4a7c4a-fg}   {#39ff14-fg}[Ctrl+B]{/#39ff14-fg} {#4a7c4a-fg}Sidebar{/#4a7c4a-fg}   {#39ff14-fg}[Ctrl+1..9]{/#39ff14-fg} {#4a7c4a-fg}Tabs{/#4a7c4a-fg}`
  });

  // Helper: Get Active Tab & Log
  function getActiveTab() {
    return tabs[activeTabIndex];
  }

  function getActiveLog() {
    return tabs[activeTabIndex].log;
  }

  // Focus Management Function (Tab cycling)
  function setFocus(idx) {
    focusIndex = (idx + 3) % 3;

    sidebar.style.border.fg = PALETTE.mutedBorder;
    terminalPane.style.border.fg = PALETTE.mutedBorder;

    if (focusIndex === 0) {
      sidebar.style.border.fg = PALETTE.activeBorder;
      if (isFileTreeVisible) {
        fileTreeBox.focus();
      } else {
        tasksBox.focus();
      }
    } else if (focusIndex === 1) {
      terminalPane.style.border.fg = PALETTE.activeBorder;
      getActiveLog().focus();
    } else {
      terminalPane.style.border.fg = PALETTE.activeBorder;
      inputBox.focus();
      inputBox.readInput();
    }
    screen.render();
  }

  function cycleFocus() {
    setFocus(focusIndex + 1);
  }

  // Sidebar Collapse / Expand (Ctrl+B)
  function toggleSidebar() {
    isSidebarCollapsed = !isSidebarCollapsed;

    if (isSidebarCollapsed) {
      sidebar.width = 4;
      spacesBox.hide();
      tasksBox.hide();
      locksBox.hide();
      fileTreeBox.hide();
      peersBox.hide();
      collapseBtn.top = 0;
      collapseBtn.height = '100%-2';
      collapseBtn.setContent(' »');
      terminalPane.left = 4;
      terminalPane.width = '100%-4';
    } else {
      sidebar.width = '30%';
      if (isFileTreeVisible) {
        fileTreeBox.show();
        spacesBox.hide();
        tasksBox.hide();
        locksBox.hide();
      } else {
        fileTreeBox.hide();
        spacesBox.show();
        tasksBox.show();
        locksBox.show();
      }
      peersBox.show();
      collapseBtn.top = undefined;
      collapseBtn.bottom = 0;
      collapseBtn.height = 1;
      collapseBtn.setContent(`{#4a7c4a-fg}« Collapse Sidebar [Ctrl+B]{/#4a7c4a-fg}`);
      terminalPane.left = '30%';
      terminalPane.width = '70%';
    }
    screen.render();
  }

  collapseBtn.on('press', toggleSidebar);

  // Tab Switching Function (Ctrl+1 through Ctrl+9)
  function switchTab(index) {
    if (index < 0 || index >= tabs.length) return;
    if (index === activeTabIndex) return;

    tabs[activeTabIndex].log.hide();
    activeTabIndex = index;
    tabs[activeTabIndex].log.show();

    renderTabs();
    if (focusIndex === 1) {
      tabs[activeTabIndex].log.focus();
    }
    screen.render();
  }

  // Subprocess Execution: NEVER call screen.spawn(), pipe stdout/stderr to log box
  function spawnTerminalProcess(command, args = [], spawnOpts = {}) {
    const activeTab = getActiveTab();
    const log = activeTab.log;

    let procCmd = command;
    let procArgs = args;
    const isWin = process.platform === 'win32';

    if (!Array.isArray(args) || args.length === 0) {
      const shellExe = isWin ? 'powershell.exe' : (process.env.SHELL || 'bash');
      procArgs = isWin
        ? ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', command]
        : ['-c', command];
      procCmd = shellExe;
    }

    const env = {
      ...process.env,
      TURF_ROOM: roomCode,
      TURF_USER: userName,
      TURF_ROLE: role,
      ...spawnOpts.env
    };

    const runCwd = spawnOpts.cwd || currentDir;

    log.log(`${TAGS.dim('[EXEC]')} ${TAGS.lime(command)} ${Array.isArray(args) && args.length > 0 ? TAGS.dim(args.join(' ')) : ''}`);
    screen.render();

    let proc;
    try {
      proc = spawn(procCmd, procArgs, {
        cwd: runCwd,
        env,
        detached: !isWin
      });
    } catch (err) {
      log.log(`${TAGS.red(`[SPAWN ERROR] Failed to launch ${command}: ${err.message}`)}`);
      screen.render();
      return null;
    }

    activeTab.activeProc = proc;

    proc.stdout.on('data', (data) => {
      const lines = data.toString().split(/\r?\n/);
      for (const line of lines) {
        if (line.trim().length > 0) {
          log.log(line);
        }
      }
      screen.render();
    });

    proc.stderr.on('data', (data) => {
      const lines = data.toString().split(/\r?\n/);
      for (const line of lines) {
        if (line.trim().length > 0) {
          log.log(TAGS.red(line));
        }
      }
      screen.render();
    });

    proc.on('close', (code, signal) => {
      activeTab.activeProc = null;
      if (code !== 0 && code !== null) {
        log.log(`${TAGS.dim('[PROCESS]')} ${TAGS.red(`Exited with code ${code}${signal ? ` (${signal})` : ''}`)}`);
      } else {
        log.log(`${TAGS.dim('[PROCESS]')} ${TAGS.lime('Completed successfully')}`);
      }
      screen.render();
    });

    proc.on('error', (err) => {
      activeTab.activeProc = null;
      log.log(`${TAGS.red(`[PROCESS ERROR] ${err.message}`)}`);
      screen.render();
    });

    return proc;
  }

  // Handle Input Submission
  inputBox.on('submit', (value) => {
    const inputStr = (value || '').trim();
    inputBox.clearValue();

    if (!inputStr) {
      setFocus(2);
      return;
    }

    const activeTab = getActiveTab();

    // Built-in commands
    if (inputStr === 'clear' || inputStr === 'cls') {
      activeTab.log.setContent('');
      printWelcomeBanner(activeTab.log);
      setFocus(2);
      return;
    }

    if (inputStr === 'exit' || inputStr === ':quit' || inputStr === ':q') {
      destroy();
      process.exit(0);
      return;
    }

    if (inputStr.startsWith('cd ')) {
      const targetDir = inputStr.substring(3).trim();
      currentDir = path.resolve(currentDir, targetDir);
      header.setContent(` {bold}{#39ff14-fg}TURFCODE{/#39ff14-fg}{/bold} │ Room: {bold}{#eab308-fg}${roomCode}{/#eab308-fg}{/bold} │ Mode: {bold}{#39ff14-fg}${mode.toUpperCase()}{/#39ff14-fg}{/bold} │ User: {#06b6d4-fg}${userName}{/#06b6d4-fg} │ Dir: {#4a7c4a-fg}${path.basename(currentDir) || currentDir}{/#4a7c4a-fg}`);
      activeTab.log.log(`${TAGS.cyan('Directory changed to:')} ${TAGS.lime(currentDir)}`);
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/tab ')) {
      const tabNum = parseInt(inputStr.substring(5).trim(), 10);
      if (!isNaN(tabNum) && tabNum >= 1 && tabNum <= tabs.length) {
        switchTab(tabNum - 1);
      }
      setFocus(2);
      return;
    }

    if (inputStr === '/sidebar' || inputStr === '/b') {
      toggleSidebar();
      setFocus(2);
      return;
    }

    if (inputStr === '/diff' || inputStr === '/d') {
      openDiffTab();
      setFocus(2);
      return;
    }

    if (inputStr === '/tree' || inputStr === '/files' || inputStr === '/filetree' || inputStr === '/f') {
      toggleFileTree();
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/task add ')) {
      const title = inputStr.substring(10).trim();
      if (title) {
        if (mode === 'peer' && client) {
          client.addTask({ title, assignee: userName });
          activeTab.log.log(`${TAGS.lime('[INTENT] Task add request sent:')} ${title}`);
        } else {
          const added = intentBoard.addTask({ title, assignee: userName, status: 'queued' });
          activeTab.log.log(`${TAGS.lime('[INTENT] Added task')} #${added.id}: ${added.title}`);
        }
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/task done ')) {
      const tId = inputStr.substring(11).trim();
      if (tId) {
        if (mode === 'peer' && client) {
          client.updateTask(tId, { status: 'done' });
          client.releaseLocks(tId);
          activeTab.log.log(`${TAGS.lime('[INTENT] Task completion sent for')} #${tId}`);
        } else {
          intentBoard.updateTask(tId, { status: 'done' });
          lockManager.releaseLocks(tId);
          bell.onTaskCompletion(tId);
          activeTab.log.log(`${TAGS.lime('[INTENT] Task')} #${tId} ${TAGS.lime('completed and locks released')}`);
        }
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/lock ')) {
      const parts = inputStr.substring(6).trim().split(/\s+/);
      const tId = parts[0];
      const files = parts.slice(1);
      if (tId && files.length > 0) {
        if (mode === 'peer' && client) {
          client.acquireLock(tId, files);
          activeTab.log.log(`${TAGS.lime(`[LOCK REQUEST] Sent lock request for task #${tId}`)}`);
        } else {
          const res = lockManager.acquireAtomicSequence(userName, tId, files);
          if (res.success) {
            activeTab.log.log(`${TAGS.lime(`[LOCK ACQUIRED] Task #${tId} locked ${files.length} file(s)`)}`);
          } else {
            bell.onLockConflict(res.conflictFile || res.error);
            activeTab.log.log(`${TAGS.red(res.error || `[LOCK CONFLICT] Cannot lock ${res.conflictFile}`)}`);
          }
        }
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/unlock ')) {
      const tId = inputStr.substring(8).trim();
      if (tId) {
        if (mode === 'peer' && client) {
          client.releaseLocks(tId);
          activeTab.log.log(`${TAGS.lime(`[UNLOCK REQUEST] Sent release request for task #${tId}`)}`);
        } else {
          const res = lockManager.releaseLocks(tId);
          activeTab.log.log(`${TAGS.lime(`[LOCKS RELEASED] Released ${res.released.length} file(s) for task #${tId}`)}`);
        }
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/force-unlock ')) {
      const file = inputStr.substring(14).trim();
      if (file) {
        const res = lockManager.forceUnlock(file);
        if (res.success) {
          activeTab.log.log(`${TAGS.lime(`[FORCE UNLOCKED] Manually broke lock on ${file}`)}`);
        } else {
          activeTab.log.log(`${TAGS.dim(`[UNLOCK] File was not locked: ${file}`)}`);
        }
      }
      setFocus(2);
      return;
    }

    if (mode === 'peer' && client) {
      activeTab.log.log(`${TAGS.cyan(`[${userName}]`)} ${inputStr}`);
      client.submitPrompt(inputStr);
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/agent ') || inputStr.startsWith('/agy ')) {
      const prompt = inputStr.replace(/^\/(agent|agy)\s+/, '').trim();
      if (prompt) {
        const taskId = String(Date.now()).slice(-4);
        intentBoard.addTask({ title: prompt, assignee: userName, status: 'in_progress' });
        activeTab.log.log(`${TAGS.lime('[AGENT] Spawning agy agent for task')} #${taskId}...`);
        try {
          const tab = addAgentTab({ taskId, ownerId: userName });
          const tabIndex = tabs.indexOf(tab);
          if (tabIndex >= 0) {
            switchTab(tabIndex);
          }
          agentRunner.spawnAgent({
            taskId,
            prompt,
            ownerId: userName,
            worktreePath: currentDir
          });
        } catch (err) {
          activeTab.log.log(`${TAGS.red(`[SPAWN FAILED] ${err.message}`)}`);
        }
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/terminate ') || inputStr.startsWith('/kill ')) {
      const tId = inputStr.replace(/^\/(terminate|kill)\s+/, '').trim();
      if (tId) {
        try {
          agentRunner.terminateAgent(tId, userName);
          activeTab.log.log(`${TAGS.lime(`[TERMINATE] Terminated agent for task #${tId}`)}`);
        } catch (err) {
          activeTab.log.log(`${TAGS.red(`[TERMINATE ERROR] ${err.message}`)}`);
        }
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/pause ')) {
      const tId = inputStr.substring(7).trim();
      if (tId) {
        try {
          agentRunner.pauseAgent(tId);
          activeTab.log.log(`${TAGS.yellow(`[PAUSE] Paused agent for task #${tId}`)}`);
        } catch (err) {
          activeTab.log.log(`${TAGS.red(`[PAUSE ERROR] ${err.message}`)}`);
        }
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/resume ')) {
      const tId = inputStr.substring(8).trim();
      if (tId) {
        try {
          agentRunner.resumeAgent(tId);
          activeTab.log.log(`${TAGS.lime(`[RESUME] Resumed agent for task #${tId}`)}`);
        } catch (err) {
          activeTab.log.log(`${TAGS.red(`[RESUME ERROR] ${err.message}`)}`);
        }
      }
      setFocus(2);
      return;
    }

    // Execute through spawnTerminalProcess
    spawnTerminalProcess(inputStr);
    setFocus(2);
  });

  // Keybindings ([MD-032])
  // 1. Tab: Cycle Focus
  screen.key(['tab'], () => {
    cycleFocus();
  });

  // 2. Ctrl+B: Toggle Sidebar
  screen.key(['C-b'], () => {
    toggleSidebar();
  });

  // 3. Ctrl+1 through Ctrl+9: Switch Tabs
  const numKeys = ['1', '2', '3', '4', '5', '6', '7', '8', '9'];
  numKeys.forEach((n, idx) => {
    screen.key([`C-${n}`, `M-${n}`], () => {
      switchTab(idx);
    });
  });

  // Keypress listener fallback for Ctrl+Number in various terminals
  screen.on('keypress', (ch, key) => {
    if (!key) return;
    if (key.ctrl && !key.meta) {
      const digit = parseInt(key.name || ch, 10);
      if (!isNaN(digit) && digit >= 1 && digit <= 9) {
        switchTab(digit - 1);
      }
    }
  });

  // 4. Ctrl+C: Clear Input Box / Interrupt Process
  screen.key(['C-c'], () => {
    const activeTab = getActiveTab();
    if (inputBox.getValue && inputBox.getValue().length > 0) {
      inputBox.clearValue();
      setFocus(2);
      return;
    }

    if (activeTab.activeProc) {
      try {
        if (process.platform === 'win32') {
          spawn('taskkill', ['/F', '/T', '/PID', activeTab.activeProc.pid]);
        } else {
          process.kill(-activeTab.activeProc.pid, 'SIGINT');
        }
      } catch {
        try {
          activeTab.activeProc.kill('SIGINT');
        } catch {
          // ignore already exited process
        }
      }
      activeTab.log.log(TAGS.red('^C (Process Interrupted)'));
      activeTab.activeProc = null;
      screen.render();
      setFocus(2);
      return;
    }

    // If input was empty and no process, clean clear & refocus
    inputBox.clearValue();
    setFocus(2);
  });

  // 5. [D]: Open Diff Viewer tab in right pane [MD-016]
  screen.key(['d', 'D'], () => {
    if (focusIndex !== 2) {
      openDiffTab();
    }
  });

  // 6. [F]: Toggle File Tree navigation in Intent Board [MD-027]
  screen.key(['f', 'F'], () => {
    if (focusIndex !== 2) {
      toggleFileTree();
    }
  });

  // Click handlers for mouse navigation
  tasksBox.on('click', () => setFocus(0));
  fileTreeBox.on('click', () => setFocus(0));
  terminalPane.on('click', () => setFocus(1));
  inputContainer.on('click', () => setFocus(2));
  inputBox.on('click', () => setFocus(2));

  // Initialize UI content
  renderTasks();
  renderLocks();
  renderPeers();
  renderTabs();
  tabs.forEach(t => printWelcomeBanner(t.log));

  // Initial focus on input box
  setFocus(2);

  function destroy() {
    if (tunnelServer && typeof tunnelServer.stop === 'function') {
      tunnelServer.stop().catch(() => {});
    }
    if (client && typeof client.disconnect === 'function') {
      client.disconnect();
    }
    recordSessionExit({ cwd: currentDir, cleanExit: true });
    lockManager.destroy();
    agentRunner.destroy();
    tabs.forEach(t => {
      if (t.activeProc) {
        try {
          t.activeProc.kill('SIGKILL');
        } catch {}
      }
    });
    try {
      if (screen.program) {
        screen.program.destroy();
      }
      screen.destroy();
    } catch {}
    if (process.stdin && typeof process.stdin.pause === 'function') {
      process.stdin.pause();
    }
  }

  return {
    screen,
    header,
    sidebar,
    spacesBox,
    tasksBox,
    locksBox,
    fileTreeBox,
    peersBox,
    collapseBtn,
    terminalPane,
    tabBar,
    tabs,
    inputContainer,
    inputBox,
    footer,
    intentBoard,
    lockManager,
    agentRunner,
    bell,
    diffViewer,
    fileTree,
    get isFileTreeVisible() {
      return isFileTreeVisible;
    },
    toggleFileTree,
    openDiffTab,
    selfHealResult: recoveryResult,
    addAgentTab,
    spawnAgent: (opts) => {
      const tab = addAgentTab(opts);
      const agent = agentRunner.spawnAgent(opts);
      tab.activeProc = agent.process;
      return agent;
    },
    terminateAgent: (taskId, requesterId, signal) => agentRunner.terminateAgent(taskId, requesterId, signal),
    pauseAgent: (taskId) => agentRunner.pauseAgent(taskId),
    resumeAgent: (taskId) => agentRunner.resumeAgent(taskId),
    tunnelServer,
    tunnelUrl,
    client,
    get tasks() {
      return intentBoard.getTasks();
    },
    get locks() {
      return lockManager.getLocks();
    },
    peers,
    get isSidebarCollapsed() {
      return isSidebarCollapsed;
    },
    get activeTabIndex() {
      return activeTabIndex;
    },
    get focusIndex() {
      return focusIndex;
    },
    setFocus,
    cycleFocus,
    toggleSidebar,
    switchTab,
    spawnTerminalProcess,
    renderTasks,
    renderLocks,
    renderPeers,
    renderTabs,
    addTask: (task) => intentBoard.addTask(task),
    updateTask: (tId, updates) => intentBoard.updateTask(tId, updates),
    acquireLock: (ownerId, tId, files, opts) => lockManager.acquireAtomicSequence(ownerId, tId, files, opts),
    releaseLocks: (tId) => lockManager.releaseLocks(tId),
    forceUnlock: (fPath) => lockManager.forceUnlock(fPath),
    destroy
  };
}
