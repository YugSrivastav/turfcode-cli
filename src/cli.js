/**
 * TurfCode CLI - Command-Line Interface & Launcher
 * 
 * Supports:
 * - turf host [dir]
 * - turf join <id>
 * - Interactive menu: [1] Host a Turf, [2] Join a Turf
 * 
 * Adheres to [MD-006], [MD-025], [MD-038].
 */

import fs from 'fs';
import path from 'path';
import crypto from 'crypto';
import readline from 'readline';
import chalk from 'chalk';
import { runPreflightChecks, printPreflightErrors } from './preflight.js';
import { createCockpit } from './cockpit.js';
import { CHALK } from './theme.js';
import { TunnelServer, generateRoomCode as generateRoomCodeFromTunnel } from './tunnel.js';
import { TurfClient } from './client.js';

export function generateRoomCode() {
  return generateRoomCodeFromTunnel(4);
}

export function normalizeWorkspacePath(inputPath, baseDir = process.cwd()) {
  if (!inputPath || inputPath.trim() === '' || inputPath.trim() === '.') {
    return baseDir;
  }
  let p = inputPath.trim().replace(/^["']|["']$/g, '');
  const winMatch = p.match(/^([a-zA-Z]):[\\/](.*)$/);
  if (winMatch && process.platform === 'linux') {
    const driveLetter = winMatch[1].toLowerCase();
    const rest = winMatch[2].replace(/\\/g, '/');
    return path.resolve(`/mnt/${driveLetter}/${rest}`);
  }
  p = p.replace(/\\/g, '/');
  return path.resolve(baseDir, p);
}

export function parseArgs(rawArgs) {
  const args = rawArgs.slice(2);
  const result = {
    command: null,
    dir: null,
    roomId: null,
    help: false,
    version: false,
    nonInteractive: false,
    silent: false
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];

    if (arg === '--help' || arg === '-h') {
      result.help = true;
    } else if (arg === '--version' || arg === '-v') {
      result.version = true;
    } else if (arg === '--non-interactive') {
      result.nonInteractive = true;
    } else if (arg === '--silent' || arg === '-s') {
      result.silent = true;
    } else if (arg === 'host') {
      result.command = 'host';
      if (args[i + 1] && !args[i + 1].startsWith('-')) {
        result.dir = args[i + 1];
        i++;
      }
    } else if (arg === 'join') {
      result.command = 'join';
      if (args[i + 1] && !args[i + 1].startsWith('-')) {
        result.roomId = args[i + 1];
        i++;
      }
    }
  }

  return result;
}

export function printHelp() {
  console.log(`
${CHALK.limeBold('TURFCODE CLI')} — ${CHALK.text('Real-time multiplayer AI coding in the terminal')}

${CHALK.dim('USAGE:')}
  ${CHALK.lime('turfcode')}                 ${CHALK.text('Open interactive launcher menu (Create or Join a Turf)')}
  ${CHALK.lime('turf')}                     ${CHALK.text('Alias for turfcode')}
  ${CHALK.lime('turf host [dir]')}          ${CHALK.text('Host a shared Turf session in the given directory (default: .)')}
  ${CHALK.lime('turf join <id>')}           ${CHALK.text('Join an active Turf session using a 6-character room code')}
  ${CHALK.lime('turf --silent, -s')}        ${CHALK.text('Run in silent mode (mute ANSI \\x07 audio bell)')}
  ${CHALK.lime('turf --help, -h')}          ${CHALK.text('Show this help information')}
  ${CHALK.lime('turf --version, -v')}       ${CHALK.text('Show version')}

${CHALK.dim('NAVIGATION KEYBINDINGS:')}
  ${CHALK.lime('[Tab]')}                    ${CHALK.text('Cycle focus: Intent Board ⇄ Terminal Pane ⇄ Input Box')}
  ${CHALK.lime('[D]')}                      ${CHALK.text('Open live colorized git diff viewer')}
  ${CHALK.lime('[F]')}                      ${CHALK.text('Toggle workspace file tree in Intent Board')}
  ${CHALK.lime('[Ctrl+B]')}                 ${CHALK.text('Toggle Intent Board sidebar collapsed (4 cols) / expanded (30%)')}
  ${CHALK.lime('[Ctrl+1..9]')}              ${CHALK.text('Switch active terminal tabs')}
  ${CHALK.lime('[Ctrl+C]')}                 ${CHALK.text('Clear input box / Interrupt active process')}
`);
}

export function printSplash() {
  console.log(CHALK.lime(`
  ╔═══════════════════════════════════════════════════════════════════╗
  ║   ████████╗██╗   ██╗██████╗ ███████╗ ██████╗ ██████╗ ██████╗    ║
  ║   ╚══██╔══╝██║   ██║██╔══██╗██╔════╝██╔════╝██╔═══██╗██╔══██╗   ║
  ║      ██║   ██║   ██║██████╔╝█████╗  ██║     ██║   ██║██║  ██║   ║
  ║      ██║   ██║   ██║██╔══██╗██╔══╝  ██║     ██║   ██║██║  ██║   ║
  ║      ██║   ╚██████╔╝██║  ██║██║     ╚██████╗╚██████╔╝██████╔╝   ║
  ║      ╚═╝    ╚═════╝ ╚═╝  ╚═╝╚═╝      ╚═════╝ ╚═════╝ ╚═════╝    ║
  ║                   ELECTRIC LIME COCKPIT v0.1.0                    ║
  ╚═══════════════════════════════════════════════════════════════════╝
`));
}

export async function promptInteractiveMenu() {
  printSplash();

  console.log(`  ${CHALK.lime('[1]')} Create a Turf   ${CHALK.dim('(Host a local folder / live session)')}`);
  console.log(`  ${CHALK.lime('[2]')} Join a Turf     ${CHALK.dim('(Connect to an active room code)')}`);
  console.log(`  ${CHALK.dim('[3] Exit')}\n`);

  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
  });

  return new Promise((resolve) => {
    rl.question(CHALK.lime('  Select option [1-3]: '), (ans) => {
      const choice = ans.trim();
      if (choice === '1') {
        rl.question(CHALK.lime('  Enter folder to host [default: .]: '), (inputDir) => {
          rl.close();
          const resolvedDir = normalizeWorkspacePath(inputDir);

          if (!fs.existsSync(resolvedDir)) {
            console.log(CHALK.amber(`\n  ⚠️  Notice: Directory "${resolvedDir}" does not exist. Creating it...`));
            fs.mkdirSync(resolvedDir, { recursive: true });
          }

          const roomCode = generateRoomCode();

          console.log('\n  ' + CHALK.limeBold('⚡ TURF CREATED: ') + CHALK.amberBold(roomCode));
          console.log(CHALK.dim('  📂 Hosting Folder: ') + CHALK.cyan(resolvedDir));
          console.log(CHALK.dim('  🔗 Share this Session ID with your collaborator to join!\n'));

          resolve({ command: 'host', dir: resolvedDir, roomCode });
        });
      } else if (choice === '2') {
        rl.question(CHALK.lime('  Enter Session ID / Room Code (e.g. TURF-7A9B): '), (code) => {
          rl.close();
          resolve({ command: 'join', roomId: code.trim().toUpperCase() });
        });
      } else {
        rl.close();
        resolve(null);
      }
    });
  });
}

export async function runCLI(argv = process.argv) {
  const parsed = parseArgs(argv);

  if (parsed.version) {
    console.log('0.1.0');
    return 0;
  }

  if (parsed.help) {
    printHelp();
    return 0;
  }

  let command = parsed.command;
  let dir = parsed.dir;
  let roomId = parsed.roomId;
  let interactiveRoomCode = null;

  if (!command) {
    if (parsed.nonInteractive || !process.stdout.isTTY) {
      printHelp();
      return 0;
    }
    const selection = await promptInteractiveMenu();
    if (!selection) {
      console.log(CHALK.dim('\nExited TurfCode.\n'));
      return 0;
    }
    command = selection.command;
    dir = selection.dir;
    roomId = selection.roomId;
    interactiveRoomCode = selection.roomCode;
  }

  // Pre-flight validation
  const preflight = await runPreflightChecks({ allowNonTTY: parsed.nonInteractive });
  if (!preflight.ok) {
    printPreflightErrors(preflight.issues);
    return 1;
  }

  if (command === 'host') {
    const targetDir = normalizeWorkspacePath(dir || '.');
    const roomCode = interactiveRoomCode || generateRoomCode();
    const port = process.env.TURF_PORT ? parseInt(process.env.TURF_PORT, 10) : 0;
    const tunnelServer = new TunnelServer({
      cwd: targetDir,
      roomCode,
      port
    });

    let serverInfo = null;
    try {
      serverInfo = await tunnelServer.start();
    } catch (err) {
      console.error(CHALK.alertRed(`Failed to start tunnel server: ${err.message}`));
      return 1;
    }

    createCockpit({
      mode: 'host',
      cwd: targetDir,
      roomCode: serverInfo?.roomCode || roomCode,
      tunnelServer,
      tunnelUrl: serverInfo?.tunnelUrl,
      userName: process.env.USER || 'Host',
      role: 'Host',
      silent: parsed.silent
    });
    return 0;
  }

  if (command === 'join') {
    if (!roomId) {
      console.error(CHALK.alertRed('\nError: Room code is required to join a turf session.'));
      console.error(CHALK.dim('Usage: turf join <ID> (e.g. turf join TURF-7A9B)\n'));
      return 1;
    }
    const cleanRoom = roomId.startsWith('TURF-') ? roomId : `TURF-${roomId}`;
    const hostUrl = process.env.TURF_HOST_URL || 'ws://127.0.0.1:7873';
    const client = new TurfClient({
      roomCode: cleanRoom,
      hostUrl,
      username: process.env.USER || 'Peer',
      role: 'Peer',
      autoReconnect: true
    });

    try {
      await client.connect();
    } catch (err) {
      if (!parsed.nonInteractive) {
        console.warn(CHALK.amber(`Connecting in background to ${hostUrl} (${err.message})...`));
      }
    }

    createCockpit({
      mode: 'peer',
      cwd: process.cwd(),
      roomCode: cleanRoom,
      client,
      userName: process.env.USER || 'Peer',
      role: 'Peer',
      silent: parsed.silent
    });
    return 0;
  }

  printHelp();
  return 0;
}
