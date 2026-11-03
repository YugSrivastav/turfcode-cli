import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'fs';
import path from 'path';
import os from 'os';
import { execFileSync } from 'child_process';
import { WorktreeManager, formatCoAuthors } from '../src/worktree-manager.js';

/**
 * Helper to initialize a disposable scratch git repository in a temp directory.
 * Keeps all git operations strictly isolated from the host pool worktree [Rule 7].
 */
function createTempGitRepo() {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'turf-wt-repo-'));
  
  execFileSync('git', ['init', '-b', 'main'], { cwd: tmpDir, stdio: 'pipe' });
  execFileSync('git', ['config', 'user.name', 'Host Dev'], { cwd: tmpDir, stdio: 'pipe' });
  execFileSync('git', ['config', 'user.email', 'host@turfcode.local'], { cwd: tmpDir, stdio: 'pipe' });

  // Initial commit so HEAD exists
  fs.writeFileSync(path.join(tmpDir, 'README.md'), '# Test Project\n', 'utf8');
  execFileSync('git', ['add', '.'], { cwd: tmpDir, stdio: 'pipe' });
  execFileSync('git', ['commit', '-m', 'Initial commit'], { cwd: tmpDir, stdio: 'pipe' });

  return tmpDir;
}

test('formatCoAuthors formats git trailers according to [MD-012]', () => {
  assert.deepEqual(formatCoAuthors([]), []);
  assert.deepEqual(formatCoAuthors(null), []);

  const res1 = formatCoAuthors(['Ayush <ayush@example.com>']);
  assert.deepEqual(res1, ['Co-authored-by: Ayush <ayush@example.com>']);

  const res2 = formatCoAuthors([
    { name: 'Ayush', email: 'ayush@example.com' },
    { name: 'Yug', email: 'yug@example.com' }
  ]);
  assert.deepEqual(res2, [
    'Co-authored-by: Ayush <ayush@example.com>',
    'Co-authored-by: Yug <yug@example.com>'
  ]);

  const res3 = formatCoAuthors(['Ayush']);
  assert.deepEqual(res3, ['Co-authored-by: Ayush <ayush@turfcode.local>']);

  const res4 = formatCoAuthors(['Co-authored-by: Ayush <ayush@example.com>']);
  assert.deepEqual(res4, ['Co-authored-by: Ayush <ayush@example.com>']);
});

test('WorktreeManager: createTaskWorktree creates isolated worktree under .turf/worktrees/<taskId> [MD-022]', () => {
  const repoDir = createTempGitRepo();
  const manager = new WorktreeManager({ repoRoot: repoDir, baseBranch: 'main' });

  try {
    const wt = manager.createTaskWorktree('task-101');
    assert.equal(wt.taskId, 'task-101');
    assert.equal(wt.branchName, 'turf/task-101');
    assert.ok(fs.existsSync(wt.worktreePath), 'Worktree directory must exist on disk');

    const worktrees = manager.listWorktrees();
    const found = worktrees.some((w) => path.resolve(w.path) === path.resolve(wt.worktreePath));
    assert.ok(found, 'Worktree must be registered with git worktree list');
  } finally {
    manager.removeTaskWorktree('task-101');
    fs.rmSync(repoDir, { recursive: true, force: true });
  }
});

test('WorktreeManager: mergeTaskWorktree auto-squashes into main with Co-authored-by trailers [MD-012], [MD-033]', () => {
  const repoDir = createTempGitRepo();
  const manager = new WorktreeManager({ repoRoot: repoDir, baseBranch: 'main' });

  try {
    const wt = manager.createTaskWorktree('task-202');

    // Simulate agent work in the isolated worktree
    const agentFilePath = path.join(wt.worktreePath, 'agent-output.txt');
    fs.writeFileSync(agentFilePath, 'Agent generated code\n', 'utf8');

    execFileSync('git', ['add', '.'], { cwd: wt.worktreePath, stdio: 'pipe' });
    execFileSync('git', ['commit', '-m', 'Agent commit inside worktree'], { cwd: wt.worktreePath, stdio: 'pipe' });

    // Host fast-forward squash-merges into main with Co-authored-by trailer
    const mergeRes = manager.mergeTaskWorktree(
      'task-202',
      'feat: add agent-output.txt',
      ['Ayush <ayush@example.com>']
    );

    assert.equal(mergeRes.success, true);
    assert.ok(mergeRes.commitHash, 'Commit hash must be returned');

    // Verify file exists on main branch
    const mainFilePath = path.join(repoDir, 'agent-output.txt');
    assert.ok(fs.existsSync(mainFilePath), 'Merged file must exist in main repo');

    // Verify git log contains Co-authored-by trailer
    const logOutput = execFileSync('git', ['log', '-1', '--pretty=%B'], { cwd: repoDir, encoding: 'utf8' });
    assert.ok(logOutput.includes('feat: add agent-output.txt'));
    assert.ok(logOutput.includes('Co-authored-by: Ayush <ayush@example.com>'));
  } finally {
    manager.removeTaskWorktree('task-202');
    fs.rmSync(repoDir, { recursive: true, force: true });
  }
});

test('WorktreeManager: removeTaskWorktree cleanly prunes and deletes directory [MD-022], [MD-033]', () => {
  const repoDir = createTempGitRepo();
  const manager = new WorktreeManager({ repoRoot: repoDir, baseBranch: 'main' });

  try {
    const wt = manager.createTaskWorktree('task-303');
    assert.ok(fs.existsSync(wt.worktreePath));

    const removeRes = manager.removeTaskWorktree('task-303');
    assert.equal(removeRes.success, true);
    assert.equal(fs.existsSync(wt.worktreePath), false, 'Directory must be deleted');

    const worktrees = manager.listWorktrees();
    const found = worktrees.some((w) => path.resolve(w.path) === path.resolve(wt.worktreePath));
    assert.equal(found, false, 'Worktree must no longer be in git worktree list');
  } finally {
    fs.rmSync(repoDir, { recursive: true, force: true });
  }
});

test('WorktreeManager: mergeTaskWorktree handles merge conflicts cleanly', () => {
  const repoDir = createTempGitRepo();
  const manager = new WorktreeManager({ repoRoot: repoDir, baseBranch: 'main' });

  try {
    const wt = manager.createTaskWorktree('task-404');

    // Make change on worktree branch
    fs.writeFileSync(path.join(wt.worktreePath, 'conflict.txt'), 'Worktree content\n', 'utf8');
    execFileSync('git', ['add', '.'], { cwd: wt.worktreePath, stdio: 'pipe' });
    execFileSync('git', ['commit', '-m', 'Worktree commit'], { cwd: wt.worktreePath, stdio: 'pipe' });

    // Make conflicting change on main
    fs.writeFileSync(path.join(repoDir, 'conflict.txt'), 'Main conflicting content\n', 'utf8');
    execFileSync('git', ['add', '.'], { cwd: repoDir, stdio: 'pipe' });
    execFileSync('git', ['commit', '-m', 'Main conflict commit'], { cwd: repoDir, stdio: 'pipe' });

    // Attempting squash-merge must throw diagnostic error and reset cleanly
    assert.throws(() => {
      manager.mergeTaskWorktree('task-404', 'Merge conflicting task');
    }, /\[MERGE CONFLICT\]/);

    // Verify main working tree remains clean
    const status = execFileSync('git', ['status', '--porcelain'], { cwd: repoDir, encoding: 'utf8' }).trim();
    assert.equal(status, '', 'Main repo status must be clean after failed merge');
  } finally {
    manager.removeTaskWorktree('task-404');
    fs.rmSync(repoDir, { recursive: true, force: true });
  }
});
