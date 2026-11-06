use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::Terminal;
use turf_protocol::intent::{TaskCard, TaskStatus};
use turf_protocol::locks::FileLock;
use turf_tui::layout::CockpitLayout;
use turf_tui::pane::SplitDirection;
use turf_tui::theme::{COLOR_ACCENT, COLOR_ACTIVE_PILL, COLOR_ELECTRIC_LIME};
use turf_tui::{FocusArea, TerminalCockpit};

#[test]
fn test_layout_rect_calculation_with_sidebar() {
    let area = Rect::new(0, 0, 100, 40);
    let layout = CockpitLayout::calculate(area, true);

    assert!(layout.sidebar_visible);
    // 22% of 100 is 22
    assert_eq!(layout.sidebar_area.width, 22);
    assert_eq!(layout.sidebar_area.height, 40);

    // Single vertical divider is 1 column wide
    assert_eq!(layout.sidebar_divider.x, 22);
    assert_eq!(layout.sidebar_divider.width, 1);
    assert_eq!(layout.sidebar_divider.height, 40);

    // Main area is remaining width: 100 - (22 + 1) = 77
    assert_eq!(layout.main_area.x, 23);
    assert_eq!(layout.main_area.width, 77);
    assert_eq!(layout.main_area.height, 40);

    // Top row tab strip
    assert_eq!(layout.tab_strip_area.x, 23);
    assert_eq!(layout.tab_strip_area.y, 0);
    assert_eq!(layout.tab_strip_area.height, 1);

    // Bottom input bar and divider
    assert_eq!(layout.input_divider_area.y, 38);
    assert_eq!(layout.input_divider_area.height, 1);
    assert_eq!(layout.input_bar_area.y, 39);
    assert_eq!(layout.input_bar_area.height, 1);

    // Terminal surface area: height = 40 - 3 = 37 (y: 1..38)
    assert_eq!(layout.terminal_area.y, 1);
    assert_eq!(layout.terminal_area.height, 37);
}

#[test]
fn test_layout_rect_calculation_collapsed_sidebar() {
    let area = Rect::new(0, 0, 100, 40);
    let layout = CockpitLayout::calculate(area, false);

    assert!(!layout.sidebar_visible);
    assert_eq!(layout.sidebar_area.width, 0);
    assert_eq!(layout.sidebar_divider.width, 0);
    assert_eq!(layout.main_area.x, 0);
    assert_eq!(layout.main_area.width, 100);
    assert_eq!(layout.tab_strip_area.width, 100);
    assert_eq!(layout.terminal_area.width, 100);
}

#[test]
fn test_sidebar_section_layout() {
    let area = Rect::new(0, 0, 100, 40);
    let layout = CockpitLayout::calculate(area, true);
    let sections = layout.calculate_sidebar_sections().expect("Sidebar visible");

    // All 4 sections exist and are bounded
    assert!(sections.spaces_area.height > 0);
    assert_eq!(sections.div1_area.height, 1);
    assert!(sections.tasks_area.height > 0);
    assert_eq!(sections.div2_area.height, 1);
    assert!(sections.locks_area.height > 0);
    assert_eq!(sections.div3_area.height, 1);
    assert!(sections.peers_area.height > 0);

    // Total height matches sidebar height
    let total_h = sections.spaces_area.height
        + sections.div1_area.height
        + sections.tasks_area.height
        + sections.div2_area.height
        + sections.locks_area.height
        + sections.div3_area.height
        + sections.peers_area.height;
    assert_eq!(total_h, 40);
}

#[test]
fn test_focus_cycling() {
    let mut cockpit = TerminalCockpit::new_headless("TURF-TEST", "HOST", "tester", 24, 80);

    // Initial focus is Terminal
    assert_eq!(cockpit.focus, FocusArea::Terminal);

    // Tab -> Input
    cockpit.cycle_focus();
    assert_eq!(cockpit.focus, FocusArea::Input);

    // Tab -> Sidebar
    cockpit.cycle_focus();
    assert_eq!(cockpit.focus, FocusArea::Sidebar);

    // Tab -> Terminal
    cockpit.cycle_focus();
    assert_eq!(cockpit.focus, FocusArea::Terminal);

    // When sidebar is hidden, focus cycling skips Sidebar
    cockpit.toggle_sidebar();
    assert!(!cockpit.sidebar_visible);

    cockpit.cycle_focus();
    assert_eq!(cockpit.focus, FocusArea::Input);

    cockpit.cycle_focus();
    assert_eq!(cockpit.focus, FocusArea::Terminal);
}

#[test]
fn test_tab_management_and_switching() {
    let mut cockpit = TerminalCockpit::new_headless("TURF-TEST", "HOST", "tester", 24, 80);
    assert_eq!(cockpit.tabs.len(), 1);
    assert_eq!(cockpit.active_tab_index, 0);

    // Add new tab
    let id2 = cockpit.add_tab_headless("Build");
    assert_eq!(id2, 2);
    assert_eq!(cockpit.tabs.len(), 2);
    assert_eq!(cockpit.active_tab_index, 1);

    // Add another tab
    let id3 = cockpit.add_tab_headless("Agent");
    assert_eq!(id3, 3);
    assert_eq!(cockpit.tabs.len(), 3);
    assert_eq!(cockpit.active_tab_index, 2);

    // Switch tab
    assert!(cockpit.switch_tab(0));
    assert_eq!(cockpit.active_tab_index, 0);
    assert_eq!(cockpit.tabs[cockpit.active_tab_index].title, "Shell");

    // Next / Prev tab
    cockpit.next_tab();
    assert_eq!(cockpit.active_tab_index, 1);
    assert_eq!(cockpit.tabs[cockpit.active_tab_index].title, "Build");

    cockpit.prev_tab();
    assert_eq!(cockpit.active_tab_index, 0);

    // Close tab
    assert!(cockpit.close_active_tab());
    assert_eq!(cockpit.tabs.len(), 2);
    assert_eq!(cockpit.active_tab_index, 0);
}

#[test]
fn test_split_pane_management() {
    let mut cockpit = TerminalCockpit::new_headless("TURF-TEST", "HOST", "tester", 24, 80);
    let tab = &cockpit.tabs[cockpit.active_tab_index];
    assert_eq!(tab.pane_manager.pane_count(), 1);

    // Vertical split
    let pane2_id = cockpit.split_active_pane_headless(SplitDirection::Vertical);
    assert_eq!(pane2_id, 2);
    let tab = &cockpit.tabs[cockpit.active_tab_index];
    assert_eq!(tab.pane_manager.pane_count(), 2);
    assert_eq!(tab.pane_manager.active_pane_id(), 2);

    // Horizontal split on pane 2
    let pane3_id = cockpit.split_active_pane_headless(SplitDirection::Horizontal);
    assert_eq!(pane3_id, 3);
    let tab = &cockpit.tabs[cockpit.active_tab_index];
    assert_eq!(tab.pane_manager.pane_count(), 3);
    assert_eq!(tab.pane_manager.active_pane_id(), 3);

    // Focus cycling between split panes
    cockpit.cycle_pane_focus();
    let tab = &cockpit.tabs[cockpit.active_tab_index];
    assert_eq!(tab.pane_manager.active_pane_id(), 1);

    // Close active pane
    assert!(cockpit.close_active_pane());
    let tab = &cockpit.tabs[cockpit.active_tab_index];
    assert_eq!(tab.pane_manager.pane_count(), 2);
}

#[test]
fn test_vte_cell_blitting_and_styling() {
    let mut pane = turf_tui::Pane::new_headless(1, "TestPane", 10, 20);

    // Feed ANSI TrueColor and bold escape sequences into VTE
    pane.terminal
        .process_str("\x1b[1;38;2;57;255;20mHello TurfCode!\x1b[0m");

    let mut buf = Buffer::empty(Rect::new(0, 0, 20, 10));
    pane.render(Rect::new(0, 0, 20, 10), true, &mut buf);

    // Cell (0, 0) is 'H' with Electric Lime color and BOLD flag
    let cell_h = buf.cell((0, 0)).expect("cell exists");
    assert_eq!(cell_h.symbol(), "H");
    assert_eq!(cell_h.fg, COLOR_ELECTRIC_LIME);
    assert!(cell_h.modifier.contains(Modifier::BOLD));

    // Cell (6, 0) is 'T'
    let cell_t = buf.cell((6, 0)).expect("cell exists");
    assert_eq!(cell_t.symbol(), "T");
    assert_eq!(cell_t.fg, COLOR_ELECTRIC_LIME);
}

#[test]
fn test_headless_rendering_cockpit_full_surface() {
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut cockpit = TerminalCockpit::new_headless("TURF-7A9B", "HOST", "yugsr", 26, 92);

    // Populate sidebar with intent task, lock, and peer
    cockpit.sidebar_state.tasks.push(TaskCard::new(
        "task-1",
        "Build Ratatui Cockpit",
        "yugsr",
        TaskStatus::InProgress,
        vec!["crates/turf-tui".to_string()],
    ));
    cockpit.sidebar_state.locks.push(FileLock::new(
        "src/cockpit.rs",
        "yugsr",
        "task-1",
        30_000,
        None,
    ));

    // Render frame to headless TestBackend
    terminal
        .draw(|f| {
            cockpit.render(f.area(), f.buffer_mut());
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let layout = CockpitLayout::calculate(Rect::new(0, 0, 120, 30), true);

    // Verify Tab Strip on Row 0:
    // Should have active tab pill: ` 1: Shell `
    let mut row0_text = String::new();
    for x in 0..120 {
        row0_text.push_str(buffer.cell((x, 0)).unwrap().symbol());
    }
    assert!(
        row0_text.contains("1: Shell"),
        "Row 0 must contain '1: Shell': {}",
        row0_text
    );
    assert!(
        row0_text.contains("TURF-7A9B"),
        "Row 0 must contain session code 'TURF-7A9B': {}",
        row0_text
    );
    assert!(
        row0_text.contains("HOST"),
        "Row 0 must contain role 'HOST': {}",
        row0_text
    );

    // Verify Active Pill styling: Catppuccin active pill background and accent fg
    let pill_cell = buffer.cell((layout.tab_strip_area.x + 1, 0)).unwrap();
    assert_eq!(pill_cell.bg, COLOR_ACTIVE_PILL);
    assert_eq!(pill_cell.fg, COLOR_ACCENT);

    // Verify Sidebar single vertical divider (│) on column 26
    let div_x = layout.sidebar_divider.x;
    for y in 0..30 {
        let cell = buffer.cell((div_x, y)).unwrap();
        assert_eq!(cell.symbol(), "│", "Divider cell at ({}, {}) should be │", div_x, y);
    }

    // Verify Sidebar content
    let mut sidebar_text = String::new();
    for y in 0..30 {
        for x in 0..div_x {
            sidebar_text.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        sidebar_text.push('\n');
    }
    assert!(sidebar_text.contains("spaces"));
    assert!(sidebar_text.contains("intents & tasks"));
    assert!(sidebar_text.contains("active locks"));
    assert!(sidebar_text.contains("peers"));
    assert!(sidebar_text.contains("Build Ra"));
    assert!(sidebar_text.contains("src/cockpit.rs"));

    // Verify Input divider and prompt
    let input_div_y = layout.input_divider_area.y;
    let div_cell = buffer.cell((layout.main_area.x, input_div_y)).unwrap();
    assert_eq!(div_cell.symbol(), "─");

    let prompt_y = layout.input_bar_area.y;
    let mut prompt_row = String::new();
    for x in layout.main_area.x..120 {
        prompt_row.push_str(buffer.cell((x, prompt_y)).unwrap().symbol());
    }
    assert!(prompt_row.contains("> "));
    assert!(prompt_row.contains("« ? for shortcuts"));
}

#[test]
fn test_keyboard_events_and_commands() {
    let mut cockpit = TerminalCockpit::new_headless("TURF-TEST", "HOST", "tester", 24, 80);

    // Alt+B toggles sidebar
    assert!(cockpit.sidebar_visible);
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT));
    assert!(!cockpit.sidebar_visible);
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT));
    assert!(cockpit.sidebar_visible);

    // In Terminal focus, Tab writes directly to PTY instead of hijacking focus
    assert_eq!(cockpit.focus, FocusArea::Terminal);
    let handled = cockpit.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    assert!(handled);
    assert_eq!(cockpit.focus, FocusArea::Terminal);

    // Alt+Enter toggles prompt focus
    cockpit.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
    assert_eq!(cockpit.focus, FocusArea::Input);

    // Typing in Input mode
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
    assert_eq!(cockpit.input_state.value, "/sidebar");

    // Enter executes command
    cockpit.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(cockpit.input_state.value, "");
    // /sidebar toggles sidebar
    assert!(!cockpit.sidebar_visible);

    // Alt+Q or Ctrl+Q exits
    assert!(!cockpit.should_exit);
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::ALT));
    assert!(cockpit.should_exit);
}

#[test]
fn test_cockpit_viewport_scrolling_hotkeys_and_badge() {
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();

    let mut cockpit = TerminalCockpit::new_headless("TURF-TEST", "HOST", "tester", 20, 80);

    // Populate active pane with 50 lines so scrollback accumulates
    {
        let tab = &mut cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane_mut().unwrap();
        for i in 0..50 {
            pane.terminal.process_str(&format!("Line {i}\r\n"));
        }
        assert!(pane.terminal.scrollback_len() >= 30);
        assert_eq!(pane.terminal.scroll_offset(), 0);
    }

    // 1. Alt+Up scrolls up by 10 lines
    cockpit.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
    {
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().unwrap();
        assert_eq!(pane.terminal.scroll_offset(), 10);
    }

    // 2. Alt+[ scrolls up by 10 lines
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('['), KeyModifiers::ALT));
    {
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().unwrap();
        assert_eq!(pane.terminal.scroll_offset(), 20);
    }

    // 3. Shift+PageUp scrolls up by 10 lines
    cockpit.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::SHIFT));
    {
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().unwrap();
        assert_eq!(pane.terminal.scroll_offset(), 30);
    }

    // 4. Render cockpit and verify dimmed badge in tab strip and pane area
    terminal
        .draw(|f| {
            cockpit.render(f.area(), f.buffer_mut());
        })
        .unwrap();

    let buffer = terminal.backend().buffer();

    // Verify row 0 (tab strip) contains the badge [+30 lines]
    let mut row0_text = String::new();
    for x in 0..100 {
        row0_text.push_str(buffer.cell((x, 0)).unwrap().symbol());
    }
    assert!(
        row0_text.contains("[+30 lines]"),
        "Tab strip must contain [+30 lines], found: {}",
        row0_text
    );

    // Verify pane area also contains [+30 lines]
    let mut pane_text = String::new();
    for y in 1..28 {
        for x in 23..100 {
            pane_text.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        pane_text.push('\n');
    }
    assert!(
        pane_text.contains("[+30 lines]"),
        "Pane area must contain [+30 lines], found: {}",
        pane_text
    );

    // 5. Alt+Down scrolls down by 10 lines
    cockpit.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
    {
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().unwrap();
        assert_eq!(pane.terminal.scroll_offset(), 20);
    }

    // 6. Shift+PageDown scrolls down by 10 lines
    cockpit.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::SHIFT));
    {
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().unwrap();
        assert_eq!(pane.terminal.scroll_offset(), 10);
    }

    // 7. Esc snaps scroll_offset back to 0
    cockpit.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    {
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().unwrap();
        assert_eq!(pane.terminal.scroll_offset(), 0);
    }

    // 8. Scroll up again and type raw user input -> snaps scroll_offset back to 0
    cockpit.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT));
    {
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().unwrap();
        assert_eq!(pane.terminal.scroll_offset(), 10);
    }

    // Type a character 'a' in Terminal focus
    cockpit.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    {
        let tab = &cockpit.tabs[cockpit.active_tab_index];
        let pane = tab.pane_manager.active_pane().unwrap();
        assert_eq!(pane.terminal.scroll_offset(), 0);
    }
}

#[test]
fn test_dynamic_tab_capsule_pill_badges() {
    let mut buf = Buffer::empty(Rect::new(0, 0, 100, 1));

    // Test 1: Active tab with vim command
    let tabs = vec![
        turf_tui::TabItem::new(1, "Shell")
            .with_foreground_cmd(Some("vim src/main.rs".to_string())),
        turf_tui::TabItem::new(2, "Build"),
    ];

    turf_tui::render_tab_strip(
        Rect::new(0, 0, 100, 1),
        &tabs,
        0,
        "TURF-7A9B",
        "HOST",
        "pilot",
        &mut buf,
    );

    let mut row_str = String::new();
    for x in 0..100 {
        row_str.push_str(buf.cell((x, 0)).unwrap().symbol());
    }
    assert!(
        row_str.contains(" 1: vim src/main.rs "),
        "Tab strip should render dynamic active process pill ' 1: vim src/main.rs ', got: {}",
        row_str
    );
    assert!(row_str.contains(" 2 "));

    // Test 2: Idle shell tab pill
    let mut buf2 = Buffer::empty(Rect::new(0, 0, 100, 1));
    let idle_tabs = vec![
        turf_tui::TabItem::new(1, "Shell").with_foreground_cmd(Some("bash".to_string())),
    ];
    turf_tui::render_tab_strip(
        Rect::new(0, 0, 100, 1),
        &idle_tabs,
        0,
        "TURF-7A9B",
        "HOST",
        "pilot",
        &mut buf2,
    );

    let mut row2_str = String::new();
    for x in 0..100 {
        row2_str.push_str(buf2.cell((x, 0)).unwrap().symbol());
    }
    assert!(
        row2_str.contains(" 1: bash "),
        "Tab strip should render dynamic idle shell pill ' 1: bash ', got: {}",
        row2_str
    );
}

#[test]
fn test_dynamic_pane_badge_and_header_rendering() {
    let mut cockpit = TerminalCockpit::new_headless("TURF-TEST", "HOST", "tester", 30, 100);

    // Set active pane foreground command
    if let Some(tab) = cockpit.tabs.get_mut(cockpit.active_tab_index) {
        if let Some(pane) = tab.pane_manager.active_pane_mut() {
            pane.foreground_cmd = Some("cargo test".to_string());
            assert_eq!(pane.display_badge(), "1: cargo test");
        }

        // Split horizontally
        let new_id = tab.pane_manager.split_active_headless(turf_tui::SplitDirection::Horizontal);
        if let Some(new_pane) = tab.pane_manager.get_pane_mut(new_id) {
            new_pane.foreground_cmd = Some("git diff".to_string());
            assert_eq!(new_pane.display_badge(), format!("{new_id}: git diff"));
        }
    }

    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| cockpit.render(f.area(), f.buffer_mut())).unwrap();

    let buffer = terminal.backend().buffer();
    let mut full_output = String::new();
    for y in 0..30 {
        for x in 0..100 {
            full_output.push_str(buffer.cell((x, y)).unwrap().symbol());
        }
        full_output.push('\n');
    }

    // Verify pane badge is rendered in horizontal divider
    assert!(
        full_output.contains(" 2: git diff "),
        "Split horizontal divider should render pane badge header"
    );
}
