use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::RenderableContent;
use alacritty_terminal::vte::ansi::{Color as AnsiColor, NamedColor};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Widget};

use crate::pty::output::OutputBuffer;

pub struct TerminalWidget<'a> {
    buffer: &'a OutputBuffer,
    block: Option<Block<'a>>,
    style: Style,
    show_cursor: bool,
}

impl<'a> TerminalWidget<'a> {
    pub fn new(buffer: &'a OutputBuffer) -> Self {
        Self {
            buffer,
            block: None,
            style: Style::default(),
            show_cursor: true,
        }
    }

    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn show_cursor(mut self, show: bool) -> Self {
        self.show_cursor = show;
        self
    }
}

impl Widget for TerminalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Clear.render(area, buf);
        let inner = match self.block {
            Some(block) => {
                let inner = block.inner(area);
                block.render(area, buf);
                inner
            }
            None => area,
        };

        buf.set_style(inner, self.style);

        let content = self.buffer.renderable_content();
        render_content(content, inner, buf, self.style, self.show_cursor);
    }
}

fn render_content(
    mut content: RenderableContent<'_>,
    area: Rect,
    buf: &mut Buffer,
    base_style: Style,
    show_cursor: bool,
) {
    let base_modifiers = base_style.add_modifier;
    let display_offset = content.display_offset as i32;

    for indexed in content.display_iter.by_ref() {
        let point = indexed.point;
        let row = point.line.0 + display_offset;
        if row < 0 {
            continue;
        }
        let row = row as u16;
        if row >= area.height {
            continue;
        }

        let col = point.column.0 as u16;
        if col >= area.width {
            continue;
        }

        let cell = indexed.cell;
        let target = &mut buf[(area.x + col, area.y + row)];

        if !cell.flags.contains(Flags::WIDE_CHAR_SPACER)
            && !cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
        {
            target.set_char(cell.c);
        }

        let mut style = Style::default().add_modifier(base_modifiers);
        apply_flags(&mut style, cell.flags);

        target.modifier = style.add_modifier;

        let fg = map_color(cell.fg, content.colors);
        if fg != Color::Reset {
            target.set_fg(fg);
        }
        let bg = map_color(cell.bg, content.colors);
        if bg != Color::Reset {
            target.set_bg(bg);
        }
    }

    if show_cursor && content.display_offset == 0 {
        let cursor = content.cursor;
        if cursor.shape != alacritty_terminal::vte::ansi::CursorShape::Hidden {
            let row = cursor.point.line.0;
            let col = cursor.point.column.0 as u16;
            if row >= 0 && (row as u16) < area.height && col < area.width {
                let target = &mut buf[(area.x + col, area.y + row as u16)];
                target.modifier.insert(Modifier::REVERSED);
            }
        }
    }
}

fn apply_flags(style: &mut Style, flags: Flags) {
    if flags.contains(Flags::BOLD) {
        style.add_modifier.insert(Modifier::BOLD);
    }
    if flags.contains(Flags::DIM) {
        style.add_modifier.insert(Modifier::DIM);
    }
    if flags.contains(Flags::ITALIC) {
        style.add_modifier.insert(Modifier::ITALIC);
    }
    if flags.contains(Flags::UNDERLINE)
        || flags.contains(Flags::DOUBLE_UNDERLINE)
        || flags.contains(Flags::UNDERCURL)
        || flags.contains(Flags::DOTTED_UNDERLINE)
        || flags.contains(Flags::DASHED_UNDERLINE)
    {
        style.add_modifier.insert(Modifier::UNDERLINED);
    }
    if flags.contains(Flags::STRIKEOUT) {
        style.add_modifier.insert(Modifier::CROSSED_OUT);
    }
    if flags.contains(Flags::INVERSE) {
        style.add_modifier.insert(Modifier::REVERSED);
    }
    if flags.contains(Flags::HIDDEN) {
        style.add_modifier.insert(Modifier::HIDDEN);
    }
}

fn map_color(color: AnsiColor, palette: &Colors) -> Color {
    match color {
        AnsiColor::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
        AnsiColor::Indexed(index) => Color::Indexed(index),
        AnsiColor::Named(named) => map_named_color(named, palette),
    }
}

fn map_named_color(color: NamedColor, palette: &Colors) -> Color {
    if let Some(rgb) = palette[color] {
        return Color::Rgb(rgb.r, rgb.g, rgb.b);
    }

    match color {
        NamedColor::Black => Color::Black,
        NamedColor::Red => Color::Red,
        NamedColor::Green => Color::Green,
        NamedColor::Yellow => Color::Yellow,
        NamedColor::Blue => Color::Blue,
        NamedColor::Magenta => Color::Magenta,
        NamedColor::Cyan => Color::Cyan,
        NamedColor::White => Color::White,
        NamedColor::BrightBlack => Color::DarkGray,
        NamedColor::BrightRed => Color::LightRed,
        NamedColor::BrightGreen => Color::LightGreen,
        NamedColor::BrightYellow => Color::LightYellow,
        NamedColor::BrightBlue => Color::LightBlue,
        NamedColor::BrightMagenta => Color::LightMagenta,
        NamedColor::BrightCyan => Color::LightCyan,
        NamedColor::BrightWhite => Color::White,
        NamedColor::DimBlack
        | NamedColor::DimRed
        | NamedColor::DimGreen
        | NamedColor::DimYellow
        | NamedColor::DimBlue
        | NamedColor::DimMagenta
        | NamedColor::DimCyan
        | NamedColor::DimWhite => Color::DarkGray,
        NamedColor::Foreground
        | NamedColor::Background
        | NamedColor::Cursor
        | NamedColor::BrightForeground
        | NamedColor::DimForeground => Color::Reset,
    }
}
