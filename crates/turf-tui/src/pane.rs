//! Multiplexer Terminal Surface & Split Pane Tree.
//!
//! Provides borderless terminal panes blitting cells directly from `turf-vte::ScreenBuffer`,
//! live PTY streaming, dynamic resizing, and vertical/horizontal pane splits with clean single divider lines.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
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

/// A single borderless terminal multiplexer pane.
pub struct Pane {
    pub id: usize,
    pub title: String,
    pub terminal: Terminal,
    pub pty: Option<PtySession>,
    pub current_rect: Rect,
}

impl Pane {
    /// Create a new terminal pane without PTY (for headless testing or manual feed).
    pub fn new_headless(id: usize, title: impl Into<String>, rows: usize, cols: usize) -> Self {
        Self {
            id,
            title: title.into(),
            terminal: Terminal::new(rows, cols),
            pty: None,
            current_rect: Rect::new(0, 0, cols as u16, rows as u16),
        }
    }

    /// Spawn a live PTY session backed terminal pane.
    pub fn spawn_shell(id: usize, title: impl Into<String>, rows: u16, cols: u16) -> Result<Self, turf_pty::PtyError> {
        let pty = PtySession::spawn(PtyOptions::new(rows.max(1), cols.max(1)))?;
        Ok(Self {
            id,
            title: title.into(),
            terminal: Terminal::new(rows.max(1) as usize, cols.max(1) as usize),
            pty: Some(pty),
            current_rect: Rect::new(0, 0, cols, rows),
        })
    }

    /// Poll non-blocking chunks from the PTY reader and feed into VTE terminal.
    pub fn poll_pty(&mut self) -> usize {
        let mut total_bytes = 0;
        if let Some(pty) = &mut self.pty {
            while let Ok(chunk) = pty.reader_mut().try_recv() {
                total_bytes += chunk.len();
                self.terminal.process(&chunk);
            }
        }
        total_bytes
    }

    /// Forward raw keystroke bytes directly to the PTY master.
    pub fn write_pty(&self, data: &[u8]) {
        if let Some(pty) = &self.pty {
            let _ = pty.write_bytes(data);
        }
    }

    /// Resize terminal emulator and PTY to match new screen dimensions.
    pub fn resize(&mut self, new_rect: Rect) {
        if new_rect.width == 0 || new_rect.height == 0 {
            return;
        }
        if self.current_rect.width != new_rect.width || self.current_rect.height != new_rect.height {
            self.terminal.resize(new_rect.height as usize, new_rect.width as usize);
            if let Some(pty) = &mut self.pty {
                let _ = pty.resize(new_rect.height, new_rect.width);
            }
            self.current_rect = new_rect;
        }
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

    pub fn get_pane_mut(&mut self, id: usize) -> Option<&mut Pane> {
        self.panes.get_mut(&id)
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

        // 2. Render each pane's VTE buffer
        for (id, r) in pane_rects {
            if let Some(pane) = self.panes.get_mut(&id) {
                let is_focused = terminal_focused && id == self.active_pane_id;
                pane.render(r, is_focused, buf);
            }
        }
    }
}
