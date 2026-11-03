# Architectural Micro-Decisions Log (ADR)

This file records all agreed micro-decisions and invariant choices for TurfCode CLI.
All crewmates, contributors, and agents MUST adhere to these invariants when modifying this codebase.

---

## [MD-001] 2026-11-03: Two-Section TUI Layout Orientation
- **Context:** Organizing screen real estate between shared intent/roadmap and active agent execution.
- **The Fork:** Vertical split (Herdr-style sidebar + main pane) vs. horizontal split (top intent banner + bottom terminals).
- **Resolution:** Option A: Vertical split. 30% left pane for Intent Board (shared goals, active file locks, connected peers), 70% right pane for streaming agent tabs and terminal outputs.
- **Worker Invariant:** The cockpit must never destroy the 2-section layout on subprocess execution. Subprocess outputs must stream inside the right pane.

---

## [MD-002] 2026-11-03: Coarse-Grained Task Scope File Locking
- **Context:** Preventing concurrent agents or users from editing the same file simultaneously and causing merge collisions.
- **The Fork:** Upfront coarse-grained task locks vs. low-level filesystem inotify write-interception.
- **Resolution:** Option A: Upfront coarse-grained task scope. When an agent task begins, its target files/modules are registered in the Intent Board lock table (`LOCKED BY <USER/AGENT>`). Any attempt by another peer or agent to touch locked files is immediately rejected with a lock conflict warning.
- **Worker Invariant:** All file edit operations by agents must verify lock table availability before writing to disk.

---

## [MD-003] 2026-11-03: Parallel Multi-Agent Concurrency
- **Context:** Defining whether peers share one AI dialogue or run independent agents.
- **The Fork:** Single shared AI pair-programming session vs. parallel concurrent agents.
- **Resolution:** Option A: Parallel concurrent agents. Both peers can spawn and run agents simultaneously (e.g. Host runs an `agy` task on backend while Peer runs an `agy` task on frontend). Agent streams appear in dedicated tabs within the Agent Pane.
- **Worker Invariant:** Agent harness architecture must support multiple concurrent running sessions without cross-talk or stdout pollution.

---

## [MD-004] 2026-11-03: Electric Lime TrueColor Terminal Atmosphere
- **Context:** Establishing the visual identity and immersive atmosphere when entering TurfCode CLI.
- **The Fork:** 24-bit TrueColor ANSI palette shift vs. native OS window/Electron wrapper.
- **Resolution:** Option A: Pure CLI TrueColor palette. Dark obsidian-emerald background (`#081008`), high-contrast electric lime borders and accents (`#39FF14`), and distinct status indicators.
- **Worker Invariant:** Never rely on default terminal monochrome. All borders, headers, and status lines must render in the signature Electric Lime design token scale.

---

## [MD-005] 2026-11-03: Host-Centric Agent Execution
- **Context:** Deciding where AI agents physically execute when triggered by connected peers.
- **The Fork:** Host-centric execution (agents run on host machine) vs. distributed execution (peer runs agents locally and syncs files).
- **Resolution:** Option A: Host-Centric Execution. The host machine holds the primary git repository, filesystem, and spawns the `agy` subprocesses. The peer CLI operates as a thin client streaming inputs and live terminal tokens. Eliminates git merge divergence and peer environment prerequisites.
- **Worker Invariant:** All agent subprocesses must spawn in the host workspace. Peer inputs are routed to host PTY streams.

---

## [MD-006] 2026-11-03: Ephemeral Tunnel NAT Traversal
- **Context:** Connecting peers across the internet without manual port forwarding or LAN requirements.
- **The Fork:** Ephemeral tunnel (Cloudflare Quick Tunnel / localtunnel) vs. centralized relay server vs. LAN-only.
- **Resolution:** Option A: Ephemeral HTTPS/WSS Tunnel. TurfCode spins up an automated tunnel on session creation, issuing a 6-character room code (e.g. `TURF-7A9B`). Peers join globally across firewalls without manual router configuration.
- **Worker Invariant:** Connection client must fail over gracefully if tunnel connection drops and attempt automatic reconnection.

---

## [MD-007] 2026-11-03: Process-Bound Lock Heartbeats & Force Unlock
- **Context:** Preventing permanent file lock deadlocks if an agent process hangs or crashes.
- **The Fork:** Process-bound lease with TTL and manual force unlock vs. manual unlock only.
- **Resolution:** Option A: Process-bound lease with heartbeat. Locks are linked to the active agent process PID. If the process terminates or heartbeats cease for >10 seconds, the lock is automatically dissolved. Host retains a manual `[Force Unlock]` action on the Intent Board.
- **Worker Invariant:** Never leave orphaned locks on disk or in memory. Always verify process liveness before rejecting file writes.

---

## [MD-008] 2026-11-03: Decoupled Scrollable Viewport Geometry
- **Context:** Managing terminal screen dimension mismatches between peers (e.g. 27" monitor vs. 13" laptop).
- **The Fork:** Decoupled scrollable viewports vs. smallest common denominator forced window shrinking.
- **Resolution:** Option A: Decoupled scrollable viewports. Each peer retains their own local screen buffer dimensions and scroll position. Output text wraps dynamically to each peer's terminal width.
- **Worker Invariant:** Terminal resize events (SIGWINCH) on the peer must not forcibly crush the host's terminal dimensions.
