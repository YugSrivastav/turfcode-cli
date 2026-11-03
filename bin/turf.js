#!/usr/bin/env node

/**
 * TurfCode CLI - Executable Binary Entry Point
 * 
 * Maps to "bin": { "turf": "./bin/turf.js", "turfcode": "./bin/turf.js" } in package.json.
 * Complies with [MD-028], [MD-038], [MD-041].
 */

import { runCLI } from '../src/cli.js';

runCLI(process.argv)
  .then((exitCode) => {
    if (typeof exitCode === 'number' && exitCode !== 0) {
      process.exit(exitCode);
    }
  })
  .catch((err) => {
    console.error('[FATAL ERROR]', err);
    process.exit(1);
  });
