/**
 * TurfCode CLI - Worktree Manager
 *
 * Wraps git worktree operations to isolate concurrent agent runs under
 * .turf/worktrees/<task-id> and fast-forward squash-merges completed tasks
 * into main with git Co-authored-by trailers.
 *
 * Complies with [MD-012], [MD-022], [MD-033], [MD-039].
 */

import fs from 'fs';
import path from 'path';
import { execFileSync } from 'child_process';

/**
 * Format Co-authored-by trailers according to [MD-012].
 * @param {Array<string|Object>|string} coAuthors
 * @returns {string[]}
 */
export function formatCoAuthors(coAuthors) {
  if (!coAuthors) return [];
  const list = Array.isArray(coAuthors) ? coAuthors : [coAuthors];
  const trailers = [];

  for (const item of list) {
    if (!item) continue;
    if (typeof item === 'string') {
      const trimmed = item.trim();
      if (!trimmed) continue;
      if (trimmed.toLowerCase().startsWith('co-authored-by:')) {
        trailers.push(trimmed);
      } else if (trimmed.includes('<') && trimmed.includes('>')) {
        trailers.push(`Co-authored-by: ${trimmed}`);
      } else {
        const emailSafe = trimmed.toLowerCase().replace(/[^a-z0-9]/g, '.');
        trailers.push(`Co-authored-by: ${trimmed} <${emailSafe}@turfcode.local>`);
      }
    } else if (typeof item === 'object') {
      const name = item.name || 'Collaborator';
      const email = item.email || `${name.toLowerCase().replace(/[^a-z0-9]/g, '.')}@turfcode.local`;
      trailers.push(`Co-authored-by: ${name} <${email}>`);
    }
  }

  return trailers;
}

export class WorktreeManager {
  /**
   * @param {Object} options
   * @param {string} [options.repoRoot] Root directory of the repository (default process.cwd())
   * @param {string} [options.worktreesDir] Base path for worktrees (default <repoRoot>/.turf/worktrees)
   * @param {string} [options.baseBranch] Target branch to branch from / merge into
   * @param {string} [options.gitBinary] Git executable path (default 'git')
   */
  constructor(options = {}) {
    this.repoRoot = path.resolve(options.repoRoot || process.cwd());
    this.worktreesDir = path.resolve(options.worktreesDir || path.join(this.repoRoot, '.turf', 'worktrees'));
    this.gitBinary = options.gitBinary || 'git';

    // Auto-detect base branch if not explicitly provided
    if (options.baseBranch) {
      this.baseBranch = options.baseBranch;
    } else {
      try {
        this.baseBranch = this.runGit(['rev-parse', '--abbrev-ref', 'HEAD']).trim() || 'main';
      } catch {
        this.baseBranch = 'main';
      }
    }
  }

  /**
   * Execute git command synchronously within repoRoot or a custom directory.
   * @param {string[]} args
   * @param {string} [cwd]
   * @returns {string} stdout
   */
  runGit(args, cwd = this.repoRoot) {
    return execFileSync(this.gitBinary, args, {
      cwd,
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'pipe'],
      env: {
        ...process.env,
        GIT_TERMINAL_PROMPT: '0'
      }
    });
  }

  /**
   * Get resolved filesystem path for a task worktree.
   * @param {string|number} taskId
   * @returns {string}
   */
  getWorktreePath(taskId) {
    return path.join(this.worktreesDir, String(taskId));
  }

  /**
   * Creates an isolated worktree under .turf/worktrees/<taskId>.
   * Complies with [MD-022].
   *
   * @param {string|number} taskId
   * @param {string} [branchName] Branch name (default: turf/task-<taskId>)
   * @returns {{ taskId: string, branchName: string, worktreePath: string }}
   */
  createTaskWorktree(taskId, branchName) {
    const id = String(taskId);
    const branch = branchName || (id.startsWith('task-') ? `turf/${id}` : `turf/task-${id}`);
    const worktreePath = this.getWorktreePath(id);

    if (!fs.existsSync(this.worktreesDir)) {
      fs.mkdirSync(this.worktreesDir, { recursive: true });
    }

    // If worktree directory already exists and is registered, return existing
    if (fs.existsSync(worktreePath)) {
      const activeList = this.listWorktrees();
      const existing = activeList.find((wt) => path.resolve(wt.path) === path.resolve(worktreePath));
      if (existing) {
        return {
          taskId: id,
          branchName: existing.branch || branch,
          worktreePath
        };
      } else {
        // Clean orphaned directory if not an active worktree
        fs.rmSync(worktreePath, { recursive: true, force: true });
      }
    }

    // Check if the branch already exists in local repo
    let branchExists = false;
    try {
      const branchOutput = this.runGit(['branch', '--list', branch]).trim();
      branchExists = branchOutput.length > 0;
    } catch {
      branchExists = false;
    }

    if (branchExists) {
      this.runGit(['worktree', 'add', worktreePath, branch]);
    } else {
      // Determine commit-ish to branch from
      let startPoint = this.baseBranch;
      try {
        this.runGit(['rev-parse', '--verify', startPoint]);
      } catch {
        startPoint = 'HEAD';
      }
      this.runGit(['worktree', 'add', '-b', branch, worktreePath, startPoint]);
    }

    return {
      taskId: id,
      branchName: branch,
      worktreePath
    };
  }

  /**
   * Fast-forward auto-squash merge completed task worktree into main.
   * Includes Co-authored-by trailers.
   * Complies with [MD-012], [MD-033], [MD-039].
   *
   * @param {string|number} taskId
   * @param {string} commitMessage
   * @param {Array<string|Object>} [coAuthors=[]]
   * @param {Object} [options]
   * @param {string} [options.branchName]
   * @param {string} [options.targetBranch] Target branch to merge into (default baseBranch)
   * @returns {{ success: boolean, commitHash?: string, noChanges?: boolean, taskId: string, branch: string, targetBranch: string }}
   */
  mergeTaskWorktree(taskId, commitMessage, coAuthors = [], options = {}) {
    const id = String(taskId);
    const branch = options.branchName || (id.startsWith('task-') ? `turf/${id}` : `turf/task-${id}`);
    const targetBranch = options.targetBranch || this.baseBranch || 'main';

    // Verify current branch in host repository
    const currentBranch = this.runGit(['rev-parse', '--abbrev-ref', 'HEAD']).trim();
    if (currentBranch !== targetBranch) {
      this.runGit(['checkout', targetBranch]);
    }

    // Perform squash merge
    try {
      this.runGit(['merge', '--squash', branch]);
    } catch (err) {
      // On conflict or merge error, abort/reset to keep tree clean
      try {
        this.runGit(['reset', '--hard', 'HEAD']);
      } catch {}
      throw new Error(`[MERGE CONFLICT] Failed to squash-merge worktree ${branch} into ${targetBranch}: ${err.message}`);
    }

    // Check if any changes are staged
    const status = this.runGit(['status', '--porcelain']).trim();
    if (!status) {
      return {
        success: true,
        noChanges: true,
        taskId: id,
        branch,
        targetBranch
      };
    }

    // Assemble commit message with Co-authored-by trailers
    let finalMessage = (commitMessage || `Auto-squash task #${id}`).trim();
    const trailers = formatCoAuthors(coAuthors);
    if (trailers.length > 0) {
      finalMessage += `\n\n${trailers.join('\n')}`;
    }

    // Commit on host repo with fallback author if not set in git config
    try {
      this.runGit([
        '-c', 'user.name=TurfCode Host',
        '-c', 'user.email=host@turfcode.local',
        'commit',
        '-m', finalMessage
      ]);
    } catch (err) {
      throw new Error(`[COMMIT ERROR] Failed to commit auto-squash merge: ${err.message}`);
    }

    const commitHash = this.runGit(['rev-parse', 'HEAD']).trim();

    return {
      success: true,
      commitHash,
      taskId: id,
      branch,
      targetBranch
    };
  }

  /**
   * Cleanly prunes and removes the worktree directory.
   * Complies with [MD-022], [MD-033].
   *
   * @param {string|number} taskId
   * @param {Object} [options]
   * @param {boolean} [options.deleteBranch=true] Whether to delete the task branch
   * @param {string} [options.branchName]
   * @returns {{ success: boolean, taskId: string, worktreePath: string }}
   */
  removeTaskWorktree(taskId, options = {}) {
    const id = String(taskId);
    const branch = options.branchName || (id.startsWith('task-') ? `turf/${id}` : `turf/task-${id}`);
    const worktreePath = this.getWorktreePath(id);

    if (fs.existsSync(worktreePath)) {
      try {
        this.runGit(['worktree', 'remove', '--force', worktreePath]);
      } catch {
        // Fallback: manually delete directory if git worktree remove encounters issue
      }
    }

    try {
      this.runGit(['worktree', 'prune']);
    } catch {}

    if (fs.existsSync(worktreePath)) {
      fs.rmSync(worktreePath, { recursive: true, force: true });
    }

    if (options.deleteBranch !== false) {
      try {
        this.runGit(['branch', '-D', branch]);
      } catch {
        // Ignore if branch doesn't exist
      }
    }

    return {
      success: true,
      taskId: id,
      worktreePath
    };
  }

  /**
   * List all registered git worktrees in repo.
   * @returns {Array<{ path: string, commit: string, branch: string }>}
   */
  listWorktrees() {
    try {
      const output = this.runGit(['worktree', 'list', '--porcelain']);
      const lines = output.split('\n');
      const worktrees = [];
      let current = {};

      for (const line of lines) {
        if (line.startsWith('worktree ')) {
          if (current.path) worktrees.push(current);
          current = { path: line.substring(9).trim() };
        } else if (line.startsWith('HEAD ')) {
          current.commit = line.substring(5).trim();
        } else if (line.startsWith('branch ')) {
          current.branch = line.substring(7).replace('refs/heads/', '').trim();
        }
      }
      if (current.path) worktrees.push(current);

      return worktrees;
    } catch {
      return [];
    }
  }

  /**
   * Check if a task worktree exists.
   * @param {string|number} taskId
   * @returns {boolean}
   */
  hasWorktree(taskId) {
    const wtPath = this.getWorktreePath(taskId);
    return fs.existsSync(wtPath);
  }
}
