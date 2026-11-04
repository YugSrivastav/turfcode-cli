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

---

## [MD-029] 2026-11-03: Direct Binary Subprocess Execution
- **Context:** Invoking agent CLI binaries without shell escape corruption.
- **The Fork:** Direct binary execution via `child_process.spawn('agy', ...)` vs. interactive shell subshell wrapper.
- **Resolution:** Option A: Direct Binary Execution. `agy` and helper tools spawn directly with explicit argument arrays and isolated environment tables. Prevents quote stripping and shell variable leakage.
- **Worker Invariant:** Never pass raw unsanitized prompt strings into `bash -c` or `powershell -Command`.

---

## [MD-030] 2026-11-03: Upfront Target Scope File Locking
- **Context:** Determining which files are locked when a task begins.
- **The Fork:** Upfront target scope declaration in task prompt vs. dynamic pre-write interception.
- **Resolution:** Option A: Upfront Target Scope. Task creation declares the target files/directories. TurfCode acquires the locks before the agent starts writing, preventing race conditions from the very first second.
- **Worker Invariant:** Agent execution must not begin until all declared target files are verified unlocked.

---

## [MD-031] 2026-11-03: Reverse Port Forwarding for Dev Server Sharing
- **Context:** Allowing the connected peer to view live web apps (e.g. Next.js, Vite on port 3000) running on the host.
- **The Fork:** Reverse port forwarding over the tunnel vs. terminal-only without web preview.
- **Resolution:** Option A: Reverse Port Forwarding. TurfCode tunnels common web dev ports (e.g. 3000, 5173, 8080) through the connection tunnel, providing the peer with a local URL to preview running web UIs.
- **Worker Invariant:** Port forwarder must bind only when the respective host port is actively listening.

---

## [MD-032] 2026-11-03: Herdr / Tmux Hybrid Navigation Keybindings
- **Context:** Keyboard ergonomics for switching panels, collapsing sidebars, and jumping tabs.
- **The Fork:** Herdr/Tmux keybindings vs. Vim-style modal navigation.
- **Resolution:** Option A: Herdr/Tmux keybindings. `Tab` cycles focus (Intent Board -> Terminal Pane -> Input Box). `Ctrl+B` collapses/expands the Intent sidebar. `Ctrl+1..9` jumps to agent tabs. `Ctrl+C` clears inputs.
- **Worker Invariant:** Key event handlers must intercept control sequences cleanly without leaking raw escape codes to stdout.

---

## [MD-033] 2026-11-03: Fast-Forward Auto-Squash on Task Completion
- **Context:** Merging finished agent worktree branches into the main working branch.
- **The Fork:** Fast-forward auto-squash vs. manual peer approval gate.
- **Resolution:** Option A: Fast-forward auto-squash. When an agent passes its validation tests, its isolated `.turf/worktrees/<task-id>` branch is squash-merged into `main`, and locks are released immediately.
- **Worker Invariant:** Worktree teardown must verify a clean merge before deleting the worktree directory.

---

## [MD-034] 2026-11-03: Subtle Terminal Bell Audio Cues
- **Context:** Notifying developers when background agent tasks complete or lock conflicts happen.
- **The Fork:** ANSI `\x07` terminal bell vs. purely visual indicators.
- **Resolution:** Option A: ANSI `\x07` subtle terminal bell on task completion and lock conflict alerts. Allows developers to listen for completion while multitasking.
- **Worker Invariant:** Audio bell must be toggleable via `turf --silent`.

---

## [MD-035] 2026-11-03: Text-Only WebSocket Diff Streaming
- **Context:** Bandwidth management and binary asset handling over the live sync channel.
- **The Fork:** Text-only code/diff sync vs. full chunked binary streaming.
- **Resolution:** Option A: Text-only streaming. Only source code diffs, markdown, JSON, and lock states stream over the WebSocket. Files >1MB or binary assets remain on host disk.
- **Worker Invariant:** Watcher and diff generator must filter out binary extensions and large blobs.

---

## [MD-036] 2026-11-03: Automatic Self-Healing on Boot
- **Context:** Recovering from unexpected crashes, power cuts, or killed processes.
- **The Fork:** Automatic self-healing on startup vs. halting with manual confirmation prompts.
- **Resolution:** Option A: Automatic self-healing. On startup, TurfCode inspects `.turf/session.json`. If an unclean exit occurred, it resets stale file locks, checks git status, and prints a recovery notice.
- **Worker Invariant:** Boot recovery must never discard uncommitted git changes; uncommitted work is safely preserved.

---

## [MD-037] 2026-11-03: Minimalist 4-Field Task Schema
- **Context:** Structuring task cards for the 30% Intent Board.
- **The Fork:** Minimalist 4-field schema vs. rich Kanban cards.
- **Resolution:** Option A: Minimalist 4-field schema (`id`, `title`, `assignee`, `status`). Fits high-density terminal layouts cleanly without vertical overflow.
- **Worker Invariant:** Task records must strictly conform to `{ id: string, title: string, assignee: string, status: 'queued'|'in_progress'|'done' }`.

---

## [MD-038] 2026-11-03: Interactive Cockpit Launcher & CLI Subcommands
- **Context:** CLI entry points and invocation workflow.
- **The Fork:** Interactive launcher (`turf` / `npx turfcode`) with shortcut flags vs. strict subcommand-only CLI.
- **Resolution:** Option A: Interactive Cockpit Launcher. Running `turf` displays an Electric Lime splash with `[1] Host a Turf`, `[2] Join a Turf`. Shortcut flags (`turf host .`, `turf join <ID>`, `turf kill`) provide instant power-user access.
- **Worker Invariant:** Launcher must run pre-flight port and dependency checks before entering raw TTY mode.

---

## [MD-039] 2026-11-03: Host-Owned Worktree Merging & Host Quota Attribution
- **Context:** Determining who bears the token and compute quota when merging completed agent worktrees back into `main`.
- **The Fork:** Peer quota vs. host quota for git merge resolution.
- **Resolution:** Host quota. The host machine is the authoritative owner of the git repository. When an agent finishes its task, the merge resolution, git verification, and conflict checks execute on the host machine using the host's quota and git identity.
- **Reason:** The host owns the disk and final branch state. Merges must be resolved locally on the host to prevent desynchronization between remote peers and the host's local git tree.
- **Worker Invariant:** Merge operations must execute within the host process context and never bill against peer session tokens.

---

## [MD-040] 2026-11-03: Pre-Flight Sequence Declaration & Atomic Multi-File Locking
- **Context:** Handling agents that touch multiple files (2-3 files) simultaneously or in rapid succession.
- **The Fork:** Lazy lock-on-write vs. mandatory upfront intent declaration with file sequence locking.
- **Resolution:** Mandatory upfront intent declaration. Before any agent can touch workspace files, it must declare its intent and the explicit sequence of files it plans to modify (e.g. `[1. src/models/user.ts -> 2. src/routes/auth.ts -> 3. test/auth.test.ts]`). TurfCode acquires an atomic lock on all files in the sequence before granting write permissions.
- **Reason:** If an agent modifies file 1, and then tries to modify file 2 only to find file 2 is locked by another peer, the agent is left in a dirty, half-finished state. Atomic sequence locking eliminates partial-write deadlocks.
- **Worker Invariant:** An agent process must acquire all sequence locks atomically before writing to its worktree. If any file in the sequence is held, the entire operation halts pre-flight.

---

## [MD-041] 2026-11-03: Two-Option CLI Interactive Launcher Flow with Live Hosting Folder Prompt & Session ID Disclosure
- **Context:** Streamlining the terminal entry experience when a user executes `turfcode` or `turf` with zero arguments.
- **The Fork:** Immediate cockpit boot on cwd with subcommands vs. Guided two-option interactive prompt (`[1] Create a Turf`, `[2] Join a Turf`) prompting for workspace folder and displaying generated Session ID.
- **Resolution:** Option B: Guided two-option interactive prompt. When `turfcode` or `turf` is run without arguments in an interactive terminal, it renders:
  `[1] Create a Turf`
  `[2] Join a Turf`
  `[3] Exit`
  Choosing `[1] Create a Turf` prompts for the folder to live host (defaulting to current folder `.`). It validates and normalizes the folder path (resolves relative paths, strips surrounding quotes, expands directory if non-existent), generates the 6-character Session ID (e.g. `TURF-7A9B`), displays a clear visual banner with the Session ID and instructions to share it with collaborators, and boots the Electric Lime Cockpit hosting that folder.
- **Reason:** Users downloading `turfcode` want zero friction to spin up a collaborative session without remembering subcommands. Explicitly prompting for the directory and displaying the Session ID upfront gives the host immediate confirmation of what is being hosted and what code to send to peers.
- **Worker Invariant:** Both `turf` and `turfcode` binary symlinks must be defined in `package.json`. Interactive menu must strictly offer `[1] Create a Turf` and `[2] Join a Turf`, validate the input directory, and disclose the generated Session ID before cockpit initialization.

---

## [MD-042] 2026-11-03: Herdr-Style Unified Typographic Layout & Mockup Removal
- **Context:** Initial cockpit appearance had heavy box borders cutting the sidebar into separate boxed widgets, mock initial tasks (`#1`, `#2`, `#0`), and mock agent tabs (`2: agy (Host)`, `3: agy (Peer)`).
- **The Fork:** Clunky nested boxed widgets with placeholder mockups vs. Herdr-style seamless vertical typographic hierarchy with 100% genuine system state.
- **Resolution:** Herdr-style seamless vertical typographic hierarchy.
  1. **Purge Mockups:** Remove hardcoded initial tasks from `IntentBoard` initialization and remove mock tabs from `cockpit.js`. Default tasks are empty until user creates them; default tabs is strictly `1: Shell` until dynamic agent tabs or diff tab are spawned.
  2. **Unified Continuous Sidebar:** Transform the left sidebar from 4 heavy bordered boxes into a single continuous, borderless column with Herdr-inspired typographic sections (`SPACES`, `INTENTS & TASKS`, `ACTIVE FILE LOCKS`, `CONNECTED PEERS`).
  3. **Streamlined Right Pane:** Eliminate nested border boxes around the terminal log, streamline the tab strip to an inverted highlight pill (` 1: Shell `), and turn the input box into a sleek single-line prompt (`turf > `) with a subtle top border line instead of a heavy boxed container with an emoji label.
  4. **WSL Windows Path Normalization:** Implement `normalizeWorkspacePath` to seamlessly handle Windows drive paths pasted in WSL/Linux, preventing double `/mnt/c/mnt/c/...` path corruptions.
- **Reason:** Herdr's spatial distribution succeeds because of clean typographic hierarchy and generous whitespace, rather than drawing heavy ASCII boxes inside boxes. Eliminating mockups ensures the cockpit reflects 100% genuine system state.
- **Worker Invariant:** No hardcoded mock tasks or mock tabs. Sidebar sections must not nest bordered boxes inside `sidebar`. Input box must render as a sleek single-line prompt.

---

## [MD-043] 2026-11-03: Terminal Mouse Motion Leak Prevention & Input Sanitization
- **Context:** Moving the mouse across the terminal window caused raw X10 coordinate sequences (e.g. `CF,CF-CF.CG.CG-CF-CG-CH...`) to be typed automatically into `inputBox`.
- **The Fork:** Leave default blessed mouse configuration vs. Explicitly disable `allMotion` (`1003`) mouse movement tracking, disable mouse on `inputBox`, and sanitize raw escape sequences in the textbox listener.
- **Resolution:** Explicitly disable all motion tracking and sanitize input.
  1. Overrode `screen.program.enableMouse()` to enforce `allMotion: false` and `cellMotion: false`, sending `\x1b[?1003l\x1b[?1002l`. This instructs the terminal emulator (Windows Terminal / xterm) to only report explicit button clicks, never streaming unprompted mouse movement deltas into stdin.
  2. Set `mouse: false` on `inputBox` and guarded `setFocus(2)` with `!inputBox._reading` to prevent stacked keypress listeners.
  3. Added defense-in-depth sanitization to `inputBox._listener` to discard any escape codes or mouse sequence artifacts.
  4. Sent clean mouse mode reset (`\x1b[?1003l\x1b[?1002l\x1b[?1000l`) on cockpit teardown.
- **Reason:** In terminals supporting 24-bit TrueColor and SGR/X10 mouse reporting, any-event tracking (`1003`) produces thousands of coordinate packets per second. When packets arrive in chunks, the parser strips the escape header of the first packet, leaving trailing coordinate bytes in stdin where they get misinterpreted as keyboard strokes. Disabling movement tracking eliminates the issue at the hardware escape protocol level.
- **Worker Invariant:** `allMotion` must never be enabled in cockpit mouse options. Textbox elements must not attach duplicate `readInput` listeners.

---

## [MD-044] 2026-11-03: Herdr-Engine UI Alignment & Zero-Leak Terminal Input Prompt
- **Context:** The Captain noted that TurfCode CLI cockpit still appeared cluttered compared to Herdr's pristine interface, with box borders enclosing the terminal pane and sidebar, and mysterious characters (`¤77CC¥¥77CC...`) automatically streaming into the input box on Windows Terminal.
- **The Fork:** Keep Blessed's default boxed widget layout and surface-level keypress filtering vs. Deeply inspect Herdr's actual Rust/Ratatui source code (`/tmp/herdr`), replicate its borderless layout architecture, and permanently eradicate terminal mouse sequence leaks at the protocol level.
- **Resolution:** Replicate Herdr's exact multiplexer layout architecture and apply triple-lock mouse isolation.
  1. **Herdr Layout Architecture Replication:**
     - Inspected `/tmp/herdr/src/client/shell/render.rs`, `sidebar.rs`, `composition.rs`, and `tabs.rs`. Herdr uses zero surrounding rectangular box frames.
     - **Sidebar:** Full-height borderless column with only a single vertical right divider line (`│`, `border: { right: true, top: false, bottom: false, left: false }`). Inside, sections (`spaces`, `intents & tasks`, `active locks`, `peers`) are demarcated by subtle thin horizontal rules (`─`).
     - **Terminal Pane:** Completely borderless (`top: false, bottom: false, left: false, right: false`). Terminal output flows edge-to-edge from the sidebar divider to the window edge.
     - **Unified Tab Strip:** Positioned at row 0 of the terminal pane. Displays active tab as a solid Electric Lime pill (` 1: Shell `), followed by `+`, with right-aligned session metadata (`● TURF-XXXX • HOST • User`).
     - **Understated Input Bar:** Positioned at the bottom row beneath a single horizontal divider (`─`). Features `turf > ` prompt on the left and right-aligned keybinding shortcuts (`[Tab] Focus  [D] Diff  [F] Files  [Ctrl+B] Sidebar`).
  2. **Ghost Typing Root-Cause Elimination (`¤77CC¥¥77CC...`):**
     - Root Cause: In Blessed, when mouse tracking is active, `utfMouse` mode (`1005`) encodes terminal column/row coordinates > 127 as UTF-8 bytes (e.g. byte 164 is `\xc2\xa4` which decodes as `¤`, byte 165 decodes as `¥`, and `77CC` are SGR/X10 cell coordinates).
     - **Triple-Lock Solution:**
       1. Set `process.env.BLESSED_FORCE_MODES = 'ALLMOTION=0,CELLMOTION=0,UTFMOUSE=0,X10MOUSE=0,SGRMOUSE=0,VT200MOUSE=0,SENDFOCUS=0'` before screen instantiation.
       2. Neutered `screen.program.enableMouse`, `screen.program.disableMouse`, `screen.program.setMouse`, and `screen.enableMouse` into permanent NO-OPs.
       3. Sent raw escape sequence `\x1b[?1006l\x1b[?1016l\x1b[?1015l\x1b[?1005l\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?9l` directly to stdout to ensure the terminal emulator drops all mouse reporting.
       4. Hardened `inputBox._listener` with regex `/^[\u0080-\u00FF]/` to discard any Latin-1 coordinate remnants before they reach the text buffer.
- **Reason:** Matching Herdr's spatial distribution requires dropping nested box containers in favor of single-line dividers and generous whitespace. Neutering terminal mouse tracking prevents terminal emulators from flooding stdin with escape packets, completely resolving ghost typing.
- **Worker Invariant:** Terminal pane must remain borderless. Mouse tracking modes 1003 and 1005 must never be enabled. Input box must strictly filter ANSI escape codes and Latin-1 coordinate sequences.

---

## [MD-045] 2026-11-03: Rust Atomic File Lock Engine, Thread-Safe Intent Board, and Git Worktree Automation
- **Context:** TurfCode requires robust, zero-collision concurrency guarantees across multi-agent pairing sessions. To prevent agents and peers from colliding, clobbering active files, or leaving repositories in half-written deadlocks, core coordination primitives are ported and implemented in Rust under `crates/turf-protocol` and `crates/turf-core`.
- **The Fork:** Relying solely on single-threaded JS runtime locks vs. Native Rust core workspace (`turf-protocol` + `turf-core`) with fail-fast multi-file atomic sequence locking, process-bound heartbeat auto-dissolution, thread-safe intent persistence, and automated git worktree squash-merging.
- **Resolution:** Implemented `crates/turf-protocol` and `crates/turf-core` in the Cargo workspace with:
  1. **Atomic Multi-File Sequence Locking (`LockManager`):**
     - Pre-flight conflict inspection ensures that if ANY file in a declared sequence is held by another task, acquisition fails fast immediately with zero partial locks acquired ([MD-023], [MD-040]).
     - Atomic JSON persistence to `.turf/locks.json` via temporary file writing and atomic rename.
     - TTL lease expiration and background Tokio heartbeat monitor auto-dissolves stale locks.
     - Process-bound lease dissolution via `libc::kill(pid, 0)` automatically releases locks if the owning agent process terminates ([MD-007]).
     - Host manual force-unlock capability (`force_unlock`).
  2. **Thread-Safe Intent Board (`IntentBoard`):**
     - Conforms to task card schema: `{ id, title, assignee, status, target_files, created_at }` with statuses (`queued`, `in_progress`, `done`, `blocked`).
     - Safe concurrent in-memory storage backed by `parking_lot::RwLock` and atomic JSON disk persistence to `.turf/intent-board.json`.
  3. **Git Worktree Automation (`WorktreeManager`):**
     - Automated isolated worktree creation under `.turf/worktrees/<task-id>` with branch `task/<task-id>`.
     - Automated squash-merging into main branch with git trailer formatting (`Co-authored-by: Name <email>`) conforming to [MD-012] and [MD-039].
     - Fail-safe merge conflict handling with automatic clean working tree reset (`git reset --hard HEAD`).
     - Worktree pruning and task branch cleanup (`remove_task_worktree`).
- **Reason:** Guaranteeing concurrency safety across multiple simultaneous AI agents requires strict atomic sequence acquisition before any disk writes occur. Process binding guarantees that crashed or SIGKILLed agent processes never hold locks indefinitely.
- **Worker Invariant:** Multi-file locking must never acquire partial locks on conflict. Merging completed agent worktrees must squash into target branch with Co-authored-by trailers and preserve repository cleanliness on conflict.

---

## [MD-046] 2026-11-03: Native Rust Multiplexer Foundation: `turf-pty` Cross-Platform PTY & `turf-vte` 2D Character Cell Grid
- **Context:** To match and surpass Herdr's speed, robustness, and resource efficiency, TurfCode CLI requires a native Rust terminal multiplexing core to power PTY process containment, high-throughput asynchronous byte streaming, and high-fidelity ANSI terminal emulation.
- **The Fork:** Rely solely on Node.js/Blessed wrappers (`node-pty`) vs. Scaffold a native Rust workspace with modular crates (`turf-pty` using `portable-pty` and `turf-vte` using `vte`) maintaining strict 2D character cell grids, TrueColor ANSI parsing, and cross-platform PTY abstraction.
- **Resolution:** Implement a high-performance native Rust core in a Cargo workspace:
  1. **Cargo Workspace Root:** Initialized at repository root targeting Rust 2021 edition with workspace members `crates/turf-pty`, `crates/turf-vte`, `crates/turf-protocol`, and `crates/turf-core`, with shared dependencies (`portable-pty 0.8`, `vte 0.14`, `tokio 1.x`, `thiserror 1.x`, `tracing 0.1`, `unicode-width 0.2`).
  2. **`crates/turf-pty`:**
     - **Cross-Platform PTY Allocation:** Uses `portable_pty::native_pty_system()` to allocate POSIX PTYs on Linux/WSL/macOS and ConPTY on Windows.
     - **Shell Spawner:** Automatically defaults to `$SHELL` or `bash` on Unix/Linux, and `powershell.exe` on Windows (`#[cfg(windows)]`). Configures standard multiplexer environment (`TERM=xterm-256color`, `COLORTERM=truecolor`).
     - **Thread/Task-Safe Streaming Channels:** Encapsulates the PTY master I/O in worker threads paired with Tokio `mpsc` channels (`PtyReader`, `PtyWriter`) implementing `PtyReadStream` and `PtyWriteStream` traits. Non-blocking with `try_send` and runtime-aware block-in-place execution to prevent Tokio worker thread blocking.
     - **Dynamic Resizing & Lifecycle:** Exposes `resize(rows, cols)` along with `is_alive()`, `try_wait()`, `wait()`, and `kill()`.
  3. **`crates/turf-vte`:**
     - **2D Character Grid Buffer (`ScreenBuffer`):** Maintains row/column bounds, scrolling margins (DECSTBM), character cell storage (`Vec<Vec<Cell>>`), clearing modes (`ED`, `EL`), and text extraction helpers.
     - **Character Cell & Styling:** Rich `Cell` struct supporting UTF-8 characters with `unicode-width` calculation, ANSI 16 named colors, 256-color indexed palette, 24-bit TrueColor RGB (`Color::Rgb(r, g, b)`), and text styling flags (`bold`, `dim`, `italic`, `underline`, `inverse`, `strikethrough`, `blink`, `hidden`).
     - **ANSI State Machine (`vte::Perform`):** Implements `print`, `execute` (`\r`, `\n`, `\t`, `\b`), `csi_dispatch` (SGR formatting, cursor movements CUU, CUD, CUF, CUB, CNL, CPL, CHA, CUP, VPA, ED, EL, SU, SD, margins, cursor save/restore), `esc_dispatch` (DECSC, DECRC, RIS, IND, RI, NEL), and `osc_dispatch` (window titles).
     - **Alternate Screen Buffer (`?1049h` / `?1049l`):** Seamless buffer toggling saving/restoring primary cursor and isolating fullscreen terminal tools (e.g., Vim, htop, less) from scrollback.
  4. **Verification & Testing:** Comprehensive unit and integration test suite (15 tests passing in 0.05s) validating shell spawning, echo transmission, ANSI/RGB color parsing, cursor movement, alternate buffer toggling, and end-to-end PTY-to-VTE streaming pipelines.
- **Reason:** Native Rust gives zero garbage collection latency, microsecond ANSI dispatch, precise ConPTY/POSIX terminal semantics, and complete immunity to node-pty memory bloat or Blessed screen redraw artifacts.
- **Worker Invariant:** `turf-pty` must maintain non-blocking channel streaming safe for both synchronous threads and asynchronous Tokio runtimes. `turf-vte` must correctly preserve primary buffer state when toggling alternate screen buffers.

---

---

## [MD-047] 2026-11-03: Herdr-Grade Ratatui Spatial Multiplexer Cockpit: Borderless Panes, Electric Lime Pill Tabs, and 60fps VTE Frame Blitter
- **Context:** Delivering the pristine, lightning-fast terminal experience demanded by the Captain requires replacing heavy boxed widgets and Node.js wrappers with a native Rust Ratatui cockpit inspired directly by Herdr's spatial layout architecture.
- **The Fork:** Keep nested boxed containers and Blessed/Node rendering vs. Implement `crates/turf-tui` in Ratatui 0.29 + Crossterm 0.28 with borderless terminal panes, solid Electric Lime pill tab strips, a full-height single-divider sidebar, and a 60fps flicker-free VTE-to-Ratatui cell blitter.
- **Resolution:** Implemented `crates/turf-tui` in the Cargo workspace with:
  1. **Herdr Spatial Layout Engine (`CockpitLayout`):**
     - **Full-Height Borderless Sidebar (Left, ~22% width):** Bounded proportionally (22% clamped to min 22 cols) and separated from the main surface by a single vertical divider line (`│`, `COLOR_MUTED_BORDER`). Features 4 borderless typographic sections (`SPACES`, `INTENTS & TASKS`, `ACTIVE LOCKS`, `PEERS`) separated by subtle single horizontal rules (`─`).
     - **Unified Tab Strip (Top Row):** Row 0 renders active tab as an inverted high-contrast Electric Lime pill (` 1: Shell `) in `COLOR_ELECTRIC_LIME` background and `COLOR_BG` foreground with bold styling, followed by `+`, and right-aligned session metadata (`● TURF-XXXX • HOST • user`).
     - **Borderless Terminal Surface:** Direct 2D cell blitting from `turf-vte::ScreenBuffer` into Ratatui frame buffers without surrounding box frames. Supports binary space partition (BSP) split panes (vertical `│` and horizontal `─` splits).
     - **Understated Input Bar (Bottom Row):** Single-line prompt (`turf > `) separated from terminal output above by a subtle single `─` divider line, with right-aligned keybinding shortcuts (`[Tab] Focus`, `[Ctrl+B] Sidebar`, `[Ctrl+T] Tab`, `[Ctrl+S] Split`, `[Ctrl+Q] Quit`).
  2. **Zero-Flicker 60fps Cell Blitter (`vte_blit`):**
     - Zero heap allocations during blit: maps `turf_vte::Cell` directly into Ratatui `Buffer` cells with full SGR flag mapping (bold, dim, italic, underline, reverse, strike, blink, hidden) and 24-bit TrueColor RGB.
     - Blitting occurs in Ratatui's double-buffered diffing engine, delivering pristine 60fps terminal blitting with zero visual flicker.
  3. **Direct PTY Stream & Keystroke Forwarding:**
     - Non-blocking draining of PTY output via `PtyReader::try_recv()` into `turf_vte::Terminal`.
     - Raw Crossterm keystrokes translated into standard ANSI/POSIX terminal sequences and forwarded directly to the active pane's `PtyWriter`.
  4. **Spatial Navigation & Focus Cycling:**
     - `Tab` cycles focus smoothly between `Terminal -> Input -> Sidebar -> Terminal` (or skips Sidebar when hidden).
     - `Ctrl+B` collapses and expands the left sidebar.
     - `Ctrl+W` cycles focus across split panes in the current tab.
     - `Ctrl+1..9` jumps directly to tabs.
     - `Ctrl+S` and `Ctrl+H` trigger vertical and horizontal pane splits.
  5. **Triple-Lock Mouse Escape Suppression:**
     - Transmits `\x1b[?1006l\x1b[?1016l\x1b[?1015l\x1b[?1005l\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?9l` directly to stdout and filters Latin-1 mouse sequence remnants (`0x80..=0xFF`), completely eradicating ghost typing.
  6. **Interactive Demo & Test Suite:**
     - Provided `examples/multiplexer_demo.rs` for immediate interactive testing via `cargo run -p turf-tui --example multiplexer_demo`.
     - 9 comprehensive unit and headless rendering tests asserting layout rect geometry, 22% sidebar bounding, divider lines, tab pill rendering, split tree partitioning, and focus cycling.
- **Reason:** Ratatui's diff-based frame blitting eliminates all screen flicker and tearing. A pure single-divider layout maximizes available terminal columns while preserving clean spatial hierarchy and genuine system visibility.
- **Worker Invariant:** Cockpit terminal panes must remain strictly borderless. All dividers must render as single lines (`│` and `─`) without enclosing rectangular frames. Active tab must render as a high-contrast Electric Lime pill. Terminal keystrokes must route non-blockingly to the active PTY master.

---

## [MD-048] 2026-11-03: Native Rust Multiplayer P2P WebSocket Tunnel, AES-256-GCM Encrypted State Synchronization, and Unified Single Binary CLI Entry Point
- **Context:** TurfCode requires a robust, low-latency, cross-platform multiplayer networking layer to connect developers and remote AI coding agents in real-time, authenticate sessions across firewalls using unambiguous room codes, maintain synchronized intent boards and file locks without data corruption, and provide a single binary CLI executable (`turf` / `turfcode`).
- **The Fork:** Rely on Node.js WebSocket wrappers (`ws`) and external proxy processes vs. Build a native, asynchronous Rust networking crate (`crates/turf-net`) powered by `tokio-tungstenite 0.24`, `aes-gcm 0.10`, `sha2 0.10`, and `futures-util 0.3`, paired with a compiled binary CLI entry point.
- **Resolution:** Implement `crates/turf-net` in the Cargo workspace:
  1. **Cryptographic Engine & Unambiguous Room Codes (`crypto.rs`):**
     - Ephemeral 6-character room code generator in `TURF-XXXX` format using an unambiguous 32-character alphabet (`23456789ABCDEFGHJKLMNPQRSTUVWXYZ`, excluding `0`, `1`, `I`, `O`) conforming to [MD-025].
     - SHA-256 key derivation yielding a 256-bit symmetric key from normalized room codes.
     - Authenticated payload encryption and decryption via AES-256-GCM with 96-bit random nonces and 128-bit authentication tags (`EncryptedPayload { iv, tag, data }`), ensuring cross-language compatibility with Node.js clients and complete immunity to ciphertext tampering or replay.
     - In-memory credential exchange [MD-019]: client transmits ephemeral tokens over AES-GCM without writing tokens or secrets to disk.
  2. **WebSocket Tunnel Server (`TunnelServer`):**
     - Binds asynchronously using Tokio `TcpListener` on a designated or ephemeral port with WebSocket upgrade via `tokio_tungstenite::accept_async`.
     - 10-second handshake timeout enforcing room code verification and credentials payload decryption before granting access.
     - Maintains in-memory connected peer registry (`ServerPeer`) with unbounded `mpsc` outgoing message channels for full-duplex non-blocking streaming.
     - Integrates directly with `turf-core`'s `IntentBoard` and `LockManager` (with fallback to thread-safe in-memory collections when running standalone).
  3. **WebSocket Tunnel Client (`TunnelClient`):**
     - Asynchronous WebSocket client using `tokio_tungstenite::connect_async`.
     - Automatically derives key and encrypts `PeerCredentials` with the target room code during handshake.
     - Spawns background worker task for concurrent full-duplex message transmission and reception.
     - Maintains thread-safe replicated local copies of Intent Board tasks (`replicated_tasks`), active file locks (`replicated_locks`), and connected peers (`connected_peers`), accessible via non-blocking asynchronous getter methods.
  4. **Live State Delta Replication & Presence:**
     - Bidirectional delta broadcast: additions and status updates to tasks (`TaskAdd`, `TaskUpdate`) and atomic file lock acquisitions/releases (`LockAcquire`, `LockRelease`) automatically broadcast updated state snapshots (`TasksSync`, `LocksSync`) to all connected peers.
     - Automatic peer presence notifications (`PeerJoined`, `PeerLeft`, `PeersSync`) broadcast on peer connection and clean or abnormal disconnection.
     - Pre-flight atomic collision detection: prevents concurrent agents from acquiring locks on overlapping files across the network.
  5. **Unified Single Binary CLI Entry Point (`cli.rs`, `bin/turf.rs`, `bin/turfcode.rs`):**
     - Provides native compiled binaries `turf` and `turfcode`.
     - Interactive two-option launcher prompt conforming to [MD-041] (`[1] Create a Turf`, `[2] Join a Turf`, `[3] Exit`) when run without arguments in an interactive terminal.
     - Subcommands: `turf host [dir]` (boots server, initializes `.turf/`, reveals room code and local endpoint) and `turf join <id> [url]` (executes encrypted handshake and synchronizes state).
  6. **Comprehensive Test Suite:**
     - 8 unit tests in `crypto` and `protocol` verifying key derivation, AES-GCM encryption/decryption roundtrip, tampered ciphertext rejection, and WebSocket message conversion.
     - 4 integration tests in `handshake_tests` verifying successful encrypted handshake, room code mismatch rejection, corrupted payload rejection, and multi-client connection.
     - 2 integration tests in `state_replication_tests` validating end-to-end live state delta replication between host and multiple peers, task addition and status sync, multi-file lock synchronization with conflict rejection, peer presence join/leave events, and integration with `turf-core` disk persistence.
- **Reason:** Native Rust networking eliminates Node.js GC stalls and connection dropped errors, provides cryptographically verified payload integrity using hardware-accelerated AES-GCM, and allows TurfCode to distribute as a single standalone binary with zero Node.js runtime dependencies.
- **Worker Invariant:** Room code handshakes must verify room code and decrypt credentials payload before peer registration. State mutations on tasks or locks must immediately broadcast state deltas to all authenticated peers. Unambiguous room codes must strictly exclude `0`, `1`, `I`, `O`.

---

## [MD-049] 2026-11-04: TurfCode Interactive Launcher Cockpit Integration, Banner Typographic Fix, Clean ANSI Screen Clear, and Cross-Platform File Manager Automation
- **Context:** The interactive TurfCode CLI launcher (`turf` / `turfcode`) required several key UX and functional enhancements:
  1. The ASCII art splash banner in `crates/turf-net/src/cli.rs` had a missing trailing `E`, spelling `TURFCOD` instead of `TURFCODE`.
  2. When selecting option 1 (`Create a Turf`) or option 2 (`Join a Turf`), the previous menu options remained on screen, creating visual clutter.
  3. Creating a Turf needed to prompt for folder path, automatically open the system file manager directly on that folder, display the session ID clearly, and boot directly into the Herdr-grade `turf-tui` Ratatui Multiplexer Cockpit (`TerminalCockpit`) with a live PTY shell.
  4. Joining a Turf needed a clean prompt asking directly for the Session ID / room code (`TURF-XXXX`) and to connect and boot directly into the live `TerminalCockpit`.
  5. State synchronization (connected peers, intent board tasks, and file locks) needed to flow continuously into the cockpit's `SidebarState`.
- **The Fork:** Keep the launcher as a basic text-only console loop vs. Integrate `turf-net` with `turf-tui` to directly boot the high-performance 60fps Ratatui multiplexer cockpit while preserving headless non-terminal waiting for automated CI/tests.
- **Resolution:**
  1. **Complete `TURFCODE` Block Banner:**
     - Added the missing letter `E` in matching 6-row block font (`███████╗`, `██╔════╝`, `█████╗  `, `██╔══╝  `, `███████╗`, `╚══════╝`).
     - Expanded the border width from 67 to 73 `═` characters, aligning outer box corners (`╔`, `╗`, `╚`, `╝`) and centering the subtitle `ELECTRIC LIME COCKPIT v{VERSION}` perfectly with 77 characters total line width.
  2. **ANSI Clean Screen Clearing & Header Re-render:**
     - Upon menu selection (`1` or `2`), emitted standard ANSI escape sequence `\x1b[2J\x1b[1;1H` and re-rendered the clean splash header so menu selection choices cleanly disappear before prompting.
  3. **Cross-Platform Detached File Manager Opening (`open_file_manager`):**
     - WSL detection via `/proc/sys/fs/binfmt_misc/WSLInterop` or `WSL_DISTRO_NAME` env var, converting UNIX path to Windows path via `wslpath -w` and spawning `explorer.exe <path>` detached with stdio suppressed.
     - Native Windows: spawns `explorer.exe <path>` detached.
     - macOS: spawns `open <path>` detached.
     - Native Linux: spawns `xdg-open <path>` detached.
     - Skips opening when headless or when `--no-cockpit` flag is provided; gracefully ignores exit status if no desktop GUI is available.
  4. **Live Multiplexer Cockpit Booting (`TerminalCockpit`):**
     - Connected `turf-net` to `turf-tui`, `ratatui 0.29`, and `crossterm 0.28`.
     - In `run_host`: launches `TerminalCockpit::new_live(&room_code, "HOST", &username, rows, cols)` on interactive terminals, binding to default port 7873 (with dynamic port fallback).
     - In `run_join`: cleans and normalizes user input into `TURF-XXXX` format and boots `TerminalCockpit::new_live(&clean_code, "PEER", &username, rows, cols)`.
     - Added `TerminalCockpit::run_event_loop_with` to allow callers to drain incoming network events and synchronize state on every 60fps frame tick.
  5. **Live Sidebar State Forwarding:**
     - Continuously translates and forwards server/client broadcast events (`PeerConnected`, `PeerDisconnected`, `PeerJoined`, `PeerLeft`, `TasksChanged`, `LocksChanged`, `TasksSync`, `LocksSync`, `PeersSync`) into `cockpit.sidebar_state`.
  6. **Headless & Test Preservation:**
     - If stdout is not a terminal or if `--no-cockpit` is passed, the CLI preserves headless console logging and signal waiting for automated integration tests and server environments.
- **Reason:** Provides an immediate, Herdr-grade multi-pane developer cockpit upon creating or joining a turf, with live interactive terminal multiplexing, auto-opened workspace directories, and real-time state synchronization.
- **Worker Invariant:** ASCII banners must maintain uniform column width across all rows. Cockpit event loops must drain PTY chunks and network deltas at 60fps without blocking user input. Non-interactive environments must safely bypass TUI initialization.

---

## [MD-050] 2026-11-04: Herdr-Grade Visual Architecture, Catppuccin Mocha / Obsidian Slate Theme, Non-Conflicting Alt+Key Navigation, and Uninterrupted PTY Input Streaming
- **Context:** TurfCode's terminal multiplexer required elevation to true Herdr-grade stability, visual polish, and performance benchmarks based on direct reverse-engineering of Herdr's architecture:
  1. The global event loop was intercepting raw `Tab` even while focused on the active terminal pane, breaking tab-completion in bash/zsh, and command shortcuts like `Ctrl+W` (delete word in readline/vim), `Ctrl+S` (terminal XOFF / search), and `Ctrl+B` (page up) conflicted with standard terminal applications.
  2. The neon-green high-contrast block matrix theme filled terminal cell backgrounds with opaque `#081008` green-black, preventing native terminal transparency/blur and causing jarring inverted neon cursor artifacts.
  3. The left sidebar rendered blocky boxes with giant vertical gaps rather than Herdr's seamless continuous layout with muted lowercase headers and clean hairline dividers.
  4. The top tab strip and bottom prompt needed sleek full-width chrome matching Herdr (soft blue/slate active tab pill, compact inactive tabs, right-aligned session badge, minimal `> █` prompt, and dimmed `« ? for shortcuts` helper).
- **The Fork:** Keep the neon matrix theme and Ctrl-based global hotkeys vs. Adopt a refined Catppuccin Mocha / Herdr Dark Slate theme with native `Color::Reset` transparency, non-conflicting `Alt+Key` spatial multiplexer navigation, and 100% transparent PTY key forwarding.
- **Resolution:**
  1. **Keyboard & PTY Input Stability:**
     - Removed the global `Tab` intercept in `handle_key` when focused on `FocusArea::Terminal`. All raw keys (`Tab`, `Enter`, `Backspace`, `Ctrl+C`, `Ctrl+D`, `Ctrl+W`, arrows, letters) now stream 100% directly to the active PTY master via `key_to_bytes` and `pane.write_pty`.
     - Re-mapped multiplexer hotkeys from intrusive `Ctrl+...` to non-conflicting `Alt+...`: `Alt+T` (new tab), `Alt+W` (close tab), `Alt+1..9` (switch tab), `Alt+V` (vertical split), `Alt+S` (horizontal split), `Alt+B` (toggle sidebar), and `Alt+Enter` (toggle prompt focus).
     - Preserved `Ctrl+Q` and `Alt+Q` for clean exit.
  2. **Catppuccin Mocha & Herdr Dark Slate Theme Architecture:**
     - Updated `crates/turf-tui/src/theme.rs` to the official Catppuccin Mocha / Obsidian Slate palette:
       - `COLOR_BG = Color::Reset` for true native terminal transparency and blur blending.
       - `COLOR_HEADER_BG = Color::Rgb(24, 25, 38)` (Catppuccin Mantle #181926).
       - `COLOR_ACCENT = Color::Rgb(137, 180, 250)` (Catppuccin Blue #89b4fa).
       - `COLOR_MUTED_BORDER = Color::Rgb(49, 50, 68)` (Catppuccin Surface0 #313244).
       - `COLOR_TEXT_PRIMARY = Color::Rgb(205, 214, 244)` (Catppuccin Text #cdd6f4).
       - `COLOR_TEXT_MUTED = Color::Rgb(108, 112, 134)` (Catppuccin Overlay0 #6c7086).
       - `COLOR_TEXT_DIM = Color::Rgb(88, 91, 112)` (Catppuccin Surface2 #585b70).
       - `COLOR_ACTIVE_PILL = Color::Rgb(30, 32, 48)` (Catppuccin Crust / dark slate highlight).
       - `COLOR_GREEN_DOT = Color::Rgb(166, 227, 161)` (Catppuccin Green #a6e3a1).
       - `COLOR_CYAN = Color::Rgb(137, 220, 235)` (Catppuccin Sky #89dceb).
  3. **Zero-Artifact VTE Cell Blitting & Steady Cursor:**
     - In `vte_blit.rs`, mapped `VteColor::Default` background to `RatColor::Reset` and trailing/empty cells to `RatColor::Reset`.
     - Replaced neon green inverted cursor artifacting with a clean reverse-video steady block cursor (`Modifier::REVERSED` with `COLOR_TEXT_PRIMARY`).
  4. **Continuous Seamless Sidebar Architecture:**
     - Replaced rectangular box containers with a continuous, unified sidebar layout (`row_gap = 0`, background `Color::Reset`).
     - Rendered lowercase muted headers: `spaces`, `intents & tasks ({})`, `active locks ({})`, and `peers ({})`.
     - Top spaces section displays `○ <dir>` with branch in subtle cyan (`COLOR_CYAN`).
     - Bottom peers section renders active user capsule pill (`✓ <username> (you) · host`) on dark slate rounded highlight (`COLOR_ACTIVE_PILL` background, `COLOR_ACCENT` text).
  5. **Top Tab Bar & Bottom Prompt Chrome:**
     - Top tab strip: soft blue/slate pill for active tab (` 1: Shell `), compact numbers for inactive tabs (` 2 `), subtle `+` button, and right-aligned session badge (`● TURF-XXXX • HOST`).
     - Bottom input prompt: single subtle hairline divider (`─`), minimal prompt `> █`, and right-aligned dimmed `« ? for shortcuts` helper text.
- **Reason:** Restores full terminal utility (tab completion and standard shell keybindings work without multiplexer interference), provides high-end visual elegance matching Herdr, and enables native terminal transparency without jarring color blocks.
- **Worker Invariant:** Raw `Tab` and readline `Ctrl+...` keystrokes must never be intercepted when terminal panes have focus. Multiplexer control must strictly use `Alt+...` combinations. Default cell backgrounds in VTE blitting must remain `Color::Reset`.




