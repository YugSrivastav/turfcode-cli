//! Spatial Layout calculations for Herdr-grade TurfCode Cockpit.
//!
//! Conforms to [MD-001] Two-Section TUI Layout Orientation, [MD-042], and [MD-044].

use ratatui::layout::Rect;

/// Complete calculated cockpit geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CockpitLayout {
    pub total_area: Rect,
    pub sidebar_visible: bool,
    pub sidebar_area: Rect,
    pub sidebar_divider: Rect,
    pub main_area: Rect,
    pub tab_strip_area: Rect,
    pub terminal_area: Rect,
    pub input_divider_area: Rect,
    pub input_bar_area: Rect,
}

impl CockpitLayout {
    /// Calculate layout rectangles based on terminal dimensions and sidebar visibility.
    pub fn calculate(area: Rect, sidebar_visible: bool) -> Self {
        if area.width == 0 || area.height == 0 {
            return Self::empty(area);
        }

        let (sidebar_area, sidebar_divider, main_area) = if sidebar_visible && area.width >= 40 {
            // Target ~22% width, bounded to sane minimum (22 cols) and maximum (35% of screen width)
            let raw_w = ((area.width as f32) * 0.22).round() as u16;
            let sidebar_w = raw_w.max(22).min(area.width.saturating_sub(25));
            let s_area = Rect {
                x: area.x,
                y: area.y,
                width: sidebar_w,
                height: area.height,
            };
            let div_area = Rect {
                x: area.x + sidebar_w,
                y: area.y,
                width: 1,
                height: area.height,
            };
            let m_x = area.x + sidebar_w + 1;
            let m_w = area.width.saturating_sub(sidebar_w + 1);
            let m_area = Rect {
                x: m_x,
                y: area.y,
                width: m_w,
                height: area.height,
            };
            (s_area, div_area, m_area)
        } else {
            (
                Rect::default(),
                Rect::default(),
                Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: area.height,
                },
            )
        };

        // Within main_area:
        // Row 0: Tab strip (height: 1)
        // Row height-2: Input divider line (height: 1)
        // Row height-1: Input bar prompt (height: 1)
        // Center: Terminal surface
        let (tab_strip_area, terminal_area, input_divider_area, input_bar_area) =
            if main_area.height >= 4 {
                let tab_strip = Rect {
                    x: main_area.x,
                    y: main_area.y,
                    width: main_area.width,
                    height: 1,
                };
                let term_h = main_area.height.saturating_sub(3);
                let term = Rect {
                    x: main_area.x,
                    y: main_area.y + 1,
                    width: main_area.width,
                    height: term_h,
                };
                let in_div = Rect {
                    x: main_area.x,
                    y: main_area.y + 1 + term_h,
                    width: main_area.width,
                    height: 1,
                };
                let in_bar = Rect {
                    x: main_area.x,
                    y: main_area.y + 2 + term_h,
                    width: main_area.width,
                    height: 1,
                };
                (tab_strip, term, in_div, in_bar)
            } else if main_area.height == 3 {
                (
                    Rect {
                        x: main_area.x,
                        y: main_area.y,
                        width: main_area.width,
                        height: 1,
                    },
                    Rect {
                        x: main_area.x,
                        y: main_area.y + 1,
                        width: main_area.width,
                        height: 1,
                    },
                    Rect::default(),
                    Rect {
                        x: main_area.x,
                        y: main_area.y + 2,
                        width: main_area.width,
                        height: 1,
                    },
                )
            } else {
                (main_area, Rect::default(), Rect::default(), Rect::default())
            };

        Self {
            total_area: area,
            sidebar_visible,
            sidebar_area,
            sidebar_divider,
            main_area,
            tab_strip_area,
            terminal_area,
            input_divider_area,
            input_bar_area,
        }
    }

    fn empty(area: Rect) -> Self {
        Self {
            total_area: area,
            sidebar_visible: false,
            sidebar_area: Rect::default(),
            sidebar_divider: Rect::default(),
            main_area: Rect::default(),
            tab_strip_area: Rect::default(),
            terminal_area: Rect::default(),
            input_divider_area: Rect::default(),
            input_bar_area: Rect::default(),
        }
    }

    /// Calculate inner 4-section layout of the sidebar.
    pub fn calculate_sidebar_sections(&self) -> Option<SidebarSectionLayout> {
        if !self.sidebar_visible || self.sidebar_area.width == 0 || self.sidebar_area.height == 0 {
            return None;
        }

        let area = self.sidebar_area;
        let h = area.height;

        // If height is small, compress sections
        let spaces_h = if h >= 24 { 4 } else { 3 };
        let locks_h = if h >= 24 { 4 } else { 3 };
        let peers_h = if h >= 24 { 4 } else { 3 };
        let dividers_total = 3; // 3 horizontal divider lines

        let fixed_h = spaces_h + locks_h + peers_h + dividers_total;
        let tasks_h = if h > fixed_h {
            h - fixed_h
        } else {
            (h / 4).max(1)
        };

        let mut curr_y = area.y;

        let spaces_area = Rect {
            x: area.x,
            y: curr_y,
            width: area.width,
            height: spaces_h.min(h.saturating_sub(curr_y - area.y)),
        };
        curr_y += spaces_area.height;

        let div1_area = if curr_y < area.y + h {
            let r = Rect {
                x: area.x,
                y: curr_y,
                width: area.width,
                height: 1,
            };
            curr_y += 1;
            r
        } else {
            Rect::default()
        };

        let tasks_area = Rect {
            x: area.x,
            y: curr_y,
            width: area.width,
            height: tasks_h.min(h.saturating_sub(curr_y - area.y)),
        };
        curr_y += tasks_area.height;

        let div2_area = if curr_y < area.y + h {
            let r = Rect {
                x: area.x,
                y: curr_y,
                width: area.width,
                height: 1,
            };
            curr_y += 1;
            r
        } else {
            Rect::default()
        };

        let locks_area = Rect {
            x: area.x,
            y: curr_y,
            width: area.width,
            height: locks_h.min(h.saturating_sub(curr_y - area.y)),
        };
        curr_y += locks_area.height;

        let div3_area = if curr_y < area.y + h {
            let r = Rect {
                x: area.x,
                y: curr_y,
                width: area.width,
                height: 1,
            };
            curr_y += 1;
            r
        } else {
            Rect::default()
        };

        let peers_area = Rect {
            x: area.x,
            y: curr_y,
            width: area.width,
            height: (area.y + h).saturating_sub(curr_y),
        };

        Some(SidebarSectionLayout {
            spaces_area,
            div1_area,
            tasks_area,
            div2_area,
            locks_area,
            div3_area,
            peers_area,
        })
    }
}

/// Geometry for the 4 borderless sections within the left sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SidebarSectionLayout {
    pub spaces_area: Rect,
    pub div1_area: Rect,
    pub tasks_area: Rect,
    pub div2_area: Rect,
    pub locks_area: Rect,
    pub div3_area: Rect,
    pub peers_area: Rect,
}
