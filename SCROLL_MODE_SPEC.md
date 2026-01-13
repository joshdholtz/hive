# Scroll Mode Specification

## Problem Statement

Hive is a TUI that displays multiple terminal panes running AI coding agents (Claude Code, Codex CLI). Users need to scroll back through terminal history to see earlier output, similar to tmux's copy mode.

**Current behavior:**
- Scrolling works for **Claude Code** (mostly)
- Scrolling does **NOT work for Codex CLI** - the scroll offset changes in state but the displayed content doesn't update

## Background: How Terminals Work

### Normal vs Alternate Screen Buffer

Terminals have two screen modes:
1. **Normal screen buffer** - Has scrollback history. Used by simple CLI apps.
2. **Alternate screen buffer** - No scrollback. Used by TUI apps (vim, htop, etc.)

Apps enter alternate screen with `ESC[?1049h` and exit with `ESC[?1049l`.

**Both Claude Code and Codex CLI are Ink-based TUIs that use alternate screen buffer.**

This is the core problem: **when an app uses alternate screen, there's no scrollback to scroll through.**

## Current Architecture

### Terminal Emulation Stack
- `vt100` crate - Terminal emulator with scrollback support
- `tui-term` crate - Renders vt100 screen to ratatui

### Key Files
- `src/pty/output.rs` - `OutputBuffer` wraps vt100 parser with scroll methods
- `src/app/state.rs` - `App` state with `scroll_mode`, `scroll_buffer`, `scroll_lines`, `scroll_pos`
- `src/ui/pane.rs` - Pane rendering, attempts scroll mode rendering
- `src/commands/attach.rs` - Key handling, scroll mode entry/exit

### Current Scroll Mode Flow

1. User presses `ESC` to enter scroll mode
2. We copy raw terminal history from `pane.raw_history` (captured bytes)
3. Filter out alternate screen sequences with `filter_alternate_screen()`
4. Create a new `OutputBuffer` and replay the filtered history
5. Use vt100's `set_scrollback()` to scroll
6. Render with tui-term's `PseudoTerminal`

### The Problem

**tui-term's `PseudoTerminal` widget does not respect vt100's scrollback offset.**

It just renders the current screen state. Even though we call `scroll_buffer.scroll_up(1)` which internally calls `parser.set_scrollback(offset)`, the rendered output doesn't change.

This is why:
- The status bar shows `off:50` (offset is updating)
- But the screen content stays the same

**Why does Claude work better than Codex?**

Claude Code seems to produce output that results in scrollback content in vt100, while Codex's output doesn't. This might be due to:
- Different escape sequences used
- Different screen clear patterns
- Different use of alternate screen

## What We've Tried

### Attempt 1: Direct vt100 scrollback
- Use `output_buffer.scroll_up()` which calls `parser.set_scrollback()`
- **Result:** Offset changes but display doesn't update (tui-term doesn't support it)

### Attempt 2: Filter alternate screen sequences
- Strip `ESC[?1049h/l`, `ESC[2J`, `ESC[3J` from history before replaying
- **Result:** Same issue - tui-term still doesn't render scrollback

### Attempt 3: Raw history capture + separate buffer
- Capture raw bytes in `pane.raw_history` (500KB ring buffer)
- On scroll mode entry, create fresh `OutputBuffer` from filtered history
- **Result:** Same issue - tui-term renders current screen, not scrollback

### Attempt 4: Plain text extraction + Paragraph widget
- Extract plain text by stripping ALL ANSI sequences
- Store as `Vec<String>` lines
- Render with ratatui's `Paragraph` widget with manual offset
- **Result:** Buggy, didn't work properly (reverted)

## Root Cause Analysis

The fundamental issue is the rendering layer:

```
vt100 parser --> set_scrollback(offset) --> screen with scrollback
                                                |
                                                v
                                        PseudoTerminal widget
                                                |
                                                v
                                        Renders current screen only (ignores scrollback)
```

tui-term's `PseudoTerminal::new(screen)` just iterates over `screen.rows()` which gives the current viewport, not the scrollback.

## Possible Solutions

### Option A: Fix tui-term rendering
Modify how we render to actually use scrollback. The vt100 `Screen` has:
- `scrollback()` - returns scrollback lines
- `rows_formatted(start, end)` - can get formatted rows

We could render scrollback + current screen manually instead of using PseudoTerminal.

### Option B: Custom terminal renderer
Build our own renderer that:
1. Gets scrollback from `screen.scrollback()`
2. Gets current screen from `screen.rows()`
3. Combines them based on scroll offset
4. Renders with proper ANSI colors preserved

### Option C: Different terminal emulator
Consider using a different terminal emulator crate that has better scrollback support with its rendering.

### Option D: Plain text approach (done right)
The Paragraph approach could work if implemented correctly:
1. On scroll mode entry, extract text from raw_history
2. Parse ANSI and convert to ratatui Spans with colors
3. Render as scrollable text widget
4. Need to handle cursor position, line wrapping correctly

## Key Code Locations

### State definitions
```rust
// src/app/state.rs
pub struct App {
    pub scroll_mode: bool,
    pub scroll_buffer: Option<OutputBuffer>,
    pub scroll_lines: Vec<String>,
    pub scroll_pos: usize,
}

pub struct ClientPane {
    pub raw_history: VecDeque<u8>,
    pub raw_history_max: usize,  // 500KB
}
```

### Scroll mode entry
```rust
// src/commands/attach.rs around line 647
} else if key.code == KeyCode::Esc ... {
    if app.scroll_mode {
        // Exit scroll mode
        app.scroll_mode = false;
        app.scroll_buffer = None;
        return Ok(false);
    }
    // Enter scroll mode - create scroll_buffer from raw_history
    if let Some(pane) = app.panes.get(app.focused_pane) {
        let history: Vec<u8> = pane.raw_history.iter().copied().collect();
        let filtered = filter_alternate_screen(&history);
        let mut scroll_buf = OutputBuffer::new(24, 80, 10000);
        scroll_buf.push_bytes(&filtered);
        app.scroll_buffer = Some(scroll_buf);
    }
    app.scroll_mode = true;
}
```

### Scroll key handling
```rust
// src/commands/attach.rs line 854
fn handle_scroll_mode_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut scroll_buf) = app.scroll_buffer {
                scroll_buf.scroll_up(1);  // This updates vt100 but tui-term ignores it
            }
        }
        // ...
    }
}
```

### Pane rendering
```rust
// src/ui/pane.rs line 67
if let Some(scroll_buf) = scroll_buffer {
    // This tries to render scroll mode but PseudoTerminal doesn't support scrollback
    let screen = scroll_buf.screen();
    // ... attempts to extract rows
}
```

### Filter functions
```rust
// src/pty/output.rs
pub fn filter_alternate_screen(data: &[u8]) -> Vec<u8>
pub fn extract_plain_text(data: &[u8]) -> String
```

## Testing

Test with:
1. `hive up` on a project with Codex backend
2. Let Codex run for a bit to generate output
3. Press ESC to enter scroll mode
4. Press k/j to scroll - observe if content changes
5. Check status bar for offset and history size

Currently: offset changes, history has ~150KB, but screen content doesn't scroll.

## Success Criteria

1. User can enter scroll mode with ESC
2. k/j scrolls through history
3. Works for BOTH Claude Code AND Codex CLI
4. Preserves colors/formatting (nice to have)
5. Can exit with q or ESC
