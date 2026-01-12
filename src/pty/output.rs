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
        }
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = (self.scroll_offset + amount).min(self.scrollback_len);
        self.parser.set_scrollback(self.scroll_offset);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        self.parser.set_scrollback(self.scroll_offset);
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
        self.parser.set_scrollback(0);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = self.scrollback_len;
        self.parser.set_scrollback(self.scroll_offset);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = 0;
        self.parser.set_scrollback(0);
    }
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
