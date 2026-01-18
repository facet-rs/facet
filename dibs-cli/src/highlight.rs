//! Syntax highlighting for the TUI using arborium.

use arborium::Highlighter;
use arborium_highlight::spans_to_themed;
use arborium_theme::Theme;
use ratatui::prelude::*;
use ratatui::text::{Line, Span as RatatuiSpan};

/// Convert arborium theme color to ratatui color.
fn to_ratatui_color(color: &arborium_theme::Color) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

/// Convert arborium style to ratatui style.
fn to_ratatui_style(style: &arborium_theme::Style) -> Style {
    let mut ratatui_style = Style::default();

    if let Some(fg) = &style.fg {
        ratatui_style = ratatui_style.fg(to_ratatui_color(fg));
    }
    if let Some(bg) = &style.bg {
        ratatui_style = ratatui_style.bg(to_ratatui_color(bg));
    }
    if style.modifiers.bold {
        ratatui_style = ratatui_style.bold();
    }
    if style.modifiers.italic {
        ratatui_style = ratatui_style.italic();
    }
    if style.modifiers.underline {
        ratatui_style = ratatui_style.underlined();
    }

    ratatui_style
}

/// Highlight source code and return ratatui Lines.
pub fn highlight_to_lines(
    highlighter: &mut Highlighter,
    theme: &Theme,
    language: &str,
    source: &str,
) -> Vec<Line<'static>> {
    // Get spans from arborium
    let raw_spans = match highlighter.highlight_spans(language, source) {
        Ok(spans) => spans,
        Err(_) => {
            // Fallback: return unhighlighted lines
            return source
                .lines()
                .map(|line| Line::from(line.to_string()))
                .collect();
        }
    };

    // Use arborium's spans_to_themed which handles deduplication and coalescing
    let themed_spans = spans_to_themed(raw_spans);

    if themed_spans.is_empty() {
        // No highlighting - return plain lines
        return source
            .lines()
            .map(|line| Line::from(line.to_string()))
            .collect();
    }

    // Build events from spans: (position, is_start, span_index)
    let mut events: Vec<(u32, bool, usize)> = Vec::new();
    for (i, span) in themed_spans.iter().enumerate() {
        events.push((span.start, true, i));
        events.push((span.end, false, i));
    }

    // Sort events: by position, then ends before starts at same position
    events.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    // Process events with a stack to handle overlapping spans
    let mut lines = Vec::new();
    let mut current_line_spans: Vec<RatatuiSpan<'static>> = Vec::new();
    let mut last_pos: usize = 0;
    let mut stack: Vec<usize> = Vec::new(); // indices into themed_spans

    for (pos, is_start, span_idx) in events {
        let pos = pos as usize;

        // Emit any source text before this position
        if pos > last_pos && pos <= source.len() {
            let text = &source[last_pos..pos];
            let style = if let Some(&top_idx) = stack.last() {
                let theme_idx = themed_spans[top_idx].theme_index;
                theme
                    .style(theme_idx)
                    .map(to_ratatui_style)
                    .unwrap_or_default()
            } else {
                Style::default()
            };

            // Split by newlines and add to lines
            for (i, part) in text.split('\n').enumerate() {
                if i > 0 {
                    lines.push(Line::from(std::mem::take(&mut current_line_spans)));
                }
                if !part.is_empty() {
                    current_line_spans.push(RatatuiSpan::styled(part.to_string(), style));
                }
            }
            last_pos = pos;
        }

        // Update the stack
        if is_start {
            stack.push(span_idx);
        } else {
            // Remove this span from stack
            if let Some(idx) = stack.iter().rposition(|&x| x == span_idx) {
                stack.remove(idx);
            }
        }
    }

    // Emit remaining text after last event
    if last_pos < source.len() {
        let text = &source[last_pos..];
        let style = if let Some(&top_idx) = stack.last() {
            let theme_idx = themed_spans[top_idx].theme_index;
            theme
                .style(theme_idx)
                .map(to_ratatui_style)
                .unwrap_or_default()
        } else {
            Style::default()
        };

        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Line::from(std::mem::take(&mut current_line_spans)));
            }
            if !part.is_empty() {
                current_line_spans.push(RatatuiSpan::styled(part.to_string(), style));
            }
        }
    }

    // Don't forget the last line
    if !current_line_spans.is_empty() {
        lines.push(Line::from(current_line_spans));
    }

    // If we got no lines, return at least one empty line
    if lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}
