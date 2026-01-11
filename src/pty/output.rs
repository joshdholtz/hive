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
        self.scroll_offset = 0;
        self.parser.set_scrollback(0);
    }

    pub fn push_bytes(&mut self, data: &[u8]) {
        self.parser.process(data);
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
