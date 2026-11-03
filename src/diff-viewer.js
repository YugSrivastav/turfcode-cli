/**
 * TurfCode CLI - Live Diff Viewer Component
 *
 * Generates unified colorized git diff of uncommitted changes.
 * Uses green for additions (#39FF14), red for deletions (#ff3333), and cyan for file headers.
 * Filters out binary files and files > 1MB without blocking the event loop.
 *
 * Complies with [MD-016] and [MD-035].
 */

import fs from 'fs';
import path from 'path';
import { execFile } from 'child_process';
import { promisify } from 'util';
import chalk from 'chalk';
import { PALETTE } from './theme.js';

const execFileAsync = promisify(execFile);

export const DEFAULT_MAX_FILE_SIZE = 1024 * 1024; // 1MB [MD-035]

export const BINARY_EXTENSIONS = new Set([
  '.png', '.jpg', '.jpeg', '.gif', '.bmp', '.ico', '.webp', '.tiff', '.svgz',
  '.pdf', '.zip', '.tar', '.gz', '.tgz', '.bz2', '.7z', '.rar',
  '.exe', '.bin', '.dll', '.so', '.dylib', '.node',
  '.woff', '.woff2', '.ttf', '.eot', '.otf',
  '.mp4', '.mp3', '.wav', '.ogg', '.mov', '.avi', '.flv', '.mkv',
  '.class', '.jar', '.pyc', '.wasm', '.lockb'
]);

/**
 * Check if a file path is a known binary file extension.
 * @param {string} filePath
 * @returns {boolean}
 */
export function isBinaryFile(filePath) {
  if (!filePath || typeof filePath !== 'string') return false;
  const ext = path.extname(filePath).toLowerCase();
  return BINARY_EXTENSIONS.has(ext);
}

/**
 * Check if a file exceeds the size limit.
 * @param {string} filePath
 * @param {string} [cwd=process.cwd()]
 * @param {number} [maxSize=DEFAULT_MAX_FILE_SIZE]
 * @returns {boolean}
 */
export function isLargeFile(filePath, cwd = process.cwd(), maxSize = DEFAULT_MAX_FILE_SIZE) {
  try {
    const fullPath = path.isAbsolute(filePath) ? filePath : path.join(cwd, filePath);
    if (!fs.existsSync(fullPath)) return false;
    const stat = fs.statSync(fullPath);
    return stat.size > maxSize;
  } catch {
    return false;
  }
}

/**
 * Colorize a unified git diff text string.
 * Uses green (#39FF14) for additions, red (#ff3333) for deletions, and cyan for headers.
 *
 * @param {string} diffText Raw git diff output
 * @param {Object} [options]
 * @param {'tags'|'ansi'} [options.format='tags']
 * @returns {string} Colorized diff string
 */
export function colorizeDiff(diffText, options = {}) {
  if (!diffText || typeof diffText !== 'string') return '';
  const format = options.format || 'tags';

  const lines = diffText.split(/\r?\n/);
  const colorizedLines = lines.map((line) => {
    // 1. File and chunk header lines (cyan)
    const isHeader =
      line.startsWith('diff --git') ||
      line.startsWith('index ') ||
      line.startsWith('--- ') ||
      line.startsWith('+++ ') ||
      line.startsWith('@@ ') ||
      line.startsWith('Binary files ') ||
      line.startsWith('new file mode ') ||
      line.startsWith('deleted file mode ') ||
      line.startsWith('similarity index ');

    if (isHeader) {
      if (format === 'ansi') {
        return chalk.hex('#06b6d4')(line);
      }
      return `{#06b6d4-fg}${line}{/#06b6d4-fg}`;
    }

    // 2. Additions (green #39FF14)
    if (line.startsWith('+') && !line.startsWith('+++')) {
      if (format === 'ansi') {
        return chalk.hex(PALETTE.electricLime)(line);
      }
      return `{#39ff14-fg}${line}{/#39ff14-fg}`;
    }

    // 3. Deletions (red #ff3333)
    if (line.startsWith('-') && !line.startsWith('---')) {
      if (format === 'ansi') {
        return chalk.hex('#ff3333')(line);
      }
      return `{#ff3333-fg}${line}{/#ff3333-fg}`;
    }

    // 4. Warning / notice lines
    if (line.startsWith('[DIFF NOTICE]') || line.startsWith('[Binary file') || line.startsWith('[File >')) {
      if (format === 'ansi') {
        return chalk.hex('#eab308')(line);
      }
      return `{#eab308-fg}${line}{/#eab308-fg}`;
    }

    // 5. Context / unmodified lines
    if (format === 'ansi') {
      return chalk.hex(PALETTE.textBright)(line);
    }
    return `{#f0fff0-fg}${line}{/#f0fff0-fg}`;
  });

  return colorizedLines.join('\n');
}

export class DiffViewer {
  /**
   * @param {Object} [options]
   * @param {string} [options.cwd=process.cwd()] Repository working directory
   * @param {number} [options.maxFileSize=DEFAULT_MAX_FILE_SIZE] Max file size to diff (bytes)
   * @param {'tags'|'ansi'} [options.format='tags'] Output formatting mode
   */
  constructor(options = {}) {
    this.cwd = options.cwd || process.cwd();
    this.maxFileSize = typeof options.maxFileSize === 'number' ? options.maxFileSize : DEFAULT_MAX_FILE_SIZE;
    this.format = options.format || 'tags';
    this.lastDiff = '';
    this.lastSummary = null;
  }

  /**
   * Get uncommitted modified file paths from git without blocking the event loop.
   * @returns {Promise<{allFiles: string[], diffFiles: string[], ignoredFiles: string[]}>}
   */
  async getModifiedFiles() {
    let rawOutput = '';
    try {
      // Check changes against HEAD (both staged and unstaged)
      const res = await execFileAsync('git', ['diff', '--name-only', 'HEAD'], {
        cwd: this.cwd,
        maxBuffer: 5 * 1024 * 1024
      });
      rawOutput = res.stdout;
    } catch {
      try {
        // Fallback if HEAD does not exist (e.g. empty repo)
        const res = await execFileAsync('git', ['diff', '--name-only'], {
          cwd: this.cwd,
          maxBuffer: 5 * 1024 * 1024
        });
        rawOutput = res.stdout;
      } catch {
        return { allFiles: [], diffFiles: [], ignoredFiles: [] };
      }
    }

    const files = rawOutput
      .split(/\r?\n/)
      .map((f) => f.trim())
      .filter(Boolean);

    const diffFiles = [];
    const ignoredFiles = [];

    for (const f of files) {
      if (isBinaryFile(f)) {
        ignoredFiles.push({ file: f, reason: 'binary' });
      } else if (isLargeFile(f, this.cwd, this.maxFileSize)) {
        ignoredFiles.push({ file: f, reason: 'exceeds_1mb' });
      } else {
        diffFiles.push(f);
      }
    }

    return {
      allFiles: files,
      diffFiles,
      ignoredFiles
    };
  }

  /**
   * Generate unified colorized git diff of uncommitted changes.
   * Ignores binary files and files > 1MB [MD-016], [MD-035].
   *
   * @param {Object} [options]
   * @param {'tags'|'ansi'} [options.format]
   * @param {string[]} [options.files] Optional explicit file list
   * @returns {Promise<{diff: string, colorized: string, fileCount: number, ignoredCount: number}>}
   */
  async getUncommittedDiff(options = {}) {
    const format = options.format || this.format;
    const modified = await this.getModifiedFiles();
    const targetFiles = options.files || modified.diffFiles;

    const notices = [];
    if (modified.ignoredFiles.length > 0) {
      for (const item of modified.ignoredFiles) {
        if (item.reason === 'binary') {
          notices.push(`[Binary file ignored: ${item.file}]`);
        } else if (item.reason === 'exceeds_1mb') {
          notices.push(`[File > 1MB ignored: ${item.file}]`);
        }
      }
    }

    if (targetFiles.length === 0) {
      const noticeText = notices.length > 0 ? `\n\n${notices.join('\n')}` : '';
      const emptyMsg = `(No uncommitted text changes in working tree)${noticeText}`;
      const colorized = colorizeDiff(emptyMsg, { format });
      this.lastDiff = emptyMsg;
      return {
        diff: emptyMsg,
        colorized,
        fileCount: 0,
        ignoredCount: modified.ignoredFiles.length
      };
    }

    let rawDiff = '';
    try {
      const gitArgs = ['diff', 'HEAD', '--', ...targetFiles];
      const res = await execFileAsync('git', gitArgs, {
        cwd: this.cwd,
        maxBuffer: 10 * 1024 * 1024
      });
      rawDiff = res.stdout;
    } catch {
      try {
        const gitArgs = ['diff', '--', ...targetFiles];
        const res = await execFileAsync('git', gitArgs, {
          cwd: this.cwd,
          maxBuffer: 10 * 1024 * 1024
        });
        rawDiff = res.stdout;
      } catch (err) {
        rawDiff = `Error generating git diff: ${err.message}`;
      }
    }

    if (notices.length > 0) {
      rawDiff = `${notices.join('\n')}\n\n${rawDiff}`;
    }

    const colorized = colorizeDiff(rawDiff, { format });
    this.lastDiff = rawDiff;
    this.lastSummary = {
      fileCount: targetFiles.length,
      ignoredCount: modified.ignoredFiles.length
    };

    return {
      diff: rawDiff,
      colorized,
      fileCount: targetFiles.length,
      ignoredCount: modified.ignoredFiles.length
    };
  }

  /**
   * Render diff content directly into a Blessed box/log widget.
   * @param {Object} box Blessed box or log widget
   * @param {Object} [options]
   * @returns {Promise<void>}
   */
  async renderToBox(box, options = {}) {
    if (!box) return;
    const res = await this.getUncommittedDiff(options);
    if (typeof box.setContent === 'function') {
      box.setContent(res.colorized);
    } else if (typeof box.log === 'function') {
      box.log(res.colorized);
    }
    if (box.screen) {
      box.screen.render();
    }
  }
}
