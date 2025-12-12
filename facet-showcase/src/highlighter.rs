//! Syntax highlighting support for showcases.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;
use std::sync::LazyLock;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use arborium::highlights::{HIGHLIGHTS, tag_for_capture};
use arborium::theme::{self, Theme};
use arborium::{Grammar, GrammarProvider, HighlightConfig, Injection, Span, StaticProvider};
use miette_arborium::MietteHighlighter;
use owo_colors::OwoColorize;

const INDENT: &str = "    ";

/// Supported languages for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// JSON format
    Json,
    /// YAML format
    Yaml,
    /// XML format
    Xml,
    /// KDL format
    Kdl,
    /// Rust code (for type definitions)
    Rust,
}

impl Language {
    /// Returns the file extension used to look up the syntax.
    pub fn extension(self) -> &'static str {
        match self {
            Language::Json => "json",
            Language::Yaml => "yaml",
            Language::Xml => "xml",
            Language::Kdl => "kdl",
            Language::Rust => "rs",
        }
    }

    /// Returns a human-readable name for the language.
    pub fn name(self) -> &'static str {
        match self {
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Xml => "XML",
            Language::Kdl => "KDL",
            Language::Rust => "Rust",
        }
    }

    fn arborium_name(self) -> &'static str {
        match self {
            Language::Json => "json",
            Language::Yaml => "yaml",
            Language::Xml => "xml",
            Language::Kdl => "kdl",
            Language::Rust => "rust",
        }
    }
}

/// Syntax highlighter using Tokyo Night theme powered by arborium.
pub struct Highlighter {
    engine: RefCell<ArboriumEngine>,
    theme: Theme,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    /// Create a new highlighter with the Tokyo Night theme.
    pub fn new() -> Self {
        Self {
            engine: RefCell::new(ArboriumEngine::new()),
            theme: theme::builtin::tokyo_night().clone(),
        }
    }

    /// KDL grammars ship with arborium, so this is a no-op retained for API compatibility.
    pub fn with_kdl_syntaxes(self, _syntax_dir: &str) -> Self {
        self
    }

    /// Get a reference to the theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Highlight code and return terminal-escaped string.
    pub fn highlight_to_terminal(&self, code: &str, lang: Language) -> String {
        match self.collect_segments(code, lang) {
            Some(segments) => {
                render_segments_to_terminal(&segments, &self.theme, LineNumberMode::None)
            }
            None => self.plain_text_with_indent(code),
        }
    }

    /// Highlight code with line numbers for terminal output.
    pub fn highlight_to_terminal_with_line_numbers(&self, code: &str, lang: Language) -> String {
        match self.collect_segments(code, lang) {
            Some(segments) => {
                render_segments_to_terminal(&segments, &self.theme, LineNumberMode::Numbers)
            }
            None => self.plain_text_with_line_numbers(code),
        }
    }

    /// Build a miette highlighter using arborium.
    pub fn build_miette_highlighter(&self, _lang: Language) -> MietteHighlighter {
        MietteHighlighter::new()
    }

    /// Highlight code and return HTML with inline styles.
    pub fn highlight_to_html(&self, code: &str, lang: Language) -> String {
        match self.collect_segments(code, lang) {
            Some(segments) => render_segments_to_html(&segments, &self.theme),
            None => wrap_plain_text_html(code, &self.theme),
        }
    }

    fn collect_segments<'a>(&'a self, code: &'a str, lang: Language) -> Option<Vec<Segment<'a>>> {
        let mut engine = self.engine.borrow_mut();
        let spans = engine.collect_spans(lang.arborium_name(), code)?;
        Some(segments_from_spans(code, spans))
    }

    fn plain_text_with_indent(&self, code: &str) -> String {
        let mut output = String::new();
        for line in code.lines() {
            output.push_str(INDENT);
            output.push_str(line);
            output.push('\n');
        }
        output
    }

    fn plain_text_with_line_numbers(&self, code: &str) -> String {
        use owo_colors::OwoColorize;

        let mut output = String::new();
        for (i, line) in code.lines().enumerate() {
            output.push_str(&format!(
                "{} {} {}\n",
                format!("{:3}", i + 1).dimmed(),
                "│".dimmed(),
                line
            ));
        }
        output
    }
}

/// Escape HTML special characters.
pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Convert ANSI escape codes to HTML spans with inline styles.
/// Uses non-breaking spaces to preserve alignment in monospace output.
pub fn ansi_to_html(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();
    let mut in_span = false;

    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['

            // Parse the escape sequence
            let mut seq = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_ascii_digit() || ch == ';' {
                    seq.push(chars.next().unwrap());
                } else {
                    break;
                }
            }

            // Consume the final character (usually 'm')
            let final_char = chars.next();

            if final_char == Some('m') {
                // Close any existing span
                if in_span {
                    output.push_str("</span>");
                    in_span = false;
                }

                // Parse the style
                if let Some(style) = parse_ansi_style(&seq)
                    && !style.is_empty()
                {
                    output.push_str(&format!("<span style=\"{style}\">"));
                    in_span = true;
                }
            }
        } else if c == '<' {
            output.push_str("&lt;");
        } else if c == '>' {
            output.push_str("&gt;");
        } else if c == '&' {
            output.push_str("&amp;");
        } else if c == '`' {
            // Escape backticks to prevent markdown interpretation
            output.push_str("&#96;");
        } else if c == ' ' {
            // Use non-breaking space to preserve alignment
            output.push('\u{00A0}');
        } else {
            output.push(c);
        }
    }

    if in_span {
        output.push_str("</span>");
    }

    output
}

/// Parse ANSI style codes and return CSS style string.
fn parse_ansi_style(seq: &str) -> Option<String> {
    if seq.is_empty() || seq == "0" {
        return Some(String::new()); // Reset
    }

    let parts: Vec<&str> = seq.split(';').collect();
    let mut styles = Vec::new();

    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "0" => return Some(String::new()), // Reset
            "1" => styles.push("font-weight:bold".to_string()),
            "2" => styles.push("opacity:0.7".to_string()), // Dim
            "3" => styles.push("font-style:italic".to_string()),
            "4" => styles.push("text-decoration:underline".to_string()),
            "30" => styles.push("color:#000".to_string()),
            "31" => styles.push("color:#e06c75".to_string()), // Red
            "32" => styles.push("color:#98c379".to_string()), // Green
            "33" => styles.push("color:#e5c07b".to_string()), // Yellow
            "34" => styles.push("color:#61afef".to_string()), // Blue
            "35" => styles.push("color:#c678dd".to_string()), // Magenta
            "36" => styles.push("color:#56b6c2".to_string()), // Cyan
            "37" => styles.push("color:#abb2bf".to_string()), // White
            "38" => {
                // Extended color
                if i + 1 < parts.len() && parts[i + 1] == "2" {
                    // 24-bit RGB
                    if i + 4 < parts.len() {
                        let r = parts[i + 2];
                        let g = parts[i + 3];
                        let b = parts[i + 4];
                        styles.push(format!("color:rgb({r},{g},{b})"));
                        i += 4;
                    }
                } else if i + 1 < parts.len()
                    && parts[i + 1] == "5"
                    && i + 2 < parts.len()
                    && let Ok(n) = parts[i + 2].parse::<u8>()
                {
                    let color = ansi_256_to_rgb(n);
                    styles.push(format!("color:{color}"));
                    i += 2;
                }
            }
            "39" => styles.push("color:inherit".to_string()),
            "40" => styles.push("background-color:#000".to_string()),
            "41" => styles.push("background-color:#e06c75".to_string()),
            "42" => styles.push("background-color:#98c379".to_string()),
            "43" => styles.push("background-color:#e5c07b".to_string()),
            "44" => styles.push("background-color:#61afef".to_string()),
            "45" => styles.push("background-color:#c678dd".to_string()),
            "46" => styles.push("background-color:#56b6c2".to_string()),
            "47" => styles.push("background-color:#abb2bf".to_string()),
            "48" => {
                if i + 1 < parts.len() && parts[i + 1] == "2" {
                    if i + 4 < parts.len() {
                        let r = parts[i + 2];
                        let g = parts[i + 3];
                        let b = parts[i + 4];
                        styles.push(format!("background-color:rgb({r},{g},{b})"));
                        i += 4;
                    }
                } else if i + 1 < parts.len()
                    && parts[i + 1] == "5"
                    && i + 2 < parts.len()
                    && let Ok(n) = parts[i + 2].parse::<u8>()
                {
                    let color = ansi_256_to_rgb(n);
                    styles.push(format!("background-color:{color}"));
                    i += 2;
                }
            }
            "49" => styles.push("background-color:transparent".to_string()),
            "90" => styles.push("color:#5c6370".to_string()), // Bright black (dim)
            "91" => styles.push("color:#e06c75".to_string()), // Bright red
            "92" => styles.push("color:#98c379".to_string()),
            "93" => styles.push("color:#e5c07b".to_string()), // Bright yellow
            "94" => styles.push("color:#61afef".to_string()),
            "95" => styles.push("color:#c678dd".to_string()), // Bright magenta
            "96" => styles.push("color:#56b6c2".to_string()),
            "97" => styles.push("color:#fff".to_string()), // Bright white
            _ => {}
        }
        i += 1;
    }

    if styles.is_empty() {
        None
    } else {
        Some(styles.join(";"))
    }
}

fn ansi_256_to_rgb(n: u8) -> &'static str {
    match n {
        0 => "#000000",
        1 => "#800000",
        2 => "#008000",
        3 => "#808000",
        4 => "#000080",
        5 => "#800080",
        6 => "#008080",
        7 => "#c0c0c0",
        8 => "#808080",
        9 => "#ff0000",
        10 => "#00ff00",
        11 => "#ffff00",
        12 => "#0000ff",
        13 => "#ff00ff",
        14 => "#00ffff",
        15 => "#ffffff",
        _ => "#888888",
    }
}

#[cfg(test)]
mod tests {
    use super::Language;

    #[test]
    fn xml_language_metadata_is_exposed() {
        assert_eq!(Language::Xml.name(), "XML");
        assert_eq!(Language::Xml.extension(), "xml");
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

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
        let grammar = self.get_grammar(language)?;
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
            let Some(grammar) = self.get_grammar_optional(&injection.language) else {
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

    fn get_grammar(
        &mut self,
        language: &str,
    ) -> Option<&mut <StaticProvider as arborium::GrammarProvider>::Grammar> {
        self.poll_provider(language)
    }

    fn get_grammar_optional(
        &mut self,
        language: &str,
    ) -> Option<&mut <StaticProvider as arborium::GrammarProvider>::Grammar> {
        self.poll_provider(language)
    }

    fn poll_provider(
        &mut self,
        language: &str,
    ) -> Option<&mut <StaticProvider as arborium::GrammarProvider>::Grammar> {
        let future = self.provider.get(language);
        let mut future = std::pin::pin!(future);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(result) => result,
            Poll::Pending => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LineNumberMode {
    None,
    Numbers,
}

struct Segment<'a> {
    text: &'a str,
    tag: Option<&'static str>,
}

fn render_segments_to_terminal(
    segments: &[Segment<'_>],
    theme: &Theme,
    mode: LineNumberMode,
) -> String {
    let mut output = String::new();
    let mut active_code: Option<String> = None;
    let mut line = 1usize;
    let mut at_line_start = true;

    for segment in segments {
        let target_code = segment
            .tag
            .and_then(|tag| ansi_for_tag(theme, tag))
            .filter(|s| !s.is_empty());

        if target_code != active_code {
            output.push_str(Theme::ANSI_RESET);
            if let Some(code) = &target_code {
                output.push_str(code);
            }
            active_code = target_code;
        }

        for ch in segment.text.chars() {
            if at_line_start {
                output.push_str(Theme::ANSI_RESET);
                output.push_str(&line_prefix(mode, line));
                if let Some(code) = &active_code {
                    output.push_str(code);
                }
                at_line_start = false;
            }
            output.push(ch);
            if ch == '\n' {
                at_line_start = true;
                line += 1;
            }
        }
    }

    output.push_str(Theme::ANSI_RESET);
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn render_segments_to_html(segments: &[Segment<'_>], theme: &Theme) -> String {
    let mut body = String::new();
    for segment in segments {
        let escaped = html_escape(segment.text);
        if let Some(tag) = segment.tag {
            if let Some(style) = css_for_tag(theme, tag) {
                body.push_str("<span style=\"");
                body.push_str(&style);
                body.push_str("\">");
                body.push_str(&escaped);
                body.push_str("</span>");
            } else {
                body.push_str(&escaped);
            }
        } else {
            body.push_str(&escaped);
        }
    }
    wrap_with_pre(body, theme)
}

fn wrap_plain_text_html(code: &str, theme: &Theme) -> String {
    wrap_with_pre(html_escape(code), theme)
}

fn wrap_with_pre(content: String, theme: &Theme) -> String {
    let mut styles = Vec::new();
    if let Some(bg) = theme.background {
        styles.push(format!("background-color:{};", bg.to_hex()));
    }
    if let Some(fg) = theme.foreground {
        styles.push(format!("color:{};", fg.to_hex()));
    }
    styles.push("padding:12px;".to_string());
    styles.push("border-radius:8px;".to_string());
    styles.push(
        "font-family:var(--facet-mono, SFMono-Regular, Consolas, 'Liberation Mono', monospace);"
            .to_string(),
    );
    styles.push("font-size:0.9rem;".to_string());
    styles.push("overflow:auto;".to_string());
    format!(
        "<pre style=\"{}\"><code>{}</code></pre>",
        styles.join(" "),
        content
    )
}

fn line_prefix(mode: LineNumberMode, line: usize) -> String {
    match mode {
        LineNumberMode::None => INDENT.to_string(),
        LineNumberMode::Numbers => format!("{} {} ", format!("{:3}", line).dimmed(), "│".dimmed()),
    }
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
    let mut deduped = HashMap::new();
    for span in spans {
        let key = (span.start, span.end);
        let new_has_style = tag_for_capture(&span.capture).is_some();
        deduped
            .entry(key)
            .and_modify(|existing: &mut Span| {
                let existing_has_style = tag_for_capture(&existing.capture).is_some();
                if new_has_style || !existing_has_style {
                    *existing = span.clone();
                }
            })
            .or_insert(span);
    }
    deduped.into_values().collect()
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

static TAG_TO_INDEX: LazyLock<HashMap<&'static str, usize>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for (idx, highlight) in HIGHLIGHTS.iter().enumerate() {
        if !highlight.tag.is_empty() {
            map.insert(highlight.tag, idx);
        }
    }
    map
});

fn css_for_tag(theme: &Theme, tag: &str) -> Option<String> {
    let index = find_style_index(theme, tag)?;
    let style = theme.style(index)?;
    if style.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    if let Some(fg) = style.fg {
        parts.push(format!("color:{};", fg.to_hex()));
    }
    if let Some(bg) = style.bg {
        parts.push(format!("background-color:{};", bg.to_hex()));
    }
    if style.modifiers.bold {
        parts.push("font-weight:bold;".to_string());
    }
    if style.modifiers.italic {
        parts.push("font-style:italic;".to_string());
    }
    let mut decorations = Vec::new();
    if style.modifiers.underline {
        decorations.push("underline");
    }
    if style.modifiers.strikethrough {
        decorations.push("line-through");
    }
    if !decorations.is_empty() {
        parts.push(format!("text-decoration:{};", decorations.join(" ")));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn ansi_for_tag(theme: &Theme, tag: &str) -> Option<String> {
    let index = find_style_index(theme, tag)?;
    let ansi = theme.ansi_style(index);
    if ansi.is_empty() { None } else { Some(ansi) }
}

fn find_style_index(theme: &Theme, tag: &str) -> Option<usize> {
    let mut current = tag.strip_prefix("a-").unwrap_or(tag);
    loop {
        let &idx = TAG_TO_INDEX.get(current)?;
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
    const RAW_WAKER: RawWaker = RawWaker::new(std::ptr::null(), &VTABLE);
    unsafe { Waker::from_raw(RAW_WAKER) }
}
