//! Structured diagnostics core and layout planning.
//!
//! The core crate intentionally owns the reusable model and the explicit layout
//! artifact, leaving terminal emission to `margin-term`.
//!
//! d[impl package.name]
//! d[impl package.no-vixen-prefix]
//! d[impl package.primary-split]
//! d[impl package.additional-renderers]
//! d[impl product.compiler-grade]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Range;

use facet::Facet;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// d[impl api.stable-fixtures]
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Severity {
    Error,
    Warning,
    Advice,
}

/// d[impl model.source]
/// d[impl input.source-identity]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct SourceId(pub String);

impl From<&str> for SourceId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// d[impl unicode.normalization-stability]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Source {
    pub id: SourceId,
    pub name: String,
    pub hyperlink: Option<String>,
    pub text: String,
}

/// d[impl input.unified-spans]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Span {
    pub source_id: SourceId,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(source_id: impl Into<SourceId>, start: usize, end: usize) -> Self {
        Self {
            source_id: source_id.into(),
            start,
            end,
        }
    }

    /// d[impl input.span-bounds]
    /// d[impl unicode.grapheme-safety]
    fn validate(&self, source: &Source) -> Result<(), LayoutError> {
        if self.start > self.end {
            return Err(LayoutError::InvalidSpan {
                source: source.id.0.clone(),
                start: self.start,
                end: self.end,
                reason: "start is greater than end",
            });
        }
        if self.end > source.text.len() {
            return Err(LayoutError::InvalidSpan {
                source: source.id.0.clone(),
                start: self.start,
                end: self.end,
                reason: "end is out of bounds",
            });
        }
        if !source.text.is_char_boundary(self.start) || !source.text.is_char_boundary(self.end) {
            return Err(LayoutError::InvalidSpan {
                source: source.id.0.clone(),
                start: self.start,
                end: self.end,
                reason: "span does not align to UTF-8 boundaries",
            });
        }
        Ok(())
    }
}

/// d[impl model.role-stable]
/// d[impl input.overlay-kinds]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Facet)]
#[repr(u8)]
pub enum AnnotationRole {
    PrimaryLabel,
    SecondaryLabel,
    RelatedLabel,
    SyntaxToken,
    SearchHighlight,
    Selection,
    Emphasis,
}

/// d[impl syntax.overlay-model]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Facet)]
#[repr(u8)]
pub enum SyntaxClass {
    Keyword,
    Function,
    String,
    Comment,
    Type,
    Variable,
    Constant,
    Number,
    Operator,
    Punctuation,
    Property,
    Attribute,
    Tag,
    Macro,
    Label,
    Namespace,
    Constructor,
    Title,
    Strong,
    Emphasis,
    Link,
    Literal,
    Strikethrough,
    DiffAdd,
    DiffDelete,
    Embedded,
    Error,
}

/// d[impl model.annotation]
/// d[impl label.multispan]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Annotation {
    pub spans: Vec<Span>,
    pub role: AnnotationRole,
    pub syntax_class: Option<SyntaxClass>,
    pub message: Option<String>,
    pub priority: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum NoteKind {
    Note,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Note {
    pub kind: NoteKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct ReportSection {
    pub title: String,
    pub notes: Vec<Note>,
}

/// d[impl api.core-model]
/// d[impl model.report]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Report {
    pub severity: Severity,
    pub title: String,
    pub annotations: Vec<Annotation>,
    pub notes: Vec<Note>,
    pub sections: Vec<ReportSection>,
}

/// d[impl input.structured]
/// d[impl input.multiple-sources]
#[derive(Debug, Clone, Default, PartialEq, Eq, Facet)]
pub struct Diagnostics {
    pub sources: Vec<Source>,
    pub reports: Vec<Report>,
}

/// d[impl layout.width-aware]
/// d[impl unicode.tab-policy]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct LayoutOptions {
    pub width: usize,
    pub primary_context_lines: usize,
    pub secondary_context_lines: usize,
    pub merge_distance_lines: usize,
    pub tab_width: usize,
    pub long_line_mode: LongLineMode,
}

impl LayoutOptions {
    pub fn with_width(width: usize) -> Self {
        Self {
            width,
            ..Self::default()
        }
    }
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            width: 80,
            primary_context_lines: 2,
            secondary_context_lines: 1,
            merge_distance_lines: 1,
            tab_width: 4,
            long_line_mode: LongLineMode::Clip,
        }
    }
}

/// d[impl layout.long-lines]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum LongLineMode {
    Clip,
}

/// d[impl api.explicit-layout-artifact]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct LayoutPlan {
    pub width: usize,
    pub reports: Vec<PlannedReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct PlannedReport {
    pub severity: Severity,
    pub title: String,
    pub windows: Vec<SourceWindow>,
    pub notes: Vec<Note>,
    pub sections: Vec<ReportSection>,
}

/// d[impl model.window]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct SourceWindow {
    pub source_id: SourceId,
    pub source_name: String,
    pub source_hyperlink: Option<String>,
    pub geometry: GutterGeometry,
    pub first_line_number: usize,
    pub last_line_number: usize,
    pub omitted_before: bool,
    pub omitted_after: bool,
    pub lines: Vec<WindowLine>,
    pub annotations: Vec<WindowAnnotation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct WindowLine {
    pub line_number: usize,
    pub text: String,
    pub clipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct WindowAnnotation {
    pub role: AnnotationRole,
    pub syntax_class: Option<SyntaxClass>,
    pub message: Option<String>,
    pub placement: PlacementMode,
    pub priority: u16,
    pub segments: Vec<ResolvedSpan>,
}

/// d[impl layout.placement-modes]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum PlacementMode {
    Side,
    BelowSpan,
    Stacked,
}

/// d[impl layout.gutter-geometry]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct GutterGeometry {
    pub line_number_width: usize,
    pub separator_columns: usize,
    pub connector_columns: usize,
    pub source_columns: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct ResolvedSpan {
    pub line_number: usize,
    pub start_column: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    DuplicateSourceId(String),
    MissingSource(String),
    InvalidSpan {
        source: String,
        start: usize,
        end: usize,
        reason: &'static str,
    },
}

impl Display for LayoutError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::DuplicateSourceId(source) => {
                write!(f, "duplicate source id in diagnostics: {source}")
            }
            LayoutError::MissingSource(source) => {
                write!(f, "report annotation references unknown source: {source}")
            }
            LayoutError::InvalidSpan {
                source,
                start,
                end,
                reason,
            } => write!(f, "invalid span {start}..{end} for {source}: {reason}"),
        }
    }
}

impl Error for LayoutError {}

/// d[impl api.layout-render-separation]
/// d[impl layout.pipeline]
/// d[impl layout.pure-planning]
pub fn plan(diagnostics: &Diagnostics, options: &LayoutOptions) -> Result<LayoutPlan, LayoutError> {
    let sources = index_sources(&diagnostics.sources)?;
    let reports = diagnostics
        .reports
        .iter()
        .map(|report| plan_report(report, &sources, options))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(LayoutPlan {
        width: options.width,
        reports,
    })
}

fn index_sources(sources: &[Source]) -> Result<BTreeMap<&SourceId, &Source>, LayoutError> {
    let mut indexed = BTreeMap::new();
    for source in sources {
        if indexed.insert(&source.id, source).is_some() {
            return Err(LayoutError::DuplicateSourceId(source.id.0.clone()));
        }
    }
    Ok(indexed)
}

fn plan_report(
    report: &Report,
    sources: &BTreeMap<&SourceId, &Source>,
    options: &LayoutOptions,
) -> Result<PlannedReport, LayoutError> {
    let grouped = group_annotations_by_source(&report.annotations, sources, options)?;
    let windows = grouped
        .into_iter()
        .map(|(source_id, source_annotations)| {
            build_windows(source_id, source_annotations, sources, options)
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(PlannedReport {
        severity: report.severity,
        title: report.title.clone(),
        windows,
        notes: report.notes.clone(),
        sections: report.sections.clone(),
    })
}

/// d[impl window.cross-file]
fn group_annotations_by_source<'a>(
    annotations: &'a [Annotation],
    sources: &BTreeMap<&SourceId, &Source>,
    options: &LayoutOptions,
) -> Result<BTreeMap<&'a SourceId, Vec<PendingAnnotation<'a>>>, LayoutError> {
    let mut grouped = BTreeMap::new();

    for annotation in annotations {
        for span in &annotation.spans {
            let source_id = &span.source_id;
            let source = sources
                .get(source_id)
                .copied()
                .ok_or_else(|| LayoutError::MissingSource(source_id.0.clone()))?;
            span.validate(source)?;
            let context = annotation_context(annotation.role, options);
            let segments = resolve_span(source, span.clone(), options.tab_width)?;
            let line_range = segments
                .iter()
                .map(|segment| segment.line_number)
                .fold(None::<(usize, usize)>, |acc, line| match acc {
                    Some((start, end)) => Some((start.min(line), end.max(line))),
                    None => Some((line, line)),
                })
                .unwrap_or((1, 1));

            grouped
                .entry(source_id)
                .or_insert_with(Vec::new)
                .push(PendingAnnotation {
                    annotation,
                    line_start: line_range.0.saturating_sub(context),
                    line_end: line_range.1 + context,
                    segments,
                });
        }
    }

    Ok(grouped)
}

/// d[impl window.primary-context]
/// d[impl window.context-policy]
fn annotation_context(role: AnnotationRole, options: &LayoutOptions) -> usize {
    match role {
        AnnotationRole::PrimaryLabel => options.primary_context_lines,
        _ => options.secondary_context_lines,
    }
}

/// d[impl syntax.non-owning]
/// d[impl syntax.window-bounded]
fn build_windows(
    source_id: &SourceId,
    annotations: Vec<PendingAnnotation<'_>>,
    sources: &BTreeMap<&SourceId, &Source>,
    options: &LayoutOptions,
) -> Result<Vec<SourceWindow>, LayoutError> {
    let source = sources
        .get(source_id)
        .copied()
        .ok_or_else(|| LayoutError::MissingSource(source_id.0.clone()))?;
    let source_lines = split_lines(&source.text);
    let driving_annotations = annotations
        .iter()
        .filter(|annotation| annotation.annotation.role != AnnotationRole::SyntaxToken)
        .collect::<Vec<_>>();
    let ranges = merge_line_ranges(
        if driving_annotations.is_empty() {
            annotations.iter().collect()
        } else {
            driving_annotations
        }
        .into_iter()
        .map(|annotation| (annotation.line_start.max(1), annotation.line_end.max(1)))
        .collect(),
        options.merge_distance_lines,
    );

    ranges
        .into_iter()
        .map(|(line_start, line_end)| {
            let last_line_number = line_end.min(source_lines.len());
            let geometry = compute_geometry(last_line_number, options.width);
            let lines = source_lines
                .iter()
                .enumerate()
                .filter(|(index, _)| {
                    let line_number = index + 1;
                    (line_start..=last_line_number).contains(&line_number)
                })
                .map(|(index, line)| WindowLine {
                    line_number: index + 1,
                    text: (*line).to_string(),
                    clipped: is_line_clipped(
                        line,
                        &geometry,
                        options.long_line_mode,
                        options.tab_width,
                    ),
                })
                .collect::<Vec<_>>();

            let annotations =
                planned_window_annotations(&annotations, line_start, last_line_number, &geometry);

            Ok(SourceWindow {
                source_id: source.id.clone(),
                source_name: source.name.clone(),
                source_hyperlink: source.hyperlink.clone(),
                geometry,
                first_line_number: line_start,
                last_line_number,
                omitted_before: line_start > 1,
                omitted_after: last_line_number < source_lines.len(),
                lines,
                annotations,
            })
        })
        .collect()
}

/// d[impl window.merge-nearby]
/// d[impl window.stable-selection]
/// d[impl coalesce.report-sections]
fn merge_line_ranges(
    mut ranges: Vec<(usize, usize)>,
    merge_distance_lines: usize,
) -> Vec<(usize, usize)> {
    ranges.sort_unstable();

    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some((_, current_end)) = merged.last_mut()
            && start <= *current_end + merge_distance_lines + 1
        {
            *current_end = (*current_end).max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}

fn planned_window_annotations(
    annotations: &[PendingAnnotation<'_>],
    line_start: usize,
    last_line_number: usize,
    geometry: &GutterGeometry,
) -> Vec<WindowAnnotation> {
    let mut crowded_lines = BTreeMap::<usize, usize>::new();
    let mut visible = annotations
        .iter()
        .filter_map(|annotation| {
            let segments = annotation
                .segments
                .iter()
                .copied()
                .filter(|segment| (line_start..=last_line_number).contains(&segment.line_number))
                .collect::<Vec<_>>();
            if segments.is_empty() {
                return None;
            }
            Some(WindowAnnotation {
                role: annotation.annotation.role,
                syntax_class: annotation.annotation.syntax_class,
                message: annotation.annotation.message.clone(),
                placement: PlacementMode::BelowSpan,
                priority: annotation.annotation.priority,
                segments,
            })
        })
        .collect::<Vec<_>>();

    visible.sort_by(window_annotation_sort_key);
    visible = coalesce_window_annotations(visible);
    visible = apply_priority_lattice(visible);
    visible.sort_by(window_annotation_sort_key);

    for annotation in &mut visible {
        annotation.segments.sort_by(resolved_span_sort_key);
        annotation.placement = choose_placement(annotation, &mut crowded_lines, geometry);
    }

    visible
}

fn window_annotation_sort_key(
    left: &WindowAnnotation,
    right: &WindowAnnotation,
) -> std::cmp::Ordering {
    right
        .priority
        .cmp(&left.priority)
        .then_with(|| role_rank(left.role).cmp(&role_rank(right.role)))
        .then_with(|| left.syntax_class.cmp(&right.syntax_class))
        .then_with(|| left.message.cmp(&right.message))
        .then_with(|| resolved_span_sort_key(&left.segments[0], &right.segments[0]))
}

fn resolved_span_sort_key(left: &ResolvedSpan, right: &ResolvedSpan) -> std::cmp::Ordering {
    left.line_number
        .cmp(&right.line_number)
        .then_with(|| left.start_column.cmp(&right.start_column))
        .then_with(|| left.end_column.cmp(&right.end_column))
}

/// d[impl label.clustered]
/// d[impl label.single-message-bearing]
/// d[impl coalesce.deterministic]
/// d[impl coalesce.identical]
fn coalesce_window_annotations(annotations: Vec<WindowAnnotation>) -> Vec<WindowAnnotation> {
    let mut merged: Vec<WindowAnnotation> = Vec::new();

    for mut annotation in annotations {
        annotation.segments.sort_by(resolved_span_sort_key);
        annotation.segments.dedup();

        if let Some(existing) = merged
            .iter_mut()
            .find(|existing| can_cluster(existing, &annotation))
        {
            existing
                .segments
                .extend(annotation.segments.iter().copied());
            existing.segments.sort_by(resolved_span_sort_key);
            existing.segments.dedup();
            if existing.message.is_none() {
                existing.message = annotation.message.clone();
            }
            continue;
        }

        merged.push(annotation);
    }

    merged
}

/// d[impl coalesce.nearby]
/// d[impl coalesce.message-bearing]
fn can_cluster(left: &WindowAnnotation, right: &WindowAnnotation) -> bool {
    if left.role != right.role
        || left.syntax_class != right.syntax_class
        || left.priority != right.priority
    {
        return false;
    }
    if !messages_compatible(left.message.as_deref(), right.message.as_deref()) {
        return false;
    }

    let left_anchor = left.segments[0];
    let right_anchor = right.segments[0];

    left_anchor.line_number.abs_diff(right_anchor.line_number) <= 1
        && left_anchor.start_column.abs_diff(right_anchor.start_column) <= 8
}

fn messages_compatible(left: Option<&str>, right: Option<&str>) -> bool {
    left.is_none() || right.is_none() || left == right
}

/// d[impl label.priority]
/// d[impl label.priority-lattice]
/// d[impl label.semantic-over-syntax]
/// d[impl syntax.overlay-priority]
fn apply_priority_lattice(annotations: Vec<WindowAnnotation>) -> Vec<WindowAnnotation> {
    let mut accepted = Vec::<WindowAnnotation>::new();
    let mut occupied = Vec::<(ResolvedSpan, u16, u8)>::new();

    for mut annotation in annotations {
        let rank = role_rank(annotation.role);
        annotation.segments.retain(|segment| {
            !occupied
                .iter()
                .any(|(accepted_segment, priority, accepted_rank)| {
                    overlaps(*segment, *accepted_segment)
                        && (*priority > annotation.priority
                            || (*priority == annotation.priority && *accepted_rank <= rank))
                })
        });

        if annotation.segments.is_empty() {
            continue;
        }

        for segment in &annotation.segments {
            occupied.push((*segment, annotation.priority, rank));
        }
        accepted.push(annotation);
    }

    accepted
}

fn overlaps(left: ResolvedSpan, right: ResolvedSpan) -> bool {
    left.line_number == right.line_number
        && left.start_column < right.end_column
        && right.start_column < left.end_column
}

fn role_rank(role: AnnotationRole) -> u8 {
    match role {
        AnnotationRole::PrimaryLabel => 0,
        AnnotationRole::SecondaryLabel => 1,
        AnnotationRole::RelatedLabel => 2,
        AnnotationRole::Emphasis => 3,
        AnnotationRole::Selection => 4,
        AnnotationRole::SearchHighlight => 5,
        AnnotationRole::SyntaxToken => 6,
    }
}

/// d[impl layout.message-placement]
/// d[impl layout.crowding-policy]
fn choose_placement(
    annotation: &WindowAnnotation,
    crowded_lines: &mut BTreeMap<usize, usize>,
    geometry: &GutterGeometry,
) -> PlacementMode {
    let Some(message) = annotation.message.as_deref() else {
        return PlacementMode::Stacked;
    };

    let anchor = annotation.segments[0];
    let side_budget = geometry
        .source_columns
        .saturating_sub(anchor.start_column)
        .saturating_sub(1);
    let message_width = UnicodeWidthStr::width(message);
    let used_slots = crowded_lines.entry(anchor.line_number).or_default();

    if message_width <= side_budget && *used_slots == 0 {
        *used_slots += 1;
        PlacementMode::Side
    } else if *used_slots <= 1 {
        *used_slots += 1;
        PlacementMode::BelowSpan
    } else {
        *used_slots += 1;
        PlacementMode::Stacked
    }
}

/// d[impl layout.cell-grid]
fn compute_geometry(last_line_number: usize, width: usize) -> GutterGeometry {
    let line_number_width = last_line_number.max(1).to_string().len();
    let separator_columns = 1;
    let connector_columns = 1;
    let source_columns = width.saturating_sub(line_number_width + 2 + separator_columns + 1);

    GutterGeometry {
        line_number_width,
        separator_columns,
        connector_columns,
        source_columns,
    }
}

fn is_line_clipped(
    line: &str,
    geometry: &GutterGeometry,
    long_line_mode: LongLineMode,
    tab_width: usize,
) -> bool {
    match long_line_mode {
        LongLineMode::Clip => display_width(line, tab_width) > geometry.source_columns,
    }
}

/// d[impl span.zero-width]
/// d[impl span.line-breaks]
fn resolve_span(
    source: &Source,
    span: Span,
    tab_width: usize,
) -> Result<Vec<ResolvedSpan>, LayoutError> {
    span.validate(source)?;
    let line_index = LineIndex::new(&source.text, tab_width);
    let start = line_index.resolve(span.start);
    let end = line_index.resolve(span.end);

    if start.line == end.line {
        return Ok(vec![ResolvedSpan {
            line_number: start.line,
            start_column: start.column,
            end_column: end
                .column
                .max(start.column + usize::from(span.start == span.end)),
        }]);
    }

    let mut segments = Vec::new();
    for line in start.line..=end.line {
        let line_range = line_index.line_range(line);
        let segment_start = if line == start.line { start.column } else { 0 };
        let segment_end = if line == end.line {
            end.column
                .max(segment_start + usize::from(span.start == span.end))
        } else {
            display_width(&source.text[line_range], tab_width)
        };
        segments.push(ResolvedSpan {
            line_number: line,
            start_column: segment_start,
            end_column: segment_end,
        });
    }

    Ok(segments)
}

/// d[impl unicode.display-width]
pub(crate) fn display_width(text: &str, tab_width: usize) -> usize {
    let mut width = 0;
    for ch in text.chars() {
        if ch == '\t' {
            let next_tab = tab_width.max(1);
            let offset = width % next_tab;
            width += next_tab - offset;
        } else {
            width += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
    }
    width
}

fn split_lines(text: &str) -> Vec<&str> {
    let mut lines = text.lines().collect::<Vec<_>>();
    if text.ends_with('\n') {
        lines.push("");
    }
    if lines.is_empty() {
        lines.push("");
    }
    lines
}

#[derive(Debug, Clone)]
struct PendingAnnotation<'a> {
    annotation: &'a Annotation,
    line_start: usize,
    line_end: usize,
    segments: Vec<ResolvedSpan>,
}

/// d[impl span.unicode-correct]
/// d[impl span.authoritative-source]
#[derive(Debug)]
struct LineIndex<'a> {
    source: &'a str,
    starts: Vec<usize>,
    tab_width: usize,
}

impl<'a> LineIndex<'a> {
    fn new(source: &'a str, tab_width: usize) -> Self {
        let mut starts = vec![0];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' && index < source.len() {
                starts.push(index + 1);
            }
        }
        Self {
            source,
            starts,
            tab_width,
        }
    }

    fn resolve(&self, offset: usize) -> LineColumn {
        let line_index = self
            .starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        let line_start = self.starts[line_index];
        let column = display_width(&self.source[line_start..offset], self.tab_width);
        LineColumn {
            line: line_index + 1,
            column,
        }
    }

    fn line_range(&self, line_number: usize) -> Range<usize> {
        let start = self.starts[line_number - 1];
        let end = self
            .starts
            .get(line_number)
            .copied()
            .map(|offset| offset.saturating_sub(1))
            .unwrap_or(self.source.len());
        start..end
    }
}

#[derive(Debug, Clone, Copy)]
struct LineColumn {
    line: usize,
    column: usize,
}
