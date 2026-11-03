import test from 'node:test';
import assert from 'node:assert/strict';
import { parseArgs, generateRoomCode, runCLI, normalizeWorkspacePath } from '../src/cli.js';
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

  const silentArgs = parseArgs(['node', 'turf', 'host', '--silent']);
  assert.equal(silentArgs.silent, true);

  const silentShortArgs = parseArgs(['node', 'turf', '-s']);
  assert.equal(silentShortArgs.silent, true);
});

test('generateRoomCode produces 6-character TURF-XXXX room codes', () => {
  const code = generateRoomCode();
  assert.match(code, /^TURF-[2-9A-Z]{4}$/, 'Room code must match TURF-XXXX format');
});

test('Preflight checks validate dependencies and environment', async () => {
  const result = await runPreflightChecks({ allowNonTTY: true });
  assert.equal(result.ok, true, `Preflight checks must pass when allowNonTTY is true: ${result.issues.join(', ')}`);
});

test('runCLI handles --version and --help without error', async () => {
  const versionExit = await runCLI(['node', 'turf', '--version']);
  assert.equal(versionExit, 0);

  const helpExit = await runCLI(['node', 'turf', '--help']);
  assert.equal(helpExit, 0);
});

test('runCLI join without room code returns error code 1', async () => {
  const joinExit = await runCLI(['node', 'turf', 'join', '--non-interactive']);
  assert.equal(joinExit, 1);
});

test('package.json defines bin entries for both turf and turfcode [MD-041]', async () => {
  const fs = await import('fs');
  const { fileURLToPath } = await import('url');
  const pkgPath = fileURLToPath(new URL('../package.json', import.meta.url));
  const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
  assert.equal(pkg.bin.turf, './bin/turf.js');
  assert.equal(pkg.bin.turfcode, './bin/turf.js');
});

test('normalizeWorkspacePath handles Windows paths, quotes, and relative paths [MD-042]', () => {
  const resolvedDot = normalizeWorkspacePath('.');
  assert.equal(resolvedDot, process.cwd());

  const resolvedRelative = normalizeWorkspacePath('./test-sub');
  assert.ok(resolvedRelative.endsWith('test-sub'));

  if (process.platform === 'linux') {
    const winPath = normalizeWorkspacePath('C:\\Users\\yugsr\\firstmate\\projects\\turfcode-cli');
    assert.equal(winPath, '/mnt/c/Users/yugsr/firstmate/projects/turfcode-cli');

    const winSlashPath = normalizeWorkspacePath('"C:/Users/yugsr/firstmate/projects/turfcode-cli"');
    assert.equal(winSlashPath, '/mnt/c/Users/yugsr/firstmate/projects/turfcode-cli');
  }
});
