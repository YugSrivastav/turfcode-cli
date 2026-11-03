/**
 * TurfCode CLI - Subtle Terminal Bell Audio Cues
 *
 * Emits subtle ANSI terminal bell (\x07) on task completion and lock conflicts.
 * Allows developers to listen for completion or conflicts while multitasking.
 *
 * Complies with [MD-034]. Toggleable via --silent flag.
 */

export class AudioBell {
  /**
   * @param {Object} [options]
   * @param {boolean} [options.silent=false] Mute audio bell cues
   * @param {stream.Writable} [options.outputStream=process.stdout] Stream to write bell character to
   */
  constructor(options = {}) {
    this.silent = typeof options.silent === 'boolean'
      ? options.silent
      : (process.argv.includes('--silent') || process.argv.includes('-s'));
    this.outputStream = options.outputStream || process.stdout;
    this.history = [];
  }

  /**
   * Ring the terminal bell with ANSI \x07.
   * @param {string} [type='cue']
   * @returns {boolean} Whether the bell was emitted (false if silenced)
   */
  ring(type = 'cue') {
    if (this.silent) {
      return false;
    }

    try {
      if (this.outputStream && typeof this.outputStream.write === 'function') {
        this.outputStream.write('\x07');
      }
    } catch {
      // In case stream writing fails (e.g. closed pipe)
    }

    this.history.push({
      type,
      timestamp: Date.now()
    });
    return true;
  }

  /**
   * Cue for background task or agent completion.
   * @param {string|number} [taskId]
   * @returns {boolean}
   */
  onTaskCompletion(taskId) {
    return this.ring(taskId ? `completion:${taskId}` : 'completion');
  }

  /**
   * Cue for lock conflict / collision alert.
   * @param {Object|string} [conflictDetails]
   * @returns {boolean}
   */
  onLockConflict(conflictDetails) {
    return this.ring(typeof conflictDetails === 'string' ? `conflict:${conflictDetails}` : 'conflict');
  }

  /**
   * Toggle silent mode.
   * @param {boolean} silent
   */
  setSilent(silent) {
    this.silent = Boolean(silent);
  }

  /**
   * Check if bell is muted.
   * @returns {boolean}
   */
  isSilent() {
    return this.silent;
  }

  /**
   * Get history of emitted cues.
   * @returns {Array<{type: string, timestamp: number}>}
   */
  getHistory() {
    return [...this.history];
  }

  /**
   * Clear emitted history.
   */
  clearHistory() {
    this.history = [];
  }
}

export function createBell(options) {
  return new AudioBell(options);
}

export const bell = new AudioBell();
