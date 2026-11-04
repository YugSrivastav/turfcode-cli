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
use turf_protocol::intent::{TaskCard, TaskStatus};
use turf_protocol::locks::FileLock;

use crate::input::{render_input_bar, InputState};
use crate::keys::key_to_bytes;
use crate::layout::CockpitLayout;
use crate::pane::{Pane, PaneManager, SplitDirection};
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
    pub next_tab_id: usize,
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
            next_tab_id: 2,
        }
    }

    /// Initialize a new TerminalCockpit with live PTY shell panes.
    pub fn new_live(
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
            next_tab_id: 2,
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
                let _ = self.split_active_pane(SplitDirection::Vertical);
                self.focus = FocusArea::Terminal;
            }
            "/split-h" | "/hsplit" => {
                let _ = self.split_active_pane(SplitDirection::Horizontal);
                self.focus = FocusArea::Terminal;
            }
            "/close" => {
                self.close_active_pane();
                self.focus = FocusArea::Terminal;
            }
            "/tab" | "/new" => {
                let title = parts.get(1).copied().unwrap_or("Shell");
                let _ = self.add_tab_shell(title);
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
            _ => {}
        }
    }

    /// Process a Crossterm keyboard event.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        // --- 1. Global Multiplexer Hotkeys ---
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    self.should_exit = true;
                    return true;
                }
                KeyCode::Char('b') | KeyCode::Char('B') => {
                    self.toggle_sidebar();
                    return true;
                }
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    let _ = self.add_tab_shell("Shell");
                    return true;
                }
                KeyCode::Char('w') | KeyCode::Char('W') => {
                    self.cycle_pane_focus();
                    return true;
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    let _ = self.split_active_pane(SplitDirection::Vertical);
                    return true;
                }
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    let _ = self.split_active_pane(SplitDirection::Horizontal);
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

        // Tab cycles focus area
        if key.code == KeyCode::Tab && !key.modifiers.contains(KeyModifiers::CONTROL) {
            self.cycle_focus();
            return true;
        }

        // --- 2. Focus-Specific Key Handling ---
        match self.focus {
            FocusArea::Terminal => {
                // Keystrokes in terminal focus forward directly to the active PTY master
                if let Some(tab) = self.tabs.get_mut(self.active_tab_index) {
                    if let Some(pane) = tab.pane_manager.active_pane_mut() {
                        if let Some(bytes) = key_to_bytes(&key) {
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
            .map(|t| TabItem {
                id: t.id,
                title: t.title.clone(),
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
