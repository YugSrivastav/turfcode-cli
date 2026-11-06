use std::collections::VecDeque;

use crate::cell::Cell;

/// Default maximum number of historical lines stored in the scrollback ring buffer.
pub const DEFAULT_MAX_SCROLLBACK: usize = 10_000;

/// 2D character grid buffer representing a terminal screen page with a bounded scrollback ring buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenBuffer {
    rows: usize,
    cols: usize,
    cells: Vec<Vec<Cell>>,
    scroll_top: usize,
    scroll_bottom: usize,
    scrollback: VecDeque<Vec<Cell>>,
    max_scrollback: usize,
    scroll_offset: usize,
    allow_scrollback: bool,
}

impl ScreenBuffer {
    /// Allocate a new screen buffer initialized with blank cells and default scrollback limit (10,000).
    pub fn new(rows: usize, cols: usize) -> Self {
        Self::with_scrollback(rows, cols, DEFAULT_MAX_SCROLLBACK)
    }

    /// Allocate a new screen buffer with a custom maximum scrollback limit.
    pub fn with_scrollback(rows: usize, cols: usize, max_scrollback: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let cells = vec![vec![Cell::default(); cols]; rows];
        Self {
            rows,
            cols,
            cells,
            scroll_top: 0,
            scroll_bottom: rows - 1,
            scrollback: VecDeque::new(),
            max_scrollback,
            scroll_offset: 0,
            allow_scrollback: true,
        }
    }

    /// Number of vertical rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Number of horizontal columns.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Reference to the scrollback ring buffer.
    pub fn scrollback(&self) -> &VecDeque<Vec<Cell>> {
        &self.scrollback
    }

    /// Mutable reference to the scrollback ring buffer.
    pub fn scrollback_mut(&mut self) -> &mut VecDeque<Vec<Cell>> {
        &mut self.scrollback
    }

    /// Number of lines currently in the scrollback history buffer.
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Maximum number of scrollback lines retained.
    pub fn max_scrollback(&self) -> usize {
        self.max_scrollback
    }

    /// Update the maximum scrollback capacity limit.
    pub fn set_max_scrollback(&mut self, max: usize) {
        self.max_scrollback = max;
        while self.scrollback.len() > self.max_scrollback {
            self.scrollback.pop_front();
        }
        self.scroll_offset = self.scroll_offset.min(self.scrollback.len());
    }

    /// Whether this screen buffer records evicted lines into scrollback history.
    pub fn allow_scrollback(&self) -> bool {
        self.allow_scrollback
    }

    /// Enable or disable scrollback history eviction for this buffer.
    pub fn set_allow_scrollback(&mut self, allow: bool) {
        self.allow_scrollback = allow;
        if !allow {
            self.scrollback.clear();
            self.scroll_offset = 0;
        }
    }

    /// Current viewport scroll offset (0 = live bottom view, >0 = scrolled into history).
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Set explicit viewport scroll offset, clamped to available scrollback lines.
    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset.min(self.scrollback.len());
    }

    /// Scroll viewport up by `n` lines into history.
    pub fn scroll_up_viewport(&mut self, n: usize) {
        self.scroll_offset = (self.scroll_offset + n).min(self.scrollback.len());
    }

    /// Scroll viewport down by `n` lines toward the live bottom stream.
    pub fn scroll_down_viewport(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }

    /// Reset viewport scroll back to the live bottom view (scroll_offset = 0).
    pub fn reset_viewport_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    /// Clear all historical rows from the scrollback buffer.
    pub fn clear_scrollback(&mut self) {
        self.scrollback.clear();
        self.scroll_offset = 0;
    }

    /// Retrieve a reference to the cell at `(x, y)` if within bounds on the active screen grid.
    pub fn cell(&self, x: usize, y: usize) -> Option<&Cell> {
        self.cells.get(y).and_then(|row| row.get(x))
    }

    /// Retrieve a mutable reference to the cell at `(x, y)` if within bounds on the active screen grid.
    pub fn cell_mut(&mut self, x: usize, y: usize) -> Option<&mut Cell> {
        self.cells.get_mut(y).and_then(|row| row.get_mut(x))
    }

    /// Write a cell at `(x, y)` if within bounds.
    pub fn set_cell(&mut self, x: usize, y: usize, cell: Cell) {
        if y < self.rows && x < self.cols {
            self.cells[y][x] = cell;
        }
    }

    /// Set the top and bottom scrolling margins (DECSTBM).
    pub fn set_margins(&mut self, top: usize, bottom: usize) {
        let top = top.min(self.rows.saturating_sub(1));
        let bottom = bottom.min(self.rows.saturating_sub(1));
        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
        } else {
            self.scroll_top = 0;
            self.scroll_bottom = self.rows.saturating_sub(1);
        }
    }

    /// Get the current scrolling margins `(top, bottom)`.
    pub fn scroll_margins(&self) -> (usize, usize) {
        (self.scroll_top, self.scroll_bottom)
    }

    /// Reset scrolling margins to the entire screen.
    pub fn reset_margins(&mut self) {
        self.scroll_top = 0;
        self.scroll_bottom = self.rows.saturating_sub(1);
    }

    /// Scroll up by `count` lines within the defined scrolling margins.
    ///
    /// When scrolling the full screen in a scrollback-enabled buffer, evicted rows
    /// are pushed into the bounded `scrollback` ring buffer.
    pub fn scroll_up(&mut self, count: usize) {
        let count = count.min(self.scroll_bottom.saturating_sub(self.scroll_top) + 1);
        for _ in 0..count {
            if self.scroll_top <= self.scroll_bottom && self.scroll_bottom < self.rows {
                let removed = self.cells.remove(self.scroll_top);

                // Evict into scrollback if full-screen scrolling is active and scrollback enabled
                if self.allow_scrollback && self.max_scrollback > 0 && self.scroll_top == 0 {
                    while self.scrollback.len() >= self.max_scrollback {
                        self.scrollback.pop_front();
                    }
                    self.scrollback.push_back(removed);
                }

                self.cells
                    .insert(self.scroll_bottom, vec![Cell::default(); self.cols]);
            }
        }
    }

    /// Scroll down by `count` lines within the defined scrolling margins.
    pub fn scroll_down(&mut self, count: usize) {
        let count = count.min(self.scroll_bottom.saturating_sub(self.scroll_top) + 1);
        for _ in 0..count {
            if self.scroll_top <= self.scroll_bottom && self.scroll_bottom < self.rows {
                self.cells.remove(self.scroll_bottom);
                self.cells
                    .insert(self.scroll_top, vec![Cell::default(); self.cols]);
            }
        }
    }

    /// Render visible viewport rows taking into account the historical `offset` and requested row `count`.
    ///
    /// When `offset == 0`, returns the live bottom grid rows.
    /// When `offset > 0`, historical rows from `scrollback` are seamlessly sliced into view.
    pub fn render_viewport_rows(&self, offset: usize, count: usize) -> Vec<Vec<Cell>> {
        if count == 0 {
            return Vec::new();
        }

        let hist_len = self.scrollback.len();
        let clamped_offset = offset.min(hist_len);
        let start_idx = hist_len.saturating_sub(clamped_offset);

        let mut rows = Vec::with_capacity(count);
        for y in 0..count {
            let timeline_idx = start_idx + y;
            if timeline_idx < hist_len {
                rows.push(self.scrollback[timeline_idx].clone());
            } else {
                let cell_idx = timeline_idx - hist_len;
                if cell_idx < self.rows {
                    rows.push(self.cells[cell_idx].clone());
                } else {
                    rows.push(vec![Cell::default(); self.cols]);
                }
            }
        }

        rows
    }

    /// Render visible viewport rows using the current buffer's `scroll_offset`.
    pub fn render_viewport(&self) -> Vec<Vec<Cell>> {
        self.render_viewport_rows(self.scroll_offset, self.rows)
    }

    /// Clear all cells on the screen.
    pub fn clear(&mut self) {
        for row in &mut self.cells {
            for cell in row {
                cell.reset();
            }
        }
    }

    /// Clear line `y`.
    pub fn clear_line(&mut self, y: usize) {
        if let Some(row) = self.cells.get_mut(y) {
            for cell in row {
                cell.reset();
            }
        }
    }

    /// Clear a subsegment of row `y` between `start_x` and `end_x` (inclusive).
    pub fn clear_line_part(&mut self, y: usize, start_x: usize, end_x: usize) {
        if let Some(row) = self.cells.get_mut(y) {
            let start = start_x.min(self.cols);
            let end = (end_x + 1).min(self.cols);
            for cell in &mut row[start..end] {
                cell.reset();
            }
        }
    }

    /// Clear display according to ANSI ED mode:
    /// - `0`: From cursor to end of screen
    /// - `1`: From beginning of screen to cursor
    /// - `2`: Entire screen
    /// - `3`: Entire screen and clear scrollback buffer
    pub fn clear_display(&mut self, mode: u16, cursor_x: usize, cursor_y: usize) {
        match mode {
            0 => {
                // Clear from cursor to end of line
                self.clear_line_part(cursor_y, cursor_x, self.cols.saturating_sub(1));
                // Clear all remaining lines below
                for y in (cursor_y + 1)..self.rows {
                    self.clear_line(y);
                }
            }
            1 => {
                // Clear all lines above
                for y in 0..cursor_y {
                    self.clear_line(y);
                }
                // Clear from start of line to cursor
                self.clear_line_part(cursor_y, 0, cursor_x);
            }
            2 => {
                self.clear();
            }
            3 => {
                self.clear();
                self.clear_scrollback();
            }
            _ => {}
        }
    }

    /// Clear line according to ANSI EL mode:
    /// - `0`: From cursor to end of line
    /// - `1`: From start of line to cursor
    /// - `2`: Entire line
    pub fn clear_line_mode(&mut self, mode: u16, cursor_x: usize, cursor_y: usize) {
        match mode {
            0 => self.clear_line_part(cursor_y, cursor_x, self.cols.saturating_sub(1)),
            1 => self.clear_line_part(cursor_y, 0, cursor_x),
            2 => self.clear_line(cursor_y),
            _ => {}
        }
    }

    /// Resize the grid buffer and historical scrollback rows to new dimensions.
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        let new_rows = new_rows.max(1);
        let new_cols = new_cols.max(1);

        // Adjust column width for existing rows in grid
        for row in &mut self.cells {
            row.resize(new_cols, Cell::default());
        }

        // Adjust column width for historical rows in scrollback
        for row in &mut self.scrollback {
            row.resize(new_cols, Cell::default());
        }

        // Adjust row count
        if new_rows > self.rows {
            for _ in self.rows..new_rows {
                self.cells.push(vec![Cell::default(); new_cols]);
            }
        } else if new_rows < self.rows {
            self.cells.truncate(new_rows);
        }

        self.rows = new_rows;
        self.cols = new_cols;
        self.scroll_offset = self.scroll_offset.min(self.scrollback.len());
        self.reset_margins();
    }

    /// Extract row text as a UTF-8 string.
    pub fn row_text(&self, y: usize) -> String {
        match self.cells.get(y) {
            Some(row) => row.iter().map(|cell| cell.c).collect(),
            None => String::new(),
        }
    }

    /// Extract row text with trailing blank spaces trimmed.
    pub fn row_text_trimmed(&self, y: usize) -> String {
        self.row_text(y).trim_end().to_string()
    }

    /// Extract the entire screen as a newline-separated string.
    pub fn text(&self) -> String {
        (0..self.rows)
            .map(|y| self.row_text(y))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
