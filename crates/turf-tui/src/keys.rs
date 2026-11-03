//! Keyboard event translation for forwarding raw Crossterm key events
//! directly to PTY master input.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Convert a crossterm `KeyEvent` into raw byte sequences recognized by POSIX/ConPTY shells.
pub fn key_to_bytes(key: &KeyEvent) -> Option<Vec<u8>> {
    match key.code {
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match c.to_ascii_lowercase() {
                    'a'..='z' => Some(vec![c.to_ascii_lowercase() as u8 - b'a' + 1]),
                    '@' | ' ' => Some(vec![0]),
                    '[' => Some(vec![27]),
                    '\\' => Some(vec![28]),
                    ']' => Some(vec![29]),
                    '^' => Some(vec![30]),
                    '_' => Some(vec![31]),
                    '?' => Some(vec![127]),
                    _ => None,
                }
            } else if key.modifiers.contains(KeyModifiers::ALT) {
                let mut bytes = vec![0x1b];
                let mut char_buf = [0u8; 4];
                let s = c.encode_utf8(&mut char_buf);
                bytes.extend_from_slice(s.as_bytes());
                Some(bytes)
            } else {
                let mut char_buf = [0u8; 4];
                let s = c.encode_utf8(&mut char_buf);
                Some(s.as_bytes().to_vec())
            }
        }
        KeyCode::Enter => Some(b"\r".to_vec()),
        KeyCode::Backspace => Some(b"\x7f".to_vec()),
        KeyCode::Tab => Some(b"\t".to_vec()),
        KeyCode::BackTab => Some(b"\x1b[Z".to_vec()),
        KeyCode::Esc => Some(b"\x1b".to_vec()),
        KeyCode::Up => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                Some(b"\x1b[1;5A".to_vec())
            } else {
                Some(b"\x1b[A".to_vec())
            }
        }
        KeyCode::Down => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                Some(b"\x1b[1;5B".to_vec())
            } else {
                Some(b"\x1b[B".to_vec())
            }
        }
        KeyCode::Right => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                Some(b"\x1b[1;5C".to_vec())
            } else {
                Some(b"\x1b[C".to_vec())
            }
        }
        KeyCode::Left => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                Some(b"\x1b[1;5D".to_vec())
            } else {
                Some(b"\x1b[D".to_vec())
            }
        }
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::Insert => Some(b"\x1b[2~".to_vec()),
        KeyCode::F(1) => Some(b"\x1bOP".to_vec()),
        KeyCode::F(2) => Some(b"\x1bOQ".to_vec()),
        KeyCode::F(3) => Some(b"\x1bOR".to_vec()),
        KeyCode::F(4) => Some(b"\x1bOS".to_vec()),
        KeyCode::F(5) => Some(b"\x1b[15~".to_vec()),
        KeyCode::F(6) => Some(b"\x1b[16~".to_vec()),
        KeyCode::F(7) => Some(b"\x1b[17~".to_vec()),
        KeyCode::F(8) => Some(b"\x1b[18~".to_vec()),
        KeyCode::F(9) => Some(b"\x1b[19~".to_vec()),
        KeyCode::F(10) => Some(b"\x1b[20~".to_vec()),
        KeyCode::F(11) => Some(b"\x1b[21~".to_vec()),
        KeyCode::F(12) => Some(b"\x1b[22~".to_vec()),
        _ => None,
    }
}
