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
    lockManager: customLockManager,
    intentBoard: customIntentBoard,
    agentRunner: customAgentRunner
  } = options;

  let currentDir = cwd;
  let isSidebarCollapsed = false;
  let focusIndex = 2; // 0: Sidebar, 1: Terminal Log, 2: Input Box (default focused)
  let activeTabIndex = 0;

  // Initialize Lock Manager & Intent Board
  const lockManager = customLockManager || new LockManager({
    cwd: currentDir,
    initialLocks: initialLocks.length > 0 ? initialLocks : undefined,
    autoStartMonitor: true
  });

  const defaultInitialTasks = initialTasks.length > 0 ? initialTasks : [
    { id: '1', title: 'Initialize Electric Lime Cockpit', assignee: userName, status: 'in_progress', targetFiles: [] },
    { id: '2', title: 'Connect Tunnel & Lock Manager', assignee: 'Team', status: 'queued', targetFiles: [] },
    { id: '0', title: 'Repository & Architecture Spec', assignee: 'Yug', status: 'done', targetFiles: [] }
  ];

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

  // 2. Top Header Bar
  const header = blessed.box({
    parent: screen,
    ...BLESSED_STYLES.header,
    content: ` {bold}{#39ff14-fg}TURFCODE{/#39ff14-fg}{/bold} │ Room: {bold}{#eab308-fg}${roomCode}{/#eab308-fg}{/bold} │ Mode: {bold}{#39ff14-fg}${mode.toUpperCase()}{/#39ff14-fg}{/bold} │ User: {#06b6d4-fg}${userName}{/#06b6d4-fg} │ Dir: {#4a7c4a-fg}${path.basename(currentDir) || currentDir}{/#4a7c4a-fg}`
  });

  // 3. Left Sidebar (30% Intent Board)
  const sidebar = blessed.box({
    parent: screen,
    top: 1,
    left: 0,
    width: '30%',
    height: '100%-2',
    style: {
      bg: PALETTE.bg
    }
  });

  // 3a. Tasks Container (MD-037 4-field schema)
  const tasksBox = blessed.box({
    parent: sidebar,
    top: 0,
    left: 0,
    width: '100%',
    height: '48%',
    label: ` {bold}{#39ff14-fg}📋 INTENT & TASKS{/#39ff14-fg}{/bold} `,
    border: { type: 'line', fg: PALETTE.mutedBorder },
    tags: true,
    scrollable: true,
    alwaysScroll: true,
    mouse: true,
    keys: true,
    vi: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg,
      border: { fg: PALETTE.mutedBorder }
    }
  });

  function renderTasks() {
    const currentTasks = intentBoard.getTasks();
    const currentLocks = lockManager.getLocks();
    if (!currentTasks || currentTasks.length === 0) {
      tasksBox.setContent(`\n ${TAGS.dim('(No tasks in intent queue)')}`);
      return;
    }
    const lines = currentTasks.map(t => intentBoard.formatTaskCard(t, currentLocks));
    tasksBox.setContent(lines.join('\n\n'));
  }

  // 3b. File Lock Table (MD-002, MD-040)
  const locksBox = blessed.box({
    parent: sidebar,
    top: '48%',
    left: 0,
    width: '100%',
    height: '28%',
    label: ` {bold}{#ef4444-fg}🔒 FILE LOCK TABLE{/#ef4444-fg}{/bold} `,
    border: { type: 'line', fg: PALETTE.mutedBorder },
    tags: true,
    scrollable: true,
    mouse: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg,
      border: { fg: PALETTE.mutedBorder }
    }
  });

  function renderLocks() {
    const currentLocks = lockManager.getLocks();
    if (!currentLocks || currentLocks.length === 0) {
      locksBox.setContent(`\n ${TAGS.dim('(No active file locks - workspace free)')}`);
      return;
    }
    const lines = currentLocks.map(l => {
      const holder = l.ownerId || l.holder || 'unknown';
      const fPath = l.filePath || l.file;
      return ` {#ef4444-fg}🔒 ${fPath}{/#ef4444-fg} ${TAGS.dim(`[Held by ${holder}]`)}`;
    });
    locksBox.setContent(lines.join('\n'));
  }

  // Live synchronizer between state and TUI
  intentBoard.on('change', () => {
    renderTasks();
    renderLocks();
    screen.render();
  });

  lockManager.on('change', () => {
    renderLocks();
    renderTasks();
    screen.render();
  });

  // 3c. Connected Peers Container
  const peersBox = blessed.box({
    parent: sidebar,
    top: '76%',
    left: 0,
    width: '100%',
    height: '100%-76%-3',
    label: ` {bold}{#06b6d4-fg}👥 CONNECTED PEERS{/#06b6d4-fg}{/bold} `,
    border: { type: 'line', fg: PALETTE.mutedBorder },
    tags: true,
    scrollable: true,
    mouse: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg,
      border: { fg: PALETTE.mutedBorder }
    }
  });

  function renderPeers() {
    if (!peers || peers.length === 0) {
      peersBox.setContent(`\n ${TAGS.dim('(Solo session)')}`);
      return;
    }
    const lines = peers.map(p => {
      const dot = TAGS.lime('●');
      const selfLabel = p.isSelf ? ` ${TAGS.limeBold('(You)')}` : '';
      return ` ${dot} {#f0fff0-fg}${p.name}{/#f0fff0-fg}${selfLabel} ${TAGS.cyan(`[${p.role}]`)}`;
    });
    peersBox.setContent(lines.join('\n'));
  }

  // 3d. Sidebar Collapse Button
  const collapseBtn = blessed.button({
    parent: sidebar,
    bottom: 0,
    left: 0,
    width: '100%',
    height: 3,
    border: { type: 'line', fg: PALETTE.mutedBorder },
    content: ` {bold}{#39ff14-fg}«{/#39ff14-fg} Collapse Sidebar {/bold}${TAGS.dim('[Ctrl+B]')}`,
    tags: true,
    mouse: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg,
      border: { fg: PALETTE.mutedBorder },
      hover: { bg: PALETTE.headerBg },
      focus: { border: { fg: PALETTE.activeBorder } }
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

  // 4a. Tab Bar
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
      bg: PALETTE.headerBg
    }
  });

  // Define Tabs: 1: Shell, 2: Agent (agy), 3: Agent 2
  const tabs = [
    {
      id: 'shell',
      name: '1: Shell',
      log: null,
      activeProc: null,
      history: []
    },
    {
      id: 'agent-1',
      name: '2: agy (Host)',
      log: null,
      activeProc: null,
      history: []
    },
    {
      id: 'agent-2',
      name: '3: agy (Peer)',
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
      height: '100%-5',
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
        return `{bold}{#081008-bg}{#39ff14-fg} [ ${t.name} ] {/#39ff14-fg}{/#081008-bg}{/bold}`;
      } else {
        return `{#4a7c4a-fg} [ ${t.name} ] {/#4a7c4a-fg}`;
      }
    });
    tabParts.push(`{#2a5a2a-fg}[ + New ]{/#2a5a2a-fg}`);
    tabBar.setContent(` ${tabParts.join(' ')}`);
  }

  function printWelcomeBanner(log) {
    log.log(`{#1b3b1b-fg}┌─────────────────────────────────────────────────────────────┐{/#1b3b1b-fg}`);
    log.log(`{#1b3b1b-fg}│{/#1b3b1b-fg}  {bold}{#39ff14-fg}TURFCODE ELECTRIC LIME COCKPIT v0.1.0{/#39ff14-fg}{/bold} — {#22c55e-fg}READY{/#22c55e-fg}             {#1b3b1b-fg}│{/#1b3b1b-fg}`);
    log.log(`{#1b3b1b-fg}│{/#1b3b1b-fg}  Session: {#eab308-fg}${roomCode}{/#eab308-fg}  │ Mode: {#39ff14-fg}${mode.toUpperCase()}{/#39ff14-fg}  │ Directory: {#4a7c4a-fg}${currentDir}{/#4a7c4a-fg} {#1b3b1b-fg}│{/#1b3b1b-fg}`);
    log.log(`{#1b3b1b-fg}│{/#1b3b1b-fg}  Multi-Agent Terminal: Parallel execution without screen wipe {#1b3b1b-fg}│{/#1b3b1b-fg}`);
    log.log(`{#1b3b1b-fg}│{/#1b3b1b-fg}  [Ctrl+B] Toggle Sidebar │ [Ctrl+1..9] Tabs │ [Tab] Focus    {#1b3b1b-fg}│{/#1b3b1b-fg}`);
    log.log(`{#1b3b1b-fg}└─────────────────────────────────────────────────────────────┘{/#1b3b1b-fg}\n`);
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
      height: '100%-5',
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

  // 4b. Command / Prompt Input Box
  const inputContainer = blessed.box({
    parent: terminalPane,
    bottom: 0,
    left: 0,
    width: '100%-2',
    height: 3,
    border: { type: 'line', fg: PALETTE.activeBorder },
    label: ` {bold}{#39ff14-fg}✍️ Command / Agent Prompt{/#39ff14-fg}{/bold} `,
    tags: true,
    mouse: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg,
      border: { fg: PALETTE.activeBorder }
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

  // 5. Bottom Footer Bar
  const footer = blessed.box({
    parent: screen,
    ...BLESSED_STYLES.footer,
    content: ` {bold}{#39ff14-fg}[Tab]{/#39ff14-fg}{/bold} Focus │ {bold}{#39ff14-fg}[Ctrl+B]{/#39ff14-fg}{/bold} Sidebar │ {bold}{#39ff14-fg}[Ctrl+1..9]{/#39ff14-fg}{/bold} Tabs │ {bold}{#39ff14-fg}[Ctrl+C]{/#39ff14-fg}{/bold} Clear │ {bold}{#39ff14-fg}[Enter]{/#39ff14-fg}{/bold} Run `
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

    // Reset all borders to muted
    tasksBox.style.border.fg = PALETTE.mutedBorder;
    terminalPane.style.border.fg = PALETTE.mutedBorder;
    inputContainer.style.border.fg = PALETTE.mutedBorder;

    if (focusIndex === 0) {
      // Focus Sidebar (Intent Board)
      tasksBox.style.border.fg = PALETTE.activeBorder;
      tasksBox.focus();
    } else if (focusIndex === 1) {
      // Focus Terminal Log
      terminalPane.style.border.fg = PALETTE.activeBorder;
      getActiveLog().focus();
    } else {
      // Focus Input Box
      inputContainer.style.border.fg = PALETTE.activeBorder;
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
      tasksBox.hide();
      locksBox.hide();
      peersBox.hide();
      collapseBtn.top = 0;
      collapseBtn.height = '100%-2';
      collapseBtn.setContent(' »');
      terminalPane.left = 4;
      terminalPane.width = '100%-4';
    } else {
      sidebar.width = '30%';
      tasksBox.show();
      locksBox.show();
      peersBox.show();
      collapseBtn.top = undefined;
      collapseBtn.bottom = 0;
      collapseBtn.height = 3;
      collapseBtn.setContent(` {bold}{#39ff14-fg}«{/#39ff14-fg} Collapse Sidebar {/bold}${TAGS.dim('[Ctrl+B]')}`);
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

    if (inputStr.startsWith('/task add ')) {
      const title = inputStr.substring(10).trim();
      if (title) {
        const added = intentBoard.addTask({ title, assignee: userName, status: 'queued' });
        activeTab.log.log(`${TAGS.lime('[INTENT] Added task')} #${added.id}: ${added.title}`);
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/task done ')) {
      const tId = inputStr.substring(11).trim();
      if (tId) {
        intentBoard.updateTask(tId, { status: 'done' });
        lockManager.releaseLocks(tId);
        activeTab.log.log(`${TAGS.lime('[INTENT] Task')} #${tId} ${TAGS.lime('completed and locks released')}`);
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/lock ')) {
      const parts = inputStr.substring(6).trim().split(/\s+/);
      const tId = parts[0];
      const files = parts.slice(1);
      if (tId && files.length > 0) {
        const res = lockManager.acquireAtomicSequence(userName, tId, files);
        if (res.success) {
          activeTab.log.log(`${TAGS.lime(`[LOCK ACQUIRED] Task #${tId} locked ${files.length} file(s)`)}`);
        } else {
          activeTab.log.log(`${TAGS.red(res.error || `[LOCK CONFLICT] Cannot lock ${res.conflictFile}`)}`);
        }
      }
      setFocus(2);
      return;
    }

    if (inputStr.startsWith('/unlock ')) {
      const tId = inputStr.substring(8).trim();
      if (tId) {
        const res = lockManager.releaseLocks(tId);
        activeTab.log.log(`${TAGS.lime(`[LOCKS RELEASED] Released ${res.released.length} file(s) for task #${tId}`)}`);
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

  // Click handlers for mouse navigation
  tasksBox.on('click', () => setFocus(0));
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
    tasksBox,
    locksBox,
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
