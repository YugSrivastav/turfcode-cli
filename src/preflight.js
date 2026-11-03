/**
 * TurfCode CLI - Pre-flight Checks
 * 
 * Ensures terminal TTY capability, geometry, and runtime dependencies
 * before initializing raw terminal cockpit mode.
 * Adheres to [MD-038].
 */

import chalk from 'chalk';

export async function runPreflightChecks(options = {}) {
  const issues = [];
  const warnings = [];

  // 1. Terminal TTY capability
  const isTTY = Boolean(process.stdout && process.stdout.isTTY);
  if (!isTTY && !options.allowNonTTY && !process.env.TURF_TEST) {
    issues.push('TurfCode requires an interactive TTY terminal (e.g. xterm, Windows Terminal, iTerm2, tmux).');
  }

  // 2. Terminal geometry
  if (isTTY) {
    const cols = process.stdout.columns || 80;
    const rows = process.stdout.rows || 24;
    if (cols < 60 || rows < 15) {
      warnings.push(`Terminal size (${cols}x${rows}) is smaller than recommended (80x24). Some panels may compress.`);
    }
  }

  // 3. Dependency validation
  const deps = ['blessed', 'chalk', 'node-pty'];
  for (const dep of deps) {
    try {
      await import(dep);
    } catch (err) {
      issues.push(`Required dependency "${dep}" failed to load: ${err.message}`);
    }
  }

  return {
    ok: issues.length === 0,
    issues,
    warnings,
    isTTY
  };
}

export function printPreflightErrors(issues) {
  console.error(chalk.hex('#ef4444').bold('\n[PRE-FLIGHT CHECK FAILED]'));
  for (const issue of issues) {
    console.error(chalk.hex('#ef4444')(`  ✖ ${issue}`));
  }
  console.error(chalk.hex('#4a7c4a')('\nPlease ensure you are running in an interactive terminal with dependencies installed.\n'));
}
