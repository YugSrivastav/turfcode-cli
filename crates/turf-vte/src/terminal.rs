use vte::{Params, Perform};

use crate::buffer::ScreenBuffer;
use crate::cell::{Cell, CellFlags};
use crate::color::{Color, NamedColor};
use crate::cursor::Cursor;

/// Complete terminal emulator maintaining a primary and alternate `ScreenBuffer`,
/// cursor state, styling rendition attributes, and the ANSI escape parser.
pub struct Terminal {
    primary_buffer: ScreenBuffer,
    alt_buffer: ScreenBuffer,
    active_is_alt: bool,
    primary_cursor: Cursor,
    alt_cursor: Cursor,
    saved_cursor: Option<(Cursor, Color, Color, CellFlags)>,
    current_fg: Color,
    current_bg: Color,
    current_flags: CellFlags,
    title: String,
    parser: vte::Parser,
}

impl Terminal {
    /// Create a new terminal instance with given dimensions.
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let primary_buffer = ScreenBuffer::new(rows, cols);
        let mut alt_buffer = ScreenBuffer::new(rows, cols);
        alt_buffer.set_allow_scrollback(false);
        alt_buffer.set_max_scrollback(0);
        Self {
            primary_buffer,
            alt_buffer,
            active_is_alt: false,
            primary_cursor: Cursor::default(),
            alt_cursor: Cursor::default(),
            saved_cursor: None,
            current_fg: Color::Default,
            current_bg: Color::Default,
            current_flags: CellFlags::default(),
            title: String::new(),
            parser: vte::Parser::new(),
        }
    }

    /// Process a stream of raw bytes through the ANSI state machine.
    pub fn process(&mut self, bytes: &[u8]) {
        let mut parser = std::mem::take(&mut self.parser);
        parser.advance(self, bytes);
        self.parser = parser;
    }

    /// Process a UTF-8 string slice through the ANSI state machine.
    pub fn process_str(&mut self, s: &str) {
        self.process(s.as_bytes());
    }

    /// Number of vertical rows.
    pub fn rows(&self) -> usize {
        self.primary_buffer.rows()
    }

    /// Number of horizontal columns.
    pub fn cols(&self) -> usize {
        self.primary_buffer.cols()
    }

    /// Resize both primary and alternate screen buffers.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        self.primary_buffer.resize(rows, cols);
        self.alt_buffer.resize(rows, cols);
        self.primary_cursor.x = self.primary_cursor.x.min(cols - 1);
        self.primary_cursor.y = self.primary_cursor.y.min(rows - 1);
        self.alt_cursor.x = self.alt_cursor.x.min(cols - 1);
        self.alt_cursor.y = self.alt_cursor.y.min(rows - 1);
    }

    /// Whether the alternate screen buffer (`?1049h`) is currently active.
    pub fn is_alt_buffer(&self) -> bool {
        self.active_is_alt
    }

    /// Reference to the currently active screen buffer.
    pub fn active_buffer(&self) -> &ScreenBuffer {
        if self.active_is_alt {
            &self.alt_buffer
        } else {
            &self.primary_buffer
        }
    }

    /// Mutable reference to the currently active screen buffer.
    pub fn active_buffer_mut(&mut self) -> &mut ScreenBuffer {
        if self.active_is_alt {
            &mut self.alt_buffer
        } else {
            &mut self.primary_buffer
        }
    }

    /// Reference to the primary screen buffer.
    pub fn primary_buffer(&self) -> &ScreenBuffer {
        &self.primary_buffer
    }

    /// Mutable reference to the primary screen buffer.
    pub fn primary_buffer_mut(&mut self) -> &mut ScreenBuffer {
        &mut self.primary_buffer
    }

    /// Reference to the alternate screen buffer.
    pub fn alt_buffer(&self) -> &ScreenBuffer {
        &self.alt_buffer
    }

    /// Mutable reference to the alternate screen buffer.
    pub fn alt_buffer_mut(&mut self) -> &mut ScreenBuffer {
        &mut self.alt_buffer
    }

    /// Scroll viewport up by `n` lines into history (primary buffer only).
    pub fn scroll_up_viewport(&mut self, n: usize) {
        if !self.active_is_alt {
            self.primary_buffer.scroll_up_viewport(n);
        }
    }

    /// Scroll viewport down by `n` lines toward the live stream.
    pub fn scroll_down_viewport(&mut self, n: usize) {
        if !self.active_is_alt {
            self.primary_buffer.scroll_down_viewport(n);
        }
    }

    /// Reset viewport scroll back to the live bottom view.
    pub fn reset_viewport_scroll(&mut self) {
        if !self.active_is_alt {
            self.primary_buffer.reset_viewport_scroll();
        }
    }

    /// Current viewport scroll offset (0 = live bottom view).
    pub fn scroll_offset(&self) -> usize {
        if self.active_is_alt {
            0
        } else {
            self.primary_buffer.scroll_offset()
        }
    }

    /// Current count of historical lines in scrollback history.
    pub fn scrollback_len(&self) -> usize {
        if self.active_is_alt {
            0
        } else {
            self.primary_buffer.scrollback_len()
        }
    }

    /// Current active cursor position and visibility.
    pub fn cursor(&self) -> &Cursor {
        if self.active_is_alt {
            &self.alt_cursor
        } else {
            &self.primary_cursor
        }
    }

    /// Current active cursor position and visibility (mutable).
    pub fn cursor_mut(&mut self) -> &mut Cursor {
        if self.active_is_alt {
            &mut self.alt_cursor
        } else {
            &mut self.primary_cursor
        }
    }

    /// Window title set via OSC 0 or 2.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Current active text foreground color.
    pub fn current_fg(&self) -> Color {
        self.current_fg
    }

    /// Current active text background color.
    pub fn current_bg(&self) -> Color {
        self.current_bg
    }

    /// Current active cell styling flags.
    pub fn current_flags(&self) -> CellFlags {
        self.current_flags
    }

    /// Cell at `(x, y)` on the active screen buffer.
    pub fn cell(&self, x: usize, y: usize) -> Option<&Cell> {
        self.active_buffer().cell(x, y)
    }

    /// Text of row `y` on the active screen buffer.
    pub fn row_text(&self, y: usize) -> String {
        self.active_buffer().row_text(y)
    }

    /// Text of row `y` on the active screen buffer trimmed.
    pub fn row_text_trimmed(&self, y: usize) -> String {
        self.active_buffer().row_text_trimmed(y)
    }

    /// Entire active screen buffer as text.
    pub fn text(&self) -> String {
        self.active_buffer().text()
    }

    /// Switch into the alternate screen buffer (`?1049h`).
    pub fn enter_alt_buffer(&mut self) {
        if !self.active_is_alt {
            self.save_cursor();
            self.active_is_alt = true;
            self.alt_buffer.clear();
            self.alt_cursor = Cursor::default();
        }
    }

    /// Return to the primary screen buffer (`?1049l`).
    pub fn leave_alt_buffer(&mut self) {
        if self.active_is_alt {
            self.active_is_alt = false;
            self.restore_cursor();
            self.alt_buffer.clear();
        }
    }

    /// Save active cursor position and styling attributes.
    pub fn save_cursor(&mut self) {
        let cursor = *self.cursor();
        self.saved_cursor = Some((cursor, self.current_fg, self.current_bg, self.current_flags));
    }

    /// Restore saved cursor position and styling attributes.
    pub fn restore_cursor(&mut self) {
        if let Some((saved_cur, fg, bg, flags)) = self.saved_cursor {
            let cols = self.cols();
            let rows = self.rows();
            let cur = self.cursor_mut();
            cur.x = saved_cur.x.min(cols.saturating_sub(1));
            cur.y = saved_cur.y.min(rows.saturating_sub(1));
            cur.visible = saved_cur.visible;
            self.current_fg = fg;
            self.current_bg = bg;
            self.current_flags = flags;
        }
    }

    /// Reset full terminal to initial state (RIS `\x1bc`).
    pub fn reset_terminal(&mut self) {
        self.active_is_alt = false;
        self.primary_buffer.clear();
        self.primary_buffer.clear_scrollback();
        self.primary_buffer.reset_viewport_scroll();
        self.alt_buffer.clear();
        self.alt_buffer.clear_scrollback();
        self.alt_buffer.reset_viewport_scroll();
        self.primary_cursor = Cursor::default();
        self.alt_cursor = Cursor::default();
        self.saved_cursor = None;
        self.current_fg = Color::Default;
        self.current_bg = Color::Default;
        self.current_flags.reset();
        self.title.clear();
    }

    fn clamp_cursor(&mut self) {
        let cols = self.cols();
        let rows = self.rows();
        let cur = self.cursor_mut();
        cur.x = cur.x.min(cols.saturating_sub(1));
        cur.y = cur.y.min(rows.saturating_sub(1));
    }

    fn handle_sgr(&mut self, params: &Params) {
        let flat: Vec<u16> = params.iter().flat_map(|s| s.iter().copied()).collect();
        if flat.is_empty() {
            self.current_fg = Color::Default;
            self.current_bg = Color::Default;
            self.current_flags.reset();
            return;
        }

        let mut iter = flat.into_iter();
        while let Some(code) = iter.next() {
            match code {
                0 => {
                    self.current_fg = Color::Default;
                    self.current_bg = Color::Default;
                    self.current_flags.reset();
                }
                1 => self.current_flags.bold = true,
                2 => self.current_flags.dim = true,
                3 => self.current_flags.italic = true,
                4 => self.current_flags.underline = true,
                5 | 6 => self.current_flags.blink = true,
                7 => self.current_flags.inverse = true,
                8 => self.current_flags.hidden = true,
                9 => self.current_flags.strikethrough = true,
                21 => self.current_flags.underline = true,
                22 => {
                    self.current_flags.bold = false;
                    self.current_flags.dim = false;
                }
                23 => self.current_flags.italic = false,
                24 => self.current_flags.underline = false,
                25 => self.current_flags.blink = false,
                27 => self.current_flags.inverse = false,
                28 => self.current_flags.hidden = false,
                29 => self.current_flags.strikethrough = false,
                30..=37 => {
                    if let Some(c) = NamedColor::from_index((code - 30) as u8) {
                        self.current_fg = Color::Named(c);
                    }
                }
                38 => {
                    match iter.next() {
                        Some(2) => {
                            // TrueColor RGB: 38;2;r;g;b
                            let r = iter.next().unwrap_or(0) as u8;
                            let g = iter.next().unwrap_or(0) as u8;
                            let b = iter.next().unwrap_or(0) as u8;
                            self.current_fg = Color::Rgb(r, g, b);
                        }
                        Some(5) => {
                            // 256 Color: 38;5;idx
                            if let Some(idx) = iter.next() {
                                self.current_fg = Color::Indexed(idx as u8);
                            }
                        }
                        _ => {}
                    }
                }
                39 => self.current_fg = Color::Default,
                40..=47 => {
                    if let Some(c) = NamedColor::from_index((code - 40) as u8) {
                        self.current_bg = Color::Named(c);
                    }
                }
                48 => {
                    match iter.next() {
                        Some(2) => {
                            // TrueColor RGB: 48;2;r;g;b
                            let r = iter.next().unwrap_or(0) as u8;
                            let g = iter.next().unwrap_or(0) as u8;
                            let b = iter.next().unwrap_or(0) as u8;
                            self.current_bg = Color::Rgb(r, g, b);
                        }
                        Some(5) => {
                            // 256 Color: 48;5;idx
                            if let Some(idx) = iter.next() {
                                self.current_bg = Color::Indexed(idx as u8);
                            }
                        }
                        _ => {}
                    }
                }
                49 => self.current_bg = Color::Default,
                90..=97 => {
                    if let Some(c) = NamedColor::from_index((8 + code - 90) as u8) {
                        self.current_fg = Color::Named(c);
                    }
                }
                100..=107 => {
                    if let Some(c) = NamedColor::from_index((8 + code - 100) as u8) {
                        self.current_bg = Color::Named(c);
                    }
                }
                _ => {}
            }
        }
    }
}

fn get_param(params: &Params, idx: usize, default: u16) -> u16 {
    for (i, param) in params.iter().enumerate() {
        if i == idx {
            if let Some(&first) = param.first() {
                if first != 0 {
                    return first;
                }
            }
            return default;
        }
    }
    default
}

impl Perform for Terminal {
    fn print(&mut self, c: char) {
        let width = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
        if width == 0 {
            return;
        }

        let cols = self.cols();

        let cur_x = self.cursor().x;
        if cur_x + width > cols {
            // Line wrap
            self.cursor_mut().x = 0;
            let y = self.cursor().y;
            let (_, bottom) = self.active_buffer().scroll_margins();
            if y < bottom {
                self.cursor_mut().y += 1;
            } else {
                self.active_buffer_mut().scroll_up(1);
            }
        }

        let cur_x = self.cursor().x;
        let cur_y = self.cursor().y;
        let cell = Cell::new(c, self.current_fg, self.current_bg, self.current_flags);
        self.active_buffer_mut().set_cell(cur_x, cur_y, cell);

        self.cursor_mut().x = (cur_x + width).min(cols);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x0A..=0x0C => {
                // LF (\n), VT, FF
                let y = self.cursor().y;
                let (_, bottom) = self.active_buffer().scroll_margins();
                if y < bottom {
                    self.cursor_mut().y += 1;
                } else {
                    self.active_buffer_mut().scroll_up(1);
                }
            }
            0x0D => {
                // CR (\r)
                self.cursor_mut().x = 0;
            }
            0x08 => {
                // BS (\b)
                let x = self.cursor().x;
                if x > 0 {
                    self.cursor_mut().x = x - 1;
                }
            }
            0x09 => {
                // HT (\t)
                let cols = self.cols();
                let x = self.cursor().x;
                let next_tab = ((x / 8) + 1) * 8;
                self.cursor_mut().x = next_tab.min(cols.saturating_sub(1));
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let cols = self.cols();
        let rows = self.rows();

        if intermediates == b"?" {
            // Private DEC modes
            let is_set = action == 'h';
            let is_reset = action == 'l';
            if is_set || is_reset {
                for param in params.iter() {
                    for &p in param {
                        match p {
                            25 => {
                                // DECTCEM: Show / hide cursor
                                self.cursor_mut().visible = is_set;
                            }
                            47 | 1047 | 1049 => {
                                // Alternate screen buffer toggling
                                if is_set {
                                    self.enter_alt_buffer();
                                } else {
                                    self.leave_alt_buffer();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            return;
        }

        match action {
            'm' => {
                // Select Graphic Rendition
                self.handle_sgr(params);
            }
            'A' => {
                // CUU - Cursor Up
                let n = get_param(params, 0, 1).max(1) as usize;
                let cur = self.cursor_mut();
                cur.y = cur.y.saturating_sub(n);
            }
            'B' => {
                // CUD - Cursor Down
                let n = get_param(params, 0, 1).max(1) as usize;
                let cur = self.cursor_mut();
                cur.y = (cur.y + n).min(rows - 1);
            }
            'C' => {
                // CUF - Cursor Forward
                let n = get_param(params, 0, 1).max(1) as usize;
                let cur = self.cursor_mut();
                cur.x = (cur.x + n).min(cols - 1);
            }
            'D' => {
                // CUB - Cursor Back
                let n = get_param(params, 0, 1).max(1) as usize;
                let cur = self.cursor_mut();
                cur.x = cur.x.saturating_sub(n);
            }
            'E' => {
                // CNL - Cursor Next Line
                let n = get_param(params, 0, 1).max(1) as usize;
                let cur = self.cursor_mut();
                cur.x = 0;
                cur.y = (cur.y + n).min(rows - 1);
            }
            'F' => {
                // CPL - Cursor Previous Line
                let n = get_param(params, 0, 1).max(1) as usize;
                let cur = self.cursor_mut();
                cur.x = 0;
                cur.y = cur.y.saturating_sub(n);
            }
            'G' => {
                // CHA - Cursor Character Absolute
                let col = get_param(params, 0, 1).max(1) as usize - 1;
                let cur = self.cursor_mut();
                cur.x = col.min(cols - 1);
            }
            'H' | 'f' => {
                // CUP / HVP - Cursor Position
                let row = get_param(params, 0, 1).max(1) as usize - 1;
                let col = get_param(params, 1, 1).max(1) as usize - 1;
                let cur = self.cursor_mut();
                cur.y = row.min(rows - 1);
                cur.x = col.min(cols - 1);
            }
            'd' => {
                // VPA - Vertical Position Absolute
                let row = get_param(params, 0, 1).max(1) as usize - 1;
                let cur = self.cursor_mut();
                cur.y = row.min(rows - 1);
            }
            'J' => {
                // ED - Erase in Display
                let mode = get_param(params, 0, 0);
                let (x, y) = (self.cursor().x, self.cursor().y);
                self.active_buffer_mut().clear_display(mode, x, y);
            }
            'K' => {
                // EL - Erase in Line
                let mode = get_param(params, 0, 0);
                let (x, y) = (self.cursor().x, self.cursor().y);
                self.active_buffer_mut().clear_line_mode(mode, x, y);
            }
            'S' => {
                // SU - Scroll Up
                let n = get_param(params, 0, 1).max(1) as usize;
                self.active_buffer_mut().scroll_up(n);
            }
            'T' => {
                // SD - Scroll Down
                let n = get_param(params, 0, 1).max(1) as usize;
                self.active_buffer_mut().scroll_down(n);
            }
            'r' => {
                // DECSTBM - Set Top and Bottom Margins
                let top = get_param(params, 0, 1).max(1) as usize - 1;
                let bottom = get_param(params, 1, rows as u16).max(1) as usize - 1;
                self.active_buffer_mut().set_margins(top, bottom);
                self.cursor_mut().x = 0;
                self.cursor_mut().y = 0;
            }
            's' => {
                // Save Cursor
                self.save_cursor();
            }
            'u' => {
                // Restore Cursor
                self.restore_cursor();
            }
            _ => {}
        }
        self.clamp_cursor();
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, byte: u8) {
        match byte {
            b'7' => self.save_cursor(),
            b'8' => self.restore_cursor(),
            b'c' => self.reset_terminal(),
            b'D' => {
                // IND - Index
                let y = self.cursor().y;
                let (_, bottom) = self.active_buffer().scroll_margins();
                if y < bottom {
                    self.cursor_mut().y += 1;
                } else {
                    self.active_buffer_mut().scroll_up(1);
                }
            }
            b'M' => {
                // RI - Reverse Index
                let y = self.cursor().y;
                let (top, _) = self.active_buffer().scroll_margins();
                if y > top {
                    self.cursor_mut().y -= 1;
                } else {
                    self.active_buffer_mut().scroll_down(1);
                }
            }
            b'E' => {
                // NEL - Next Line
                self.cursor_mut().x = 0;
                let y = self.cursor().y;
                let (_, bottom) = self.active_buffer().scroll_margins();
                if y < bottom {
                    self.cursor_mut().y += 1;
                } else {
                    self.active_buffer_mut().scroll_up(1);
                }
            }
            _ => {}
        }
        self.clamp_cursor();
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.len() >= 2 {
            let code = params[0];
            if code == b"0" || code == b"2" {
                if let Ok(title_str) = std::str::from_utf8(params[1]) {
                    self.title = title_str.to_string();
                }
            }
        }
    }
}
