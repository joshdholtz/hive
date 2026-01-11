use vt100::Parser;

pub struct OutputBuffer {
    parser: Parser,
    scroll_offset: usize,
}

impl OutputBuffer {
    pub fn new(rows: u16, cols: u16, scrollback: usize) -> Self {
        Self {
            parser: Parser::new(rows, cols, scrollback),
            scroll_offset: 0,
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.set_size(rows, cols);
        self.scroll_offset = 0;
    }

    pub fn push_bytes(&mut self, data: &[u8]) {
        self.parser.process(data);
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub fn scroll_up(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }
}
