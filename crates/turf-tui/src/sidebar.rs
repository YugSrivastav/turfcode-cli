//! Full-height borderless sidebar separated from the main surface by a single vertical divider.
//!
//! Conforms to [MD-042], [MD-044], and [MD-050]: 4 typographic sections (`spaces`, `intents & tasks`,
//! `active locks`, `peers`) separated by subtle hairline rules (`─`) with seamless Catppuccin / Herdr styling.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use turf_protocol::intent::TaskCard;
use turf_protocol::locks::FileLock;

use crate::layout::SidebarSectionLayout;
use crate::theme::{
    Theme, COLOR_ACTIVE_PILL, COLOR_BG, COLOR_CYAN, COLOR_MUTED_BORDER, COLOR_STATUS_DONE,
    COLOR_STATUS_LOCKED, COLOR_STATUS_PEER, COLOR_STATUS_PROGRESS, COLOR_STATUS_QUEUED,
    COLOR_TEXT_DIM, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY, GLYPH_DIVIDER_H, GLYPH_DIVIDER_V,
};

/// Model data powering the sidebar display.
#[derive(Debug, Clone)]
pub struct SidebarState {
    pub current_space: String,
    pub current_branch: String,
    pub tasks: Vec<TaskCard>,
    pub selected_task_index: usize,
    pub locks: Vec<FileLock>,
    pub peers: Vec<PeerInfo>,
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub name: String,
    pub role: String,
    pub is_self: bool,
    pub is_online: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            current_space: "turfcode-cli".to_string(),
            current_branch: "main".to_string(),
            tasks: Vec::new(),
            selected_task_index: 0,
            locks: Vec::new(),
            peers: vec![PeerInfo {
                name: "host".to_string(),
                role: "owner".to_string(),
                is_self: true,
                is_online: true,
            }],
        }
    }
}

/// Render the continuous Herdr-grade sidebar into the Ratatui buffer.
pub fn render_sidebar(
    layout: &SidebarSectionLayout,
    divider_rect: Rect,
    state: &SidebarState,
    buf: &mut Buffer,
    is_focused: bool,
) {
    let div_style = if is_focused {
        Theme::active_divider()
    } else {
        Theme::divider()
    };

    // 1. Draw single vertical divider line (│) on the right edge
    for y in divider_rect.y..(divider_rect.y + divider_rect.height) {
        let cell = &mut buf[(divider_rect.x, y)];
        cell.set_char(GLYPH_DIVIDER_V);
        cell.set_style(div_style);
        cell.set_bg(COLOR_BG);
    }

    // Helper to draw horizontal divider line (─)
    let draw_h_div = |r: Rect, b: &mut Buffer| {
        if r.height > 0 && r.width > 0 {
            for x in r.x..(r.x + r.width) {
                let cell = &mut b[(x, r.y)];
                cell.set_char(GLYPH_DIVIDER_H);
                cell.set_style(Style::default().fg(COLOR_MUTED_BORDER));
                cell.set_bg(COLOR_BG);
            }
        }
    };

    // Helper to write text at (x, y) with truncation and bounds check
    let write_line = |x: u16, y: u16, max_w: u16, text: &str, style: Style, b: &mut Buffer| {
        if y >= b.area().height {
            return;
        }
        let chars: Vec<char> = text.chars().collect();
        for (i, &ch) in chars.iter().take(max_w as usize).enumerate() {
            let px = x + i as u16;
            if px < b.area().width {
                let cell = &mut b[(px, y)];
                cell.set_char(ch);
                cell.set_style(style);
                cell.set_bg(COLOR_BG);
            }
        }
        // Fill remainder with blank space
        for i in chars.len()..(max_w as usize) {
            let px = x + i as u16;
            if px < b.area().width {
                let cell = &mut b[(px, y)];
                cell.set_char(' ');
                cell.set_style(Style::default());
                cell.set_bg(COLOR_BG);
            }
        }
    };

    // Helper to write styled spans in a single line
    let write_spans = |x: u16, y: u16, max_w: u16, spans: &[(&str, Style)], b: &mut Buffer| {
        if y >= b.area().height {
            return;
        }
        let mut curr_x = x;
        for &(text, style) in spans {
            for ch in text.chars() {
                if curr_x >= x + max_w || curr_x >= b.area().width {
                    break;
                }
                let cell = &mut b[(curr_x, y)];
                cell.set_char(ch);
                cell.set_style(style);
                curr_x += 1;
            }
        }
        while curr_x < x + max_w && curr_x < b.area().width {
            let cell = &mut b[(curr_x, y)];
            cell.set_char(' ');
            cell.set_style(Style::default());
            cell.set_bg(COLOR_BG);
            curr_x += 1;
        }
    };

    // --- Section 1: spaces ---
    let s = layout.spaces_area;
    if s.height > 0 {
        write_line(s.x + 1, s.y, s.width.saturating_sub(2), "spaces", Theme::sidebar_header(), buf);
        if s.height > 1 {
            let branch_str = format!("({})", state.current_branch);
            write_spans(
                s.x + 1,
                s.y + 1,
                s.width.saturating_sub(2),
                &[
                    ("○ ", Style::default().fg(COLOR_TEXT_MUTED).bg(COLOR_BG)),
                    (&state.current_space, Style::default().fg(COLOR_TEXT_PRIMARY).bg(COLOR_BG)),
                    (" ", Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_BG)),
                    (&branch_str, Style::default().fg(COLOR_CYAN).bg(COLOR_BG)),
                ],
                buf,
            );
        }
        if s.height > 2 {
            write_line(
                s.x + 1,
                s.y + 2,
                s.width.saturating_sub(2),
                "mode: host-pairing",
                Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_BG),
                buf,
            );
        }
    }

    draw_h_div(layout.div1_area, buf);

    // --- Section 2: intents & tasks ---
    let t = layout.tasks_area;
    if t.height > 0 {
        let task_header = format!("intents & tasks ({})", state.tasks.len());
        write_line(t.x + 1, t.y, t.width.saturating_sub(2), &task_header, Theme::sidebar_header(), buf);

        if state.tasks.is_empty() {
            if t.height > 1 {
                write_line(
                    t.x + 1,
                    t.y + 1,
                    t.width.saturating_sub(2),
                    "no active tasks",
                    Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_BG),
                    buf,
                );
            }
        } else {
            for (row, (idx, task)) in (1..).zip(state.tasks.iter().enumerate()) {
                if row >= t.height {
                    break;
                }
                let status_color = match task.status.as_str() {
                    "queued" => COLOR_STATUS_QUEUED,
                    "in_progress" => COLOR_STATUS_PROGRESS,
                    "done" => COLOR_STATUS_DONE,
                    "blocked" => COLOR_STATUS_LOCKED,
                    _ => COLOR_TEXT_MUTED,
                };

                let marker = if is_focused && idx == state.selected_task_index {
                    "▶ "
                } else {
                    "  "
                };

                let badge = format!("[{}]", task.status.display_badge());
                let line_str = format!("{}{:<11} {}", marker, badge, task.title);
                let item_style = if is_focused && idx == state.selected_task_index {
                    Style::default().fg(COLOR_TEXT_PRIMARY).bg(COLOR_ACTIVE_PILL)
                } else {
                    Style::default().fg(status_color).bg(COLOR_BG)
                };

                write_line(t.x + 1, t.y + row, t.width.saturating_sub(2), &line_str, item_style, buf);
            }
        }
    }

    draw_h_div(layout.div2_area, buf);

    // --- Section 3: active locks ---
    let l = layout.locks_area;
    if l.height > 0 {
        let lock_header = format!("active locks ({})", state.locks.len());
        write_line(l.x + 1, l.y, l.width.saturating_sub(2), &lock_header, Theme::sidebar_header(), buf);

        if state.locks.is_empty() {
            if l.height > 1 {
                write_line(
                    l.x + 1,
                    l.y + 1,
                    l.width.saturating_sub(2),
                    "no file locks",
                    Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_BG),
                    buf,
                );
            }
        } else {
            for (row, lock) in (1..).zip(state.locks.iter()) {
                if row >= l.height {
                    break;
                }
                let lock_line = format!("🔒 {} ({})", lock.file_path, lock.owner_id);
                write_line(
                    l.x + 1,
                    l.y + row,
                    l.width.saturating_sub(2),
                    &lock_line,
                    Style::default().fg(COLOR_STATUS_LOCKED).bg(COLOR_BG),
                    buf,
                );
            }
        }
    }

    draw_h_div(layout.div3_area, buf);

    // --- Section 4: peers ---
    let p = layout.peers_area;
    if p.height > 0 {
        let peer_header = format!("peers ({})", state.peers.len());
        write_line(p.x + 1, p.y, p.width.saturating_sub(2), &peer_header, Theme::sidebar_header(), buf);

        for (row, peer) in (1..).zip(state.peers.iter()) {
            if row >= p.height {
                break;
            }

            if peer.is_self {
                // Active user capsule pill on dark slate rounded highlight (#232838)
                let pill_text = format!("✓ {} (you) · {}", peer.name, peer.role);
                let max_w = p.width.saturating_sub(2);
                let chars: Vec<char> = pill_text.chars().collect();
                for (i, &ch) in chars.iter().take(max_w as usize).enumerate() {
                    let px = p.x + 1 + i as u16;
                    if px < buf.area().width {
                        let cell = &mut buf[(px, p.y + row)];
                        cell.set_char(ch);
                        cell.set_style(Theme::active_peer_pill());
                    }
                }
                for i in chars.len()..(max_w as usize) {
                    let px = p.x + 1 + i as u16;
                    if px < buf.area().width {
                        let cell = &mut buf[(px, p.y + row)];
                        cell.set_char(' ');
                        cell.set_style(Style::default());
                        cell.set_bg(COLOR_BG);
                    }
                }
            } else {
                let dot_color = if peer.is_online {
                    COLOR_STATUS_PEER
                } else {
                    COLOR_TEXT_DIM
                };
                let peer_line = format!("● {} · {}", peer.name, peer.role);
                write_line(
                    p.x + 1,
                    p.y + row,
                    p.width.saturating_sub(2),
                    &peer_line,
                    Style::default().fg(dot_color).bg(COLOR_BG),
                    buf,
                );
            }
        }
    }
}
