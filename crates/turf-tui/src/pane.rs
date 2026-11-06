//! Multiplexer Terminal Surface & Split Pane Tree.
//!
//! Provides borderless terminal panes blitting cells directly from `turf-vte::ScreenBuffer`,
//! live PTY streaming, dynamic resizing, and vertical/horizontal pane splits with clean single divider lines.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use turf_daemon::DaemonClient;
use turf_pty::{PtyOptions, PtySession};
use turf_vte::Terminal;

use crate::theme::{Theme, COLOR_BG, GLYPH_DIVIDER_H, GLYPH_DIVIDER_V};
use crate::vte_blit::blit_screen_buffer;

/// Orientation of a pane split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Left and right panes separated by a vertical divider line (`│`).
    Vertical,
    /// Top and bottom panes separated by a horizontal divider line (`─`).
    Horizontal,
}

pub type PaneLayoutResult = (Vec<(usize, Rect)>, Vec<(Rect, SplitDirection)>);

/// A node in the binary space partition (BSP) split tree for terminal panes.
#[derive(Debug, Clone)]
pub enum PaneNode {
    Leaf(usize),
    Split {
        direction: SplitDirection,
        ratio: u16, // 1..99 percentage
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

impl PaneNode {
    /// Recursively calculate layout rectangles for each pane leaf and each divider line.
    pub fn layout(&self, area: Rect) -> PaneLayoutResult {
        let mut panes = Vec::new();
        let mut dividers = Vec::new();
        self.collect_layout(area, &mut panes, &mut dividers);
        (panes, dividers)
    }

    fn collect_layout(
        &self,
        area: Rect,
        panes: &mut Vec<(usize, Rect)>,
        dividers: &mut Vec<(Rect, SplitDirection)>,
    ) {
        match self {
            PaneNode::Leaf(id) => {
                panes.push((*id, area));
            }
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                match direction {
                    SplitDirection::Vertical => {
                        if area.width < 3 {
                            first.collect_layout(area, panes, dividers);
                            return;
                        }
                        let available_w = area.width.saturating_sub(1);
                        let first_w = (((available_w as u32) * (*ratio as u32)) / 100)
                            .max(1)
                            .min(available_w.saturating_sub(1) as u32) as u16;
                        let second_w = available_w.saturating_sub(first_w);

                        let first_rect = Rect {
                            x: area.x,
                            y: area.y,
                            width: first_w,
                            height: area.height,
                        };
                        let div_rect = Rect {
                            x: area.x + first_w,
                            y: area.y,
                            width: 1,
                            height: area.height,
                        };
                        let second_rect = Rect {
                            x: area.x + first_w + 1,
                            y: area.y,
                            width: second_w,
                            height: area.height,
                        };

                        dividers.push((div_rect, SplitDirection::Vertical));
                        first.collect_layout(first_rect, panes, dividers);
                        second.collect_layout(second_rect, panes, dividers);
                    }
                    SplitDirection::Horizontal => {
                        if area.height < 3 {
                            first.collect_layout(area, panes, dividers);
                            return;
                        }
                        let available_h = area.height.saturating_sub(1);
                        let first_h = (((available_h as u32) * (*ratio as u32)) / 100)
                            .max(1)
                            .min(available_h.saturating_sub(1) as u32) as u16;
                        let second_h = available_h.saturating_sub(first_h);

                        let first_rect = Rect {
                            x: area.x,
                            y: area.y,
                            width: area.width,
                            height: first_h,
                        };
                        let div_rect = Rect {
                            x: area.x,
                            y: area.y + first_h,
                            width: area.width,
                            height: 1,
                        };
                        let second_rect = Rect {
                            x: area.x,
                            y: area.y + first_h + 1,
                            width: area.width,
                            height: second_h,
                        };

                        dividers.push((div_rect, SplitDirection::Horizontal));
                        first.collect_layout(first_rect, panes, dividers);
                        second.collect_layout(second_rect, panes, dividers);
                    }
                }
            }
        }
    }

    /// Split a leaf pane by creating a new sibling with the given direction.
    pub fn split_pane(&mut self, target_id: usize, new_id: usize, direction: SplitDirection) -> bool {
        match self {
            PaneNode::Leaf(id) => {
                if *id == target_id {
                    let old_leaf = PaneNode::Leaf(target_id);
                    let new_leaf = PaneNode::Leaf(new_id);
                    *self = PaneNode::Split {
                        direction,
                        ratio: 50,
                        first: Box::new(old_leaf),
                        second: Box::new(new_leaf),
                    };
                    true
                } else {
                    false
                }
            }
            PaneNode::Split { first, second, .. } => {
                first.split_pane(target_id, new_id, direction)
                    || second.split_pane(target_id, new_id, direction)
            }
        }
    }

    /// Remove a leaf pane from the tree, promoting its sibling.
    pub fn remove_pane(&mut self, target_id: usize) -> Option<PaneNode> {
        match self {
            PaneNode::Leaf(id) => {
                if *id == target_id {
                    None
                } else {
                    Some(self.clone())
                }
            }
            PaneNode::Split { first, second, .. } => {
                if let PaneNode::Leaf(id) = **first {
                    if id == target_id {
                        return Some((**second).clone());
                    }
                }
                if let PaneNode::Leaf(id) = **second {
                    if id == target_id {
                        return Some((**first).clone());
                    }
                }

                if let Some(new_first) = first.remove_pane(target_id) {
                    **first = new_first;
                }
                if let Some(new_second) = second.remove_pane(target_id) {
                    **second = new_second;
                }
                Some(self.clone())
            }
        }
    }

    /// Collect all pane IDs contained in this tree.
    pub fn pane_ids(&self) -> Vec<usize> {
        let mut ids = Vec::new();
        self.collect_ids(&mut ids);
        ids
    }

    fn collect_ids(&self, ids: &mut Vec<usize>) {
        match self {
            PaneNode::Leaf(id) => ids.push(*id),
            PaneNode::Split { first, second, .. } => {
                first.collect_ids(ids);
                second.collect_ids(ids);
            }
        }
    }
}

/// Execution backend backing a multiplexer terminal pane.
pub enum PaneBackend {
    /// Local in-process PTY session.
    Local(PtySession),
    /// Thin-client streaming connection to background daemon PTY.
    Daemon {
        pane_id: u64,
        client: DaemonClient,
    },
    /// Headless terminal (in-memory only, for tests or manual feed).
    Headless,
}

impl std::fmt::Debug for PaneBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => write!(f, "PaneBackend::Local(..)"),
            Self::Daemon { pane_id, .. } => {
                write!(f, "PaneBackend::Daemon {{ pane_id: {} }}", pane_id)
            }
            Self::Headless => write!(f, "PaneBackend::Headless"),
        }
    }
}

/// A single borderless terminal multiplexer pane.
pub struct Pane {
    pub id: usize,
    pub title: String,
    pub foreground_cmd: Option<String>,
    pub terminal: Terminal,
    pub backend: PaneBackend,
    pub current_rect: Rect,
    pub last_proc_sample: Option<std::time::Instant>,
}

impl Pane {
    /// Create a new terminal pane without PTY (for headless testing or manual feed).
    pub fn new_headless(id: usize, title: impl Into<String>, rows: usize, cols: usize) -> Self {
        Self {
            id,
            title: title.into(),
            foreground_cmd: None,
            terminal: Terminal::new(rows, cols),
            backend: PaneBackend::Headless,
            current_rect: Rect::new(0, 0, cols as u16, rows as u16),
            last_proc_sample: None,
        }
    }

    /// Spawn a live in-process PTY session backed terminal pane.
    pub fn spawn_shell(id: usize, title: impl Into<String>, rows: u16, cols: u16) -> Result<Self, turf_pty::PtyError> {
        let pty = PtySession::spawn(PtyOptions::new(rows.max(1), cols.max(1)))?;
        let mut pane = Self {
            id,
            title: title.into(),
            foreground_cmd: None,
            terminal: Terminal::new(rows.max(1) as usize, cols.max(1) as usize),
            backend: PaneBackend::Local(pty),
            current_rect: Rect::new(0, 0, cols, rows),
            last_proc_sample: None,
        };
        pane.refresh_process_title();
        Ok(pane)
    }

    /// Return the active display command or badge for this pane:
    /// e.g. "vim src/main.rs" or "bash" if foreground_cmd is present,
    /// or fallback to self.title.
    pub fn active_cmd(&self) -> &str {
        if let Some(cmd) = &self.foreground_cmd {
            cmd.as_str()
        } else {
            &self.title
        }
    }

    /// Return dynamic pane badge formatted as e.g. "1: bash" or "1: vim main.rs".
    pub fn display_badge(&self) -> String {
        format!("{}: {}", self.id, self.active_cmd())
    }

    /// Query the current foreground process running in this pane's PTY.
    pub fn foreground_process(&self) -> Option<turf_pty::ProcessInfo> {
        self.pty().and_then(|p| p.foreground_process())
    }

    /// Refresh pane title and foreground_cmd based on active foreground process.
    pub fn refresh_process_title(&mut self) -> Option<String> {
        if let Some(pty) = self.pty() {
            if let Some(proc) = pty.foreground_process() {
                let cmd = proc.display_cmd();
                self.foreground_cmd = Some(cmd.clone());
                self.title = cmd.clone();
                return Some(cmd);
            }
        }
        None
    }

    /// Create a new terminal pane connected to a remote daemon PTY.
    pub fn new_daemon(
        id: usize,
        title: impl Into<String>,
        pane_id: u64,
        client: DaemonClient,
        rows: u16,
        cols: u16,
    ) -> Self {
        Self {
            id,
            title: title.into(),
            foreground_cmd: None,
            terminal: Terminal::new(rows.max(1) as usize, cols.max(1) as usize),
            backend: PaneBackend::Daemon { pane_id, client },
            current_rect: Rect::new(0, 0, cols, rows),
            last_proc_sample: None,
        }
    }

    /// Check if this pane is running headlessly.
    pub fn is_headless(&self) -> bool {
        matches!(self.backend, PaneBackend::Headless)
    }

    /// Check if this pane is backed by the daemon.
    pub fn is_daemon(&self) -> bool {
        matches!(self.backend, PaneBackend::Daemon { .. })
    }

    /// Check if this pane is backed by a local in-process PTY.
    pub fn is_local(&self) -> bool {
        matches!(self.backend, PaneBackend::Local(_))
    }

    /// Retrieve the remote daemon pane ID if daemon-backed.
    pub fn daemon_pane_id(&self) -> Option<u64> {
        match &self.backend {
            PaneBackend::Daemon { pane_id, .. } => Some(*pane_id),
            _ => None,
        }
    }

    /// Retrieve the connected daemon client if daemon-backed.
    pub fn daemon_client(&self) -> Option<&DaemonClient> {
        match &self.backend {
            PaneBackend::Daemon { client, .. } => Some(client),
            _ => None,
        }
    }

    /// Retrieve reference to local PTY session if local.
    pub fn pty(&self) -> Option<&PtySession> {
        match &self.backend {
            PaneBackend::Local(pty) => Some(pty),
            _ => None,
        }
    }

    /// Retrieve mutable reference to local PTY session if local.
    pub fn pty_mut(&mut self) -> Option<&mut PtySession> {
        match &mut self.backend {
            PaneBackend::Local(pty) => Some(pty),
            _ => None,
        }
    }

    /// Poll non-blocking chunks from the PTY reader and feed into VTE terminal.
    pub fn poll_pty(&mut self) -> usize {
        let mut total_bytes = 0;
        if let PaneBackend::Local(pty) = &mut self.backend {
            while let Ok(chunk) = pty.reader_mut().try_recv() {
                total_bytes += chunk.len();
                self.terminal.process(&chunk);
            }

            // Periodic sampling of foreground process (throttle to ~150ms)
            let now = std::time::Instant::now();
            let should_sample = match self.last_proc_sample {
                Some(t) => now.duration_since(t) >= std::time::Duration::from_millis(150),
                None => true,
            };
            if should_sample {
                self.last_proc_sample = Some(now);
                if let Some(proc) = pty.foreground_process() {
                    let cmd = proc.display_cmd();
                    self.foreground_cmd = Some(cmd.clone());
                    self.title = cmd;
                }
            }
        }
        total_bytes
    }

    /// Forward raw keystroke bytes directly to the PTY master (local or daemon).
    pub fn write_pty(&self, data: &[u8]) {
        match &self.backend {
            PaneBackend::Local(pty) => {
                let _ = pty.write_bytes(data);
            }
            PaneBackend::Daemon { pane_id, client } => {
                let pid = *pane_id as usize;
                if client.write_pane_sync(pid, data).is_err() {
                    let client = client.clone();
                    let bytes = data.to_vec();
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        handle.spawn(async move {
                            let _ = client.write_pane(pid, &bytes).await;
                        });
                    }
                }
            }
            PaneBackend::Headless => {}
        }
    }

    /// Resize terminal emulator and PTY (local or daemon) to match new screen dimensions.
    pub fn resize(&mut self, new_rect: Rect) {
        if new_rect.width == 0 || new_rect.height == 0 {
            return;
        }
        if self.current_rect.width != new_rect.width || self.current_rect.height != new_rect.height {
            self.terminal.resize(new_rect.height as usize, new_rect.width as usize);
            match &mut self.backend {
                PaneBackend::Local(pty) => {
                    let _ = pty.resize(new_rect.height, new_rect.width);
                }
                PaneBackend::Daemon { pane_id, client } => {
                    let pid = *pane_id as usize;
                    let cols = new_rect.width;
                    let rows = new_rect.height;
                    if client.resize_pane_sync(pid, cols, rows).is_err() {
                        let client = client.clone();
                        if let Ok(handle) = tokio::runtime::Handle::try_current() {
                            handle.spawn(async move {
                                let _ = client.resize_pane(pid, cols, rows).await;
                            });
                        }
                    }
                }
                PaneBackend::Headless => {}
            }
            self.current_rect = new_rect;
        }
    }

    /// Return the active viewport scroll offset for this pane.
    pub fn scroll_offset(&self) -> usize {
        self.terminal.scroll_offset()
    }

    /// Render borderless terminal output directly into Ratatui buffer.
    pub fn render(&mut self, area: Rect, is_focused: bool, buf: &mut Buffer) {
        self.resize(area);
        self.poll_pty();

        let cursor = if self.terminal.cursor().visible {
            Some((self.terminal.cursor().x, self.terminal.cursor().y))
        } else {
            None
        };

        blit_screen_buffer(
            self.terminal.active_buffer(),
            area,
            buf,
            cursor,
            is_focused,
        );

        // Display subtle dimmed scrollback badge if scrolled up
        let offset = self.terminal.scroll_offset();
        if offset > 0 {
            let badge = format!("[+{} lines]", offset);
            let badge_len = badge.len() as u16;
            if area.width >= badge_len + 2 && area.height >= 1 {
                let badge_x = area.x + area.width - badge_len - 1;
                let badge_y = area.y;
                let style = Theme::scrollback_badge();
                for (i, ch) in badge.chars().enumerate() {
                    let cell = &mut buf[(badge_x + i as u16, badge_y)];
                    cell.set_char(ch);
                    cell.set_style(style);
                }
            }
        }
    }
}

/// Container for managing active panes and rendering the split pane layout.
pub struct PaneManager {
    panes: HashMap<usize, Pane>,
    tree: PaneNode,
    active_pane_id: usize,
    next_pane_id: usize,
}

impl PaneManager {
    /// Create a pane manager initialized with a single pane.
    pub fn new(initial_pane: Pane) -> Self {
        let id = initial_pane.id;
        let mut panes = HashMap::new();
        panes.insert(id, initial_pane);
        Self {
            panes,
            tree: PaneNode::Leaf(id),
            active_pane_id: id,
            next_pane_id: id + 1,
        }
    }

    pub fn active_pane_id(&self) -> usize {
        self.active_pane_id
    }

    pub fn active_pane(&self) -> Option<&Pane> {
        self.panes.get(&self.active_pane_id)
    }

    pub fn active_pane_mut(&mut self) -> Option<&mut Pane> {
        self.panes.get_mut(&self.active_pane_id)
    }

    pub fn get_pane(&self, id: usize) -> Option<&Pane> {
        self.panes.get(&id)
    }

    pub fn get_pane_mut(&mut self, id: usize) -> Option<&mut Pane> {
        self.panes.get_mut(&id)
    }

    pub fn all_panes(&self) -> impl Iterator<Item = &Pane> {
        self.panes.values()
    }

    pub fn all_panes_mut(&mut self) -> impl Iterator<Item = &mut Pane> {
        self.panes.values_mut()
    }

    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// Focus the next pane in the current tab.
    pub fn focus_next(&mut self) {
        let ids = self.tree.pane_ids();
        if ids.is_empty() {
            return;
        }
        if let Some(pos) = ids.iter().position(|&id| id == self.active_pane_id) {
            let next_pos = (pos + 1) % ids.len();
            self.active_pane_id = ids[next_pos];
        } else {
            self.active_pane_id = ids[0];
        }
    }

    /// Split active pane with a new headless pane.
    pub fn split_active_headless(&mut self, direction: SplitDirection) -> usize {
        let new_id = self.next_pane_id;
        self.next_pane_id += 1;

        let title = format!("Pane {}", new_id);
        let current_rect = self
            .panes
            .get(&self.active_pane_id)
            .map(|p| p.current_rect)
            .unwrap_or(Rect::new(0, 0, 80, 24));

        let new_pane = Pane::new_headless(
            new_id,
            title,
            current_rect.height as usize,
            current_rect.width as usize,
        );
        self.panes.insert(new_id, new_pane);
        self.tree.split_pane(self.active_pane_id, new_id, direction);
        self.active_pane_id = new_id;
        new_id
    }

    /// Split active pane with a new live PTY shell.
    pub fn split_active_shell(&mut self, direction: SplitDirection) -> Result<usize, turf_pty::PtyError> {
        let new_id = self.next_pane_id;
        self.next_pane_id += 1;

        let title = format!("Shell {}", new_id);
        let current_rect = self
            .panes
            .get(&self.active_pane_id)
            .map(|p| p.current_rect)
            .unwrap_or(Rect::new(0, 0, 80, 24));

        let new_pane = Pane::spawn_shell(new_id, title, current_rect.height, current_rect.width)?;
        self.panes.insert(new_id, new_pane);
        self.tree.split_pane(self.active_pane_id, new_id, direction);
        self.active_pane_id = new_id;
        Ok(new_id)
    }

    /// Split active pane with a new remote daemon PTY pane.
    pub fn split_active_daemon(
        &mut self,
        direction: SplitDirection,
        daemon_pane_id: u64,
        client: DaemonClient,
    ) -> usize {
        let new_id = self.next_pane_id;
        self.next_pane_id += 1;

        let title = format!("Daemon {}", new_id);
        let current_rect = self
            .panes
            .get(&self.active_pane_id)
            .map(|p| p.current_rect)
            .unwrap_or(Rect::new(0, 0, 80, 24));

        let new_pane = Pane::new_daemon(
            new_id,
            title,
            daemon_pane_id,
            client,
            current_rect.height,
            current_rect.width,
        );
        self.panes.insert(new_id, new_pane);
        self.tree.split_pane(self.active_pane_id, new_id, direction);
        self.active_pane_id = new_id;
        new_id
    }

    /// Close active pane (unless it's the last remaining pane).
    pub fn close_active(&mut self) -> bool {
        if self.panes.len() <= 1 {
            return false;
        }

        let closing_id = self.active_pane_id;
        if let Some(new_tree) = self.tree.remove_pane(closing_id) {
            self.tree = new_tree;
            self.panes.remove(&closing_id);
            let ids = self.tree.pane_ids();
            if !ids.is_empty() {
                self.active_pane_id = ids[0];
            }
            true
        } else {
            false
        }
    }

    /// Poll all PTY readers across all panes.
    pub fn poll_all_ptys(&mut self) -> usize {
        let mut total = 0;
        for pane in self.panes.values_mut() {
            total += pane.poll_pty();
        }
        total
    }

    /// Refresh process titles across all active panes with PTY sessions.
    pub fn refresh_process_titles(&mut self) {
        for pane in self.panes.values_mut() {
            pane.refresh_process_title();
        }
    }

    /// Render all borderless panes and clean split dividers.
    pub fn render(&mut self, area: Rect, terminal_focused: bool, buf: &mut Buffer) {
        let (pane_rects, divider_rects) = self.tree.layout(area);

        // 1. Render divider lines between split panes
        for (div_rect, dir) in divider_rects {
            match dir {
                SplitDirection::Vertical => {
                    for y in div_rect.y..(div_rect.y + div_rect.height) {
                        let cell = &mut buf[(div_rect.x, y)];
                        cell.set_char(GLYPH_DIVIDER_V);
                        cell.set_style(Theme::divider());
                        cell.set_bg(COLOR_BG);
                    }
                }
                SplitDirection::Horizontal => {
                    for x in div_rect.x..(div_rect.x + div_rect.width) {
                        let cell = &mut buf[(x, div_rect.y)];
                        cell.set_char(GLYPH_DIVIDER_H);
                        cell.set_style(Theme::divider());
                        cell.set_bg(COLOR_BG);
                    }
                }
            }
        }

        // Render dynamic pane badges on horizontal dividers if multiple panes exist
        if pane_rects.len() > 1 {
            for (id, r) in &pane_rects {
                if r.y > area.y {
                    let header_y = r.y.saturating_sub(1);
                    if let Some(pane) = self.panes.get(id) {
                        let badge = format!(" {} ", pane.display_badge());
                        let mut hx = r.x.saturating_add(2);
                        for ch in badge.chars() {
                            if hx < r.x + r.width && hx < area.x + area.width {
                                let cell = &mut buf[(hx, header_y)];
                                cell.set_char(ch);
                                cell.set_style(Theme::inactive_tab());
                                cell.set_bg(COLOR_BG);
                                hx += 1;
                            }
                        }
                    }
                }
            }
        }

        // 2. Render each pane's VTE buffer
        for (id, r) in pane_rects {
            if let Some(pane) = self.panes.get_mut(&id) {
                let is_focused = terminal_focused && id == self.active_pane_id;
                pane.render(r, is_focused, buf);
            }
        }
    }
}
