# Question Marks Rendering Bug Specification

## Problem Statement

Sometimes panes display lots of `?????????????` characters that overlay/bleed into the sidebar area. This is a rendering corruption issue.

## Symptoms

1. Pane content shows repeated `?` characters instead of actual content
2. The `?` characters extend beyond the pane border into the sidebar
3. Happens intermittently, not consistently reproducible

## Likely Causes

### 1. Unicode/Wide Character Handling

The terminal emulator (vt100) or renderer may not properly handle:
- Emoji characters (which are 2 cells wide)
- CJK characters (also 2 cells wide)
- Box drawing characters
- Other multi-byte UTF-8 sequences

When a character can't be rendered, it may show as `?`.

### 2. Buffer Overflow / Boundary Issues

The pane renderer may not properly clip content to pane boundaries, causing:
- Content bleeding into adjacent areas (sidebar)
- Buffer writes beyond allocated area

### 3. ANSI Escape Sequence Parsing

Malformed or unexpected escape sequences might corrupt the output stream, resulting in garbage characters.

### 4. Terminal Size Mismatch

If the PTY size doesn't match the rendered area, content could wrap incorrectly or overflow.

## Architecture

### Rendering Stack

```
PTY (raw bytes) --> OutputBuffer (vt100 parser) --> TerminalWidget --> ratatui Frame
```

### Key Files

- `src/pty/output.rs` - `OutputBuffer` wraps vt100 parser
- `src/ui/terminal.rs` - Custom `TerminalWidget` for rendering (NEW - replaces tui-term)
- `src/ui/pane.rs` - Pane rendering, creates TerminalWidget
- `src/ui/mod.rs` - Main render function, layout calculation
- `src/ui/layout.rs` - Calculates pane positions and sizes

### OutputBuffer

```rust
// src/pty/output.rs
pub struct OutputBuffer {
    parser: vt100::Parser,
    scroll_offset: usize,
    scrollback_len: usize,
}

impl OutputBuffer {
    pub fn push_bytes(&mut self, data: &[u8]) {
        // Filters ESC[3J (scrollback clear) but otherwise passes to vt100
        let filtered = filter_scrollback_clear(data);
        self.parser.process(&filtered);
    }

    pub fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    pub fn size(&self) -> (u16, u16) {
        self.parser.screen().size()
    }
}
```

### TerminalWidget (new custom renderer)

Based on the system reminder, there's now a custom `TerminalWidget` in `src/ui/terminal.rs` that replaced the tui-term `PseudoTerminal`. This is where rendering bugs would likely originate.

```rust
// src/ui/terminal.rs (inferred structure)
pub struct TerminalWidget<'a> {
    buffer: &'a OutputBuffer,
    block: Option<Block<'a>>,
    style: Style,
    show_cursor: bool,
}
```

## Investigation Areas

### 1. Check TerminalWidget implementation

Look at `src/ui/terminal.rs`:
- How does it iterate over screen cells?
- Does it handle wide characters (emoji, CJK)?
- Does it properly clip to the render area?
- Does it handle the vt100 `Cell` contents correctly?

### 2. Check character width handling

vt100's `Screen` has cells that can contain:
- Single-width characters
- Wide characters (2 cells) - the first cell has the char, second is a "wide continuation"
- Empty cells

If wide character handling is wrong, you might get `?` for unrenderable chars.

### 3. Check boundary clipping

In ratatui, widgets should only write within their allocated `Rect`. If the TerminalWidget writes outside bounds, it corrupts adjacent widgets (sidebar).

```rust
// Proper clipping pattern
impl Widget for TerminalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Must only write to cells within `area`
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                // Write cell
            }
        }
    }
}
```

### 4. Check UTF-8 handling

When extracting characters from vt100 screen:
```rust
let cell = screen.cell(row, col);
let contents = cell.contents();  // This is a String
```

If `contents` contains invalid UTF-8 or characters the terminal font doesn't support, rendering issues occur.

## Debugging Steps

1. Add logging to TerminalWidget render:
   - Log when writing outside bounds
   - Log when encountering non-ASCII characters

2. Check if issue correlates with:
   - Specific content (emoji, special chars)
   - Window resizing
   - Scroll mode entry/exit
   - Specific AI backend (Claude vs Codex)

3. Test with simple ASCII-only content to see if issue disappears

## Potential Fixes

### Fix 1: Proper boundary checking
```rust
fn render(self, area: Rect, buf: &mut Buffer) {
    for row in 0..self.buffer.screen().size().0 {
        for col in 0..self.buffer.screen().size().1 {
            let x = area.left() + col;
            let y = area.top() + row;

            // CRITICAL: Check bounds before writing
            if x >= area.right() || y >= area.bottom() {
                continue;
            }

            // Write cell...
        }
    }
}
```

### Fix 2: Wide character handling
```rust
let cell = screen.cell(row, col);
if cell.is_wide_continuation() {
    continue;  // Skip the continuation cell
}

let ch = cell.contents();
let width = unicode_width::UnicodeWidthStr::width(ch.as_str());

if width == 0 {
    // Zero-width character, might need special handling
}
if width > 1 {
    // Wide character, ensure we don't double-render
}
```

### Fix 3: Replace unrenderable characters
```rust
let ch = cell.contents();
let display_char = if ch.chars().all(|c| c.is_ascii() || is_safe_unicode(c)) {
    ch
} else {
    " ".to_string()  // Or "?" but contained properly
};
```

## Testing

1. Run hive with content that includes emoji/unicode
2. Resize window while content is displaying
3. Enter/exit scroll mode
4. Check if `?` appears and if it bleeds into sidebar

## Success Criteria

1. No `?????????????` appearing unexpectedly
2. Pane content stays within pane borders
3. Sidebar is never corrupted by pane content
4. All standard terminal content renders correctly
