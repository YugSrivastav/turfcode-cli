import test from 'node:test';
import assert from 'node:assert/strict';
import { parseArgs, generateRoomCode } from '../src/cli.js';
import { runPreflightChecks } from '../src/preflight.js';

test('CLI parseArgs handles subcommands and flags', () => {
  const hostArgs = parseArgs(['node', 'turf', 'host', './my-project']);
  assert.equal(hostArgs.command, 'host');
  assert.equal(hostArgs.dir, './my-project');

  const joinArgs = parseArgs(['node', 'turf', 'join', 'TURF-7A9B']);
  assert.equal(joinArgs.command, 'join');
  assert.equal(joinArgs.roomId, 'TURF-7A9B');

  const helpArgs = parseArgs(['node', 'turf', '--help']);
  assert.equal(helpArgs.help, true);

  const versionArgs = parseArgs(['node', 'turf', '-v']);
  assert.equal(versionArgs.version, true);
});

test('generateRoomCode produces 6-character TURF-XXXX room codes', () => {
  const code = generateRoomCode();
  assert.match(code, /^TURF-[2-9A-Z]{4}$/, 'Room code must match TURF-XXXX format');
});

test('Preflight checks validate dependencies and environment', async () => {
  const result = await runPreflightChecks({ allowNonTTY: true });
  assert.equal(result.ok, true, `Preflight checks must pass when allowNonTTY is true: ${result.issues.join(', ')}`);
});
