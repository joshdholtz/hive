# Question Marks Rendering Bug Specification

## Problem Statement

Sometimes panes display lots of `?????????????` characters that overlay/bleed into the sidebar area. This is a rendering corruption issue.

## Symptoms

1. Pane content shows repeated `?` characters instead of actual content
2. The `?` characters extend beyond the pane border into the sidebar
3. Happens intermittently, not consistently reproducible

## Likely Causes

### 1. Unicode/Wide Character Handling

The terminal emulator or renderer may not properly handle:
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
PTY (raw bytes) --> OutputBuffer (alacritty_terminal) --> TerminalWidget --> ratatui Frame
```

### Key Files

- `src/pty/output.rs` - `OutputBuffer` wraps `alacritty_terminal::Term` + VTE parser
- `src/ui/terminal.rs` - Custom `TerminalWidget` for rendering (NEW - replaces tui-term)
- `src/ui/pane.rs` - Pane rendering, creates TerminalWidget
- `src/ui/mod.rs` - Main render function, layout calculation
- `src/ui/layout.rs` - Calculates pane positions and sizes

### OutputBuffer

```rust
// src/pty/output.rs
pub struct OutputBuffer {
    term: alacritty_terminal::Term<...>,
    parser: alacritty_terminal::vte::ansi::Processor,
    scrollback_len: usize,
}

impl OutputBuffer {
    pub fn push_bytes(&mut self, data: &[u8]) {
        // Filters ESC[3J (scrollback clear) but otherwise passes to VTE
        let filtered = filter_scrollback_clear(data);
        self.parser.advance(&mut self.term, &filtered);
    }

    pub fn renderable_content(&self) -> RenderableContent<'_> {
        self.term.renderable_content()
    }

    pub fn size(&self) -> (u16, u16) {
        (self.term.screen_lines() as u16, self.term.columns() as u16)
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
- Does it handle the alacritty `Cell` flags correctly?

### 2. Check character width handling

Alacritty's grid cells can contain:
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

When extracting characters from the alacritty grid, `Cell::c` is a `char`. Invalid UTF-8 should not appear, but wide-character rendering can still corrupt adjacent cells if we render at the right edge of a pane.

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
let cell = grid_cell;
if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
    continue;  // Skip the continuation cell
}

let is_wide = cell.flags.contains(Flags::WIDE_CHAR);
if is_wide && col + 1 >= area.width {
    // Don't render a wide glyph at the last column; it would bleed into the border/sidebar.
    target.set_char(' ');
} else {
    target.set_char(cell.c);
}
```

### Fix 3: Skip hidden/zero-width output
```rust
if cell.flags.contains(Flags::HIDDEN) {
    target.set_char(' ');
} else {
    target.set_char(cell.c);
}
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
