/**
 * TurfCode CLI - Intent Board Component & Task State Machine
 *
 * Manages shared tasks conforming to [MD-037], serializes state atomically to
 * .turf/intent.json [MD-014], formats task cards with status badges
 * and active lock indicators, and integrates cleanly into Cockpit sidebar.
 */

import fs from 'fs';
import path from 'path';
import { EventEmitter } from 'events';
import { TAGS } from './theme.js';
import { normalizeLockPath } from './lock-manager.js';

/**
 * Validate and normalize task status according to [MD-037].
 * @param {string} status
 * @returns {'queued'|'in_progress'|'done'}
 */
export function normalizeTaskStatus(status) {
  if (!status || typeof status !== 'string') return 'queued';
  const s = status.toLowerCase().trim().replace(/[-\s]/g, '_');
  if (s === 'in_progress' || s === 'inprogress' || s === 'progress') {
    return 'in_progress';
  }
  if (s === 'done' || s === 'completed') {
    return 'done';
  }
  return 'queued';
}

/**
 * Formats status for badge display.
 * @param {string} status
 * @returns {string}
 */
export function formatStatusLabel(status) {
  const norm = normalizeTaskStatus(status);
  switch (norm) {
    case 'in_progress':
      return 'IN PROGRESS';
    case 'done':
      return 'DONE';
    case 'queued':
    default:
      return 'QUEUED';
  }
}

export class IntentBoard extends EventEmitter {
  /**
   * @param {Object} options
   * @param {string} [options.cwd]
   * @param {string} [options.intentPath] Path to .turf/intent.json
   * @param {Array} [options.initialTasks] Initial task list
   * @param {Object} [options.lockManager] Associated LockManager instance
   */
  constructor(options = {}) {
    super();
    this.cwd = options.cwd || process.cwd();
    this.intentPath = options.intentPath || path.join(this.cwd, '.turf', 'intent.json');
    this.lockManager = options.lockManager || null;
    this.tasks = [];

    if (Array.isArray(options.initialTasks) && options.initialTasks.length > 0) {
      this.tasks = options.initialTasks.map((t) => this._validateAndNormalizeTask(t));
      this.save();
    } else {
      this.load();
    }
  }

  /**
   * Validate and conform task object to [MD-037] minimalist schema:
   * { id: string, title: string, assignee: string, status: 'queued'|'in_progress'|'done', targetFiles: string[] }
   * @param {Object} rawTask
   * @returns {Object}
   */
  _validateAndNormalizeTask(rawTask) {
    if (!rawTask || typeof rawTask !== 'object') {
      throw new Error('Task must be an object');
    }

    const id = String(rawTask.id ?? Date.now());
    const title = String(rawTask.title || 'Untitled Task').trim();
    const assignee = String(rawTask.assignee || 'Unassigned').trim();
    const status = normalizeTaskStatus(rawTask.status);

    let targetFiles = [];
    if (Array.isArray(rawTask.targetFiles)) {
      targetFiles = rawTask.targetFiles
        .map((f) => normalizeLockPath(f, this.cwd))
        .filter(Boolean);
    }

    return {
      id,
      title,
      assignee,
      status,
      targetFiles
    };
  }

  /**
   * Atomically save intent state to .turf/intent.json
   * Complies with [MD-014].
   */
  save() {
    try {
      const dir = path.dirname(this.intentPath);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }

      const payload = JSON.stringify({
        version: 1,
        updatedAt: new Date().toISOString(),
        tasks: this.tasks
      }, null, 2);

      const tempFile = `${this.intentPath}.tmp.${Date.now()}.${Math.random().toString(36).slice(2)}`;
      fs.writeFileSync(tempFile, payload, 'utf8');
      fs.renameSync(tempFile, this.intentPath);
    } catch (err) {
      this.emit('error', err);
    }
  }

  /**
   * Load intent tasks from .turf/intent.json
   */
  load() {
    try {
      if (fs.existsSync(this.intentPath)) {
        const raw = fs.readFileSync(this.intentPath, 'utf8');
        const parsed = JSON.parse(raw);
        const taskList = Array.isArray(parsed) ? parsed : (Array.isArray(parsed.tasks) ? parsed.tasks : []);
        this.tasks = taskList.map((t) => this._validateAndNormalizeTask(t));
      }
    } catch (err) {
      this.emit('error', err);
    }
  }

  /**
   * Add a new task to the Intent Board.
   * @param {Object} task
   * @returns {Object} Added task
   */
  addTask(task) {
    const normalized = this._validateAndNormalizeTask(task);
    
    // Check if task ID already exists
    const existingIndex = this.tasks.findIndex((t) => t.id === normalized.id);
    if (existingIndex >= 0) {
      this.tasks[existingIndex] = normalized;
    } else {
      this.tasks.push(normalized);
    }

    this.save();
    this.emit('change', this.getTasks());
    return normalized;
  }

  /**
   * Update an existing task.
   * @param {string|number} taskId
   * @param {Object} updates
   * @returns {Object|null} Updated task or null
   */
  updateTask(taskId, updates = {}) {
    const tId = String(taskId);
    const task = this.tasks.find((t) => t.id === tId);
    if (!task) return null;

    if (updates.title !== undefined) task.title = String(updates.title).trim();
    if (updates.assignee !== undefined) task.assignee = String(updates.assignee).trim();
    if (updates.status !== undefined) task.status = normalizeTaskStatus(updates.status);
    if (updates.targetFiles !== undefined && Array.isArray(updates.targetFiles)) {
      task.targetFiles = updates.targetFiles
        .map((f) => normalizeLockPath(f, this.cwd))
        .filter(Boolean);
    }

    this.save();
    this.emit('change', this.getTasks());
    return task;
  }

  /**
   * Remove a task by ID.
   * @param {string|number} taskId
   * @returns {Object|null} Removed task or null
   */
  removeTask(taskId) {
    const tId = String(taskId);
    const index = this.tasks.findIndex((t) => t.id === tId);
    if (index === -1) return null;

    const [removed] = this.tasks.splice(index, 1);
    this.save();
    this.emit('change', this.getTasks());
    return removed;
  }

  /**
   * Get all tasks (cloned array).
   * @returns {Array}
   */
  getTasks() {
    return this.tasks.map((t) => ({ ...t, targetFiles: [...(t.targetFiles || [])] }));
  }

  /**
   * Get a specific task by ID.
   * @param {string|number} taskId
   * @returns {Object|null}
   */
  getTask(taskId) {
    const tId = String(taskId);
    const task = this.tasks.find((t) => t.id === tId);
    return task ? { ...task, targetFiles: [...(task.targetFiles || [])] } : null;
  }

  /**
   * Format a task card for Blessed/terminal display.
   * Displays status badge ([QUEUED], [IN PROGRESS], [DONE]), title, assignee,
   * and active lock indicators ([LOCKED BY <USER>]).
   *
   * @param {Object} task
   * @param {Array} [activeLocks=[]]
   * @returns {string}
   */
  formatTaskCard(task, activeLocks = []) {
    const badge = TAGS.badge(formatStatusLabel(task.status), task.status);
    const header = ` ${badge} {#f0fff0-fg}#${task.id}: ${task.title}{/#f0fff0-fg}`;
    const assigneeLine = `   ${TAGS.dim(`↳ Assignee: ${task.assignee}`)}`;

    const lines = [header, assigneeLine];

    // Find all locks associated with this task or targeting its files
    const targetSet = new Set((task.targetFiles || []).map((f) => normalizeLockPath(f, this.cwd)));
    const relevantLocks = [];

    for (const lock of activeLocks) {
      const lockPath = normalizeLockPath(lock.filePath || lock.file, this.cwd);
      const isSameTask = String(lock.taskId) === String(task.id);
      const isTargetedFile = targetSet.has(lockPath);

      if (isSameTask || isTargetedFile) {
        relevantLocks.push(lock);
      }
    }

    if (relevantLocks.length > 0) {
      for (const lock of relevantLocks) {
        const holder = (lock.ownerId || lock.holder || task.assignee).toUpperCase();
        const fPath = normalizeLockPath(lock.filePath || lock.file, this.cwd);
        lines.push(`   {#ef4444-fg}🔒 [LOCKED BY ${holder}]{/#ef4444-fg} {#4a7c4a-fg}${fPath}{/#4a7c4a-fg}`);
      }
    } else if (task.targetFiles && task.targetFiles.length > 0) {
      lines.push(`   ${TAGS.dim(`↳ Scope: ${task.targetFiles.join(', ')}`)}`);
    }

    return lines.join('\n');
  }

  /**
   * Render tasks into a Blessed tasks box widget.
   * @param {Object} tasksBox Blessed box
   * @param {Array} [locks=[]]
   */
  renderToBox(tasksBox, locks = []) {
    if (!tasksBox) return;

    if (!this.tasks || this.tasks.length === 0) {
      tasksBox.setContent(`\n ${TAGS.dim('(No tasks in intent queue)')}`);
      return;
    }

    const cards = this.tasks.map((task) => this.formatTaskCard(task, locks));
    tasksBox.setContent(cards.join('\n\n'));
  }
}
