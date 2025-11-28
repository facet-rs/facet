//! Syntax highlighting with span position conversion.
//!
//! This module provides functions to apply syntect syntax highlighting
//! to text while converting span positions from plain text to highlighted text.

use alloc::string::String;
use alloc::vec::Vec;
use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

/// Highlight text as JSON and convert span positions.
///
/// Takes plain text and spans (as plain text byte positions),
/// returns highlighted text and converted spans (as highlighted text byte positions).
pub fn highlight_json_with_spans(
    plain_text: &str,
    spans: &[(usize, usize, String)],
) -> (String, Vec<(usize, usize, String)>) {
    highlight_with_spans(plain_text, spans, "JSON")
}

/// Highlight text as Rust and convert span positions.
///
/// Takes plain text and spans (as plain text byte positions),
/// returns highlighted text and converted spans (as highlighted text byte positions).
pub fn highlight_rust_with_spans(
    plain_text: &str,
    spans: &[(usize, usize, String)],
) -> (String, Vec<(usize, usize, String)>) {
    highlight_with_spans(plain_text, spans, "Rust")
}

fn highlight_with_spans(
    plain_text: &str,
    spans: &[(usize, usize, String)],
    syntax_name: &str,
) -> (String, Vec<(usize, usize, String)>) {
    let syntax = SYNTAX_SET
        .find_syntax_by_name(syntax_name)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(syntax_name.to_lowercase().as_str()))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let theme = &THEME_SET.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut highlighted = String::new();
    // Map from plain text byte position to highlighted text byte position
    let mut position_map: Vec<usize> = Vec::with_capacity(plain_text.len() + 1);

    for line in plain_text.lines() {
        let ranges: Vec<(Style, &str)> = highlighter
            .highlight_line(line, &SYNTAX_SET)
            .unwrap_or_default();

        for (style, text) in ranges {
            // Record the mapping for each byte in the plain text
            for _ in 0..text.len() {
                position_map.push(highlighted.len());
            }
            // Write the highlighted version
            let escaped = as_24_bit_terminal_escaped(&[(style, text)], false);
            highlighted.push_str(&escaped);
        }
        // Handle newline
        position_map.push(highlighted.len());
        highlighted.push('\n');
    }
    // Final position for end-of-string
    position_map.push(highlighted.len());

    // Remove trailing newline if original didn't have one
    if !plain_text.ends_with('\n') && highlighted.ends_with('\n') {
        highlighted.pop();
    }

    // Convert spans
    let converted_spans: Vec<(usize, usize, String)> = spans
        .iter()
        .map(|(start, end, label)| {
            let new_start = position_map.get(*start).copied().unwrap_or(*start);
            let new_end = position_map.get(*end).copied().unwrap_or(*end);
            (new_start, new_end, label.clone())
        })
        .collect();

    (highlighted, converted_spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_preserves_content() {
        let plain = r#"{"name": "Alice"}"#;
        let (highlighted, _) = highlight_json_with_spans(plain, &[]);
        // The highlighted version should contain the same visible text
        // (just with ANSI codes added)
        assert!(highlighted.contains("name"));
        assert!(highlighted.contains("Alice"));
    }

    #[test]
    fn test_span_conversion() {
        let plain = "hello";
        let spans = vec![(0, 5, "test".into())];
        let (highlighted, converted) = highlight_rust_with_spans(plain, &spans);
        // The span should still cover the whole content
        assert_eq!(converted.len(), 1);
        let (_start, end, _) = &converted[0];
        // The highlighted span should encompass the content
        assert!(*end <= highlighted.len());
    }
}
