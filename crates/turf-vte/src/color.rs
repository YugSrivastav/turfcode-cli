/// Standard 16 ANSI named colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedColor {
    Black = 0,
    Red = 1,
    Green = 2,
    Yellow = 3,
    Blue = 4,
    Magenta = 5,
    Cyan = 6,
    White = 7,
    BrightBlack = 8,
    BrightRed = 9,
    BrightGreen = 10,
    BrightYellow = 11,
    BrightBlue = 12,
    BrightMagenta = 13,
    BrightCyan = 14,
    BrightWhite = 15,
}

impl NamedColor {
    /// Convert an index `0..=15` to a `NamedColor`.
    pub fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::Black),
            1 => Some(Self::Red),
            2 => Some(Self::Green),
            3 => Some(Self::Yellow),
            4 => Some(Self::Blue),
            5 => Some(Self::Magenta),
            6 => Some(Self::Cyan),
            7 => Some(Self::White),
            8 => Some(Self::BrightBlack),
            9 => Some(Self::BrightRed),
            10 => Some(Self::BrightGreen),
            11 => Some(Self::BrightYellow),
            12 => Some(Self::BrightBlue),
            13 => Some(Self::BrightMagenta),
            14 => Some(Self::BrightCyan),
            15 => Some(Self::BrightWhite),
            _ => None,
        }
    }
}

/// Representation of terminal text foreground and background colors.
/// Supports default terminal palette, standard 16 ANSI colors, 256-color indexed palette,
/// and 24-bit TrueColor RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Color {
    #[default]
    Default,
    Named(NamedColor),
    Indexed(u8),
    Rgb(u8, u8, u8),
}

impl Color {
    /// True if color is the default terminal color.
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }

    /// Approximate 24-bit sRGB representation of the color.
    pub fn to_rgb(&self) -> (u8, u8, u8) {
        match self {
            Self::Default => (204, 204, 204),
            Self::Named(named) => match named {
                NamedColor::Black => (0, 0, 0),
                NamedColor::Red => (205, 49, 49),
                NamedColor::Green => (13, 188, 121),
                NamedColor::Yellow => (229, 229, 16),
                NamedColor::Blue => (36, 114, 200),
                NamedColor::Magenta => (188, 63, 188),
                NamedColor::Cyan => (17, 168, 205),
                NamedColor::White => (229, 229, 229),
                NamedColor::BrightBlack => (102, 102, 102),
                NamedColor::BrightRed => (241, 76, 76),
                NamedColor::BrightGreen => (35, 209, 139),
                NamedColor::BrightYellow => (245, 245, 67),
                NamedColor::BrightBlue => (59, 142, 234),
                NamedColor::BrightMagenta => (214, 112, 214),
                NamedColor::BrightCyan => (41, 184, 219),
                NamedColor::BrightWhite => (255, 255, 255),
            },
            Self::Indexed(idx) => {
                if *idx < 16 {
                    Self::Named(NamedColor::from_index(*idx).unwrap()).to_rgb()
                } else if *idx < 232 {
                    // 6x6x6 color cube
                    let i = idx - 16;
                    let r = (i / 36) % 6;
                    let g = (i / 6) % 6;
                    let b = i % 6;
                    let conv = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                    (conv(r), conv(g), conv(b))
                } else {
                    // Grayscale 232..=255
                    let gray = 8 + (idx - 232) * 10;
                    (gray, gray, gray)
                }
            }
            Self::Rgb(r, g, b) => (*r, *g, *b),
        }
    }
}
