//! TurfCode TUI - Herdr-Grade Spatial Multiplexer Cockpit.
//!
//! Conforms to [MD-001], [MD-004], [MD-042], and [MD-044]:
//! - Borderless terminal panes blitting directly from `turf-vte::ScreenBuffer`
//! - Inverted high-contrast Electric Lime solid pill tab strip
//! - Borderless left sidebar with single vertical divider (`│`) and 4 sections
//! - Minimal bottom prompt line with subtle `─` divider and right-aligned shortcuts
//! - 60fps flicker-free frame blitting event loop

pub mod cockpit;
pub mod input;
pub mod keys;
pub mod layout;
pub mod pane;
pub mod sidebar;
pub mod tabs;
pub mod theme;
pub mod vte_blit;

pub use cockpit::{FocusArea, TerminalCockpit};
pub use input::InputState;
pub use layout::CockpitLayout;
pub use pane::{Pane, PaneBackend, PaneManager, PaneNode, SplitDirection};
pub use sidebar::{PeerInfo, SidebarState};
pub use tabs::TabItem;
pub use theme::Theme;
pub use vte_blit::blit_screen_buffer;
