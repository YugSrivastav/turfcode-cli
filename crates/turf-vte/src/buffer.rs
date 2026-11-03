use crate::cell::Cell;

/// 2D character grid buffer representing a terminal screen page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenBuffer {
    rows: usize,
    cols: usize,
    cells: Vec<Vec<Cell>>,
    scroll_top: usize,
    scroll_bottom: usize,
}

impl ScreenBuffer {
    /// Allocate a new screen buffer initialized with blank cells.
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        let cells = vec![vec![Cell::default(); cols]; rows];
        Self {
            rows,
            cols,
            cells,
            scroll_top: 0,
            scroll_bottom: rows - 1,
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

    /// Retrieve a reference to the cell at `(x, y)` if within bounds.
    pub fn cell(&self, x: usize, y: usize) -> Option<&Cell> {
        self.cells.get(y).and_then(|row| row.get(x))
    }

    /// Retrieve a mutable reference to the cell at `(x, y)` if within bounds.
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
    pub fn scroll_up(&mut self, count: usize) {
        let count = count.min(self.scroll_bottom.saturating_sub(self.scroll_top) + 1);
        for _ in 0..count {
            if self.scroll_top <= self.scroll_bottom && self.scroll_bottom < self.rows {
                self.cells.remove(self.scroll_top);
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
    /// - `2` or `3`: Entire screen
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
            2 | 3 => {
                self.clear();
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

    /// Resize the grid buffer.
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        let new_rows = new_rows.max(1);
        let new_cols = new_cols.max(1);

        // Adjust column width for existing rows
        for row in &mut self.cells {
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
