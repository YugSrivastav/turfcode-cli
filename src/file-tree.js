/**
 * TurfCode CLI - Interactive File Tree Component
 *
 * Compact directory hierarchy component for the Intent Board.
 * Displays repository structure with bright red 🔒 [LOCKED BY <USER>] badges
 * beside files held by active locks.
 *
 * Complies with [MD-027]. Uses cached git index to avoid blocking on node_modules.
 */

import path from 'path';
import fs from 'fs';
import { execFileSync } from 'child_process';
import chalk from 'chalk';
import { normalizeLockPath } from './lock-manager.js';
import { PALETTE } from './theme.js';

export class FileTree {
  /**
   * @param {Object} [options]
   * @param {string} [options.cwd=process.cwd()] Repository base directory
   * @param {Object} [options.lockManager] LockManager instance
   * @param {number} [options.cacheTtlMs=1500] Cache duration for git index
   * @param {'tags'|'ansi'|'plain'} [options.format='tags'] Formatting style
   */
  constructor(options = {}) {
    this.cwd = options.cwd || process.cwd();
    this.lockManager = options.lockManager || null;
    this.cacheTtlMs = typeof options.cacheTtlMs === 'number' ? options.cacheTtlMs : 1500;
    this.format = options.format || 'tags';

    this._cachedFiles = null;
    this._lastCacheTime = 0;

    if (this.lockManager && typeof this.lockManager.on === 'function') {
      this.lockManager.on('change', () => {
        this.invalidateCache();
      });
    }
  }

  /**
   * Invalidate git index cache.
   */
  invalidateCache() {
    this._cachedFiles = null;
    this._lastCacheTime = 0;
  }

  /**
   * Query git index for files, excluding ignored items (e.g. node_modules) [MD-027].
   * @returns {string[]} Relative file paths
   */
  getFiles() {
    const now = Date.now();
    if (this._cachedFiles && now - this._lastCacheTime < this.cacheTtlMs) {
      return this._cachedFiles;
    }

    let files = [];
    try {
      // Query tracked files plus untracked non-ignored files from git index
      const stdout = execFileSync('git', ['ls-files', '--cached', '--others', '--exclude-standard'], {
        cwd: this.cwd,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'ignore'],
        timeout: 3000
      });

      files = stdout
        .split(/\r?\n/)
        .map((f) => f.trim().replace(/\\/g, '/'))
        .filter(Boolean);
    } catch {
      try {
        // Fallback: standard git ls-files
        const stdout = execFileSync('git', ['ls-files'], {
          cwd: this.cwd,
          encoding: 'utf8',
          stdio: ['ignore', 'pipe', 'ignore'],
          timeout: 3000
        });
        files = stdout
          .split(/\r?\n/)
          .map((f) => f.trim().replace(/\\/g, '/'))
          .filter(Boolean);
      } catch {
        // Fallback: basic fs scan excluding node_modules, .git, .turf
        files = this._scanDirFallback(this.cwd, '');
      }
    }

    this._cachedFiles = files;
    this._lastCacheTime = now;
    return files;
  }

  /**
   * Filesystem recursive scan fallback when git is not available.
   * @param {string} dir
   * @param {string} prefix
   * @returns {string[]}
   */
  _scanDirFallback(dir, prefix) {
    let result = [];
    try {
      const entries = fs.readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        if (
          entry.name === 'node_modules' ||
          entry.name === '.git' ||
          entry.name === '.turf' ||
          entry.name.startsWith('.')
        ) {
          continue;
        }

        const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
        if (entry.isDirectory()) {
          result = result.concat(this._scanDirFallback(path.join(dir, entry.name), rel));
        } else if (entry.isFile()) {
          result.push(rel);
        }
      }
    } catch {
      // ignore unreadable dirs
    }
    return result;
  }

  /**
   * Build hierarchical tree object from list of relative file paths.
   * @param {string[]} files
   * @returns {Object} Tree node hierarchy
   */
  buildHierarchy(files) {
    const root = {
      name: '',
      path: '',
      isDirectory: true,
      children: new Map()
    };

    for (const file of files) {
      const parts = file.split('/');
      let current = root;

      for (let i = 0; i < parts.length; i++) {
        const part = parts[i];
        const isDir = i < parts.length - 1;
        const currentPath = parts.slice(0, i + 1).join('/');

        if (!current.children.has(part)) {
          current.children.set(part, {
            name: part,
            path: currentPath,
            isDirectory: isDir,
            children: isDir ? new Map() : null
          });
        }
        current = current.children.get(part);
      }
    }

    return root;
  }

  /**
   * Render file tree with lock badges.
   * @param {Object} [options]
   * @param {'tags'|'ansi'|'plain'} [options.format]
   * @param {Array} [options.locks] Optional explicit lock array override
   * @returns {string} Formatted directory tree string
   */
  render(options = {}) {
    const format = options.format || this.format;
    const files = this.getFiles();

    if (!files || files.length === 0) {
      if (format === 'ansi') {
        return chalk.hex('#4a7c4a')('(No files in workspace)');
      }
      return `{#4a7c4a-fg}(No files in workspace){/#4a7c4a-fg}`;
    }

    // Index active locks by normalized path
    const lockMap = new Map();
    const activeLocks = options.locks || (this.lockManager ? this.lockManager.getLocks() : []);
    for (const lock of activeLocks) {
      const norm = normalizeLockPath(lock.filePath || lock.file, this.cwd);
      if (norm) {
        lockMap.set(norm, lock);
      }
    }

    const root = this.buildHierarchy(files);
    const lines = [];

    const formatNode = (node, prefix = '', isLast = true, depth = 0) => {
      if (depth > 0) {
        const branch = isLast ? '└── ' : '├── ';
        const itemPrefix = prefix + branch;

        if (node.isDirectory) {
          let dirLabel = `${node.name}/`;
          if (format === 'tags') {
            dirLabel = `{#39ff14-fg}📁 ${node.name}/{/#39ff14-fg}`;
          } else if (format === 'ansi') {
            dirLabel = chalk.hex(PALETTE.electricLime)(`📁 ${node.name}/`);
          }
          lines.push(`${itemPrefix}${dirLabel}`);
        } else {
          // File node - check for active lock [MD-027]
          const lock = lockMap.get(normalizeLockPath(node.path, this.cwd));
          let fileLabel = node.name;
          let lockBadge = '';

          if (lock) {
            const holder = (lock.ownerId || lock.holder || 'UNKNOWN').toUpperCase();
            if (format === 'tags') {
              lockBadge = ` {#ef4444-fg}🔒 [LOCKED BY ${holder}]{/#ef4444-fg}`;
            } else if (format === 'ansi') {
              lockBadge = ` ${chalk.hex('#ef4444')(`🔒 [LOCKED BY ${holder}]`)}`;
            } else {
              lockBadge = ` 🔒 [LOCKED BY ${holder}]`;
            }
          }

          if (format === 'tags') {
            fileLabel = `{#f0fff0-fg}${node.name}{/#f0fff0-fg}`;
          } else if (format === 'ansi') {
            fileLabel = chalk.hex(PALETTE.textBright)(node.name);
          }

          lines.push(`${itemPrefix}${fileLabel}${lockBadge}`);
        }
      }

      if (node.isDirectory && node.children) {
        const childEntries = Array.from(node.children.values());
        // Sort: directories first, then files
        childEntries.sort((a, b) => {
          if (a.isDirectory && !b.isDirectory) return -1;
          if (!a.isDirectory && b.isDirectory) return 1;
          return a.name.localeCompare(b.name);
        });

        const nextPrefix = depth === 0 ? '' : prefix + (isLast ? '    ' : '│   ');
        for (let i = 0; i < childEntries.length; i++) {
          const isChildLast = i === childEntries.length - 1;
          formatNode(childEntries[i], nextPrefix, isChildLast, depth + 1);
        }
      }
    };

    formatNode(root, '', true, 0);
    return lines.join('\n');
  }

  /**
   * Render file tree into a Blessed widget.
   * @param {Object} box Blessed box widget
   * @param {Object} [options]
   */
  renderToBox(box, options = {}) {
    if (!box) return;
    const content = this.render({ ...options, format: 'tags' });
    if (typeof box.setContent === 'function') {
      box.setContent(`\n${content}`);
    }
    if (box.screen) {
      box.screen.render();
    }
  }
}
