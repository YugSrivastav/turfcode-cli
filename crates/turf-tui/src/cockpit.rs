//! Herdr-Grade Ratatui Spatial Multiplexer Cockpit for TurfCode CLI.
//!
//! Conforms to [MD-001], [MD-004], [MD-042], and [MD-044]:
//! - Borderless terminal panes blitting directly from `turf-vte::ScreenBuffer`
//! - Inverted high-contrast Electric Lime solid pill tab strip
//! - Borderless left sidebar with single vertical divider (`│`) and 4 sections
//! - Minimal bottom prompt line with subtle `─` divider and right-aligned shortcuts
//! - 60fps flicker-free frame blitting event loop

use std::io::{self, stdout, Write};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::Terminal as RatTerminal;
use tokio::sync::broadcast;
use turf_daemon::DaemonClient;
use turf_protocol::intent::{TaskCard, TaskStatus};
use turf_protocol::ipc::DaemonEvent;
use turf_protocol::locks::FileLock;

use crate::input::{render_input_bar, InputState};
use crate::keys::key_to_bytes;
use crate::layout::CockpitLayout;
use crate::pane::{Pane, PaneBackend, PaneManager, SplitDirection};
use crate::sidebar::{render_sidebar, SidebarState};
use crate::tabs::{render_tab_strip, TabItem};

/// Focus areas for spatial navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    Terminal,
    Sidebar,
    Input,
}

/// Descriptor for a multiplexer tab containing its own split pane manager.
pub struct Tab {
    pub id: usize,
    pub title: String,
    pub pane_manager: PaneManager,
}

/// The master Ratatui spatial multiplexer cockpit.
pub struct TerminalCockpit {
    pub session_code: String,
    pub role: String,
    pub username: String,
    pub sidebar_visible: bool,
    pub sidebar_state: SidebarState,
    pub tabs: Vec<Tab>,
    pub active_tab_index: usize,
    pub focus: FocusArea,
    pub input_state: InputState,
    pub should_exit: bool,
    pub is_detached: bool,
    pub next_tab_id: usize,
    pub daemon_client: Option<DaemonClient>,
    pub daemon_event_rx: Option<broadcast::Receiver<DaemonEvent>>,
}

impl TerminalCockpit {
    /// Initialize a new TerminalCockpit with headless panes (for tests).
    pub fn new_headless(
        session_code: impl Into<String>,
        role: impl Into<String>,
        username: impl Into<String>,
        rows: usize,
        cols: usize,
    ) -> Self {
        let initial_pane = Pane::new_headless(1, "Shell 1", rows, cols);
        let tab = Tab {
            id: 1,
            title: "Shell".to_string(),
            pane_manager: PaneManager::new(initial_pane),
        };

        Self {
            session_code: session_code.into(),
            role: role.into(),
            username: username.into(),
            sidebar_visible: true,
            sidebar_state: SidebarState::default(),
            tabs: vec![tab],
            active_tab_index: 0,
            focus: FocusArea::Terminal,
            input_state: InputState::default(),
            should_exit: false,
            is_detached: false,
            next_tab_id: 2,
            daemon_client: None,
            daemon_event_rx: None,
        }
    }

    /// Initialize a new TerminalCockpit with live in-process PTY shell panes.
    pub fn new_standalone(
        session_code: impl Into<String>,
        role: impl Into<String>,
        username: impl Into<String>,
        rows: u16,
        cols: u16,
    ) -> Result<Self, turf_pty::PtyError> {
        let initial_pane = Pane::spawn_shell(1, "Shell 1", rows, cols)?;
        let tab = Tab {
            id: 1,
            title: "Shell".to_string(),
            pane_manager: PaneManager::new(initial_pane),
        };

        Ok(Self {
            session_code: session_code.into(),
            role: role.into(),
            username: username.into(),
            sidebar_visible: true,
            sidebar_state: SidebarState::default(),
            tabs: vec![tab],
            active_tab_index: 0,
            focus: FocusArea::Terminal,
            input_state: InputState::default(),
            should_exit: false,
            is_detached: false,
            next_tab_id: 2,
            daemon_client: None,
            daemon_event_rx: None,
        })
    }

    /// Initialize a new TerminalCockpit with live PTY shell panes (standalone alias).
    pub fn new_live(
        session_code: impl Into<String>,
        role: impl Into<String>,
        username: impl Into<String>,
        rows: u16,
        cols: u16,
    ) -> Result<Self, turf_pty::PtyError> {
        Self::new_standalone(session_code, role, username, rows, cols)
    }

    /// Initialize a new thin-client TerminalCockpit connected to the background daemon.
    pub async fn new_daemon(
        client: DaemonClient,
        session_code: impl Into<String>,
        role: impl Into<String>,
        username: impl Into<String>,
        rows: u16,
        cols: u16,
    ) -> anyhow::Result<Self> {
        let session_code_str = session_code.into();
        let existing_panes = client.attach_session(&session_code_str).await.unwrap_or_default();
        let event_rx = client.subscribe();

        let mut initial_panes = Vec::new();
        if !existing_panes.is_empty() {
            for (idx, &pane_id) in existing_panes.iter().enumerate() {
                let mut p = Pane::new_daemon(
                    idx + 1,
                    format!("Shell {}", idx + 1),
                    pane_id as u64,
                    client.clone(),
                    rows,
                    cols,
                );
                if let Ok(buffer_bytes) = client.read_pane(pane_id).await {
                    let text = String::from_utf8_lossy(&buffer_bytes);
                    for (y, line) in text.lines().enumerate() {
                        let trimmed = line.trim_end();
                        if !trimmed.is_empty() {
                            p.terminal.process_str(&format!("\x1b[{};1H{}", y + 1, trimmed));
                        }
                    }
                }
                initial_panes.push(p);
            }
        } else {
            let daemon_pane_id = client.spawn_pane(1, None, None).await?;
            let p = Pane::new_daemon(
                1,
                "Shell 1",
                daemon_pane_id as u64,
                client.clone(),
                rows,
                cols,
            );
            initial_panes.push(p);
        }

        let first_pane = initial_panes.remove(0);
        let mut pane_manager = PaneManager::new(first_pane);
        for remaining_pane in initial_panes {
            pane_manager.split_active_daemon(
                SplitDirection::Vertical,
                remaining_pane.daemon_pane_id().unwrap_or(1),
                client.clone(),
            );
        }

        let tab = Tab {
            id: 1,
            title: "Shell".to_string(),
            pane_manager,
        };

        Ok(Self {
            session_code: session_code_str,
            role: role.into(),
            username: username.into(),
            sidebar_visible: true,
            sidebar_state: SidebarState::default(),
            tabs: vec![tab],
            active_tab_index: 0,
            focus: FocusArea::Terminal,
            input_state: InputState::default(),
            should_exit: false,
            is_detached: false,
            next_tab_id: 2,
            daemon_client: Some(client),
            daemon_event_rx: Some(event_rx),
        })
    }

    /// Focus cycling between Sidebar -> Terminal -> Input -> Sidebar.
    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            FocusArea::Terminal => FocusArea::Input,
            FocusArea::Input => {
                if self.sidebar_visible {
                    FocusArea::Sidebar
                } else {
                    FocusArea::Terminal
                }
            }
            FocusArea::Sidebar => FocusArea::Terminal,
        };
    }

    /// Set explicit focus area.
    pub fn set_focus(&mut self, focus: FocusArea) {
        self.focus = focus;
    }

    /// Toggle left sidebar visibility ([MD-032] Ctrl+B).
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
        if !self.sidebar_visible && self.focus == FocusArea::Sidebar {
            self.focus = FocusArea::Terminal;
        }
    }

    /// Add a new tab backed by a headless terminal pane.
    pub fn add_tab_headless(&mut self, title: impl Into<String>) -> usize {
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;
        let title_str = title.into();

        let initial_pane = Pane::new_headless(1, format!("{title_str} 1"), 24, 80);
        let tab = Tab {
            id: tab_id,
            title: title_str,
            pane_manager: PaneManager::new(initial_pane),
        };
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        tab_id
    }

    /// Add a new tab backed by a live PTY shell.
    pub fn add_tab_shell(&mut self, title: impl Into<String>) -> Result<usize, turf_pty::PtyError> {
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;
        let title_str = title.into();

        let initial_pane = Pane::spawn_shell(1, format!("{title_str} 1"), 24, 80)?;
        let tab = Tab {
            id: tab_id,
            title: title_str,
            pane_manager: PaneManager::new(initial_pane),
        };
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        Ok(tab_id)
    }

    /// Switch to tab by index (0-based).
    pub fn switch_tab(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            true
        } else {
            false
        }
    }

    /// Switch to next tab.
    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab_index = (self.active_tab_index + 1) % self.tabs.len();
        }
    }

    /// Switch to previous tab.
    pub fn prev_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active_tab_index = if self.active_tab_index == 0 {
                self.tabs.len() - 1
            } else {
                self.active_tab_index - 1
            };
        }
    }

    /// Close active tab (unless it is the only remaining tab).
    pub fn close_active_tab(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }
        self.tabs.remove(self.active_tab_index);
        if self.active_tab_index >= self.tabs.len() {
            self.active_tab_index = self.tabs.len() - 1;
        }
        true
    }

    /// Split active pane in the current tab vertically or horizontally (live shell).
    pub fn split_active_pane(&mut self, direction: SplitDirection) -> Result<usize, turf_pty::PtyError> {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.pane_manager.split_active_shell(direction)
        } else {
            Err(turf_pty::PtyError::Allocation("No active tab".to_string()))
        }
    }

    /// Split active pane in the current tab (headless).
    pub fn split_active_pane_headless(&mut self, direction: SplitDirection) -> usize {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.pane_manager.split_active_headless(direction)
        } else {
            0
        }
    }

    /// Close active pane in current tab.
    pub fn close_active_pane(&mut self) -> bool {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.pane_manager.close_active()
        } else {
            false
        }
    }

    /// Cycle focus between split panes in the current tab.
    pub fn cycle_pane_focus(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            tab.pane_manager.focus_next();
        }
    }

    /// Poll all PTY readers across all tabs.
    pub fn poll_ptys(&mut self) -> usize {
        let mut total = 0;
        for tab in &mut self.tabs {
            total += tab.pane_manager.poll_all_ptys();
        }
        total
    }

    /// Poll and drain asynchronous broadcast events from the daemon.
    pub fn poll_daemon_events(&mut self) -> usize {
        let mut events = Vec::new();
        if let Some(rx) = &mut self.daemon_event_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }

        let mut total_bytes = 0;
        for event in events {
            match event {
                DaemonEvent::PaneOutput { pane_id, bytes } => {
                    total_bytes += bytes.len();
                    self.feed_pane_output(pane_id, &bytes);
                }
                DaemonEvent::PaneStatusChanged { pane_id, foreground_cmd } => {
                    for tab in &mut self.tabs {
                        for pane in tab.pane_manager.all_panes_mut() {
                            let matches = match &pane.backend {
                                PaneBackend::Daemon { pane_id: pid, .. } => *pid as usize == pane_id,
                                _ => pane.id == pane_id,
                            };
                            if matches {
                                pane.foreground_cmd = foreground_cmd.clone();
                                if let Some(cmd) = &foreground_cmd {
                                    pane.title = cmd.clone();
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        total_bytes
    }

    /// Feed incoming streaming bytes directly into the target pane's VTE parser.
    pub fn feed_pane_output(&mut self, target_pane_id: usize, bytes: &[u8]) {
        for tab in &mut self.tabs {
            for pane in tab.pane_manager.all_panes_mut() {
                let matches = match &pane.backend {
                    PaneBackend::Daemon { pane_id, .. } => *pane_id as usize == target_pane_id,
                    _ => pane.id == target_pane_id,
                };
                if matches {
                    pane.terminal.process(bytes);
                    return;
                }
            }
        }
    }

    /// Gracefully detach from the daemon session without killing child processes or stopping the daemon.
    pub fn detach(&mut self) {
        self.is_detached = true;
        self.should_exit = true;
        if let Some(client) = &self.daemon_client {
            let client = client.clone();
            let session_id = self.session_code.clone();
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = client.detach_session(&session_id).await;
                });
            }
        }
    }

    /// Refresh process titles across all tabs and panes.
    pub fn refresh_process_titles(&mut self) {
        for tab in &mut self.tabs {
            tab.pane_manager.refresh_process_titles();
        }
    }

    /// Add a tab automatically using daemon if available, live shell, or headless fallback.
    pub fn add_tab_auto(&mut self, title: &str) -> usize {
        if let Some(client) = self.daemon_client.clone() {
            let tab_id = self.next_tab_id;
            self.next_tab_id += 1;
            let title_str = title.to_string();

            let pid_opt = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let client_c = client.clone();
                tokio::task::block_in_place(|| {
                    handle.block_on(async {
                        client_c.spawn_pane(tab_id, None, None).await.ok()
                    })
                })
            } else {
                None
            };

            let daemon_pid = pid_opt.unwrap_or(tab_id) as u64;
            let initial_pane = Pane::new_daemon(
                1,
                format!("{title_str} 1"),
                daemon_pid,
                client,
                24,
                80,
            );
            let tab = Tab {
                id: tab_id,
                title: title_str,
                pane_manager: PaneManager::new(initial_pane),
            };
            self.tabs.push(tab);
            self.active_tab_index = self.tabs.len() - 1;
            return tab_id;
        }

        let is_headless = self
            .tabs
            .first()
            .and_then(|t| t.pane_manager.active_pane())
            .is_none_or(|p| p.is_headless());

        if is_headless {
            self.add_tab_headless(title)
        } else {
            self.add_tab_shell(title).unwrap_or_else(|_| self.add_tab_headless(title))
        }
    }

    /// Split active pane automatically using daemon if available, live shell, or headless fallback.
    pub fn split_active_pane_auto(&mut self, direction: SplitDirection) -> usize {
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            if let Some(client) = self.daemon_client.clone() {
                let active_pane = tab.pane_manager.active_pane();
                let parent_daemon_pid = active_pane.and_then(|p| p.daemon_pane_id()).unwrap_or(1) as usize;
                let dir_str = match direction {
                    SplitDirection::Vertical => "vertical",
                    SplitDirection::Horizontal => "horizontal",
                };

                let pid_opt = if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    let client_c = client.clone();
                    tokio::task::block_in_place(|| {
                        handle.block_on(async {
                            client_c.split_pane(parent_daemon_pid, dir_str).await.ok()
                        })
                    })
                } else {
                    None
                };

                let new_daemon_pid = pid_opt.unwrap_or(parent_daemon_pid + 1) as u64;
                return tab.pane_manager.split_active_daemon(direction, new_daemon_pid, client);
            }

            let is_headless = tab.pane_manager.active_pane().is_none_or(|p| p.is_headless());
            if is_headless {
                tab.pane_manager.split_active_headless(direction)
            } else {
                tab.pane_manager.split_active_shell(direction).unwrap_or_else(|_| {
                    tab.pane_manager.split_active_headless(direction)
                })
            }
        } else {
            0
        }
    }

    /// Execute a built-in `/slash` command from the bottom input prompt.
    pub fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        if parts.is_empty() {
            return;
        }

        match parts[0] {
            "/quit" | "/exit" | "/q" => {
                self.should_exit = true;
            }
            "/split" | "/split-v" | "/vsplit" => {
                self.split_active_pane_auto(SplitDirection::Vertical);
                self.focus = FocusArea::Terminal;
            }
            "/split-h" | "/hsplit" => {
                self.split_active_pane_auto(SplitDirection::Horizontal);
                self.focus = FocusArea::Terminal;
            }
            "/close" => {
                self.close_active_pane();
                self.focus = FocusArea::Terminal;
            }
            "/tab" | "/new" => {
                let title = parts.get(1).copied().unwrap_or("Shell");
                self.add_tab_auto(title);
                self.focus = FocusArea::Terminal;
            }
            "/close-tab" => {
                self.close_active_tab();
                self.focus = FocusArea::Terminal;
            }
            "/task" => {
                if parts.len() > 1 {
                    let title = parts[1..].join(" ");
                    let task = TaskCard::new(
                        format!("task-{}", self.sidebar_state.tasks.len() + 1),
                        title,
                        &self.username,
                        TaskStatus::InProgress,
                        vec![],
                    );
                    self.sidebar_state.tasks.push(task);
                }
            }
            "/lock" => {
                if parts.len() > 1 {
                    let file_path = parts[1];
                    let lock = FileLock::new(
                        file_path,
                        &self.username,
                        "task-1",
                        30_000,
                        None,
                    );
                    self.sidebar_state.locks.push(lock);
                }
            }
            "/unlock" => {
                if parts.len() > 1 {
                    let file_path = parts[1];
                    self.sidebar_state.locks.retain(|l| l.file_path != file_path);
                }
            }
            "/sidebar" => {
                self.toggle_sidebar();
            }
            "/detach" => {
                self.detach();
            }
            _ => {}
        }
    }

    /// Process a Crossterm keyboard event.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // --- 1. Exit shortcut (Ctrl+Q or Alt+Q) ---
        if (key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT))
            && matches!(key.code, KeyCode::Char('q') | KeyCode::Char('Q'))
        {
            self.should_exit = true;
            return true;
        }

        // --- 2. Detach shortcut (Alt+D) ---
        if key.modifiers.contains(KeyModifiers::ALT)
            && matches!(key.code, KeyCode::Char('d') | KeyCode::Char('D'))
        {
            self.detach();
            return true;
        }

        // --- 3. Alt+Enter toggles prompt focus ---
        if key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Enter {
            self.focus = if self.focus == FocusArea::Input {
                FocusArea::Terminal
            } else {
                FocusArea::Input
            };
            return true;
        }

        // --- 3. Viewport Scrolling Hotkeys ---
        // Shift+PageUp, Alt+Up, or Alt+[: scroll up by 10 lines
        // Shift+PageDown or Alt+Down: scroll down toward live stream
        let is_scroll_up = (key.modifiers.contains(KeyModifiers::ALT) && (key.code == KeyCode::Up || key.code == KeyCode::Char('[')))
            || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::PageUp)
            || (key.code == KeyCode::PageUp && key.modifiers.is_empty());
        let is_scroll_down = (key.modifiers.contains(KeyModifiers::ALT) && key.code == KeyCode::Down)
            || (key.modifiers.contains(KeyModifiers::SHIFT) && key.code == KeyCode::PageDown)
            || (key.code == KeyCode::PageDown && key.modifiers.is_empty());

        if is_scroll_up {
            if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                if let Some(pane) = tab.pane_manager.active_pane_mut() {
                    pane.terminal.scroll_up_viewport(10);
                    return true;
                }
            }
        }

        if is_scroll_down {
            if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                if let Some(pane) = tab.pane_manager.active_pane_mut() {
                    pane.terminal.scroll_down_viewport(10);
                    return true;
                }
            }
        }

        // --- 4. Non-conflicting Alt+Key Multiplexer Hotkeys ---
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    self.add_tab_auto("Shell");
                    return true;
                }
                KeyCode::Char('w') | KeyCode::Char('W') => {
                    self.close_active_tab();
                    return true;
                }
                KeyCode::Char('b') | KeyCode::Char('B') => {
                    self.toggle_sidebar();
                    return true;
                }
                KeyCode::Char('v') | KeyCode::Char('V') => {
                    self.split_active_pane_auto(SplitDirection::Vertical);
                    return true;
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    self.split_active_pane_auto(SplitDirection::Horizontal);
                    return true;
                }
                KeyCode::Char(c @ '1'..='9') => {
                    let idx = (c as usize) - ('1' as usize);
                    self.switch_tab(idx);
                    return true;
                }
                _ => {}
            }
        }

        // --- 4. Tab cycles focus ONLY when NOT in Terminal ---
        if self.focus != FocusArea::Terminal
            && key.code == KeyCode::Tab
            && !key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.cycle_focus();
            return true;
        }

        // --- 6. Focus-Specific Key Handling ---
        match self.focus {
            FocusArea::Terminal => {
                // Keystrokes in terminal focus forward directly to the active PTY master
                if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                    if let Some(pane) = tab.pane_manager.active_pane_mut() {
                        // Esc immediately snaps scroll_offset back to 0 if scrolled up
                        if key.code == KeyCode::Esc && pane.terminal.scroll_offset() > 0 {
                            pane.terminal.reset_viewport_scroll();
                            return true;
                        }

                        if let Some(bytes) = key_to_bytes(&key) {
                            // On typing user input, immediately snap scroll_offset back to live view
                            pane.terminal.reset_viewport_scroll();
                            pane.write_pty(&bytes);
                            return true;
                        }
                    }
                }
            }
            FocusArea::Input => {
                match key.code {
                    KeyCode::Enter => {
                        if let Some(cmd) = self.input_state.submit() {
                            self.execute_command(&cmd);
                        }
                        return true;
                    }
                    KeyCode::Esc => {
                        self.focus = FocusArea::Terminal;
                        return true;
                    }
                    KeyCode::Backspace => {
                        self.input_state.backspace();
                        return true;
                    }
                    KeyCode::Delete => {
                        self.input_state.delete();
                        return true;
                    }
                    KeyCode::Left => {
                        self.input_state.move_cursor_left();
                        return true;
                    }
                    KeyCode::Right => {
                        self.input_state.move_cursor_right();
                        return true;
                    }
                    KeyCode::Home => {
                        self.input_state.move_cursor_home();
                        return true;
                    }
                    KeyCode::End => {
                        self.input_state.move_cursor_end();
                        return true;
                    }
                    KeyCode::Up => {
                        self.input_state.history_prev();
                        return true;
                    }
                    KeyCode::Down => {
                        self.input_state.history_next();
                        return true;
                    }
                    KeyCode::Char(c) => {
                        // Filter Latin-1 mouse escape leak artifacts as per [MD-044]
                        if (c as u32) >= 0x80 && (c as u32) <= 0xFF {
                            return true;
                        }
                        self.input_state.insert_char(c);
                        return true;
                    }
                    _ => {}
                }
            }
            FocusArea::Sidebar => {
                match key.code {
                    KeyCode::Esc => {
                        self.focus = FocusArea::Terminal;
                        return true;
                    }
                    KeyCode::Up => {
                        if self.sidebar_state.selected_task_index > 0 {
                            self.sidebar_state.selected_task_index -= 1;
                        }
                        return true;
                    }
                    KeyCode::Down => {
                        if !self.sidebar_state.tasks.is_empty()
                            && self.sidebar_state.selected_task_index + 1
                                < self.sidebar_state.tasks.len()
                        {
                            self.sidebar_state.selected_task_index += 1;
                        }
                        return true;
                    }
                    _ => {}
                }
            }
        }

        false
    }

    /// Render the full spatial multiplexer cockpit into Ratatui buffer.
    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        let layout = CockpitLayout::calculate(area, self.sidebar_visible);

        // 1. Render Left Sidebar if visible
        if layout.sidebar_visible {
            if let Some(sections) = layout.calculate_sidebar_sections() {
                let is_sidebar_focused = self.focus == FocusArea::Sidebar;
                render_sidebar(
                    &sections,
                    layout.sidebar_divider,
                    &self.sidebar_state,
                    buf,
                    is_sidebar_focused,
                );
            }
        }

        // 2. Render Tab Strip (Top Row)
        let tab_items: Vec<TabItem> = self
            .tabs
            .iter()
            .map(|t| {
                let badge = if let Some(pane) = t.pane_manager.active_pane() {
                    let offset = pane.terminal.scroll_offset();
                    if offset > 0 {
                        format!(" [+{} lines]", offset)
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };
                let fg_cmd = t
                    .pane_manager
                    .active_pane()
                    .and_then(|p| p.foreground_cmd.clone())
                    .or_else(|| {
                        let active = t.pane_manager.active_pane()?;
                        if !active.title.is_empty() && active.title != "Shell 1" {
                            Some(active.title.clone())
                        } else {
                            None
                        }
                    })
                    .map(|cmd| if badge.is_empty() { cmd } else { format!("{}{}", cmd, badge) });
                TabItem {
                    id: t.id,
                    title: format!("{}{}", t.title, badge),
                    foreground_cmd: fg_cmd,
                }
            })
            .collect();

        render_tab_strip(
            layout.tab_strip_area,
            &tab_items,
            self.active_tab_index,
            &self.session_code,
            &self.role,
            &self.username,
            buf,
        );

        // 3. Render Terminal Surface (borderless split panes)
        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
            let is_term_focused = self.focus == FocusArea::Terminal;
            tab.pane_manager
                .render(layout.terminal_area, is_term_focused, buf);
        }

        // 4. Render Input Bar (Bottom Row)
        let is_input_focused = self.focus == FocusArea::Input;
        render_input_bar(
            layout.input_divider_area,
            layout.input_bar_area,
            &self.input_state,
            is_input_focused,
            buf,
        );
    }

    /// Main 60fps frame blitting and event loop with a per-frame tick callback.
    pub fn run_event_loop_with<B: Backend, F>(
        &mut self,
        terminal: &mut RatTerminal<B>,
        mut on_tick: F,
    ) -> Result<(), io::Error>
    where
        F: FnMut(&mut Self),
    {
        // Enforce triple-lock mouse suppression directly on stdout conforming to [MD-044]
        let mut out = stdout();
        let _ = out.write_all(b"\x1b[?1006l\x1b[?1016l\x1b[?1015l\x1b[?1005l\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?9l");
        let _ = out.flush();

        enable_raw_mode()?;
        crossterm::execute!(out, EnterAlternateScreen)?;

        // 60fps target frame time = ~16.6ms
        let frame_duration = Duration::from_micros(16_666);

        while !self.should_exit {
            let frame_start = Instant::now();

            // Run caller-supplied tick hook (e.g. state synchronization)
            on_tick(self);

            // 1. Drain live PTY chunks into terminal emulators
            self.poll_ptys();
            self.poll_daemon_events();

            // 2. Blit Ratatui frame (diff-based 60fps render with zero flicker)
            terminal.draw(|f| {
                self.render(f.area(), f.buffer_mut());
            })?;

            // 3. Calculate remaining frame time for event polling
            let elapsed = frame_start.elapsed();
            let timeout = if elapsed < frame_duration {
                frame_duration - elapsed
            } else {
                Duration::from_millis(1)
            };

            // 4. Poll keyboard & resize events
            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => {
                        self.handle_key(key);
                    }
                    Event::Resize(cols, rows) => {
                        // Resize active pane
                        if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                            if let Some(pane) = tab.pane_manager.active_pane_mut() {
                                pane.resize(Rect::new(0, 0, cols, rows));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Cleanup terminal state upon exit
        disable_raw_mode()?;
        crossterm::execute!(stdout(), LeaveAlternateScreen)?;
        Ok(())
    }

    /// Main 60fps frame blitting and event loop.
    pub fn run_event_loop<B: Backend>(
        &mut self,
        terminal: &mut RatTerminal<B>,
    ) -> Result<(), io::Error> {
        self.run_event_loop_with(terminal, |_| {})
    }
}
