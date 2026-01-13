use vt100::Parser;

pub struct OutputBuffer {
    parser: Parser,
    scroll_offset: usize,
    scrollback_len: usize,
}

impl OutputBuffer {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        Self {
            parser: Parser::new(rows, cols, scrollback),
            scroll_offset: 0,
            scrollback_len: scrollback,
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.set_size(rows, cols);
        // Don't reset scroll_offset or scrollback - preserve history on resize
        // This prevents content from disappearing when zooming/resizing panes
    }

    pub fn push_bytes(&mut self, data: &[u8]) {
        // Filter out escape sequences that clear scrollback (ESC[3J)
        // Claude Code sends these which would wipe our history
        let filtered = filter_scrollback_clear(data);
        self.parser.process(&filtered);
        if self.scroll_offset > 0 {
            self.parser.set_scrollback(self.scroll_offset);
            self.scroll_offset = self.parser.screen().scrollback();
        }
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn scroll_up(&mut self, amount: usize) {
        let desired = (self.scroll_offset + amount).min(self.scrollback_len);
        self.parser.set_scrollback(desired);
        self.scroll_offset = self.parser.screen().scrollback();
    }

    pub fn scroll_down(&mut self, amount: usize) {
        let desired = self.scroll_offset.saturating_sub(amount);
        self.parser.set_scrollback(desired);
        self.scroll_offset = self.parser.screen().scrollback();
    }

    pub fn reset_scroll(&mut self) {
        self.parser.set_scrollback(0);
        self.scroll_offset = self.parser.screen().scrollback();
    }

    pub fn scroll_to_top(&mut self) {
        self.parser.set_scrollback(self.scrollback_len);
        self.scroll_offset = self.parser.screen().scrollback();
    }

    pub fn scroll_to_bottom(&mut self) {
        self.parser.set_scrollback(0);
        self.scroll_offset = self.parser.screen().scrollback();
    }

    /// Check if the terminal is using the alternate screen buffer
    /// Alternate screen has no scrollback (used by TUI apps like vim, codex)
    pub fn is_alternate_screen(&self) -> bool {
        self.parser.screen().alternate_screen()
    }
}

/// Extract plain text from raw terminal output by stripping ANSI escape sequences
pub fn extract_plain_text(data: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;

    while i < data.len() {
        let byte = data[i];

        // Check for ESC sequence
        if byte == 0x1b && i + 1 < data.len() {
            let next = data[i + 1];
            if next == b'[' {
                // CSI sequence - skip until we hit a letter
                i += 2;
                while i < data.len() && !data[i].is_ascii_alphabetic() {
                    i += 1;
                }
                if i < data.len() {
                    i += 1; // Skip the final letter
                }
                continue;
            } else if next == b']' {
                // OSC sequence - skip until BEL or ST
                i += 2;
                while i < data.len() {
                    if data[i] == 0x07 { // BEL
                        i += 1;
                        break;
                    }
                    if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'\\' { // ST
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            } else if next == b'(' || next == b')' {
                // Character set selection - skip 3 bytes
                i += 3;
                continue;
            } else {
                // Other ESC sequence - skip 2 bytes
                i += 2;
                continue;
            }
        }

        // Handle control characters
        if byte == b'\r' {
            // Carriage return - just skip, we'll handle newlines
            i += 1;
            continue;
        }
        if byte == b'\n' {
            result.push('\n');
            i += 1;
            continue;
        }
        if byte == b'\t' {
            result.push_str("    "); // Convert tab to spaces
            i += 1;
            continue;
        }
        if byte < 0x20 && byte != b'\n' {
            // Skip other control characters
            i += 1;
            continue;
        }

        // Regular character
        if byte.is_ascii() {
            result.push(byte as char);
        }
        i += 1;
    }

    result
}

/// Filter out alternate screen sequences and screen clears for scrollback viewing
/// This allows us to view history even when the app used alternate screen
pub fn filter_alternate_screen(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        // Check for ESC[ sequence
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'[' {
            // Check for ?1049h, ?1049l, ?1047h, ?1047l, ?47h, ?47l (alternate screen)
            if i + 2 < data.len() && data[i + 2] == b'?' {
                // Find the end of the sequence
                let mut j = i + 3;
                while j < data.len() && (data[j].is_ascii_digit() || data[j] == b';') {
                    j += 1;
                }
                if j < data.len() && (data[j] == b'h' || data[j] == b'l') {
                    let params = &data[i + 3..j];
                    // Check for 1049, 1047, or 47
                    if params == b"1049" || params == b"1047" || params == b"47" {
                        // Skip this sequence (alternate screen enter/exit)
                        i = j + 1;
                        continue;
                    }
                }
            }

            // Check for screen clear sequences: ESC[2J, ESC[3J (clear screen/scrollback)
            if i + 2 < data.len() {
                let mut j = i + 2;
                while j < data.len() && (data[j].is_ascii_digit() || data[j] == b';') {
                    j += 1;
                }
                if j < data.len() && data[j] == b'J' {
                    let params = &data[i + 2..j];
                    // ESC[2J = clear screen, ESC[3J = clear scrollback
                    if params == b"2" || params == b"3" {
                        i = j + 1;
                        continue;
                    }
                }
                // Check for ESC[H (cursor home) - often paired with clear
                if j < data.len() && data[j] == b'H' && (j == i + 2 || &data[i + 2..j] == b"1;1") {
                    // Skip cursor home, it's fine
                }
            }
        }

        result.push(data[i]);
        i += 1;
    }

    result
}

/// Filter out ANSI escape sequences that clear scrollback buffer
/// ESC[3J clears scrollback, ESC[2J clears screen - we preserve screen clear but not scrollback
fn filter_scrollback_clear(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        // Check for ESC sequence
        if data[i] == 0x1B && i + 1 < data.len() && data[i + 1] == b'[' {
            // Find the end of the CSI sequence (letter)
            let mut j = i + 2;
            while j < data.len() && (data[j].is_ascii_digit() || data[j] == b';') {
                j += 1;
            }

            // Check if this is ESC[3J (clear scrollback)
            if j < data.len() && data[j] == b'J' {
                // Check the parameter - ESC[3J clears scrollback
                let params = &data[i + 2..j];
                if params == b"3" {
                    // Skip this sequence entirely
                    i = j + 1;
                    continue;
                }
            }
        }

        result.push(data[i]);
        i += 1;
    }

    result
}
