//! High-performance, zero-flicker cell blitter from `turf_vte::ScreenBuffer`
//! directly into Ratatui's `Buffer`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color as RatColor, Modifier, Style};
use turf_vte::{Cell, CellFlags, Color as VteColor, NamedColor, ScreenBuffer};

use crate::theme::{COLOR_BG, COLOR_ELECTRIC_LIME, COLOR_TEXT_BRIGHT};

/// Convert a `turf_vte::Color` into a `ratatui::style::Color`.
pub fn vte_color_to_ratatui(color: VteColor, is_bg: bool) -> RatColor {
    match color {
        VteColor::Default => {
            if is_bg {
                COLOR_BG
            } else {
                COLOR_TEXT_BRIGHT
            }
        }
        VteColor::Named(named) => match named {
            NamedColor::Black => RatColor::Black,
            NamedColor::Red => RatColor::Red,
            NamedColor::Green => RatColor::Green,
            NamedColor::Yellow => RatColor::Yellow,
            NamedColor::Blue => RatColor::Blue,
            NamedColor::Magenta => RatColor::Magenta,
            NamedColor::Cyan => RatColor::Cyan,
            NamedColor::White => RatColor::Gray,
            NamedColor::BrightBlack => RatColor::DarkGray,
            NamedColor::BrightRed => RatColor::LightRed,
            NamedColor::BrightGreen => RatColor::LightGreen,
            NamedColor::BrightYellow => RatColor::LightYellow,
            NamedColor::BrightBlue => RatColor::LightBlue,
            NamedColor::BrightMagenta => RatColor::LightMagenta,
            NamedColor::BrightCyan => RatColor::LightCyan,
            NamedColor::BrightWhite => RatColor::White,
        },
        VteColor::Indexed(idx) => RatColor::Indexed(idx),
        VteColor::Rgb(r, g, b) => RatColor::Rgb(r, g, b),
    }
}

/// Convert `turf_vte::CellFlags` into `ratatui::style::Modifier`.
pub fn vte_flags_to_modifiers(flags: CellFlags) -> Modifier {
    let mut modifier = Modifier::empty();
    if flags.bold {
        modifier |= Modifier::BOLD;
    }
    if flags.dim {
        modifier |= Modifier::DIM;
    }
    if flags.italic {
        modifier |= Modifier::ITALIC;
    }
    if flags.underline {
        modifier |= Modifier::UNDERLINED;
    }
    if flags.inverse {
        modifier |= Modifier::REVERSED;
    }
    if flags.strikethrough {
        modifier |= Modifier::CROSSED_OUT;
    }
    if flags.blink {
        modifier |= Modifier::SLOW_BLINK;
    }
    if flags.hidden {
        modifier |= Modifier::HIDDEN;
    }
    modifier
}

/// Blit an entire `ScreenBuffer` directly into a Ratatui `Buffer` slice bounded by `area`.
///
/// This performs zero heap allocations during the render cycle, providing
/// 60fps flicker-free terminal blitting.
pub fn blit_screen_buffer(
    screen: &ScreenBuffer,
    area: Rect,
    buf: &mut Buffer,
    cursor: Option<(usize, usize)>,
    is_focused: bool,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let rows = (area.height as usize).min(screen.rows());
    let cols = (area.width as usize).min(screen.cols());

    for y in 0..rows {
        let dest_y = area.y + y as u16;
        for x in 0..cols {
            let dest_x = area.x + x as u16;

            let cell: &Cell = match screen.cell(x, y) {
                Some(c) => c,
                None => continue,
            };

            let c = if cell.c == '\0' || cell.c == ' ' {
                ' '
            } else {
                cell.c
            };

            let mut fg = vte_color_to_ratatui(cell.fg, false);
            let mut bg = vte_color_to_ratatui(cell.bg, true);
            let mut modifier = vte_flags_to_modifiers(cell.flags);

            // Render cursor if this cell matches the cursor coordinate and pane is focused
            if let Some((cx, cy)) = cursor {
                if cx == x && cy == y && is_focused {
                    bg = COLOR_ELECTRIC_LIME;
                    fg = COLOR_BG;
                    modifier |= Modifier::BOLD;
                }
            }

            let dest_cell = &mut buf[(dest_x, dest_y)];
            dest_cell.set_char(c);
            dest_cell.set_fg(fg);
            dest_cell.set_bg(bg);
            dest_cell.set_style(Style::default().add_modifier(modifier));
        }

        // Blank out any remaining trailing columns if screen width is less than area width
        for x in cols..(area.width as usize) {
            let dest_x = area.x + x as u16;
            let dest_cell = &mut buf[(dest_x, dest_y)];
            dest_cell.set_char(' ');
            dest_cell.set_bg(COLOR_BG);
            dest_cell.set_fg(COLOR_TEXT_BRIGHT);
            dest_cell.set_style(Style::default());
        }
    }

    // Blank out any remaining trailing rows if screen height is less than area height
    for y in rows..(area.height as usize) {
        let dest_y = area.y + y as u16;
        for x in 0..area.width {
            let dest_x = area.x + x;
            let dest_cell = &mut buf[(dest_x, dest_y)];
            dest_cell.set_char(' ');
            dest_cell.set_bg(COLOR_BG);
            dest_cell.set_fg(COLOR_TEXT_BRIGHT);
            dest_cell.set_style(Style::default());
        }
    }
}
