/// State of the terminal cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// 0-indexed column coordinate (horizontal, X).
    pub x: usize,
    /// 0-indexed row coordinate (vertical, Y).
    pub y: usize,
    /// Visibility state of the cursor (`\x1b[?25h` / `\x1b[?25l`).
    pub visible: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            visible: true,
        }
    }
}

impl Cursor {
    pub fn new(x: usize, y: usize, visible: bool) -> Self {
        Self { x, y, visible }
    }

    pub fn reset(&mut self) {
        self.x = 0;
        self.y = 0;
        self.visible = true;
    }
}
