//! Syntax highlighting with span position conversion powered by arborium.
//!
//! These helpers convert diagnostics spans expressed in plain-text byte positions
//! into offsets within ANSI-colored strings so labels stay aligned.

use alloc::string::String;
use alloc::vec::Vec;

use arborium::AnsiHighlighter;
use arborium::advanced::Span;
use arborium::theme::{self, Theme};

/// Highlight text as JSON and convert span positions.
///
/// Takes plain text and spans (as plain text byte positions),
/// returns highlighted text and converted spans (as highlighted text byte positions).
pub fn highlight_json_with_spans(
    plain_text: &str,
    spans: &[(usize, usize, String)],
) -> (String, Vec<(usize, usize, String)>) {
    highlight_with_language(plain_text, spans, "json")
}

/// Highlight text as Rust and convert span positions.
///
/// Takes plain text and spans (as plain text byte positions),
/// returns highlighted text and converted spans (as highlighted text byte positions).
pub fn highlight_rust_with_spans(
    plain_text: &str,
    spans: &[(usize, usize, String)],
) -> (String, Vec<(usize, usize, String)>) {
    highlight_with_language(plain_text, spans, "rust")
}

fn highlight_with_language(
    plain_text: &str,
    spans: &[(usize, usize, String)],
    language: &str,
) -> (String, Vec<(usize, usize, String)>) {
    let (highlighted, position_map) =
        highlight_text(language, plain_text).unwrap_or_else(|| fallback_highlight(plain_text));

    let converted_spans = spans
        .iter()
        .map(|(start, end, label)| {
            let new_start = position_map
                .get(*start)
                .copied()
                .unwrap_or(highlighted.len());
            let new_end = position_map.get(*end).copied().unwrap_or(highlighted.len());
            (new_start, new_end, label.clone())
        })
        .collect();

    (highlighted, converted_spans)
}

fn fallback_highlight(plain_text: &str) -> (String, Vec<usize>) {
    let mut map = Vec::with_capacity(plain_text.len() + 1);
    for i in 0..plain_text.len() {
        map.push(i);
    }
    map.push(plain_text.len());
    (plain_text.to_string(), map)
}

fn highlight_text(language: &str, source: &str) -> Option<(String, Vec<usize>)> {
    if source.is_empty() {
        return Some((String::new(), vec![0]));
    }

    let theme = theme::builtin::tokyo_night().clone();
    let mut highlighter = AnsiHighlighter::new(theme.clone());

    // Get raw spans for position mapping
    let raw_spans = highlighter.highlight(language, source).ok().and_then(|_| {
        // We need to get spans separately - use the inner highlighter
        let mut inner = arborium::Highlighter::new();
        inner.highlight_spans(language, source).ok()
    })?;

    let segments = segments_from_spans(source, raw_spans);
    Some(render_segments_to_ansi(source, &segments, &theme))
}

struct Segment<'a> {
    text: &'a str,
    tag: Option<&'static str>,
}

fn render_segments_to_ansi<'a>(
    source: &'a str,
    segments: &[Segment<'a>],
    theme: &Theme,
) -> (String, Vec<usize>) {
    let mut highlighted = String::new();
    let mut position_map = Vec::with_capacity(source.len() + 1);
    let mut active_code: Option<String> = None;

    for segment in segments {
        let target_code = segment
            .tag
            .and_then(|tag| ansi_for_tag(theme, tag))
            .filter(|code| !code.is_empty());

        if target_code != active_code {
            highlighted.push_str(Theme::ANSI_RESET);
            if let Some(code) = &target_code {
                highlighted.push_str(code);
            }
            active_code = target_code;
        }

        for ch in segment.text.chars() {
            for _ in 0..ch.len_utf8() {
                position_map.push(highlighted.len());
            }
            highlighted.push(ch);
        }
    }

    highlighted.push_str(Theme::ANSI_RESET);
    position_map.push(highlighted.len());
    (highlighted, position_map)
}

fn segments_from_spans<'a>(source: &'a str, spans: Vec<Span>) -> Vec<Segment<'a>> {
    if source.is_empty() {
        return vec![Segment {
            text: "",
            tag: None,
        }];
    }

    let normalized = normalize_and_coalesce(spans);
    if normalized.is_empty() {
        return vec![Segment {
            text: source,
            tag: None,
        }];
    }

    let mut events: Vec<(u32, bool, usize)> = Vec::new();
    for (idx, span) in normalized.iter().enumerate() {
        events.push((span.start, true, idx));
        events.push((span.end, false, idx));
    }
    events.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let mut segments = Vec::new();
    let mut last_pos = 0usize;
    let mut stack: Vec<usize> = Vec::new();

    for (pos, is_start, idx) in events {
        let pos = pos as usize;
        if pos > last_pos && pos <= source.len() {
            let text = &source[last_pos..pos];
            let tag = stack.last().map(|&active| normalized[active].tag);
            segments.push(Segment { text, tag });
            last_pos = pos;
        }

        if is_start {
            stack.push(idx);
        } else if let Some(position) = stack.iter().rposition(|&active| active == idx) {
            stack.remove(position);
        }
    }

    if last_pos < source.len() {
        let tag = stack.last().map(|&active| normalized[active].tag);
        segments.push(Segment {
            text: &source[last_pos..],
            tag,
        });
    }

    segments
}

struct NormalizedSpan {
    start: u32,
    end: u32,
    tag: &'static str,
}

fn normalize_and_coalesce(spans: Vec<Span>) -> Vec<NormalizedSpan> {
    // In arborium 2.x, Span has a `capture` field that contains the tag directly
    let mut normalized: Vec<NormalizedSpan> = spans
        .into_iter()
        .filter_map(|span| {
            // The capture field contains the highlight tag
            let tag = tag_for_capture(&span.capture)?;
            Some(NormalizedSpan {
                start: span.start,
                end: span.end,
                tag,
            })
        })
        .collect();

    if normalized.is_empty() {
        return normalized;
    }

    normalized.sort_by_key(|s| (s.start, s.end));
    let mut coalesced: Vec<NormalizedSpan> = Vec::with_capacity(normalized.len());

    for span in normalized {
        if let Some(last) = coalesced.last_mut()
            && span.tag == last.tag
            && span.start <= last.end
        {
            last.end = last.end.max(span.end);
            continue;
        }
        coalesced.push(span);
    }

    coalesced
}

/// Convert a capture name to a highlight tag.
/// Capture names like "keyword" or "function" map to tags.
fn tag_for_capture(capture: &str) -> Option<&'static str> {
    // Common highlight captures - these are the tree-sitter highlight names
    // that map to arborium's tag system
    match capture {
        "keyword" | "keyword.function" | "keyword.control" | "keyword.operator"
        | "keyword.return" | "keyword.storage" => Some("keyword"),
        "function" | "function.call" | "function.method" | "function.builtin" => Some("function"),
        "type" | "type.builtin" => Some("type"),
        "variable" | "variable.builtin" | "variable.parameter" => Some("variable"),
        "string" | "string.special" => Some("string"),
        "number" | "float" => Some("number"),
        "comment" | "comment.line" | "comment.block" => Some("comment"),
        "operator" => Some("operator"),
        "punctuation" | "punctuation.bracket" | "punctuation.delimiter" => Some("punctuation"),
        "constant" | "constant.builtin" => Some("constant"),
        "property" => Some("property"),
        "attribute" => Some("attribute"),
        "namespace" => Some("namespace"),
        "label" => Some("label"),
        _ => None,
    }
}

fn ansi_for_tag(theme: &Theme, tag: &str) -> Option<String> {
    // Map our simplified tags to theme style indices
    let index = match tag {
        "keyword" => 0,
        "function" => 1,
        "type" => 2,
        "variable" => 3,
        "string" => 4,
        "number" => 5,
        "comment" => 6,
        "operator" => 7,
        "punctuation" => 8,
        "constant" => 9,
        "property" => 10,
        "attribute" => 11,
        "namespace" => 12,
        "label" => 13,
        _ => return None,
    };

    let ansi = theme.ansi_style(index);
    if ansi.is_empty() { None } else { Some(ansi) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_preserves_bytes() {
        let plain = r#"{"name":"Facet"}"#;
        let (highlighted, spans) = highlight_json_with_spans(plain, &[]);
        assert!(highlighted.contains("name"));
        assert!(spans.is_empty());
    }

    #[test]
    fn span_conversion_stays_in_bounds() {
        let plain = "fn main() {}";
        let (highlighted, spans) = highlight_rust_with_spans(plain, &[(3, 7, "body".into())]);
        assert_eq!(spans.len(), 1);
        let (start, end, _) = &spans[0];
        assert!(start <= end);
        assert!(*end <= highlighted.len());
    }
}
