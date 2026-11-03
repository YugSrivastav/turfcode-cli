use crate::color::Color;

/// Text rendering flags and styling attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct CellFlags {
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
    pub strikethrough: bool,
    pub blink: bool,
    pub hidden: bool,
}

impl CellFlags {
    pub fn is_empty(&self) -> bool {
        !self.bold
            && !self.dim
            && !self.italic
            && !self.underline
            && !self.inverse
            && !self.strikethrough
            && !self.blink
            && !self.hidden
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// A single character cell on the terminal screen grid.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: CellFlags,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Color::Default,
            bg: Color::Default,
            flags: CellFlags::default(),
        }
    }
}

impl Cell {
    /// Construct a new cell with the specified character and styling.
    pub fn new(c: char, fg: Color, bg: Color, flags: CellFlags) -> Self {
        Self { c, fg, bg, flags }
    }

    /// Construct a plain cell with default colors and no styling.
    pub fn with_char(c: char) -> Self {
        Self {
            c,
            ..Default::default()
        }
    }

    /// Reset cell to blank space with default attributes.
    pub fn reset(&mut self) {
        self.c = ' ';
        self.fg = Color::Default;
        self.bg = Color::Default;
        self.flags.reset();
    }
}
