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

---

## [MD-009] 2026-11-03: Graceful Host Shutdown with Stash (Failover on Roadmap)
- **Context:** Handling abrupt host disconnection (e.g. laptop lid closes or network drops).
- **The Fork:** Graceful pause and safe stash vs. instant automatic peer host-migration.
- **Resolution:** Option A for MVP: 30-second reconnection grace period. If host does not reconnect, session terminates cleanly, uncommitted changes are auto-stashed on host disk, and peer is notified. Option B (automatic state migration and peer promotion to host) is scheduled as a future milestone.
- **Worker Invariant:** An ungraceful host drop must never leave corrupted git working states or uncommitted work deleted.

---

## [MD-010] 2026-11-03: Per-User AGY Account & Session ID Isolation
- **Context:** Managing Antigravity quota and identity when multiple peers prompt agents on the host.
- **The Fork:** Host pays for all agent tokens vs. per-user credential passthrough and session isolation.
- **Resolution:** Option B: Per-User Credential Passthrough. Peers connect with their own Antigravity auth/account credentials and distinct session IDs. When Ayush launches an agent, the host executes `agy` scoped to Ayush's account token and session ID, consuming Ayush's quota.
- **Worker Invariant:** Subprocess environment variables must dynamically inject the invoking user's auth token and session ID rather than defaulting to host credentials.

---

## [MD-011] 2026-11-03: Trusted Peer Boundary (Permission Gating on Roadmap)
- **Context:** Security and command execution boundaries for connected peers.
- **The Fork:** Trusted peer execution within project root vs. mandatory host confirmation prompt for every command.
- **Resolution:** Option A for MVP: Trusted peer model scoped strictly to project root (`cwd`). Host confirmation gating for sensitive commands/paths is scheduled for Phase 2.
- **Worker Invariant:** Peer terminal sessions must never have their working directory set outside the project repository root.

---

## [MD-012] 2026-11-03: Git Co-Authorship via Standard Trailers
- **Context:** Attributing commits made through multiplayer collaborative agent runs.
- **The Fork:** Co-authored commits vs. actor-only single author commits.
- **Resolution:** Option A: Git Co-Authorship. Commits list the host as the primary author, with the contributing peer credited via standard git trailers: `Co-authored-by: <Name> <<email>>`.
- **Worker Invariant:** Automated commit generation must parse active session contributors and append valid Co-authored-by trailers.

---

## [MD-013] 2026-11-03: Owner-Only Agent Interruption & Floor Control
- **Context:** Controlling who can cancel or steer a running agent.
- **The Fork:** Mutual interruption (anyone can Ctrl+C) vs. owner-only floor control.
- **Resolution:** Option B: Owner-Only Floor Control. Only the user who spawned a specific agent has authority to terminate or send steering inputs to that agent's process. Prevents peer interference.
- **Worker Invariant:** Signal dispatch (SIGINT/SIGTERM) to an agent PID must verify that the requesting user's session ID matches the agent's owner ID.

---

## [MD-014] 2026-11-03: Structured Intent Board (.turf/intent.json)
- **Context:** Representing shared tasks, goals, and roadmaps between peers.
- **The Fork:** Structured JSON task queue vs. freeform collaborative markdown scratchpad.
- **Resolution:** Option A: Structured in-memory task queue synced to `.turf/intent.json`. Supports discrete task states (`[QUEUED]`, `[IN PROGRESS]`, `[DONE]`), owner assignment, and binding to running agents.
- **Worker Invariant:** All Intent Board state mutations must serialize atomically to `.turf/intent.json`.

---

## [MD-015] 2026-11-03: Real-Time Activity Ticker & Status Stream
- **Context:** In-terminal peer communication and awareness.
- **The Fork:** Minimalist event ticker with `/msg` vs. heavy 3-panel chat room.
- **Resolution:** Option A: Lightweight activity stream in the Intent Board footer showing file locks, agent starts, and transient `/msg <text>` alerts. Assumes voice calls handle conversation.
- **Worker Invariant:** Event messages must be throttled to prevent terminal rendering jitter.

---

## [MD-016] 2026-11-03: Integrated Colorized Diff Inspection
- **Context:** Reviewing code changes made by agents or peers in real time.
- **The Fork:** Integrated diff tab inside the TUI vs. manual terminal `git diff` commands.
- **Resolution:** Option A: Integrated Diff Tab. Pressing `[D]` in the Agent Pane displays a live colorized diff view of modified files with green/red syntax highlighting.
- **Worker Invariant:** Diff view must generate on-demand from git index without blocking the main event loop.

---

## [MD-017] 2026-11-03: Filesystem Watcher for External IDE Sync
- **Context:** Detecting manual code changes made in VS Code, Neovim, or external tools.
- **The Fork:** Inotify/chokidar filesystem watcher vs. manual refresh command.
- **Resolution:** Option A: Native filesystem watcher. TurfCode monitors the workspace tree and updates the Intent Board and peer views whenever external files change.
- **Worker Invariant:** Watcher must debounce file change events (minimum 100ms) to avoid spamming the WebSocket channel during rapid saves.

---

## [MD-018] 2026-11-03: Stateful Session Resume (.turf/session.json)
- **Context:** Preserving state across disconnects and multi-day coding sprints.
- **The Fork:** Stateful resume via local session file vs. ephemeral wipe-on-exit.
- **Resolution:** Option A: Stateful Resume. Session metadata, task history, and past agent logs persist in `.turf/session.json`. Host can select `[Resume Previous Turf]` on startup.
- **Worker Invariant:** State serialization must exclude sensitive tokens and passwords. Sensitive credentials remain purely in-memory.

---

## [MD-019] 2026-11-03: Encrypted Join Handshake & Credential Passthrough
- **Context:** Authenticating peer identities and scoping agent API credentials.
- **The Fork:** Host pays for all tokens vs. encrypted handshake transmitting peer's own auth token and session ID.
- **Resolution:** Option A: Encrypted Join Handshake. The peer authenticates locally and transmits an ephemeral token/session ID over the encrypted tunnel. The peer's agent runs under the peer's account and session, consuming the peer's quota.
- **Worker Invariant:** Never log or leak peer credentials into persistent session files.

---

## [MD-020] 2026-11-03: Host as Live File Server, Peer Local Input Loop
- **Context:** Low-latency input handling and role of the host device.
- **The Fork:** Forwarding every raw keystroke over network vs. host as authoritative live file server while peer drives a local input loop.
- **Resolution:** Option A: Host acts as the low-latency live file server and lock coordinator. Peer types prompts locally with instant feedback; only completed prompts, file sync operations, and lock transactions cross the network.
- **Worker Invariant:** Input typing must never be blocked by network latency; local buffer handles keystrokes smoothly.

---

## [MD-021] 2026-11-03: Throttled Frame Buffering @ 30ms for Streaming
- **Context:** Managing high-speed agent token output without network packet spam or TUI redraw flickering.
- **The Fork:** Throttled frame buffering vs. raw unbuffered byte streaming.
- **Resolution:** Option A: Throttled 30ms frame buffering. Terminal deltas are collected and emitted in 30ms bursts, providing fluid visual animation without saturating the WebSocket channel.
- **Worker Invariant:** Frame buffer must auto-flush immediately on process exit or user input interruption.

---

## [MD-022] 2026-11-03: Isolated Git Worktrees per Agent
- **Context:** Preventing parallel running agents from colliding on uncommitted files.
- **The Fork:** Isolated git worktrees per agent vs. single shared root working directory.
- **Resolution:** Option A: Isolated git worktrees under `.turf/worktrees/<task-id>`. Each running agent has an independent working copy. When an agent finishes, TurfCode merges the branch back into `main`.
- **Worker Invariant:** Agents must never write directly to the primary checkout directory while parallel tasks are active.

---

## [MD-023] 2026-11-03: Fail-Fast Lock Conflict Handling
- **Context:** Action taken when an agent or peer attempts to modify a locked file.
- **The Fork:** Fail-fast with explicit diagnostic vs. background sleep and retry queue.
- **Resolution:** Option A: Fail-fast with diagnostic. The operation aborts immediately, alerting the user to the conflict and identifying who holds the lock. Prevents silent token burn during lock contention.
- **Worker Invariant:** Reject blocked file writes immediately at the pre-flight check before touching the filesystem.

---

## [MD-024] 2026-11-03: Agent Pause on Peer Disconnect
- **Context:** Managing agent processes if the sponsoring peer experiences a network drop.
- **The Fork:** Agent continues unattended vs. agent pauses until peer reconnects.
- **Resolution:** Option B: Agent Pauses on Disconnect. If Ayush's connection drops, his running agent is sent `SIGSTOP` or placed on hold. When Ayush reconnects, execution resumes, ensuring the peer doesn't lose oversight of agent actions.
- **Worker Invariant:** Process state must transition to `PAUSED` and broadcast to the Intent Board upon peer heartbeat loss.

---

## [MD-025] 2026-11-03: Code-Only Open Room Access
- **Context:** Streamlining the peer onboarding experience.
- **The Fork:** Code + Host Approval pop-up vs. Code-Only direct join.
- **Resolution:** Option B: Code-Only Direct Join for MVP. Anyone possessing the 6-character session code enters the turf immediately without requiring the host to click an approval dialog.
- **Worker Invariant:** Session codes must use cryptographically secure random alphanumeric tokens (e.g. `TURF-7A9B`).

---

## [MD-026] 2026-11-03: Per-User Local Command History
- **Context:** Arrow-key command recall in the terminal input box.
- **The Fork:** Per-user isolated history vs. global shared turf history.
- **Resolution:** Option A: Per-user local command history saved to `~/.turf/history`. Each developer cycles through their own past prompts and commands.
- **Worker Invariant:** Up/Down arrow keypresses must query local client history exclusively.

---

## [MD-027] 2026-11-03: Interactive File Tree with Live Padlock Badges
- **Context:** Navigating and inspecting workspace files from within the TUI.
- **The Fork:** Toggleable interactive file tree on Intent Board vs. task-only display.
- **Resolution:** Option A: Toggleable interactive file tree (`[F]` key). Displays directory hierarchy with bright red `🔒` padlock badges beside files held by active locks.
- **Worker Invariant:** File tree must render dynamically using cached git index to avoid blocking on large `node_modules` folders.

---

## [MD-028] 2026-11-03: Distribution via Global NPM Package
- **Context:** Package packaging and user installation workflow.
- **The Fork:** Pure Node.js ESM package (`npm install -g turfcode` / `npx turfcode`) vs. standalone compiled binary.
- **Resolution:** Option A: Global NPM package (`npx turfcode`). Instant cross-platform compatibility across Windows (PowerShell/WSL), macOS, and Linux on Node.js 20+.
- **Worker Invariant:** Must declare `"type": "module"`, specify executable bin in `package.json`, and run zero native compilation steps during install.
