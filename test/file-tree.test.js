import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import path from 'path';
import os from 'os';
import { execFileSync } from 'child_process';
import { FileTree } from '../src/file-tree.js';
import { LockManager } from '../src/lock-manager.js';

test('FileTree: builds directory hierarchy and skips node_modules using git index [MD-027]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-filetree-test-'));

  try {
    execFileSync('git', ['init', '-b', 'main'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.name', 'Turf Tester'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.email', 'tester@turfcode.dev'], { cwd: tmpDir });

    // Create workspace structure
    fs.mkdirSync(path.join(tmpDir, 'src', 'controllers'), { recursive: true });
    fs.mkdirSync(path.join(tmpDir, 'node_modules', 'huge-pkg'), { recursive: true });

    fs.writeFileSync(path.join(tmpDir, 'package.json'), '{}');
    fs.writeFileSync(path.join(tmpDir, 'src', 'index.js'), 'export default {};');
    fs.writeFileSync(path.join(tmpDir, 'src', 'controllers', 'auth.js'), '// auth');
    fs.writeFileSync(path.join(tmpDir, 'node_modules', 'huge-pkg', 'index.js'), '// skipped');

    // Add .gitignore
    fs.writeFileSync(path.join(tmpDir, '.gitignore'), 'node_modules\n');
    execFileSync('git', ['add', '.'], { cwd: tmpDir });
    execFileSync('git', ['commit', '-m', 'Initial commit'], { cwd: tmpDir });

    const tree = new FileTree({ cwd: tmpDir });
    const files = tree.getFiles();

    assert.ok(files.includes('package.json'));
    assert.ok(files.includes('src/index.js'));
    assert.ok(files.includes('src/controllers/auth.js'));
    assert.equal(files.some((f) => f.includes('node_modules')), false, 'node_modules must be excluded from git index');

    const rendered = tree.render({ format: 'plain' });
    assert.ok(rendered.includes('src/'));
    assert.ok(rendered.includes('controllers/'));
    assert.ok(rendered.includes('auth.js'));
    assert.ok(rendered.includes('package.json'));
    assert.equal(rendered.includes('huge-pkg'), false);
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('FileTree: renders red padlock badges [LOCKED BY <USER>] beside locked files [MD-027]', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-filetree-lock-'));

  try {
    execFileSync('git', ['init', '-b', 'main'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.name', 'Turf Tester'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.email', 'tester@turfcode.dev'], { cwd: tmpDir });

    fs.mkdirSync(path.join(tmpDir, 'src'), { recursive: true });
    fs.writeFileSync(path.join(tmpDir, 'src', 'locked-file.js'), 'code');
    fs.writeFileSync(path.join(tmpDir, 'src', 'free-file.js'), 'code');

    execFileSync('git', ['add', '.'], { cwd: tmpDir });
    execFileSync('git', ['commit', '-m', 'commit'], { cwd: tmpDir });

    const lockManager = new LockManager({ cwd: tmpDir });
    const tree = new FileTree({ cwd: tmpDir, lockManager });

    // Initially free
    let renderedTags = tree.render({ format: 'tags' });
    assert.equal(renderedTags.includes('🔒 [LOCKED BY'), false);

    // Acquire lock for YUG on src/locked-file.js
    lockManager.acquireAtomicSequence('Yug', 'task-42', ['src/locked-file.js']);

    renderedTags = tree.render({ format: 'tags' });
    assert.ok(
      renderedTags.includes('{#ef4444-fg}🔒 [LOCKED BY YUG]{/#ef4444-fg}'),
      'Locked file must display bright red padlock badge in tags mode'
    );
    assert.ok(renderedTags.includes('locked-file.js'));

    const renderedPlain = tree.render({ format: 'plain' });
    assert.ok(
      renderedPlain.includes('locked-file.js 🔒 [LOCKED BY YUG]'),
      'Locked file must display padlock badge in plain mode'
    );

    // Free file must NOT have lock badge
    const freeFileLine = renderedPlain.split('\n').find((l) => l.includes('free-file.js'));
    assert.ok(freeFileLine);
    assert.equal(freeFileLine.includes('🔒'), false);

    // Release lock
    lockManager.releaseLocks('task-42');
    const afterRelease = tree.render({ format: 'plain' });
    assert.equal(afterRelease.includes('🔒 [LOCKED BY YUG]'), false);

    lockManager.destroy();
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});

test('FileTree: updates dynamically when LockManager state changes', () => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-filetree-dyn-'));

  try {
    execFileSync('git', ['init', '-b', 'main'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.name', 'Turf Tester'], { cwd: tmpDir });
    execFileSync('git', ['config', 'user.email', 'tester@turfcode.dev'], { cwd: tmpDir });

    fs.writeFileSync(path.join(tmpDir, 'server.js'), '// server');
    execFileSync('git', ['add', '.'], { cwd: tmpDir });
    execFileSync('git', ['commit', '-m', 'init'], { cwd: tmpDir });

    const lockManager = new LockManager({ cwd: tmpDir });
    const tree = new FileTree({ cwd: tmpDir, lockManager });

    lockManager.acquireAtomicSequence('Ayush', 'task-10', ['server.js']);
    const rendered = tree.render({ format: 'plain' });
    assert.ok(rendered.includes('🔒 [LOCKED BY AYUSH]'));

    lockManager.destroy();
  } finally {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  }
});
