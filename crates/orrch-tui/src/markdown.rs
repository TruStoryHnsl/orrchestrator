//! Markdown → ratatui `Vec<Line>` renderer.
//!
//! Parses CommonMark with pulldown-cmark and maps semantic events to styled spans.
//! Uses the project's dark-theme color palette from ui.rs.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

// ─── Palette (mirrored from ui.rs) ──────────────────────────────────
const ACCENT: Color = Color::Rgb(233, 69, 96);
const TEXT: Color = Color::Rgb(230, 230, 240);
const TEXT_DIM: Color = Color::Rgb(180, 180, 200);
const TEXT_MUTED: Color = Color::Rgb(130, 130, 155);
const CYAN: Color = Color::Rgb(100, 200, 220);

/// Parse a markdown string into a list of styled ratatui `Line`s.
///
/// - H1 → ACCENT + Bold
/// - H2 → Cyan + Bold
/// - H3 → TEXT + Bold
/// - `**bold**` → Bold
/// - `*italic*` → Italic
/// - `` `code` `` → TEXT_MUTED
/// - ``` ```block``` ``` → TEXT_MUTED + 2-space indent per line
/// - List items → "  • " prefix
/// - Links → text + " (url)" in TEXT_MUTED
/// - `---` / `***` (horizontal rule) → 40× "─" in TEXT_MUTED
/// - Blank line between top-level blocks
///
/// Degrades gracefully: malformed input is rendered as plain text.
/// Empty input returns an empty vec.
pub fn markdown_to_lines(content: &str) -> Vec<Line<'static>> {
    if content.is_empty() {
        return Vec::new();
    }

    // Guard against very large files causing UI freeze.
    const MAX_PREVIEW_BYTES: usize = 65536; // 64 KB
    let original_len = content.len();
    let content = if original_len > MAX_PREVIEW_BYTES {
        // Walk back to the last valid UTF-8 char boundary.
        let mut end = MAX_PREVIEW_BYTES;
        while end > 0 && !content.is_char_boundary(end) {
            end -= 1;
        }
        &content[..end]
    } else {
        content
    };

    let mut output: Vec<Line<'static>> = Vec::new();

    // Per-line accumulator
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Style stack — each Start/End pair pushes/pops
    let mut style_stack: Vec<Style> = vec![Style::default().fg(TEXT)];

    // State flags
    let mut in_code_block = false;
    let mut in_list_item = false;
    let mut in_heading: Option<HeadingLevel> = None;
    // Link href accumulator: push while inside Link tag
    let mut link_url: Option<String> = None;

    // Helper: current merged style (fold the stack)
    // We store the full Style at each stack level so merging is cheap.
    // On push we compute the new merged style from the current top.
    fn top(stack: &[Style]) -> Style {
        *stack.last().unwrap_or(&Style::default())
    }

    // Flush the current span buffer as a completed Line
    let flush = |spans: &mut Vec<Span<'static>>, output: &mut Vec<Line<'static>>| {
        let line = Line::from(std::mem::take(spans));
        output.push(line);
    };

    let parser = Parser::new_ext(content, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES);

    for event in parser {
        match event {
            // ── Block starts ──────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = Some(level);
                // Flush any trailing content, then add a blank separator
                if !spans.is_empty() {
                    flush(&mut spans, &mut output);
                }
                let heading_style = match level {
                    HeadingLevel::H1 => Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                    HeadingLevel::H2 => Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
                    HeadingLevel::H3 => Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                    _ => Style::default().fg(TEXT_DIM).add_modifier(Modifier::BOLD),
                };
                style_stack.push(heading_style);
            }
            Event::End(TagEnd::Heading(_)) => {
                style_stack.pop();
                in_heading = None;
                flush(&mut spans, &mut output);
                // Blank line after heading
                output.push(Line::default());
            }

            Event::Start(Tag::Paragraph) => {
                // Nothing special — text events will fill spans
            }
            Event::End(TagEnd::Paragraph) => {
                if !spans.is_empty() {
                    flush(&mut spans, &mut output);
                }
                // Blank line after paragraph
                output.push(Line::default());
            }

            Event::Start(Tag::Strong) => {
                let new_style = top(&style_stack).add_modifier(Modifier::BOLD);
                style_stack.push(new_style);
            }
            Event::End(TagEnd::Strong) => { style_stack.pop(); }

            Event::Start(Tag::Emphasis) => {
                let new_style = top(&style_stack).add_modifier(Modifier::ITALIC);
                style_stack.push(new_style);
            }
            Event::End(TagEnd::Emphasis) => { style_stack.pop(); }

            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                style_stack.push(Style::default().fg(TEXT_MUTED));
            }
            Event::End(TagEnd::CodeBlock) => {
                if !spans.is_empty() {
                    flush(&mut spans, &mut output);
                }
                style_stack.pop();
                in_code_block = false;
                output.push(Line::default());
            }

            Event::Start(Tag::List(_)) => {
                // Lists don't need extra state at the list level
            }
            Event::End(TagEnd::List(_)) => {
                output.push(Line::default());
            }

            Event::Start(Tag::Item) => {
                in_list_item = true;
                // Bullet prefix in TEXT_DIM
                spans.push(Span::styled("  • ".to_owned(), Style::default().fg(TEXT_DIM)));
                style_stack.push(Style::default().fg(TEXT));
            }
            Event::End(TagEnd::Item) => {
                style_stack.pop();
                in_list_item = false;
                if !spans.is_empty() {
                    flush(&mut spans, &mut output);
                }
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                link_url = Some(dest_url.into_string());
                // Text inside link gets normal styling
                style_stack.push(top(&style_stack));
            }
            Event::End(TagEnd::Link) => {
                style_stack.pop();
                if let Some(url) = link_url.take() {
                    if !url.is_empty() {
                        spans.push(Span::styled(
                            format!(" ({url})"),
                            Style::default().fg(TEXT_MUTED),
                        ));
                    }
                }
            }

            Event::Start(Tag::BlockQuote(_)) => {
                let new_style = top(&style_stack).fg(TEXT_DIM);
                style_stack.push(new_style);
                spans.push(Span::styled("│ ".to_owned(), Style::default().fg(TEXT_MUTED)));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                style_stack.pop();
                if !spans.is_empty() {
                    flush(&mut spans, &mut output);
                }
                output.push(Line::default());
            }

            // ── Inline elements ───────────────────────────────────────
            Event::Code(text) => {
                // Inline code — TEXT_MUTED
                spans.push(Span::styled(
                    text.into_string(),
                    Style::default().fg(TEXT_MUTED),
                ));
            }

            Event::Text(text) => {
                let text_str = text.into_string();
                let current_style = top(&style_stack);

                if in_code_block {
                    // Emit each line of a code block with indent prefix
                    for (i, code_line) in text_str.lines().enumerate() {
                        if i > 0 {
                            flush(&mut spans, &mut output);
                        }
                        spans.push(Span::styled(
                            format!("  {code_line}"),
                            current_style,
                        ));
                    }
                    // If original text ended with newline, flush now
                    if text_str.ends_with('\n') && !spans.is_empty() {
                        flush(&mut spans, &mut output);
                    }
                } else {
                    spans.push(Span::styled(text_str, current_style));
                }
            }

            // ── Breaks ────────────────────────────────────────────────
            Event::SoftBreak => {
                // Within a paragraph — treat as a space to join words
                let current_style = top(&style_stack);
                spans.push(Span::styled(" ".to_owned(), current_style));
            }
            Event::HardBreak => {
                flush(&mut spans, &mut output);
            }

            // ── Horizontal rule ───────────────────────────────────────
            Event::Rule => {
                if !spans.is_empty() {
                    flush(&mut spans, &mut output);
                }
                output.push(Line::from(Span::styled(
                    "─".repeat(40),
                    Style::default().fg(TEXT_MUTED),
                )));
                output.push(Line::default());
            }

            // Everything else we don't handle explicitly — ignore gracefully
            _ => {}
        }
    }

    // Flush anything remaining
    if !spans.is_empty() {
        flush(&mut spans, &mut output);
    }

    // Suppress trailing blank lines
    while output.last().map(|l: &Line| l.spans.is_empty()).unwrap_or(false) {
        output.pop();
    }

    // If the input was truncated, append a notice so the user knows.
    if original_len > MAX_PREVIEW_BYTES {
        output.push(Line::default());
        output.push(Line::styled(
            "[truncated — file too large for preview]".to_owned(),
            Style::default().fg(TEXT_MUTED),
        ));
    }

    // Suppress the unused variable warnings for state we track but may not read
    let _ = in_heading;
    let _ = in_list_item;

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_empty() {
        assert!(markdown_to_lines("").is_empty());
    }

    #[test]
    fn plain_text_no_panic() {
        let lines = markdown_to_lines("hello world");
        assert!(!lines.is_empty());
    }

    #[test]
    fn heading_and_paragraph() {
        let md = "# Title\n\nSome paragraph.";
        let lines = markdown_to_lines(md);
        // Should have at least a heading line + blank + paragraph line
        assert!(lines.len() >= 3);
    }
}
