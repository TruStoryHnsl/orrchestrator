//! Minimal ANSI-to-ratatui converter for the Hypervise expanded-session
//! pane viewer. Parses the bytes that `tmux capture-pane -e` emits — SGR
//! style escapes plus the occasional cursor or screen control sequence —
//! and produces ratatui [`Line`]s with the appropriate [`Style`] applied.
//!
//! The goal is fidelity, not completeness: capture-pane output is mostly
//! `\x1b[<n>;<n>m` SGR sequences. Other CSI sequences (cursor moves,
//! erase line, etc.) are stripped silently. OSC sequences (`\x1b]...\x07`
//! or `\x1b]...\x1b\\`) are also stripped.
//!
//! Truecolor (`\x1b[38;2;r;g;b m`) and 256-color (`\x1b[38;5;n m`) are
//! both supported. Bold / dim / italic / underline / reverse map to
//! ratatui [`Modifier`]s. Reset (`\x1b[0m` or `\x1b[m`) clears all
//! attributes.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Parse ANSI bytes into a sequence of styled [`Line`]s suitable for
/// `Paragraph::new(...)`. Each `\n` in the input becomes a new line.
pub fn parse(input: &str) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_text = String::new();
    let mut style = Style::default();

    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b == b'\x1b' {
            // Flush text accumulated so far under the current style.
            if !current_text.is_empty() {
                current_spans.push(Span::styled(
                    std::mem::take(&mut current_text),
                    style,
                ));
            }
            // Dispatch on next byte.
            if let Some(&next) = bytes.get(i + 1) {
                match next {
                    b'[' => {
                        // CSI sequence — read until a final byte in 0x40..=0x7E.
                        let (consumed, params, final_byte) =
                            read_csi(&bytes[i + 2..]);
                        i += 2 + consumed;
                        if final_byte == b'm' {
                            apply_sgr(&mut style, &params);
                        }
                        // Other final bytes (H, J, K, etc.) — silently ignored.
                        continue;
                    }
                    b']' => {
                        // OSC — read until BEL or ST.
                        let consumed = read_osc(&bytes[i + 2..]);
                        i += 2 + consumed;
                        continue;
                    }
                    b'(' | b')' => {
                        // Charset selection — skip the next byte.
                        i += 3.min(bytes.len() - i);
                        continue;
                    }
                    _ => {
                        // Unknown 2-byte escape — drop it.
                        i += 2;
                        continue;
                    }
                }
            } else {
                i += 1;
                continue;
            }
        }

        if b == b'\n' {
            if !current_text.is_empty() {
                current_spans.push(Span::styled(
                    std::mem::take(&mut current_text),
                    style,
                ));
            }
            lines.push(Line::from(std::mem::take(&mut current_spans)));
            i += 1;
            continue;
        }

        if b == b'\r' {
            i += 1;
            continue;
        }

        // Skip other C0 control bytes silently to keep the rendered text clean.
        if b < 0x20 && b != b'\t' {
            i += 1;
            continue;
        }

        // Accumulate one UTF-8 char at a time so multi-byte chars stay intact.
        let ch_len = utf8_char_len(b);
        let end = (i + ch_len).min(bytes.len());
        if let Ok(s) = std::str::from_utf8(&bytes[i..end]) {
            current_text.push_str(s);
        }
        i = end;
    }

    if !current_text.is_empty() {
        current_spans.push(Span::styled(current_text, style));
    }
    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    lines
}

/// Read a CSI sequence after the leading `\x1b[`. Returns
/// `(bytes_consumed, parameters, final_byte)`.
fn read_csi(bytes: &[u8]) -> (usize, Vec<u32>, u8) {
    let mut params: Vec<u32> = Vec::new();
    let mut current: u32 = 0;
    let mut have_digit = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'0'..=b'9' => {
                current = current
                    .saturating_mul(10)
                    .saturating_add((b - b'0') as u32);
                have_digit = true;
            }
            b';' => {
                params.push(if have_digit { current } else { 0 });
                current = 0;
                have_digit = false;
            }
            0x40..=0x7E => {
                // Final byte. Push any pending param.
                if have_digit {
                    params.push(current);
                }
                return (i + 1, params, b);
            }
            _ => {
                // Intermediate (e.g. ?) — skip.
            }
        }
    }
    // Truncated sequence — consume everything we saw.
    if have_digit {
        params.push(current);
    }
    (bytes.len(), params, 0)
}

/// Read an OSC sequence after the leading `\x1b]`, terminated by either
/// `\x07` (BEL) or `\x1b\\` (ST). Returns bytes consumed.
fn read_osc(bytes: &[u8]) -> usize {
    for (i, &b) in bytes.iter().enumerate() {
        if b == 0x07 {
            return i + 1;
        }
        if b == b'\x1b' && bytes.get(i + 1) == Some(&b'\\') {
            return i + 2;
        }
    }
    bytes.len()
}

/// Apply an SGR parameter list to a [`Style`]. Walks the params left to
/// right; truecolor and 256-color sub-sequences consume extra params.
fn apply_sgr(style: &mut Style, params: &[u32]) {
    let mut i = 0;
    if params.is_empty() {
        // Bare `\x1b[m` is treated as reset.
        *style = Style::default();
        return;
    }
    while i < params.len() {
        let p = params[i];
        match p {
            0 => *style = Style::default(),
            1 => *style = style.add_modifier(Modifier::BOLD),
            2 => *style = style.add_modifier(Modifier::DIM),
            3 => *style = style.add_modifier(Modifier::ITALIC),
            4 => *style = style.add_modifier(Modifier::UNDERLINED),
            7 => *style = style.add_modifier(Modifier::REVERSED),
            8 => *style = style.add_modifier(Modifier::HIDDEN),
            9 => *style = style.add_modifier(Modifier::CROSSED_OUT),
            22 => *style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => *style = style.remove_modifier(Modifier::ITALIC),
            24 => *style = style.remove_modifier(Modifier::UNDERLINED),
            27 => *style = style.remove_modifier(Modifier::REVERSED),
            28 => *style = style.remove_modifier(Modifier::HIDDEN),
            29 => *style = style.remove_modifier(Modifier::CROSSED_OUT),
            30..=37 => *style = style.fg(basic_color(p - 30)),
            38 => {
                if let Some((color, used)) = read_extended_color(&params[i + 1..]) {
                    *style = style.fg(color);
                    i += used;
                }
            }
            39 => *style = style.fg(Color::Reset),
            40..=47 => *style = style.bg(basic_color(p - 40)),
            48 => {
                if let Some((color, used)) = read_extended_color(&params[i + 1..]) {
                    *style = style.bg(color);
                    i += used;
                }
            }
            49 => *style = style.bg(Color::Reset),
            90..=97 => *style = style.fg(bright_color(p - 90)),
            100..=107 => *style = style.bg(bright_color(p - 100)),
            _ => {}
        }
        i += 1;
    }
}

/// Map basic ANSI color index (0..=7) to ratatui [`Color`].
fn basic_color(n: u32) -> Color {
    match n {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::Gray,
        _ => Color::Reset,
    }
}

/// Bright ANSI color (0..=7).
fn bright_color(n: u32) -> Color {
    match n {
        0 => Color::DarkGray,
        1 => Color::LightRed,
        2 => Color::LightGreen,
        3 => Color::LightYellow,
        4 => Color::LightBlue,
        5 => Color::LightMagenta,
        6 => Color::LightCyan,
        7 => Color::White,
        _ => Color::Reset,
    }
}

/// Read the extended-color parameters that follow `38` or `48`.
/// Returns `(color, params_consumed)` where `params_consumed` is the
/// number of additional params after the leading `38`/`48`.
fn read_extended_color(rest: &[u32]) -> Option<(Color, usize)> {
    match rest.first()? {
        // 5;n — 256-color.
        5 => {
            let n = *rest.get(1)? as u8;
            Some((color_from_256(n), 2))
        }
        // 2;r;g;b — truecolor.
        2 => {
            let r = (*rest.get(1)?).min(255) as u8;
            let g = (*rest.get(2)?).min(255) as u8;
            let b = (*rest.get(3)?).min(255) as u8;
            Some((Color::Rgb(r, g, b), 4))
        }
        _ => None,
    }
}

/// 256-color index → ratatui [`Color`]. Indices 0..=15 map to the basic
/// palette; 16..=231 are a 6×6×6 RGB cube; 232..=255 are a grayscale ramp.
fn color_from_256(n: u8) -> Color {
    if n < 8 {
        return basic_color(n as u32);
    }
    if n < 16 {
        return bright_color((n - 8) as u32);
    }
    if (16..=231).contains(&n) {
        let idx = n - 16;
        let r = idx / 36;
        let g = (idx / 6) % 6;
        let b = idx % 6;
        let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
        return Color::Rgb(level(r), level(g), level(b));
    }
    // 232..=255 — grayscale.
    let v = 8 + (n - 232) * 10;
    Color::Rgb(v, v, v)
}

/// Length in bytes of the UTF-8 character whose first byte is `b`.
fn utf8_char_len(b: u8) -> usize {
    if b & 0x80 == 0 { 1 }
    else if b & 0xE0 == 0xC0 { 2 }
    else if b & 0xF0 == 0xE0 { 3 }
    else if b & 0xF8 == 0xF0 { 4 }
    else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_makes_one_line() {
        let lines = parse("hello world");
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn newline_splits_into_lines() {
        let lines = parse("a\nb\nc");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn red_foreground_is_applied() {
        let lines = parse("\x1b[31merror\x1b[0m\n");
        assert_eq!(lines.len(), 1);
        let spans = &lines[0].spans;
        let red_span = spans.iter().find(|s| s.content == "error").expect("error span");
        assert_eq!(red_span.style.fg, Some(Color::Red));
    }

    #[test]
    fn bold_modifier_is_applied() {
        let lines = parse("\x1b[1mbold\x1b[22mnormal");
        let bold = lines[0].spans.iter().find(|s| s.content == "bold").unwrap();
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));
        let plain = lines[0].spans.iter().find(|s| s.content == "normal").unwrap();
        assert!(!plain.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn truecolor_is_parsed() {
        let lines = parse("\x1b[38;2;100;200;50mfoo");
        let foo = &lines[0].spans[0];
        assert_eq!(foo.style.fg, Some(Color::Rgb(100, 200, 50)));
    }

    #[test]
    fn extended_256_color_is_parsed() {
        let lines = parse("\x1b[38;5;160mhello");
        // 160 = (160 - 16) = 144 → r=4, g=0, b=0 → (215, 0, 0)
        let span = &lines[0].spans[0];
        assert_eq!(span.style.fg, Some(Color::Rgb(215, 0, 0)));
    }

    #[test]
    fn cursor_sequences_are_stripped() {
        // `\x1b[2J` clears screen, `\x1b[H` moves cursor — both should
        // disappear from the rendered output without breaking the text.
        let lines = parse("\x1b[2J\x1b[Habcd");
        assert_eq!(lines.len(), 1);
        let s: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(s, "abcd");
    }

    #[test]
    fn osc_sequences_are_stripped() {
        // OSC terminator can be BEL (\x07) or ST (\x1b\\)
        let lines = parse("\x1b]0;window title\x07hello");
        let s: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(s, "hello");
    }

    #[test]
    fn reset_clears_attributes() {
        let lines = parse("\x1b[1;31mbold red\x1b[0mplain");
        let plain = lines[0].spans.iter().find(|s| s.content == "plain").unwrap();
        assert_eq!(plain.style.fg, None);
        assert!(!plain.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn utf8_multibyte_preserved() {
        let lines = parse("⬡ status");
        let s: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(s, "⬡ status");
    }
}
