//! Single-line bottom prompt separated from terminal output by a single `─` divider,
//! with right-aligned helper text.
//!
//! Conforms to [MD-044] and [MD-050].

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use crate::theme::{Theme, COLOR_ACCENT, COLOR_BG, COLOR_TEXT_DIM, GLYPH_DIVIDER_H};

/// Input state maintaining user text, cursor index, and command history.
#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub value: String,
    pub cursor_pos: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
}

impl InputState {
    pub fn insert_char(&mut self, c: char) {
        self.value.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.value.remove(self.cursor_pos);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor_pos < self.value.len() {
            self.value.remove(self.cursor_pos);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.value.len() {
            self.cursor_pos += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor_pos = self.value.len();
    }

    pub fn submit(&mut self) -> Option<String> {
        let trimmed = self.value.trim().to_string();
        if !trimmed.is_empty() {
            self.history.push(trimmed.clone());
        }
        self.value.clear();
        self.cursor_pos = 0;
        self.history_index = None;
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next_idx = match self.history_index {
            None => self.history.len().saturating_sub(1),
            Some(idx) => idx.saturating_sub(1),
        };
        self.history_index = Some(next_idx);
        if let Some(cmd) = self.history.get(next_idx) {
            self.value = cmd.clone();
            self.cursor_pos = self.value.len();
        }
    }

    pub fn history_next(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.history.len() {
                self.history_index = Some(idx + 1);
                if let Some(cmd) = self.history.get(idx + 1) {
                    self.value = cmd.clone();
                    self.cursor_pos = self.value.len();
                }
            } else {
                self.history_index = None;
                self.value.clear();
                self.cursor_pos = 0;
            }
        }
    }
}

/// Render the subtle divider line and bottom input prompt into Ratatui buffer.
pub fn render_input_bar(
    divider_area: Rect,
    input_area: Rect,
    state: &InputState,
    is_focused: bool,
    buf: &mut Buffer,
) {
    // 1. Draw single horizontal hairline divider (─) separating terminal output from input
    if divider_area.height > 0 && divider_area.width > 0 {
        let div_style = if is_focused {
            Theme::active_divider()
        } else {
            Theme::divider()
        };
        for x in divider_area.x..(divider_area.x + divider_area.width) {
            let cell = &mut buf[(x, divider_area.y)];
            cell.set_char(GLYPH_DIVIDER_H);
            cell.set_style(div_style);
            cell.set_bg(COLOR_BG);
        }
    }

    // 2. Draw input prompt row
    if input_area.height == 0 || input_area.width == 0 {
        return;
    }

    // Fill row with background
    for x in input_area.x..(input_area.x + input_area.width) {
        let cell = &mut buf[(x, input_area.y)];
        cell.set_char(' ');
        cell.set_bg(COLOR_BG);
        cell.set_style(Style::default());
    }

    // Render minimal `> ` prompt leader
    let prefix = "> ";
    let mut cursor_x = input_area.x;
    for ch in prefix.chars() {
        if cursor_x >= input_area.x + input_area.width {
            break;
        }
        let cell = &mut buf[(cursor_x, input_area.y)];
        cell.set_char(ch);
        cell.set_style(Theme::prompt_prefix());
        cursor_x += 1;
    }

    // Right-aligned helper text: `« ? for shortcuts`
    let helper_text = "« ? for shortcuts";
    let helper_len = helper_text.chars().count();

    let input_available_w = input_area
        .width
        .saturating_sub(prefix.len() as u16)
        .saturating_sub(helper_len as u16 + 2);

    // Render input string
    let chars: Vec<char> = state.value.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if (i as u16) >= input_available_w {
            break;
        }
        let px = cursor_x + i as u16;
        let is_cursor = is_focused && i == state.cursor_pos;
        let style = if is_cursor {
            Style::default()
                .bg(COLOR_ACCENT)
                .fg(COLOR_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Theme::prompt_input()
        };

        let cell = &mut buf[(px, input_area.y)];
        cell.set_char(ch);
        cell.set_style(style);
    }

    // Render block cursor if at end of string
    if is_focused && state.cursor_pos == chars.len() {
        let px = cursor_x + chars.len() as u16;
        if px < input_area.x + input_area.width {
            let cell = &mut buf[(px, input_area.y)];
            cell.set_char('█');
            cell.set_style(Style::default().fg(COLOR_ACCENT).bg(COLOR_BG));
        }
    }

    // Render right-aligned helper text: `« ? for shortcuts`
    if input_area.width as usize > helper_len + prefix.len() + 4 {
        let mut sx = (input_area.x + input_area.width) - (helper_len as u16) - 1;
        for ch in helper_text.chars() {
            if sx < input_area.x + input_area.width {
                let cell = &mut buf[(sx, input_area.y)];
                cell.set_char(ch);
                cell.set_style(Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_BG));
                sx += 1;
            }
        }
    }
}
