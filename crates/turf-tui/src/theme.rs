//! Electric Lime Design System and Theme Tokens for TurfCode TUI.
//!
//! Conforms to [MD-004] Electric Lime TrueColor Terminal Atmosphere,
//! [MD-042] Herdr-Style Unified Typographic Layout, and [MD-044] Herdr Layout Replication.

use ratatui::style::{Color, Modifier, Style};

/// Obsidian-emerald dark background (#081008).
pub const COLOR_BG: Color = Color::Rgb(8, 16, 8);

/// Slightly elevated dark emerald header background (#0d1a0d).
pub const COLOR_HEADER_BG: Color = Color::Rgb(13, 26, 13);

/// Signature High-Contrast Electric Lime accent (#39FF14).
pub const COLOR_ELECTRIC_LIME: Color = Color::Rgb(57, 255, 20);

/// Success / Done state green (#22c55e).
pub const COLOR_BRIGHT_GREEN: Color = Color::Rgb(34, 197, 94);

/// Secondary dark green (#166534).
pub const COLOR_MID_GREEN: Color = Color::Rgb(22, 101, 52);

/// Subtle green (#2a5a2a).
pub const COLOR_DIM_GREEN: Color = Color::Rgb(42, 90, 42);

/// Dark obsidian-emerald divider rule (#1b3b1b).
pub const COLOR_MUTED_BORDER: Color = Color::Rgb(27, 59, 27);

/// Focused / active border (#39FF14).
pub const COLOR_ACTIVE_BORDER: Color = Color::Rgb(57, 255, 20);

/// Crisp white with subtle emerald tint (#f0fff0).
pub const COLOR_TEXT_BRIGHT: Color = Color::Rgb(240, 255, 240);

/// Muted emerald label text (#4a7c4a).
pub const COLOR_TEXT_MUTED: Color = Color::Rgb(74, 124, 74);

/// Very dim text / comments (#2d4d2d).
pub const COLOR_TEXT_DIM: Color = Color::Rgb(45, 77, 45);

/// Status badge: Queued (#888888).
pub const COLOR_STATUS_QUEUED: Color = Color::Rgb(136, 136, 136);

/// Status badge: In Progress amber (#eab308).
pub const COLOR_STATUS_PROGRESS: Color = Color::Rgb(234, 179, 8);

/// Status badge: Done (#22c55e).
pub const COLOR_STATUS_DONE: Color = Color::Rgb(34, 197, 94);

/// Status badge: Conflict / lock red (#ef4444).
pub const COLOR_STATUS_LOCKED: Color = Color::Rgb(239, 68, 68);

/// Status badge: Cyan peer indicator (#06b6d4).
pub const COLOR_STATUS_PEER: Color = Color::Rgb(6, 182, 212);

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
    /// Inverted high-contrast Electric Lime pill for the active tab.
    pub fn active_tab_pill() -> Style {
        Style::default()
            .bg(COLOR_ELECTRIC_LIME)
            .fg(COLOR_BG)
            .add_modifier(Modifier::BOLD)
    }

    /// Inactive tab styling.
    pub fn inactive_tab() -> Style {
        Style::default()
            .fg(COLOR_TEXT_MUTED)
            .bg(COLOR_HEADER_BG)
    }

    /// Add new tab "+" indicator styling.
    pub fn tab_add_button() -> Style {
        Style::default()
            .fg(COLOR_ELECTRIC_LIME)
            .bg(COLOR_HEADER_BG)
            .add_modifier(Modifier::BOLD)
    }

    /// Background style for the top tab strip.
    pub fn tab_strip_bg() -> Style {
        Style::default().bg(COLOR_HEADER_BG).fg(COLOR_TEXT_BRIGHT)
    }

    /// Subtle single divider line style.
    pub fn divider() -> Style {
        Style::default().fg(COLOR_MUTED_BORDER)
    }

    /// Highlighted divider line style.
    pub fn active_divider() -> Style {
        Style::default().fg(COLOR_ELECTRIC_LIME)
    }

    /// Sidebar section header style.
    pub fn sidebar_header() -> Style {
        Style::default()
            .fg(COLOR_ELECTRIC_LIME)
            .add_modifier(Modifier::BOLD)
    }

    /// Subtle sidebar item style.
    pub fn sidebar_item() -> Style {
        Style::default().fg(COLOR_TEXT_BRIGHT)
    }

    /// Subtle dim sidebar label.
    pub fn sidebar_dim() -> Style {
        Style::default().fg(COLOR_TEXT_MUTED)
    }

    /// Bottom prompt symbol (`turf > `).
    pub fn prompt_prefix() -> Style {
        Style::default()
            .fg(COLOR_ELECTRIC_LIME)
            .add_modifier(Modifier::BOLD)
    }

    /// Bottom input text style.
    pub fn prompt_input() -> Style {
        Style::default().fg(COLOR_TEXT_BRIGHT)
    }

    /// Shortcut key label style (e.g. `[Tab]`).
    pub fn shortcut_key() -> Style {
        Style::default().fg(COLOR_ELECTRIC_LIME)
    }

    /// Shortcut description style (e.g. `Focus`).
    pub fn shortcut_desc() -> Style {
        Style::default().fg(COLOR_TEXT_MUTED)
    }
}
