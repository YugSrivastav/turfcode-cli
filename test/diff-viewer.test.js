import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import path from 'path';
import os from 'os';
import { execFileSync } from 'child_process';
import {
  DiffViewer,
  colorizeDiff,
  isBinaryFile,
  isLargeFile,
  DEFAULT_MAX_FILE_SIZE
} from '../src/diff-viewer.js';

test('isBinaryFile identifies binary extensions and passes source code [MD-035]', () => {
  assert.equal(isBinaryFile('assets/logo.png'), true);
  assert.equal(isBinaryFile('docs/manual.pdf'), true);
  assert.equal(isBinaryFile('dist/bundle.zip'), true);
  assert.equal(isBinaryFile('addon.node'), true);

  assert.equal(isBinaryFile('src/index.js'), false);
  assert.equal(isBinaryFile('src/models/user.ts'), false);
  assert.equal(isBinaryFile('README.md'), false);
  assert.equal(isBinaryFile('package.json'), false);
});

test('isLargeFile identifies files exceeding 1MB threshold [MD-035]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-large-test-'));
  try {
    const smallFile = path.join(tmpDir, 'small.txt');
    fs.writeFileSync(smallFile, 'hello world', 'utf8');
    assert.equal(isLargeFile(smallFile, tmpDir), false);

    const largeFile = path.join(tmpDir, 'large.txt');
    // Allocate 1MB + 1KB buffer
    const buf = Buffer.alloc(1024 * 1024 + 1024, 'x');
    fs.writeFileSync(largeFile, buf);
    assert.equal(isLargeFile(largeFile, tmpDir), true);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('colorizeDiff applies green for additions, red for deletions, cyan for headers [MD-016]', () => {
  const sampleDiff = [
    'diff --git a/src/math.js b/src/math.js',
    'index 1234567..89abcdef 100644',
    '--- a/src/math.js',
    '+++ b/src/math.js',
    '@@ -1,3 +1,3 @@',
    '-const oldVal = 1;',
    '+const newVal = 2;',
    ' const context = true;'
  ].join('\n');

  // Test tags format (Blessed TUI)
  const tagged = colorizeDiff(sampleDiff, { format: 'tags' });
  assert.ok(tagged.includes('{#06b6d4-fg}diff --git a/src/math.js b/src/math.js{/#06b6d4-fg}'));
  assert.ok(tagged.includes('{#06b6d4-fg}@@ -1,3 +1,3 @@{/#06b6d4-fg}'));
  assert.ok(tagged.includes('{#ff3333-fg}-const oldVal = 1;{/#ff3333-fg}'));
  assert.ok(tagged.includes('{#39ff14-fg}+const newVal = 2;{/#39ff14-fg}'));

  // Test ANSI format
  const ansi = colorizeDiff(sampleDiff, { format: 'ansi' });
  assert.ok(ansi.includes('-const oldVal = 1;'));
  assert.ok(ansi.includes('+const newVal = 2;'));
});

test('DiffViewer: generates unified diff of uncommitted changes and ignores binary and >1MB files [MD-016], [MD-035]', async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-diff-test-'));

  try {
    // 1. Initialize git repo and initial commit
    execFileSync('git', ['init', '-b', 'main'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.name', 'Turf Tester'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.email', 'tester@turfcode.dev'], { cwd: tmpDir });

    const sourceFile = path.join(tmpDir, 'source.js');
    fs.writeFileSync(sourceFile, 'console.log("v1");\n', 'utf8');

    const imageFile = path.join(tmpDir, 'image.png');
    fs.writeFileSync(imageFile, Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));

    const hugeFile = path.join(tmpDir, 'huge.dat');
    fs.writeFileSync(hugeFile, 'initial');

    execFileSync('git', ['add', '.'], { cwd: tmpDir });
    execFileSync('git', ['commit', '-m', 'Initial commit'], { cwd: tmpDir });

    // 2. Introduce uncommitted changes:
    // a. Source modification
    fs.writeFileSync(sourceFile, 'console.log("v2 - uncommitted");\n', 'utf8');
    // b. Binary modification
    fs.writeFileSync(imageFile, Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x00, 0x00]));
    // c. >1MB file modification
    fs.writeFileSync(hugeFile, Buffer.alloc(1024 * 1024 + 512, 'a'));

    const viewer = new DiffViewer({ cwd: tmpDir });
    const result = await viewer.getUncommittedDiff();

    assert.equal(result.fileCount, 1, 'Only text file within 1MB should be included in diff');
    assert.equal(result.ignoredCount, 2, 'Binary and >1MB files must be ignored');
    assert.ok(result.diff.includes('console.log("v2 - uncommitted");'), 'Must contain additions');
    assert.ok(result.diff.includes('console.log("v1");'), 'Must contain deletions');
    assert.ok(result.diff.includes('[Binary file ignored: image.png]'), 'Must note ignored binary file');
    assert.ok(result.diff.includes('[File > 1MB ignored: huge.dat]'), 'Must note ignored large file');

    // Test colorized output contains green and red tags
    assert.ok(result.colorized.includes('{#39ff14-fg}+console.log("v2 - uncommitted");{/#39ff14-fg}'));
    assert.ok(result.colorized.includes('{#ff3333-fg}-console.log("v1");{/#ff3333-fg}'));
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('DiffViewer: handles clean repository with no uncommitted changes', async () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-diff-clean-'));
  try {
    execFileSync('git', ['init', '-b', 'main'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.name', 'Turf Tester'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.email', 'tester@turfcode.dev'], { cwd: tmpDir });
    fs.writeFileSync(path.join(tmpDir, 'file.txt'), 'clean\n');
    execFileSync('git', ['add', '.'], { cwd: tmpDir });
    execFileSync('git', ['commit', '-m', 'clean'], { cwd: tmpDir });

    const viewer = new DiffViewer({ cwd: tmpDir });
    const result = await viewer.getUncommittedDiff();

    assert.equal(result.fileCount, 0);
    assert.ok(result.diff.includes('No uncommitted text changes'));
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
