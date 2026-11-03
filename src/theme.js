/**
 * TurfCode CLI - Electric Lime Design System & Theme
 * 
 * Centralized color tokens, Blessed widget styles, and formatters.
 * Adheres to [MD-004] Electric Lime TrueColor Terminal Atmosphere.
 */

import chalk from 'chalk';

export const PALETTE = {
  // Primary Atmosphere
  bg: '#081008',             // Obsidian-emerald dark background
  headerBg: '#0d1a0d',       // Slightly elevated dark emerald
  panelBg: '#081008',
  
  // Electric Lime & Greens
  electricLime: '#39FF14',   // High-contrast electric lime signature
  brightGreen: '#22c55e',    // Success / Done state
  midGreen: '#166534',       // Secondary green
  dimGreen: '#2a5a2a',       // Subtle green
  mutedBorder: '#1b3b1b',    // Dark obsidian-emerald border
  activeBorder: '#39FF14',   // Focused / active border
  
  // Neutral Text
  textBright: '#f0fff0',     // Crisp white with subtle emerald tint
  textMuted: '#4a7c4a',      // Muted emerald label text
  textDim: '#2d4d2d',        // Very dim text / comments
  
  // Status Badges
  statusQueued: '#888888',
  statusProgress: '#eab308', // Amber/Yellow
  statusDone: '#22c55e',     // Bright green
  statusLocked: '#ef4444',   // Conflict / lock red
  statusPeer: '#06b6d4',     // Cyan peer indicator
};

export const CHALK = {
  lime: chalk.hex(PALETTE.electricLime),
  limeBold: chalk.hex(PALETTE.electricLime).bold,
  dim: chalk.hex(PALETTE.textMuted),
  border: chalk.hex(PALETTE.mutedBorder),
  bgEmerald: chalk.bgHex(PALETTE.bg),
  text: chalk.hex(PALETTE.textBright),
  alertRed: chalk.hex(PALETTE.statusLocked),
  statusYellow: chalk.hex(PALETTE.statusProgress),
  peerCyan: chalk.hex(PALETTE.statusPeer),
};

export const TAGS = {
  lime: (txt) => `{#39ff14-fg}${txt}{/#39ff14-fg}`,
  limeBold: (txt) => `{bold}{#39ff14-fg}${txt}{/#39ff14-fg}{/bold}`,
  dim: (txt) => `{#4a7c4a-fg}${txt}{/#4a7c4a-fg}`,
  white: (txt) => `{#f0fff0-fg}${txt}{/#f0fff0-fg}`,
  red: (txt) => `{#ef4444-fg}${txt}{/#ef4444-fg}`,
  yellow: (txt) => `{#eab308-fg}${txt}{/#eab308-fg}`,
  cyan: (txt) => `{#06b6d4-fg}${txt}{/#06b6d4-fg}`,
  badge: (text, type = 'active') => {
    switch (type) {
      case 'in_progress':
      case 'progress':
        return `{#eab308-fg}[${text}]{/#eab308-fg}`;
      case 'done':
        return `{#22c55e-fg}[${text}]{/#22c55e-fg}`;
      case 'locked':
        return `{#ef4444-fg}[🔒 ${text}]{/#ef4444-fg}`;
      case 'peer':
        return `{#06b6d4-fg}[● ${text}]{/#06b6d4-fg}`;
      case 'queued':
        return `{#888888-fg}[${text}]{/#888888-fg}`;
      case 'active':
      default:
        return `{bold}{#39ff14-fg}[${text}]{/#39ff14-fg}{/bold}`;
    }
  }
};

export const BLESSED_STYLES = {
  screen: {
    smartCSR: true,
    title: 'TurfCode Cockpit',
    dockBorders: true,
    useBCE: true,
    fullUnicode: true,
    style: {
      bg: PALETTE.bg,
      fg: PALETTE.textBright
    }
  },
  header: {
    top: 0,
    left: 0,
    width: '100%',
    height: 1,
    tags: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.headerBg,
      bold: true
    }
  },
  sidebar: {
    top: 1,
    left: 0,
    width: '30%',
    height: '100%-2',
    style: {
      bg: PALETTE.bg
    }
  },
  panel: {
    border: { type: 'line' },
    tags: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg,
      border: { fg: PALETTE.mutedBorder }
    }
  },
  panelFocused: {
    border: { fg: PALETTE.activeBorder }
  },
  panelUnfocused: {
    border: { fg: PALETTE.mutedBorder }
  },
  terminalPane: {
    top: 1,
    left: '30%',
    width: '70%',
    height: '100%-2',
    border: { type: 'line' },
    tags: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg,
      border: { fg: PALETTE.mutedBorder }
    }
  },
  terminalLog: {
    top: 2,
    left: 0,
    width: '100%-2',
    height: '100%-5',
    scrollable: true,
    alwaysScroll: true,
    tags: true,
    keys: true,
    vi: true,
    mouse: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg
    }
  },
  tabBar: {
    top: 0,
    left: 0,
    width: '100%-2',
    height: 1,
    tags: true,
    style: {
      fg: PALETTE.textMuted,
      bg: PALETTE.headerBg
    }
  },
  inputBox: {
    bottom: 0,
    left: 0,
    width: '100%-2',
    height: 3,
    border: { type: 'line' },
    tags: true,
    style: {
      fg: PALETTE.textBright,
      bg: PALETTE.bg,
      border: { fg: PALETTE.mutedBorder }
    }
  },
  inputBoxFocused: {
    border: { fg: PALETTE.activeBorder }
  },
  input: {
    top: 0,
    left: 1,
    width: '100%-4',
    height: 1,
    keys: true,
    mouse: true,
    inputOnFocus: true,
    style: {
      fg: PALETTE.electricLime,
      bg: PALETTE.bg
    }
  },
  footer: {
    bottom: 0,
    left: 0,
    width: '100%',
    height: 1,
    tags: true,
    style: {
      fg: PALETTE.textMuted,
      bg: PALETTE.headerBg
    }
  }
};
