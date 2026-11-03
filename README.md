# TurfCode CLI

> Real-time multiplayer AI coding in the terminal.

TurfCode CLI brings multiplayer collaboration to developer terminals. Two developers can pair in real time on a shared repository, orchestrate parallel AI coding agents (starting with Antigravity `agy`), lock files during active edits to prevent conflicts, and coordinate through a shared Intent Board.

---

## Core Features

- **Instant Session Pairing**: One developer hosts a local repository (`Create a Turf`); the second developer joins instantly using a session code (`Join a Turf`).
- **Signature Cockpit**: High-contrast Electric Lime (`#39FF14`) TrueColor theme on dark obsidian glass.
- **Herdr-Inspired 2-Section Layout**:
  - **Left (30%)**: Intent Board with shared goals, active peers, and the live File Lock Table.
  - **Right (70%)**: Multi-agent streaming terminal pane with concurrent agent tabs.
- **Strict File Locking**: Prevents concurrent agents or peers from clobbering active files.
- **Zero Mock Data**: Operates strictly on real files, real connections, and real CLI binaries.

---

## Architectural Decisions

All architectural and implementation invariants are logged in [`microdecisions.md`](./microdecisions.md).
