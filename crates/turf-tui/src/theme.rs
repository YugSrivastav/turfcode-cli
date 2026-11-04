//! Catppuccin Mocha / Herdr Dark Slate Design System and Theme Tokens for TurfCode TUI.
//!
//! Conforms to [MD-050] Herdr-Grade Visual Polish, Catppuccin Theme Architecture,
//! and Non-Conflicting Spatial Keybindings.

use ratatui::style::{Color, Modifier, Style};

/// Transparent/native terminal background (`Color::Reset`).
pub const COLOR_BG: Color = Color::Reset;

/// Catppuccin Mantle header background (#181926).
pub const COLOR_HEADER_BG: Color = Color::Rgb(24, 25, 38);

/// Catppuccin Blue accent (#89b4fa).
pub const COLOR_ACCENT: Color = Color::Rgb(137, 180, 250);

/// Catppuccin Surface0 muted border (#313244).
pub const COLOR_MUTED_BORDER: Color = Color::Rgb(49, 50, 68);

/// Catppuccin Text primary (#cdd6f4).
pub const COLOR_TEXT_PRIMARY: Color = Color::Rgb(205, 214, 244);

/// Alias for bright / primary text.
pub const COLOR_TEXT_BRIGHT: Color = COLOR_TEXT_PRIMARY;

/// Catppuccin Overlay0 muted text (#6c7086).
pub const COLOR_TEXT_MUTED: Color = Color::Rgb(108, 112, 134);

/// Catppuccin Surface2 dim text (#585b70).
pub const COLOR_TEXT_DIM: Color = Color::Rgb(88, 91, 112);

/// Catppuccin Crust / dark slate active pill highlight (#1e2030 / #232838).
pub const COLOR_ACTIVE_PILL: Color = Color::Rgb(30, 32, 48);

/// Catppuccin Green indicator dot (#a6e3a1).
pub const COLOR_GREEN_DOT: Color = Color::Rgb(166, 227, 161);

/// Subtle cyan / sky for git branch (#89dceb).
pub const COLOR_CYAN: Color = Color::Rgb(137, 220, 235);

/// Status badge: Queued (#6c7086).
pub const COLOR_STATUS_QUEUED: Color = Color::Rgb(108, 112, 134);

/// Status badge: In Progress amber/yellow (#f9e2af).
pub const COLOR_STATUS_PROGRESS: Color = Color::Rgb(249, 226, 175);

/// Status badge: Done green (#a6e3a1).
pub const COLOR_STATUS_DONE: Color = Color::Rgb(166, 227, 161);

/// Status badge: Conflict / lock red (#f38ba8).
pub const COLOR_STATUS_LOCKED: Color = Color::Rgb(243, 139, 168);

/// Status badge: Cyan peer indicator (#89dceb).
pub const COLOR_STATUS_PEER: Color = Color::Rgb(137, 220, 235);

/// Backward compatibility alias for Electric Lime.
pub const COLOR_ELECTRIC_LIME: Color = Color::Rgb(57, 255, 20);

/// Backward compatibility alias for Bright Green.
pub const COLOR_BRIGHT_GREEN: Color = COLOR_GREEN_DOT;

/// Backward compatibility alias for Active Border.
pub const COLOR_ACTIVE_BORDER: Color = COLOR_ACCENT;

// Divider glyphs
pub const GLYPH_DIVIDER_V: char = '│';
pub const GLYPH_DIVIDER_H: char = '─';
pub const GLYPH_CROSS: char = '┼';
pub const GLYPH_TEE_L: char = '├';
pub const GLYPH_TEE_R: char = '┤';
pub const GLYPH_TEE_T: char = '┬';
pub const GLYPH_TEE_B: char = '┴';

/// Styles collection for easy application across cockpit components.
pub struct Theme;

impl Theme {
    /// Sleek soft blue/slate pill for the active tab.
    pub fn active_tab_pill() -> Style {
        Style::default()
            .bg(COLOR_ACTIVE_PILL)
            .fg(COLOR_ACCENT)
            .add_modifier(Modifier::BOLD)
    }

    /// Inactive tab styling: muted text on header background.
    pub fn inactive_tab() -> Style {
        Style::default()
            .fg(COLOR_TEXT_MUTED)
            .bg(COLOR_HEADER_BG)
    }

    /// Add new tab "+" indicator styling.
    pub fn tab_add_button() -> Style {
        Style::default()
            .fg(COLOR_TEXT_MUTED)
            .bg(COLOR_HEADER_BG)
    }

    /// Background style for the top tab strip.
    pub fn tab_strip_bg() -> Style {
        Style::default().bg(COLOR_HEADER_BG).fg(COLOR_TEXT_PRIMARY)
    }

    /// Subtle single hairline divider line style.
    pub fn divider() -> Style {
        Style::default().fg(COLOR_MUTED_BORDER).bg(COLOR_BG)
    }

    /// Highlighted divider line style.
    pub fn active_divider() -> Style {
        Style::default().fg(COLOR_ACCENT).bg(COLOR_BG)
    }

    /// Sidebar section header style: clean lowercase muted text.
    pub fn sidebar_header() -> Style {
        Style::default().fg(COLOR_TEXT_MUTED).bg(COLOR_BG)
    }

    /// Subtle sidebar item style.
    pub fn sidebar_item() -> Style {
        Style::default().fg(COLOR_TEXT_PRIMARY).bg(COLOR_BG)
    }

    /// Subtle dim sidebar label.
    pub fn sidebar_dim() -> Style {
        Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_BG)
    }

    /// Active peer capsule highlight pill.
    pub fn active_peer_pill() -> Style {
        Style::default().bg(COLOR_ACTIVE_PILL).fg(COLOR_ACCENT)
    }

    /// Bottom prompt symbol (`> `).
    pub fn prompt_prefix() -> Style {
        Style::default()
            .fg(COLOR_ACCENT)
            .bg(COLOR_BG)
            .add_modifier(Modifier::BOLD)
    }

    /// Bottom input text style.
    pub fn prompt_input() -> Style {
        Style::default().fg(COLOR_TEXT_PRIMARY).bg(COLOR_BG)
    }

    /// Shortcut key label style.
    pub fn shortcut_key() -> Style {
        Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_BG)
    }

    /// Shortcut description style.
    pub fn shortcut_desc() -> Style {
        Style::default().fg(COLOR_TEXT_DIM).bg(COLOR_BG)
    }
}
