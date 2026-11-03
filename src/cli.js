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

import path from 'path';
import crypto from 'crypto';
import readline from 'readline';
import chalk from 'chalk';
import { runPreflightChecks, printPreflightErrors } from './preflight.js';
import { createCockpit } from './cockpit.js';
import { CHALK } from './theme.js';

export function generateRoomCode() {
  const chars = '23456789ABCDEFGHJKLMNPQRSTUVWXYZ'; // exclude ambiguous chars 0, 1, I, O
  let code = '';
  const bytes = crypto.randomBytes(4);
  for (let i = 0; i < 4; i++) {
    code += chars[bytes[i] % chars.length];
  }
  return `TURF-${code}`;
}

export function parseArgs(rawArgs) {
  const args = rawArgs.slice(2);
  const result = {
    command: null,
    dir: null,
    roomId: null,
    help: false,
    version: false,
    nonInteractive: false
  };

  for (let i = 0; i < args.length; i++) {
    const arg = args[i];

    if (arg === '--help' || arg === '-h') {
      result.help = true;
    } else if (arg === '--version' || arg === '-v') {
      result.version = true;
    } else if (arg === '--non-interactive') {
      result.nonInteractive = true;
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
  ${CHALK.lime('turf')}                     ${CHALK.text('Open interactive launcher menu')}
  ${CHALK.lime('turf host [dir]')}          ${CHALK.text('Host a shared Turf session in the given directory (default: .)')}
  ${CHALK.lime('turf join <id>')}           ${CHALK.text('Join an active Turf session using a 6-character room code')}
  ${CHALK.lime('turf --help, -h')}          ${CHALK.text('Show this help information')}
  ${CHALK.lime('turf --version, -v')}       ${CHALK.text('Show version')}

${CHALK.dim('NAVIGATION KEYBINDINGS:')}
  ${CHALK.lime('[Tab]')}                    ${CHALK.text('Cycle focus: Intent Board ⇄ Terminal Pane ⇄ Input Box')}
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

  console.log(`  ${CHALK.lime('[1]')} Host a Turf   ${CHALK.dim('(Start collaborative coding session)')}`);
  console.log(`  ${CHALK.lime('[2]')} Join a Turf   ${CHALK.dim('(Connect to an active room code)')}`);
  console.log(`  ${CHALK.dim('[3] Exit')}\n`);

  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
  });

  return new Promise((resolve) => {
    rl.question(CHALK.lime('  Select option [1-3]: '), (ans) => {
      const choice = ans.trim();
      if (choice === '1') {
        rl.question(CHALK.lime('  Workspace directory [default: .]: '), (dir) => {
          rl.close();
          resolve({ command: 'host', dir: dir.trim() || '.' });
        });
      } else if (choice === '2') {
        rl.question(CHALK.lime('  Enter Room Code (e.g. TURF-7A9B): '), (code) => {
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
  }

  // Pre-flight validation
  const preflight = await runPreflightChecks({ allowNonTTY: parsed.nonInteractive });
  if (!preflight.ok) {
    printPreflightErrors(preflight.issues);
    return 1;
  }

  if (command === 'host') {
    const targetDir = path.resolve(process.cwd(), dir || '.');
    const roomCode = generateRoomCode();
    createCockpit({
      mode: 'host',
      cwd: targetDir,
      roomCode,
      userName: process.env.USER || 'Host',
      role: 'Host'
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
    createCockpit({
      mode: 'peer',
      cwd: process.cwd(),
      roomCode: cleanRoom,
      userName: process.env.USER || 'Peer',
      role: 'Peer'
    });
    return 0;
  }

  printHelp();
  return 0;
}
