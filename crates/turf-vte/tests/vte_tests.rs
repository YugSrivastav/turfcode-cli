use turf_vte::{Color, NamedColor, Terminal};

#[test]
fn test_basic_print_and_grid_coordinates() {
    let mut term = Terminal::new(10, 20);
    term.process_str("Hello TurfCode!");

    assert_eq!(term.row_text_trimmed(0), "Hello TurfCode!");
    assert_eq!(term.cursor().x, 15);
    assert_eq!(term.cursor().y, 0);

    let cell = term.cell(0, 0).unwrap();
    assert_eq!(cell.c, 'H');
    assert_eq!(cell.fg, Color::Default);
    assert_eq!(cell.bg, Color::Default);
}

#[test]
fn test_crlf_and_wrapping() {
    let mut term = Terminal::new(5, 10);
    // Line 1
    term.process_str("ABCDE\r\n");
    // Line 2
    term.process_str("12345");

    assert_eq!(term.row_text_trimmed(0), "ABCDE");
    assert_eq!(term.row_text_trimmed(1), "12345");
    assert_eq!(term.cursor().x, 5);
    assert_eq!(term.cursor().y, 1);
}

#[test]
fn test_ansi_standard_colors_and_styles() {
    let mut term = Terminal::new(5, 40);

    // Red bold text
    term.process_str("\x1b[1;31mRED_BOLD\x1b[0m ");
    // Green underline text
    term.process_str("\x1b[4;32mGREEN_UNDER\x1b[m ");
    // Blue background, yellow foreground
    term.process_str("\x1b[33;44mYEL_ON_BLUE\x1b[0m");

    // Check first word 'RED_BOLD'
    let red_cell = term.cell(0, 0).unwrap();
    assert_eq!(red_cell.c, 'R');
    assert_eq!(red_cell.fg, Color::Named(NamedColor::Red));
    assert!(red_cell.flags.bold);
    assert!(!red_cell.flags.underline);

    // Check 'GREEN_UNDER'
    let green_cell = term.cell(9, 0).unwrap();
    assert_eq!(green_cell.c, 'G');
    assert_eq!(green_cell.fg, Color::Named(NamedColor::Green));
    assert!(!green_cell.flags.bold);
    assert!(green_cell.flags.underline);

    // Check 'YEL_ON_BLUE'
    let yel_cell = term.cell(21, 0).unwrap();
    assert_eq!(yel_cell.c, 'Y');
    assert_eq!(yel_cell.fg, Color::Named(NamedColor::Yellow));
    assert_eq!(yel_cell.bg, Color::Named(NamedColor::Blue));
}

#[test]
fn test_truecolor_rgb_and_256_colors() {
    let mut term = Terminal::new(5, 40);

    // TrueColor RGB: fg = (57, 255, 20) [Electric Lime!], bg = (8, 16, 8) [Dark Obsidian]
    term.process_str("\x1b[38;2;57;255;20;48;2;8;16;8mLIME_CORE\x1b[0m ");
    // 256 Color: fg = 196 (bright red), bg = 232 (near black)
    term.process_str("\x1b[38;5;196;48;5;232mCOLOR_256\x1b[0m");

    let lime_cell = term.cell(0, 0).unwrap();
    assert_eq!(lime_cell.c, 'L');
    assert_eq!(lime_cell.fg, Color::Rgb(57, 255, 20));
    assert_eq!(lime_cell.bg, Color::Rgb(8, 16, 8));

    let idx_cell = term.cell(10, 0).unwrap();
    assert_eq!(idx_cell.c, 'C');
    assert_eq!(idx_cell.fg, Color::Indexed(196));
    assert_eq!(idx_cell.bg, Color::Indexed(232));
}

#[test]
fn test_cursor_movement_and_positioning() {
    let mut term = Terminal::new(10, 20);

    // Jump to row 4, col 6 (1-indexed: 4;6)
    term.process_str("\x1b[4;6H");
    assert_eq!(term.cursor().y, 3);
    assert_eq!(term.cursor().x, 5);

    // Move Up 2, Right 3
    term.process_str("\x1b[2A\x1b[3C");
    assert_eq!(term.cursor().y, 1);
    assert_eq!(term.cursor().x, 8);

    // Move Down 4, Left 2
    term.process_str("\x1b[4B\x1b[2D");
    assert_eq!(term.cursor().y, 5);
    assert_eq!(term.cursor().x, 6);

    // Absolute Column 2 (1-indexed: 2G => 1)
    term.process_str("\x1b[2G");
    assert_eq!(term.cursor().x, 1);
    assert_eq!(term.cursor().y, 5);
}

#[test]
fn test_cursor_save_and_restore() {
    let mut term = Terminal::new(10, 20);

    // Position cursor at (5, 7) with bold red text
    term.process_str("\x1b[8;6H\x1b[1;31m");
    assert_eq!(term.cursor().y, 7);
    assert_eq!(term.cursor().x, 5);

    // Save cursor (DECSC)
    term.process_str("\x1b7");

    // Move cursor elsewhere and change colors
    term.process_str("\x1b[1;1H\x1b[0;32m");
    assert_eq!(term.cursor().y, 0);
    assert_eq!(term.cursor().x, 0);
    assert_eq!(term.current_fg(), Color::Named(NamedColor::Green));

    // Restore cursor (DECRC)
    term.process_str("\x1b8");
    assert_eq!(term.cursor().y, 7);
    assert_eq!(term.cursor().x, 5);
    assert_eq!(term.current_fg(), Color::Named(NamedColor::Red));
    assert!(term.current_flags().bold);
}

#[test]
fn test_clear_screen_and_lines() {
    let mut term = Terminal::new(5, 10);
    term.process_str("ROW0000000\r\nROW1111111\r\nROW2222222\r\nROW3333333\r\nROW4444444");

    assert_eq!(term.row_text_trimmed(0), "ROW0000000");
    assert_eq!(term.row_text_trimmed(2), "ROW2222222");

    // Clear Line 2 completely (move to 3;1, then \x1b[2K)
    term.process_str("\x1b[3;1H\x1b[2K");
    assert_eq!(term.row_text_trimmed(2), "");

    // Clear whole display (\x1b[2J)
    term.process_str("\x1b[2J");
    for y in 0..5 {
        assert_eq!(term.row_text_trimmed(y), "");
    }
}

#[test]
fn test_scrolling_and_scrolling_margins() {
    let mut term = Terminal::new(4, 10);
    term.process_str("Line 1\r\nLine 2\r\nLine 3\r\nLine 4");

    assert_eq!(term.row_text_trimmed(0), "Line 1");
    assert_eq!(term.row_text_trimmed(3), "Line 4");

    // A newline at bottom row triggers scroll up
    term.process_str("\r\nLine 5");
    assert_eq!(term.row_text_trimmed(0), "Line 2");
    assert_eq!(term.row_text_trimmed(1), "Line 3");
    assert_eq!(term.row_text_trimmed(2), "Line 4");
    assert_eq!(term.row_text_trimmed(3), "Line 5");

    // Explicit scroll down (\x1b[1T)
    term.process_str("\x1b[1T");
    assert_eq!(term.row_text_trimmed(0), "");
    assert_eq!(term.row_text_trimmed(1), "Line 2");
}

#[test]
fn test_alternate_screen_buffer_toggling() {
    let mut term = Terminal::new(5, 20);

    // Write on primary screen
    term.process_str("PRIMARY SCREEN CONTENT");
    assert_eq!(term.row_text_trimmed(0), "PRIMARY SCREEN CONTE");
    assert!(!term.is_alt_buffer());

    // Enter alternate screen buffer (?1049h)
    term.process_str("\x1b[?1049h");
    assert!(term.is_alt_buffer());
    // Alt buffer should be freshly cleared
    assert_eq!(term.row_text_trimmed(0), "");
    assert_eq!(term.cursor().x, 0);
    assert_eq!(term.cursor().y, 0);

    // Write on alternate buffer
    term.process_str("ALT SCREEN VIM");
    assert_eq!(term.row_text_trimmed(0), "ALT SCREEN VIM");

    // Leave alternate screen buffer (?1049l)
    term.process_str("\x1b[?1049l");
    assert!(!term.is_alt_buffer());
    // Primary buffer content must be fully preserved!
    assert_eq!(term.row_text_trimmed(0), "PRIMARY SCREEN CONTE");
}

#[test]
fn test_cursor_visibility_and_title() {
    let mut term = Terminal::new(5, 20);
    assert!(term.cursor().visible);

    // Hide cursor (\x1b[?25l)
    term.process_str("\x1b[?25l");
    assert!(!term.cursor().visible);

    // Show cursor (\x1b[?25h)
    term.process_str("\x1b[?25h");
    assert!(term.cursor().visible);

    // Set window title via OSC 0
    term.process_str("\x1b]0;TurfCode Cockpit\x07");
    assert_eq!(term.title(), "TurfCode Cockpit");

    // Set window title via OSC 2
    term.process_str("\x1b]2;Herdr Multiplexer\x1b\\");
    assert_eq!(term.title(), "Herdr Multiplexer");
}

#[test]
fn test_scrollback_push_pop_and_capacity_limit() {
    use turf_vte::ScreenBuffer;

    let mut buf = ScreenBuffer::with_scrollback(3, 10, 5);
    assert_eq!(buf.max_scrollback(), 5);
    assert_eq!(buf.scrollback_len(), 0);

    // Populate rows 0, 1, 2
    for y in 0..3 {
        for (x, ch) in format!("ROW{y}").chars().enumerate() {
            buf.set_cell(x, y, turf_vte::Cell::new(ch, Color::Default, Color::Default, turf_vte::CellFlags::default()));
        }
    }

    assert_eq!(buf.row_text_trimmed(0), "ROW0");
    assert_eq!(buf.row_text_trimmed(1), "ROW1");
    assert_eq!(buf.row_text_trimmed(2), "ROW2");

    // Scroll up by 1 line -> ROW0 is pushed into scrollback
    buf.scroll_up(1);
    assert_eq!(buf.scrollback_len(), 1);
    let hist0: String = buf.scrollback()[0].iter().map(|c| c.c).collect();
    assert_eq!(hist0.trim_end(), "ROW0");
    assert_eq!(buf.row_text_trimmed(0), "ROW1");

    // Scroll up 6 more times -> pushes rows and hits capacity limit of 5
    for i in 3..9 {
        buf.set_cell(0, 2, turf_vte::Cell::new(char::from_digit(i, 10).unwrap(), Color::Default, Color::Default, turf_vte::CellFlags::default()));
        buf.scroll_up(1);
    }

    // Scrollback must be bounded at capacity 5
    assert_eq!(buf.scrollback_len(), 5);
}

#[test]
fn test_scrollback_viewport_slicing() {
    use turf_vte::ScreenBuffer;

    let mut buf = ScreenBuffer::with_scrollback(3, 10, 100);

    // Write Line 0, Line 1, Line 2
    for y in 0..3 {
        for (x, ch) in format!("LINE{y}").chars().enumerate() {
            buf.set_cell(x, y, turf_vte::Cell::new(ch, Color::Default, Color::Default, turf_vte::CellFlags::default()));
        }
    }

    // Scroll up 2 times: evicted rows will be LINE0 and LINE1
    buf.scroll_up(1);
    for (x, ch) in "LINE3".chars().enumerate() {
        buf.set_cell(x, 2, turf_vte::Cell::new(ch, Color::Default, Color::Default, turf_vte::CellFlags::default()));
    }
    buf.scroll_up(1);
    for (x, ch) in "LINE4".chars().enumerate() {
        buf.set_cell(x, 2, turf_vte::Cell::new(ch, Color::Default, Color::Default, turf_vte::CellFlags::default()));
    }

    // Scrollback has 2 rows: LINE0 (index 0) and LINE1 (index 1)
    // Live cells has: LINE2 (row 0), LINE3 (row 1), LINE4 (row 2)
    assert_eq!(buf.scrollback_len(), 2);
    assert_eq!(buf.row_text_trimmed(0), "LINE2");
    assert_eq!(buf.row_text_trimmed(1), "LINE3");
    assert_eq!(buf.row_text_trimmed(2), "LINE4");

    // Offset 0: Live view
    let v0 = buf.render_viewport_rows(0, 3);
    assert_eq!(v0.len(), 3);
    let row0_text: String = v0[0].iter().map(|c| c.c).collect();
    let row1_text: String = v0[1].iter().map(|c| c.c).collect();
    let row2_text: String = v0[2].iter().map(|c| c.c).collect();
    assert_eq!(row0_text.trim_end(), "LINE2");
    assert_eq!(row1_text.trim_end(), "LINE3");
    assert_eq!(row2_text.trim_end(), "LINE4");

    // Offset 1: Scrolled back 1 row into history (should see LINE1, LINE2, LINE3)
    let v1 = buf.render_viewport_rows(1, 3);
    let r0: String = v1[0].iter().map(|c| c.c).collect();
    let r1: String = v1[1].iter().map(|c| c.c).collect();
    let r2: String = v1[2].iter().map(|c| c.c).collect();
    assert_eq!(r0.trim_end(), "LINE1");
    assert_eq!(r1.trim_end(), "LINE2");
    assert_eq!(r2.trim_end(), "LINE3");

    // Offset 2: Scrolled back 2 rows into history (should see LINE0, LINE1, LINE2)
    let v2 = buf.render_viewport_rows(2, 3);
    let r0: String = v2[0].iter().map(|c| c.c).collect();
    let r1: String = v2[1].iter().map(|c| c.c).collect();
    let r2: String = v2[2].iter().map(|c| c.c).collect();
    assert_eq!(r0.trim_end(), "LINE0");
    assert_eq!(r1.trim_end(), "LINE1");
    assert_eq!(r2.trim_end(), "LINE2");

    // Offset 999 (exceeds scrollback_len): Clamps to 2, identical to offset 2
    let v_clamped = buf.render_viewport_rows(999, 3);
    let c0: String = v_clamped[0].iter().map(|c| c.c).collect();
    assert_eq!(c0.trim_end(), "LINE0");
}

#[test]
fn test_viewport_scroll_navigation_methods() {
    let mut term = Terminal::new(3, 10);
    // Write 10 lines to accumulate 7 historical lines in scrollback
    for i in 0..10 {
        term.process_str(&format!("Line {i}\r\n"));
    }

    assert_eq!(term.scrollback_len(), 8);
    assert_eq!(term.scroll_offset(), 0);

    // Scroll up by 3 lines
    term.scroll_up_viewport(3);
    assert_eq!(term.scroll_offset(), 3);

    // Scroll up by 4 more lines
    term.scroll_up_viewport(4);
    assert_eq!(term.scroll_offset(), 7);

    // Scroll up past max capacity -> clamped to scrollback_len (8)
    term.scroll_up_viewport(50);
    assert_eq!(term.scroll_offset(), 8);

    // Scroll down by 2 lines
    term.scroll_down_viewport(2);
    assert_eq!(term.scroll_offset(), 6);

    // Reset viewport scroll back to live bottom view
    term.reset_viewport_scroll();
    assert_eq!(term.scroll_offset(), 0);
}

#[test]
fn test_alternate_screen_buffer_bypasses_scrollback_history() {
    let mut term = Terminal::new(4, 20);

    // Stream lines into primary buffer to build scrollback
    term.process_str("P1\r\nP2\r\nP3\r\nP4\r\nP5\r\n");
    let primary_scrollback_before = term.scrollback_len();
    assert!(primary_scrollback_before > 0);

    // Enter alternate screen buffer (Vim/Nano mode: ?1049h)
    term.process_str("\x1b[?1049h");
    assert!(term.is_alt_buffer());
    assert_eq!(term.scrollback_len(), 0);
    assert_eq!(term.scroll_offset(), 0);

    // Cause heavy scrolling inside the alternate screen buffer
    for i in 0..20 {
        term.process_str(&format!("VIM_ROW_{i}\r\n"));
    }

    // Alternate screen buffer must NOT accumulate historical scrollback lines
    assert_eq!(term.alt_buffer().scrollback_len(), 0);
    assert_eq!(term.scrollback_len(), 0);

    // Leave alternate screen buffer (?1049l)
    term.process_str("\x1b[?1049l");
    assert!(!term.is_alt_buffer());

    // Primary buffer scrollback must be completely intact and preserved!
    assert_eq!(term.scrollback_len(), primary_scrollback_before);
}

