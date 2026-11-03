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
