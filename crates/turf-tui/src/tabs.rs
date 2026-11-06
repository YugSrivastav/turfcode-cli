//! Top row Tab Strip with sleek Catppuccin / Herdr active capsule pill
//! and clean right-aligned session badge.
//!
//! Conforms to [MD-044] and [MD-050].

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::theme::{
    Theme, COLOR_GREEN_DOT, COLOR_HEADER_BG, COLOR_TEXT_DIM, COLOR_TEXT_MUTED, COLOR_TEXT_PRIMARY,
};

/// Represents a tab metadata descriptor.
#[derive(Debug, Clone)]
pub struct TabItem {
    pub id: usize,
    pub title: String,
    pub foreground_cmd: Option<String>,
}

impl TabItem {
    pub fn new(id: usize, title: impl Into<String>) -> Self {
        Self {
            id,
            title: title.into(),
            foreground_cmd: None,
        }
    }

    pub fn with_foreground_cmd(mut self, cmd: Option<String>) -> Self {
        self.foreground_cmd = cmd;
        self
    }
}

/// Render the Herdr-grade Tab Strip into the given area.
pub fn render_tab_strip(
    area: Rect,
    tabs: &[TabItem],
    active_tab_index: usize,
    session_code: &str,
    role: &str,
    _username: &str,
    buf: &mut Buffer,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    // 1. Fill entire strip with header background (#181926)
    for x in area.x..(area.x + area.width) {
        let cell = &mut buf[(x, area.y)];
        cell.set_char(' ');
        cell.set_bg(COLOR_HEADER_BG);
        cell.set_style(Theme::tab_strip_bg());
    }

    // 2. Render left tabs
    let mut cursor_x = area.x;

    for (idx, tab) in tabs.iter().enumerate() {
        if cursor_x >= area.x + area.width {
            break;
        }

        let is_active = idx == active_tab_index;
        let display_title = tab
            .foreground_cmd
            .as_deref()
            .unwrap_or(tab.title.as_str());
        let pill_text = if is_active {
            format!(" {}: {} ", tab.id, display_title)
        } else {
            format!(" {} ", tab.id)
        };
        let pill_style = if is_active {
            Theme::active_tab_pill()
        } else {
            Theme::inactive_tab()
        };

        for ch in pill_text.chars() {
            if cursor_x >= area.x + area.width {
                break;
            }
            let cell = &mut buf[(cursor_x, area.y)];
            cell.set_char(ch);
            cell.set_style(pill_style);
            cursor_x += 1;
        }

        // Add a single blank spacer between pills
        if cursor_x < area.x + area.width {
            let cell = &mut buf[(cursor_x, area.y)];
            cell.set_char(' ');
            cell.set_bg(COLOR_HEADER_BG);
            cursor_x += 1;
        }
    }

    // Render "+" subtle add button
    if cursor_x + 2 < area.x + area.width {
        let cell = &mut buf[(cursor_x, area.y)];
        cell.set_char('+');
        cell.set_style(Theme::tab_add_button());
        cursor_x += 1;

        let cell = &mut buf[(cursor_x, area.y)];
        cell.set_char(' ');
        cell.set_bg(COLOR_HEADER_BG);
    }

    // 3. Render clean right-aligned session badge: `● TURF-XXXX • HOST`
    let meta_dot = "● ";
    let meta_code = session_code;
    let meta_sep = " • ";
    let meta_role = role;
    let meta_end = " ";

    let total_meta_len = meta_dot.len()
        + meta_code.len()
        + meta_sep.len()
        + meta_role.len()
        + meta_end.len();

    if area.width as usize > total_meta_len + 4 {
        let mut right_x = (area.x + area.width) - total_meta_len as u16;

        let segments: [(&str, Style); 5] = [
            (
                meta_dot,
                Style::default()
                    .fg(COLOR_GREEN_DOT)
                    .bg(COLOR_HEADER_BG),
            ),
            (
                meta_code,
                Style::default()
                    .fg(COLOR_TEXT_PRIMARY)
                    .bg(COLOR_HEADER_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            (
                meta_sep,
                Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_HEADER_BG),
            ),
            (
                meta_role,
                Style::default()
                    .fg(COLOR_TEXT_MUTED)
                    .bg(COLOR_HEADER_BG)
                    .add_modifier(Modifier::BOLD),
            ),
            (
                meta_end,
                Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_HEADER_BG),
            ),
        ];

        for (text, style) in segments {
            for ch in text.chars() {
                if right_x < area.x + area.width {
                    let cell = &mut buf[(right_x, area.y)];
                    cell.set_char(ch);
                    cell.set_style(style);
                    right_x += 1;
                }
            }
        }
    }
}
