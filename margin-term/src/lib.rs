//! Terminal rendering for `margin` layout plans.
//!
//! d[impl package.primary-split]

use std::fmt::Write;

use arborium_theme::{Theme as ArboriumTheme, ThemeSlot, slot_to_highlight_index};
use margin::{
    AnnotationRole, Diagnostics, LayoutError, LayoutOptions, LayoutPlan, Note, NoteKind,
    PlacementMode, ResolvedSpan, Severity, SourceWindow, SyntaxClass, WindowAnnotation, plan,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// d[impl test.fixture-first]
/// d[impl test.width-matrix]
/// d[impl test.capability-matrix]
/// d[impl test.unicode]
#[cfg(test)]
mod tests;

/// d[impl glyph.unicode-ascii]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphMode {
    Unicode,
    Ascii,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorLevel {
    None,
    Ansi16,
    Rgb24,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HyperlinkMode {
    None,
    Osc8,
}

/// d[impl api.renderer-options]
/// d[impl term.explicit-capabilities]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCapabilities {
    pub width: usize,
    pub glyph_mode: GlyphMode,
    pub color_level: ColorLevel,
    pub hyperlink_mode: HyperlinkMode,
    pub tab_width: usize,
}

/// d[impl theme.roles]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub severity_error: Style,
    pub severity_warning: Style,
    pub severity_advice: Style,
    pub primary_label: Style,
    pub secondary_label: Style,
    pub syntax_token: Style,
    pub note: Style,
    pub help: Style,
    pub gutter: Style,
    pub connector: Style,
    pub emphasis: Style,
    syntax_styles: [Style; SYNTAX_CLASS_COUNT],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    ansi16: Option<u8>,
    fg_rgb24: Option<Rgb24>,
    bg_rgb24: Option<Rgb24>,
    modifiers: TextModifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rgb24 {
    r: u8,
    g: u8,
    b: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TextModifiers {
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
}

const SYNTAX_CLASS_COUNT: usize = 27;

impl Default for TerminalCapabilities {
    fn default() -> Self {
        Self {
            width: 80,
            glyph_mode: GlyphMode::Unicode,
            color_level: ColorLevel::None,
            hyperlink_mode: HyperlinkMode::None,
            tab_width: 4,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            severity_error: Style::ansi16(31),
            severity_warning: Style::ansi16(33),
            severity_advice: Style::ansi16(36),
            primary_label: Style::ansi16(31),
            secondary_label: Style::ansi16(33),
            syntax_token: Style::ansi16(36),
            note: Style::ansi16(36),
            help: Style::ansi16(32),
            gutter: Style::ansi16(90),
            connector: Style::ansi16(90),
            emphasis: Style::ansi16(35),
            syntax_styles: std::array::from_fn(|index| {
                fallback_syntax_style(syntax_class_from_index(index))
            }),
        }
    }
}

impl Style {
    const fn ansi16(code: u8) -> Self {
        Self {
            ansi16: Some(code),
            fg_rgb24: None,
            bg_rgb24: None,
            modifiers: TextModifiers {
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
            },
        }
    }

    const fn plain() -> Self {
        Self {
            ansi16: None,
            fg_rgb24: None,
            bg_rgb24: None,
            modifiers: TextModifiers {
                bold: false,
                italic: false,
                underline: false,
                strikethrough: false,
            },
        }
    }

    fn from_arborium(theme: &ArboriumTheme, slot: ThemeSlot) -> Option<Self> {
        let index = slot_to_highlight_index(slot)?;
        let style = theme.style(index)?;
        if style.is_empty() {
            return None;
        }

        Some(Self {
            ansi16: None,
            fg_rgb24: style.fg.map(Rgb24::from),
            bg_rgb24: style.bg.map(Rgb24::from),
            modifiers: TextModifiers {
                bold: style.modifiers.bold,
                italic: style.modifiers.italic,
                underline: style.modifiers.underline,
                strikethrough: style.modifiers.strikethrough,
            },
        })
    }

    /// d[impl theme.capability-fallback]
    fn has_effect(self, color_level: ColorLevel) -> bool {
        match color_level {
            ColorLevel::None => false,
            ColorLevel::Ansi16 => {
                self.ansi16.is_some()
                    || self.modifiers.bold
                    || self.modifiers.italic
                    || self.modifiers.underline
                    || self.modifiers.strikethrough
            }
            ColorLevel::Rgb24 => {
                self.ansi16.is_some()
                    || self.fg_rgb24.is_some()
                    || self.bg_rgb24.is_some()
                    || self.modifiers.bold
                    || self.modifiers.italic
                    || self.modifiers.underline
                    || self.modifiers.strikethrough
            }
        }
    }
}

pub fn layout(
    diagnostics: &Diagnostics,
    capabilities: TerminalCapabilities,
) -> Result<LayoutPlan, LayoutError> {
    plan(
        diagnostics,
        &LayoutOptions {
            width: capabilities.width,
            tab_width: capabilities.tab_width,
            ..LayoutOptions::default()
        },
    )
}

pub fn render(
    diagnostics: &Diagnostics,
    capabilities: TerminalCapabilities,
) -> Result<String, LayoutError> {
    let plan = layout(diagnostics, capabilities)?;
    Ok(render_plan(&plan, capabilities))
}

/// d[impl api.layout-render-separation]
pub fn render_plan(plan: &LayoutPlan, capabilities: TerminalCapabilities) -> String {
    render_plan_with_theme(plan, capabilities, Theme::default())
}

pub fn render_plan_with_theme(
    plan: &LayoutPlan,
    capabilities: TerminalCapabilities,
    theme: Theme,
) -> String {
    let mut output = String::new();

    for (report_index, report) in plan.reports.iter().enumerate() {
        if report_index > 0 {
            output.push('\n');
        }

        let _ = writeln!(
            output,
            "{}: {}",
            colorize(
                severity_label(report.severity),
                theme.style_for_severity(report.severity),
                capabilities.color_level
            ),
            report.title
        );

        for window in &report.windows {
            render_window(&mut output, window, capabilities, theme);
        }

        for note in &report.notes {
            render_note(&mut output, note, capabilities, theme);
        }

        for section in &report.sections {
            let _ = writeln!(
                output,
                "{} {}",
                colorize(
                    glyphs(capabilities.glyph_mode).branch,
                    theme.connector,
                    capabilities.color_level
                ),
                section.title
            );
            for note in &section.notes {
                render_note(&mut output, note, capabilities, theme);
            }
        }
    }

    output
}

/// d[impl layout.ellipsis]
fn render_window(
    output: &mut String,
    window: &SourceWindow,
    capabilities: TerminalCapabilities,
    theme: Theme,
) {
    let glyphs = glyphs(capabilities.glyph_mode);
    let source_name = hyperlink_text(
        window.source_name.as_str(),
        window.source_hyperlink.as_deref(),
        capabilities.hyperlink_mode,
    );
    let _ = writeln!(
        output,
        "{} {}",
        colorize(glyphs.source, theme.connector, capabilities.color_level),
        source_name
    );
    if window.omitted_before {
        let _ = writeln!(
            output,
            "{} ...",
            colorize(glyphs.separator, theme.gutter, capabilities.color_level)
        );
    }

    for line in &window.lines {
        let expanded = expand_tabs(&line.text, capabilities.tab_width);
        let clipped = clip_to_width(&expanded, window.geometry.source_columns);
        let styled = style_source_line(
            clipped.as_str(),
            line.line_number,
            &window.annotations,
            capabilities,
            theme,
        );
        let separator = line_separator(&window.annotations, line.line_number, capabilities, theme);
        let omission = if line.clipped { "..." } else { "" };
        let _ = writeln!(
            output,
            "{:>width$} {} {}{}",
            line.line_number,
            separator,
            styled,
            omission,
            width = window.geometry.line_number_width
        );

        for annotation in annotations_for_line(&window.annotations, line.line_number)
            .into_iter()
            .filter(|annotation| annotation.role != AnnotationRole::SyntaxToken)
        {
            render_annotation(output, annotation, window, capabilities, theme);
        }
    }

    if window.omitted_after {
        let _ = writeln!(
            output,
            "{} ...",
            colorize(glyphs.separator, theme.gutter, capabilities.color_level)
        );
    }
}

fn style_source_line(
    text: &str,
    line_number: usize,
    annotations: &[WindowAnnotation],
    capabilities: TerminalCapabilities,
    theme: Theme,
) -> String {
    if capabilities.color_level == ColorLevel::None {
        return text.to_string();
    }

    let syntax_segments = annotations
        .iter()
        .filter(|annotation| annotation.role == AnnotationRole::SyntaxToken)
        .flat_map(|annotation| {
            annotation
                .segments
                .iter()
                .copied()
                .map(move |segment| (segment, annotation.syntax_class))
        })
        .filter(|(segment, _)| segment.line_number == line_number)
        .collect::<Vec<_>>();

    if syntax_segments.is_empty() {
        return text.to_string();
    }

    let mut styled = String::new();
    let mut run = String::new();
    let mut current_style = None;
    let mut column = 0;
    for ch in text.chars() {
        let width = UnicodeWidthChar::width(ch).unwrap_or(0);
        let style = syntax_segments
            .iter()
            .find_map(|(segment, syntax_class)| {
                let overlaps = if width == 0 {
                    segment.start_column <= column && column < segment.end_column
                } else {
                    segment.start_column < column + width && column < segment.end_column
                };
                overlaps.then(|| theme.style_for_syntax_class(*syntax_class))
            })
            .filter(|style| style.has_effect(capabilities.color_level));

        if style != current_style {
            flush_styled_run(
                &mut styled,
                &mut run,
                current_style,
                capabilities.color_level,
            );
            current_style = style;
        }
        run.push(ch);

        column += width;
    }
    flush_styled_run(
        &mut styled,
        &mut run,
        current_style,
        capabilities.color_level,
    );

    styled
}

fn flush_styled_run(
    output: &mut String,
    run: &mut String,
    style: Option<Style>,
    color_level: ColorLevel,
) {
    if run.is_empty() {
        return;
    }

    match style {
        Some(style) => output.push_str(&colorize(run.as_str(), style, color_level)),
        None => output.push_str(run),
    }
    run.clear();
}

fn hyperlink_text(text: &str, target: Option<&str>, mode: HyperlinkMode) -> String {
    match (mode, target) {
        (HyperlinkMode::Osc8, Some(target)) => {
            format!("\u{1b}]8;;{target}\u{1b}\\{text}\u{1b}]8;;\u{1b}\\")
        }
        _ => text.to_string(),
    }
}

/// d[impl layout.notes-wrap]
fn render_note(output: &mut String, note: &Note, capabilities: TerminalCapabilities, theme: Theme) {
    let prefix = match note.kind {
        NoteKind::Note => "note",
        NoteKind::Help => "help",
    };
    let indent = format!("  = {prefix}: ");
    let wrapped = wrap_text(
        &note.text,
        capabilities.width.saturating_sub(indent_width(&indent)),
    );

    for (index, line) in wrapped.iter().enumerate() {
        if index == 0 {
            let styled_indent = colorize(
                indent.as_str(),
                theme.style_for_note_kind(note.kind),
                capabilities.color_level,
            );
            let _ = writeln!(output, "{styled_indent}{line}");
        } else {
            let _ = writeln!(output, "{}{line}", " ".repeat(indent_width(&indent)));
        }
    }
}

/// d[impl layout.multiline-labels]
/// d[impl label.multiline-message-alignment]
fn render_annotation(
    output: &mut String,
    annotation: RenderableAnnotation<'_>,
    window: &SourceWindow,
    capabilities: TerminalCapabilities,
    theme: Theme,
) {
    let glyphs = glyphs(capabilities.glyph_mode);
    let annotation_style = theme.style_for_annotation_role(annotation.role, annotation.severity);
    if annotation.block {
        if let Some(message) = annotation.message {
            let gutter = format!(
                "{:>width$} {} ",
                "",
                colorize(glyphs.block, annotation_style, capabilities.color_level),
                width = window.geometry.line_number_width
            );
            let continuation_padding = " ".repeat(indent_width(glyphs.branch) + 1);
            let available = window
                .geometry
                .source_columns
                .saturating_sub(indent_width(glyphs.branch) + 1);
            for (index, line) in wrap_text(message, available).into_iter().enumerate() {
                let _ = writeln!(
                    output,
                    "{gutter}{}{line}",
                    if index == 0 {
                        format!(
                            "{} ",
                            colorize(glyphs.branch, annotation_style, capabilities.color_level)
                        )
                    } else {
                        continuation_padding.clone()
                    }
                );
            }
        }
        return;
    }

    let marker_line = marker_line(
        annotation.segment,
        annotation.placement,
        capabilities.glyph_mode,
    );
    let colored = colorize(
        marker_line.as_str(),
        annotation_style,
        capabilities.color_level,
    );
    let gutter = format!(
        "{:>width$} {} ",
        "",
        colorize(glyphs.separator, theme.gutter, capabilities.color_level),
        width = window.geometry.line_number_width
    );

    match annotation.placement {
        PlacementMode::Side => {
            let message = annotation
                .message
                .map(|message| format!(" {}", message))
                .unwrap_or_default();
            let _ = writeln!(output, "{gutter}{colored}{message}");
        }
        PlacementMode::BelowSpan => {
            let _ = writeln!(output, "{gutter}{colored}");
            if let Some(message) = annotation.message {
                let anchor_padding = " ".repeat(annotation.segment.start_column);
                let continuation_padding = " ".repeat(indent_width(glyphs.branch) + 1);
                let available = window
                    .geometry
                    .source_columns
                    .saturating_sub(annotation.segment.start_column + 2);
                for (index, line) in wrap_text(message, available).into_iter().enumerate() {
                    let _ = writeln!(
                        output,
                        "{gutter}{anchor_padding}{}{line}",
                        if index == 0 {
                            format!(
                                "{} ",
                                colorize(glyphs.branch, annotation_style, capabilities.color_level)
                            )
                        } else {
                            continuation_padding.clone()
                        }
                    );
                }
            }
        }
        PlacementMode::Stacked => {
            let _ = writeln!(output, "{gutter}{colored}");
            if let Some(message) = annotation.message {
                for line in wrap_text(message, window.geometry.source_columns.saturating_sub(4)) {
                    let _ = writeln!(
                        output,
                        "{gutter}{} = {line}",
                        colorize(glyphs.branch, annotation_style, capabilities.color_level)
                    );
                }
            }
        }
    }
}

fn annotations_for_line<'a>(
    annotations: &'a [WindowAnnotation],
    line_number: usize,
) -> Vec<RenderableAnnotation<'a>> {
    let mut renderable = annotations
        .iter()
        .flat_map(|annotation| {
            let block = is_gutter_block_annotation(annotation);
            let message_owner = if block {
                annotation.segments.last().copied()
            } else {
                annotation.segments.first().copied()
            };
            annotation
                .segments
                .iter()
                .copied()
                .filter(move |segment| {
                    if block {
                        message_owner == Some(*segment) && segment.line_number == line_number
                    } else {
                        segment.line_number == line_number
                    }
                })
                .map(move |segment| RenderableAnnotation {
                    block,
                    message: (message_owner == Some(segment))
                        .then_some(annotation.message.as_deref())
                        .flatten(),
                    placement: annotation.placement,
                    role: annotation.role,
                    severity: severity_for_role(annotation.role),
                    priority: annotation.priority,
                    segment,
                })
        })
        .collect::<Vec<_>>();
    renderable.sort_by_key(|item| std::cmp::Reverse(item.priority));
    renderable
}

fn line_separator(
    annotations: &[WindowAnnotation],
    line_number: usize,
    capabilities: TerminalCapabilities,
    theme: Theme,
) -> String {
    let glyphs = glyphs(capabilities.glyph_mode);
    if let Some(annotation) = annotations
        .iter()
        .filter(|annotation| is_gutter_block_annotation(annotation))
        .filter(|annotation| {
            annotation
                .segments
                .iter()
                .any(|segment| segment.line_number == line_number)
        })
        .max_by_key(|annotation| annotation.priority)
    {
        let style =
            theme.style_for_annotation_role(annotation.role, severity_for_role(annotation.role));
        colorize(glyphs.block, style, capabilities.color_level)
    } else {
        colorize(glyphs.separator, theme.gutter, capabilities.color_level)
    }
}

fn is_gutter_block_annotation(annotation: &WindowAnnotation) -> bool {
    annotation.message.is_some()
        && annotation.role != AnnotationRole::SyntaxToken
        && annotation.segments.len() >= 3
}

fn marker_line(span: ResolvedSpan, placement: PlacementMode, glyph_mode: GlyphMode) -> String {
    let (start, fill) = match (placement, glyph_mode) {
        (PlacementMode::BelowSpan, GlyphMode::Unicode) => ('┬', '─'),
        (_, GlyphMode::Unicode) => ('─', '─'),
        (_, GlyphMode::Ascii) => ('^', '^'),
    };
    let width = span.end_column.saturating_sub(span.start_column).max(1);
    let mut marker = String::with_capacity(span.start_column + width);
    marker.push_str(&" ".repeat(span.start_column));
    marker.push(start);
    if width > 1 {
        marker.push_str(&fill.to_string().repeat(width - 1));
    }
    marker
}

fn expand_tabs(text: &str, tab_width: usize) -> String {
    let mut expanded = String::new();
    let tab_width = tab_width.max(1);
    let mut col = 0;

    for ch in text.chars() {
        if ch == '\t' {
            let spaces = tab_width - (col % tab_width);
            expanded.push_str(&" ".repeat(spaces));
            col += spaces;
        } else {
            expanded.push(ch);
            col += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }

    expanded
}

fn clip_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let mut clipped = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }
        clipped.push(ch);
        used += ch_width;
    }
    clipped
}

/// d[impl layout.no-terminal-wrap]
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }

    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        let mut saw_content = false;
        for word in paragraph.split_whitespace() {
            saw_content = true;
            push_wrapped_word(&mut lines, &mut current, word, width);
        }
        if current.is_empty() && !saw_content {
            lines.push(String::new());
        } else if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

fn indent_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn push_wrapped_word(lines: &mut Vec<String>, current: &mut String, word: &str, width: usize) {
    let mut remaining = word;

    loop {
        let separator = if current.is_empty() { "" } else { " " };
        let candidate = format!("{current}{separator}{remaining}");
        if UnicodeWidthStr::width(candidate.as_str()) <= width {
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(remaining);
            return;
        }

        if !current.is_empty() {
            lines.push(std::mem::take(current));
            continue;
        }

        let (head, tail) = split_long_token(remaining, width);
        lines.push(head.to_string());
        if tail.is_empty() {
            return;
        }
        remaining = tail;
    }
}

fn split_long_token(token: &str, width: usize) -> (&str, &str) {
    let mut used = 0;
    let mut furthest_end = 0;
    let mut preferred_end = None;

    for (index, ch) in token.char_indices() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_width > width {
            break;
        }

        used += ch_width;
        furthest_end = index + ch.len_utf8();
        if is_path_wrap_boundary(ch) {
            preferred_end = Some(furthest_end);
        }
    }

    let split_at = preferred_end.or_else(|| {
        if furthest_end > 0 {
            Some(furthest_end)
        } else {
            token.chars().next().map(char::len_utf8)
        }
    });
    let split_at = split_at.unwrap_or(0);
    (&token[..split_at], &token[split_at..])
}

fn is_path_wrap_boundary(ch: char) -> bool {
    matches!(ch, '/' | '\\' | ':' | '.' | '-' | '_')
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Advice => "advice",
    }
}

fn severity_for_role(role: AnnotationRole) -> Severity {
    match role {
        AnnotationRole::PrimaryLabel => Severity::Error,
        AnnotationRole::SecondaryLabel => Severity::Warning,
        AnnotationRole::RelatedLabel
        | AnnotationRole::SyntaxToken
        | AnnotationRole::SearchHighlight
        | AnnotationRole::Selection
        | AnnotationRole::Emphasis => Severity::Advice,
    }
}

/// d[impl term.ansi-discipline]
/// d[impl term.plaintext-mode]
fn colorize(text: &str, style: Style, color_level: ColorLevel) -> String {
    if color_level == ColorLevel::None {
        return text.to_string();
    }

    let mut codes = Vec::<String>::new();
    if style.modifiers.bold {
        codes.push("1".to_string());
    }
    if style.modifiers.italic {
        codes.push("3".to_string());
    }
    if style.modifiers.underline {
        codes.push("4".to_string());
    }
    if style.modifiers.strikethrough {
        codes.push("9".to_string());
    }

    match color_level {
        ColorLevel::None => {}
        ColorLevel::Ansi16 => {
            if let Some(code) = style.ansi16 {
                codes.push(code.to_string());
            }
        }
        ColorLevel::Rgb24 => {
            if let Some(fg) = style.fg_rgb24 {
                codes.push(format!("38;2;{};{};{}", fg.r, fg.g, fg.b));
            } else if let Some(code) = style.ansi16 {
                codes.push(code.to_string());
            }
            if let Some(bg) = style.bg_rgb24 {
                codes.push(format!("48;2;{};{};{}", bg.r, bg.g, bg.b));
            }
        }
    }

    if codes.is_empty() {
        return text.to_string();
    }
    format!("\u{1b}[{}m{text}\u{1b}[0m", codes.join(";"))
}

fn glyphs(mode: GlyphMode) -> Glyphs {
    match mode {
        GlyphMode::Unicode => Glyphs {
            source: "╭─>",
            separator: "│",
            branch: "╰─",
            block: "▌",
        },
        GlyphMode::Ascii => Glyphs {
            source: "-->",
            separator: "|",
            branch: "\\-",
            block: "|",
        },
    }
}

#[derive(Debug, Clone, Copy)]
struct Glyphs {
    source: &'static str,
    separator: &'static str,
    branch: &'static str,
    block: &'static str,
}

#[derive(Debug, Clone, Copy)]
struct RenderableAnnotation<'a> {
    block: bool,
    message: Option<&'a str>,
    placement: PlacementMode,
    role: AnnotationRole,
    severity: Severity,
    priority: u16,
    segment: ResolvedSpan,
}

impl Theme {
    pub fn from_arborium(theme: &ArboriumTheme) -> Self {
        let default = Self::default();

        Self {
            severity_error: Style::from_arborium(theme, ThemeSlot::Error)
                .unwrap_or(default.severity_error),
            severity_warning: Style::from_arborium(theme, ThemeSlot::Type)
                .unwrap_or(default.secondary_label),
            severity_advice: Style::from_arborium(theme, ThemeSlot::Function)
                .unwrap_or(default.severity_advice),
            primary_label: Style::from_arborium(theme, ThemeSlot::Error)
                .unwrap_or(default.primary_label),
            secondary_label: Style::from_arborium(theme, ThemeSlot::Type)
                .unwrap_or(default.secondary_label),
            syntax_token: Style::from_arborium(theme, ThemeSlot::Function)
                .unwrap_or(default.syntax_token),
            note: Style::from_arborium(theme, ThemeSlot::Comment).unwrap_or(default.note),
            help: Style::from_arborium(theme, ThemeSlot::String).unwrap_or(default.help),
            gutter: Style::from_arborium(theme, ThemeSlot::Comment).unwrap_or(default.gutter),
            connector: Style::from_arborium(theme, ThemeSlot::Comment).unwrap_or(default.connector),
            emphasis: Style::from_arborium(theme, ThemeSlot::Keyword).unwrap_or(default.emphasis),
            syntax_styles: std::array::from_fn(|index| {
                let class = syntax_class_from_index(index);
                Style::from_arborium(theme, theme_slot_for_syntax_class(class))
                    .unwrap_or(fallback_syntax_style(class))
            }),
        }
    }

    fn style_for_severity(self, severity: Severity) -> Style {
        match severity {
            Severity::Error => self.severity_error,
            Severity::Warning => self.severity_warning,
            Severity::Advice => self.severity_advice,
        }
    }

    fn style_for_note_kind(self, kind: NoteKind) -> Style {
        match kind {
            NoteKind::Note => self.note,
            NoteKind::Help => self.help,
        }
    }

    fn style_for_annotation_role(self, role: AnnotationRole, severity: Severity) -> Style {
        match role {
            AnnotationRole::PrimaryLabel => self.primary_label,
            AnnotationRole::SecondaryLabel => self.secondary_label,
            AnnotationRole::SyntaxToken => self.syntax_token,
            AnnotationRole::Emphasis => self.emphasis,
            AnnotationRole::RelatedLabel
            | AnnotationRole::SearchHighlight
            | AnnotationRole::Selection => self.style_for_severity(severity),
        }
    }

    fn style_for_syntax_class(self, syntax_class: Option<SyntaxClass>) -> Style {
        syntax_class
            .map(|class| self.syntax_styles[syntax_class_index(class)])
            .unwrap_or(Style::plain())
    }
}

impl From<arborium_theme::Color> for Rgb24 {
    fn from(value: arborium_theme::Color) -> Self {
        Self {
            r: value.r,
            g: value.g,
            b: value.b,
        }
    }
}

fn fallback_syntax_style(class: SyntaxClass) -> Style {
    match class {
        SyntaxClass::Keyword
        | SyntaxClass::Operator
        | SyntaxClass::Macro
        | SyntaxClass::Namespace
        | SyntaxClass::Tag
        | SyntaxClass::Title
        | SyntaxClass::Strong
        | SyntaxClass::Emphasis
        | SyntaxClass::Link
        | SyntaxClass::Literal
        | SyntaxClass::Strikethrough => Style::ansi16(35),
        SyntaxClass::Function | SyntaxClass::Constructor => Style::ansi16(36),
        SyntaxClass::String | SyntaxClass::DiffAdd => Style::ansi16(32),
        SyntaxClass::Comment | SyntaxClass::Punctuation => Style::ansi16(90),
        SyntaxClass::Type | SyntaxClass::Attribute => Style::ansi16(33),
        SyntaxClass::Constant
        | SyntaxClass::Number
        | SyntaxClass::Property
        | SyntaxClass::Label
        | SyntaxClass::Embedded => Style::ansi16(36),
        SyntaxClass::DiffDelete | SyntaxClass::Error => Style::ansi16(31),
        SyntaxClass::Variable => Style::plain(),
    }
}

const fn syntax_class_index(class: SyntaxClass) -> usize {
    match class {
        SyntaxClass::Keyword => 0,
        SyntaxClass::Function => 1,
        SyntaxClass::String => 2,
        SyntaxClass::Comment => 3,
        SyntaxClass::Type => 4,
        SyntaxClass::Variable => 5,
        SyntaxClass::Constant => 6,
        SyntaxClass::Number => 7,
        SyntaxClass::Operator => 8,
        SyntaxClass::Punctuation => 9,
        SyntaxClass::Property => 10,
        SyntaxClass::Attribute => 11,
        SyntaxClass::Tag => 12,
        SyntaxClass::Macro => 13,
        SyntaxClass::Label => 14,
        SyntaxClass::Namespace => 15,
        SyntaxClass::Constructor => 16,
        SyntaxClass::Title => 17,
        SyntaxClass::Strong => 18,
        SyntaxClass::Emphasis => 19,
        SyntaxClass::Link => 20,
        SyntaxClass::Literal => 21,
        SyntaxClass::Strikethrough => 22,
        SyntaxClass::DiffAdd => 23,
        SyntaxClass::DiffDelete => 24,
        SyntaxClass::Embedded => 25,
        SyntaxClass::Error => 26,
    }
}

const fn syntax_class_from_index(index: usize) -> SyntaxClass {
    match index {
        0 => SyntaxClass::Keyword,
        1 => SyntaxClass::Function,
        2 => SyntaxClass::String,
        3 => SyntaxClass::Comment,
        4 => SyntaxClass::Type,
        5 => SyntaxClass::Variable,
        6 => SyntaxClass::Constant,
        7 => SyntaxClass::Number,
        8 => SyntaxClass::Operator,
        9 => SyntaxClass::Punctuation,
        10 => SyntaxClass::Property,
        11 => SyntaxClass::Attribute,
        12 => SyntaxClass::Tag,
        13 => SyntaxClass::Macro,
        14 => SyntaxClass::Label,
        15 => SyntaxClass::Namespace,
        16 => SyntaxClass::Constructor,
        17 => SyntaxClass::Title,
        18 => SyntaxClass::Strong,
        19 => SyntaxClass::Emphasis,
        20 => SyntaxClass::Link,
        21 => SyntaxClass::Literal,
        22 => SyntaxClass::Strikethrough,
        23 => SyntaxClass::DiffAdd,
        24 => SyntaxClass::DiffDelete,
        25 => SyntaxClass::Embedded,
        _ => SyntaxClass::Error,
    }
}

const fn theme_slot_for_syntax_class(class: SyntaxClass) -> ThemeSlot {
    match class {
        SyntaxClass::Keyword => ThemeSlot::Keyword,
        SyntaxClass::Function => ThemeSlot::Function,
        SyntaxClass::String => ThemeSlot::String,
        SyntaxClass::Comment => ThemeSlot::Comment,
        SyntaxClass::Type => ThemeSlot::Type,
        SyntaxClass::Variable => ThemeSlot::Variable,
        SyntaxClass::Constant => ThemeSlot::Constant,
        SyntaxClass::Number => ThemeSlot::Number,
        SyntaxClass::Operator => ThemeSlot::Operator,
        SyntaxClass::Punctuation => ThemeSlot::Punctuation,
        SyntaxClass::Property => ThemeSlot::Property,
        SyntaxClass::Attribute => ThemeSlot::Attribute,
        SyntaxClass::Tag => ThemeSlot::Tag,
        SyntaxClass::Macro => ThemeSlot::Macro,
        SyntaxClass::Label => ThemeSlot::Label,
        SyntaxClass::Namespace => ThemeSlot::Namespace,
        SyntaxClass::Constructor => ThemeSlot::Constructor,
        SyntaxClass::Title => ThemeSlot::Title,
        SyntaxClass::Strong => ThemeSlot::Strong,
        SyntaxClass::Emphasis => ThemeSlot::Emphasis,
        SyntaxClass::Link => ThemeSlot::Link,
        SyntaxClass::Literal => ThemeSlot::Literal,
        SyntaxClass::Strikethrough => ThemeSlot::Strikethrough,
        SyntaxClass::DiffAdd => ThemeSlot::DiffAdd,
        SyntaxClass::DiffDelete => ThemeSlot::DiffDelete,
        SyntaxClass::Embedded => ThemeSlot::Embedded,
        SyntaxClass::Error => ThemeSlot::Error,
    }
}
