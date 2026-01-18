//! Syntax highlighting for the TUI using arborium.

use arborium::Highlighter;
use arborium_highlight::Span as ArboriumSpan;
use arborium_theme::{Theme, capture_to_slot, slot_to_highlight_index};
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
    let spans = match highlighter.highlight_spans(language, source) {
        Ok(spans) => spans,
        Err(_) => {
            // Fallback: return unhighlighted lines
            return source
                .lines()
                .map(|line| Line::from(line.to_string()))
                .collect();
        }
    };

    // Convert to themed spans with style indices
    let themed = spans_to_styled(source, spans, theme);

    // Build lines
    let mut lines = Vec::new();
    let mut current_line_spans: Vec<RatatuiSpan<'static>> = Vec::new();
    let mut last_end = 0usize;

    for (start, end, style) in themed {
        let start = start as usize;
        let end = end as usize;

        // Add any unstyled text before this span
        if start > last_end {
            let unstyled = &source[last_end..start];
            for (i, part) in unstyled.split('\n').enumerate() {
                if i > 0 {
                    lines.push(Line::from(std::mem::take(&mut current_line_spans)));
                }
                if !part.is_empty() {
                    current_line_spans.push(RatatuiSpan::raw(part.to_string()));
                }
            }
        }

        // Add the styled span
        let text = &source[start..end];
        for (i, part) in text.split('\n').enumerate() {
            if i > 0 {
                lines.push(Line::from(std::mem::take(&mut current_line_spans)));
            }
            if !part.is_empty() {
                current_line_spans.push(RatatuiSpan::styled(part.to_string(), style));
            }
        }

        last_end = end;
    }

    // Add any remaining unstyled text
    if last_end < source.len() {
        let unstyled = &source[last_end..];
        for (i, part) in unstyled.split('\n').enumerate() {
            if i > 0 {
                lines.push(Line::from(std::mem::take(&mut current_line_spans)));
            }
            if !part.is_empty() {
                current_line_spans.push(RatatuiSpan::raw(part.to_string()));
            }
        }
    }

    // Don't forget the last line
    if !current_line_spans.is_empty() {
        lines.push(Line::from(current_line_spans));
    }

    // Handle case where source ends with newline
    if source.ends_with('\n') && !lines.is_empty() {
        lines.push(Line::from(""));
    }

    // If we got no lines, return at least one empty line
    if lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}

/// Convert arborium spans to (start, end, ratatui::Style) tuples.
fn spans_to_styled(
    _source: &str,
    spans: Vec<ArboriumSpan>,
    theme: &Theme,
) -> Vec<(u32, u32, Style)> {
    let mut result = Vec::new();

    // Sort spans by start position
    let mut spans = spans;
    spans.sort_by_key(|s| s.start);

    for span in spans {
        let slot = capture_to_slot(&span.capture);
        if let Some(theme_idx) = slot_to_highlight_index(slot) {
            if let Some(style) = theme.style(theme_idx) {
                let ratatui_style = to_ratatui_style(style);
                result.push((span.start, span.end, ratatui_style));
            }
        }
    }

    result
}
