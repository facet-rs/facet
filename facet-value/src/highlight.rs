//! Syntax highlighting with span position conversion powered by arborium.
//!
//! These helpers convert diagnostics spans expressed in plain-text byte positions
//! into offsets within ANSI-colored strings so `miette` can keep labels aligned.

use alloc::string::String;
use alloc::vec::Vec;
use arborium::highlights::{HIGHLIGHTS, tag_for_capture};
use arborium::theme::{self, Theme};
use arborium::{Grammar, GrammarProvider, HighlightConfig, Injection, Span, StaticProvider};
use core::future::Future;
use core::pin::pin;
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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

    let mut engine = ArboriumEngine::new();
    let spans = engine.collect_spans(language, source)?;
    let segments = segments_from_spans(source, spans);
    let theme = theme::builtin::tokyo_night().clone();
    Some(render_segments_to_ansi(source, &segments, &theme))
}

struct ArboriumEngine {
    provider: StaticProvider,
    config: HighlightConfig,
}

impl ArboriumEngine {
    fn new() -> Self {
        Self {
            provider: StaticProvider::new(),
            config: HighlightConfig::default(),
        }
    }

    fn collect_spans(&mut self, language: &str, source: &str) -> Option<Vec<Span>> {
        let grammar = self.poll_provider(language)?;
        let result = grammar.parse(source);
        let mut spans = result.spans;
        if !result.injections.is_empty() {
            self.process_injections(
                source,
                result.injections,
                0,
                self.config.max_injection_depth,
                &mut spans,
            );
        }
        Some(spans)
    }

    fn process_injections(
        &mut self,
        source: &str,
        injections: Vec<Injection>,
        base_offset: u32,
        remaining_depth: u32,
        spans: &mut Vec<Span>,
    ) {
        if remaining_depth == 0 {
            return;
        }

        for injection in injections {
            let start = injection.start as usize;
            let end = injection.end as usize;
            if start >= end || end > source.len() {
                continue;
            }

            let injected_text = &source[start..end];
            let Some(grammar) = self.poll_provider(&injection.language) else {
                continue;
            };
            let result = grammar.parse(injected_text);
            spans.extend(result.spans.into_iter().map(|mut span| {
                span.start += base_offset + injection.start;
                span.end += base_offset + injection.start;
                span
            }));

            if !result.injections.is_empty() {
                self.process_injections(
                    injected_text,
                    result.injections,
                    base_offset + injection.start,
                    remaining_depth - 1,
                    spans,
                );
            }
        }
    }

    fn poll_provider(
        &mut self,
        language: &str,
    ) -> Option<&mut <StaticProvider as arborium::GrammarProvider>::Grammar> {
        let future = self.provider.get(language);
        let mut future = pin!(future);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(result) => result,
            Poll::Pending => None,
        }
    }
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

    let normalized = normalize_and_coalesce(dedup_spans(spans));
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

fn dedup_spans(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by(|a, b| a.start.cmp(&b.start).then_with(|| b.end.cmp(&a.end)));
    let mut deduped: Vec<Span> = Vec::new();

    for span in spans {
        if let Some(existing) = deduped
            .iter_mut()
            .find(|existing| existing.start == span.start && existing.end == span.end)
        {
            let new_has_style = tag_for_capture(&span.capture).is_some();
            let existing_has_style = tag_for_capture(&existing.capture).is_some();
            if new_has_style || !existing_has_style {
                *existing = span;
            }
        } else {
            deduped.push(span);
        }
    }

    deduped
}

struct NormalizedSpan {
    start: u32,
    end: u32,
    tag: &'static str,
}

fn normalize_and_coalesce(spans: Vec<Span>) -> Vec<NormalizedSpan> {
    let mut normalized: Vec<NormalizedSpan> = spans
        .into_iter()
        .filter_map(|span| {
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

fn ansi_for_tag(theme: &Theme, tag: &str) -> Option<String> {
    let index = find_style_index(theme, tag)?;
    let ansi = theme.ansi_style(index);
    if ansi.is_empty() { None } else { Some(ansi) }
}

fn find_style_index(theme: &Theme, tag: &str) -> Option<usize> {
    let mut current = tag.strip_prefix("a-").unwrap_or(tag);
    loop {
        let (idx, _) = HIGHLIGHTS
            .iter()
            .enumerate()
            .find(|(_, highlight)| highlight.tag == current)?;
        if theme
            .style(idx)
            .map(|style| !style.is_empty())
            .unwrap_or(false)
        {
            return Some(idx);
        }
        let parent = HIGHLIGHTS[idx].parent_tag;
        if parent.is_empty() {
            return None;
        }
        current = parent;
    }
}

fn noop_waker() -> Waker {
    const VTABLE: RawWakerVTable = RawWakerVTable::new(|_| RAW_WAKER, |_| {}, |_| {}, |_| {});
    const RAW_WAKER: RawWaker = RawWaker::new(core::ptr::null(), &VTABLE);
    unsafe { Waker::from_raw(RAW_WAKER) }
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
