import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import path from 'path';
import os from 'os';
import { AgentRunner } from '../src/agent-runner.js';
import { createCockpit } from '../src/cockpit.js';

test('AgentRunner: spawnAgent executes binary directly with argument array [MD-029]', async () => {
  const runner = new AgentRunner({
    frameIntervalMs: 20
  });

  try {
    const taskId = 'task-direct-1';
    let outputReceived = '';
    runner.on('output', ({ taskId: id, text }) => {
      if (id === taskId) {
        outputReceived += text;
      }
    });

    const agent = runner.spawnAgent({
      taskId,
      agyBinary: process.execPath,
      args: ['-e', 'console.log("AGY_DIRECT_EXEC_SUCCESS")'],
      ownerId: 'Host'
    });

    assert.equal(agent.taskId, taskId);
    assert.equal(agent.ownerId, 'Host');
    assert.ok(agent.pid > 0, 'Agent process must have a valid PID');
    assert.equal(runner.hasAgent(taskId), true);
    assert.equal(runner.getAgent(taskId).status, 'RUNNING');

    const exitEvent = await new Promise((resolve) => {
      runner.on('exit', (evt) => {
        if (evt.taskId === taskId) resolve(evt);
      });
    });

    assert.equal(exitEvent.code, 0, 'Process must exit with code 0');
    assert.ok(outputReceived.includes('AGY_DIRECT_EXEC_SUCCESS'), 'Direct execution output must be captured');
  } finally {
    runner.destroy();
  }
});

test('AgentRunner: injects peer auth token and session ID into subprocess environment [MD-010], [MD-019]', async () => {
  const runner = new AgentRunner({
    frameIntervalMs: 20
  });

  try {
    const taskId = 'task-auth-pass-1';
    const testAuthToken = 'ayush-auth-token-xyz-123';
    const testSessionId = 'ayush-session-guid-789';

    let jsonOutput = '';
    runner.on('output', ({ taskId: id, text }) => {
      if (id === taskId) {
        jsonOutput += text;
      }
    });

    // Mock script that reflects injected environment variables
    const printEnvScript = `
      const result = {
        geminiKey: process.env.GEMINI_API_KEY,
        agyToken: process.env.AGY_AUTH_TOKEN,
        sessionId: process.env.AGY_SESSION_ID,
        taskId: process.env.TURF_TASK_ID,
        ownerId: process.env.TURF_OWNER_ID
      };
      console.log(JSON.stringify(result));
    `;

    const agent = runner.spawnAgent({
      taskId,
      ownerId: 'Ayush',
      authToken: testAuthToken,
      sessionId: testSessionId,
      agyBinary: process.execPath,
      args: ['-e', printEnvScript]
    });

    // Verify token is NOT persisted in agent public state [MD-018], [MD-019]
    assert.equal(agent.authToken, undefined, 'Auth token must not leak into agent public object');

    await new Promise((resolve) => {
      runner.on('exit', (evt) => {
        if (evt.taskId === taskId) resolve(evt);
      });
    });

    const parsed = JSON.parse(jsonOutput.trim());
    assert.equal(parsed.geminiKey, testAuthToken, 'GEMINI_API_KEY must match injected peer authToken [MD-010]');
    assert.equal(parsed.agyToken, testAuthToken, 'AGY_AUTH_TOKEN must match injected peer authToken [MD-010]');
    assert.equal(parsed.sessionId, testSessionId, 'AGY_SESSION_ID must match injected peer sessionId [MD-019]');
    assert.equal(parsed.taskId, taskId);
    assert.equal(parsed.ownerId, 'Ayush');
  } finally {
    runner.destroy();
  }
});

test('AgentRunner: owner-only floor control rejects unauthorized termination [MD-013]', async () => {
  const runner = new AgentRunner({
    frameIntervalMs: 20
  });

  try {
    const taskId = 'task-floor-control-1';
    
    // Spawn a long-running mock agent owned by 'Yug'
    const agent = runner.spawnAgent({
      taskId,
      ownerId: 'Yug',
      agyBinary: process.execPath,
      args: ['-e', 'setInterval(() => {}, 1000)']
    });

    assert.equal(agent.ownerId, 'Yug');

    // Ayush attempts to terminate Yug's running agent -> MUST BE REJECTED [MD-013]
    assert.throws(() => {
      runner.terminateAgent(taskId, 'Ayush');
    }, (err) => {
      assert.equal(err.code, 'ERR_UNAUTHORIZED');
      assert.ok(err.message.includes('not the owner'));
      assert.ok(err.message.includes('[MD-013]'));
      return true;
    });

    // Agent must still be active
    assert.equal(agent.status, 'RUNNING');

    // Authorized termination by Yug -> MUST SUCCEED
    const termRes = runner.terminateAgent(taskId, 'Yug');
    assert.equal(termRes.success, true);
    assert.equal(agent.status, 'TERMINATED');
  } finally {
    runner.destroy();
  }
});

test('AgentRunner: 30ms throttled frame buffer batches rapid text deltas [MD-021]', async () => {
  const runner = new AgentRunner({
    frameIntervalMs: 30
  });

  try {
    const taskId = 'task-throttle-1';
    const frames = [];

    runner.on('output', ({ taskId: id, delta }) => {
      if (id === taskId) {
        frames.push({ delta, time: Date.now() });
      }
    });

    // Emit 5 lines in rapid succession (< 10ms total)
    const burstScript = `
      process.stdout.write("FRAME_1\\n");
      process.stdout.write("FRAME_2\\n");
      process.stdout.write("FRAME_3\\n");
      process.stdout.write("FRAME_4\\n");
      process.stdout.write("FRAME_5\\n");
      setTimeout(() => process.exit(0), 100);
    `;

    runner.spawnAgent({
      taskId,
      ownerId: 'Host',
      agyBinary: process.execPath,
      args: ['-e', burstScript]
    });

    await new Promise((resolve) => {
      runner.on('exit', (evt) => {
        if (evt.taskId === taskId) resolve(evt);
      });
    });

    // With 30ms throttling, the 5 rapid writes must be batched into fewer frames
    // (typically 1 frame containing all 5 lines) instead of 5 separate unthrottled bursts
    assert.ok(frames.length < 5, `Expected batched frames (< 5), got ${frames.length}`);
    const combined = frames.map(f => f.delta).join('');
    assert.ok(combined.includes('FRAME_1'));
    assert.ok(combined.includes('FRAME_2'));
    assert.ok(combined.includes('FRAME_3'));
    assert.ok(combined.includes('FRAME_4'));
    assert.ok(combined.includes('FRAME_5'));
  } finally {
    runner.destroy();
  }
});

test('AgentRunner: frame buffer auto-flushes immediately on process close [MD-021]', async () => {
  const runner = new AgentRunner({
    frameIntervalMs: 100 // Long interval to guarantee process exits before timer fires
  });

  try {
    const taskId = 'task-flush-exit-1';
    let received = '';

    runner.on('output', ({ taskId: id, text }) => {
      if (id === taskId) {
        received += text;
      }
    });

    // Script writes and immediately exits
    runner.spawnAgent({
      taskId,
      ownerId: 'Host',
      agyBinary: process.execPath,
      args: ['-e', 'process.stdout.write("IMMEDIATE_FLUSH_ON_EXIT"); process.exit(0);']
    });

    await new Promise((resolve) => {
      runner.on('exit', (evt) => {
        if (evt.taskId === taskId) resolve(evt);
      });
    });

    assert.equal(received, 'IMMEDIATE_FLUSH_ON_EXIT', 'Frame buffer must auto-flush immediately on process close');
  } finally {
    runner.destroy();
  }
});

test('AgentRunner: pauseAgent and resumeAgent manage process state via SIGSTOP/SIGCONT [MD-024]', async () => {
  const runner = new AgentRunner({
    frameIntervalMs: 20
  });

  try {
    const taskId = 'task-pause-resume-1';

    runner.spawnAgent({
      taskId,
      ownerId: 'Ayush',
      agyBinary: process.execPath,
      args: ['-e', 'setInterval(() => {}, 1000)']
    });

    const agent = runner.getAgent(taskId);
    assert.equal(agent.status, 'RUNNING');

    // Pause agent on peer disconnect [MD-024]
    const pauseRes = runner.pauseAgent(taskId);
    assert.equal(pauseRes.status, 'PAUSED');
    assert.equal(agent.status, 'PAUSED');

    // Resume agent when peer reconnects [MD-024]
    const resumeRes = runner.resumeAgent(taskId);
    assert.equal(resumeRes.status, 'RUNNING');
    assert.equal(agent.status, 'RUNNING');

    // Terminate cleanly
    runner.terminateAgent(taskId, 'Ayush');
    assert.equal(agent.status, 'TERMINATED');
  } finally {
    runner.destroy();
  }
});

test('AgentRunner: rejects duplicate task spawn when agent is already running', () => {
  const runner = new AgentRunner();

  try {
    const taskId = 'task-dup-1';
    runner.spawnAgent({
      taskId,
      ownerId: 'Host',
      agyBinary: process.execPath,
      args: ['-e', 'setInterval(() => {}, 1000)']
    });

    assert.throws(() => {
      runner.spawnAgent({
        taskId,
        ownerId: 'Host',
        agyBinary: process.execPath,
        args: ['-e', 'setInterval(() => {}, 1000)']
      });
    }, /already running/);
  } finally {
    runner.destroy();
  }
});

test('Cockpit integration: dynamic tabs for active running agents (Ctrl+1..9) and event routing', () => {
  const cockpit = createCockpit({
    roomCode: 'TURF-AGENT-TEST',
    screenOptions: { dump: false }
  });

  try {
    assert.ok(cockpit.agentRunner, 'Cockpit must expose agentRunner');
    assert.equal(cockpit.tabs.length, 3, 'Initial default tabs');

    // 1. Add dynamic tab for active running agent
    const newTab = cockpit.addAgentTab({
      taskId: 'agent-404',
      ownerId: 'Ayush',
      name: '4: agy (#agent-404)'
    });

    assert.equal(cockpit.tabs.length, 4, 'Dynamic tab must be added to right pane');
    assert.equal(newTab.taskId, 'agent-404');
    assert.equal(newTab.ownerId, 'Ayush');

    // Spy on log output
    const tabLogs = [];
    const origLog = newTab.log.log.bind(newTab.log);
    newTab.log.log = (msg) => {
      tabLogs.push(msg);
      origLog(msg);
    };

    // 2. Route AgentRunner output event to agent tab
    cockpit.agentRunner.emit('output', {
      taskId: 'agent-404',
      text: 'Compiling authentication module...\nSuccess.'
    });

    assert.ok(tabLogs.some(l => l.includes('Compiling authentication module')), 'Output event must appear in agent tab');
    assert.ok(tabLogs.some(l => l.includes('Success')), 'Output delta must appear in agent tab');

    // 3. Route AgentRunner error event to agent tab
    cockpit.agentRunner.emit('error', {
      taskId: 'agent-404',
      error: new Error('Rate limit warning on token stream')
    });

    assert.ok(tabLogs.some(l => l.includes('Rate limit warning')), 'Error event must appear in agent tab');

    // 4. Route AgentRunner exit event to agent tab
    cockpit.agentRunner.emit('exit', {
      taskId: 'agent-404',
      code: 0
    });

    assert.ok(tabLogs.some(l => l.includes('completed successfully')), 'Exit event must appear in agent tab');

    // 5. Test tab switching to dynamic tab (Ctrl+4 / switchTab(3))
    cockpit.switchTab(3);
    assert.equal(cockpit.activeTabIndex, 3);
    assert.equal(cockpit.tabs[3].log.hidden, false);
  } finally {
    cockpit.destroy();
  }
});
