use std::ops::Range;

use ariadne::{CharSet, Color, Config, Label, Report, ReportKind, Source};

#[derive(Debug, Clone)]
pub struct ExpectedError {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct ActualError {
    pub span: Range<usize>,
    pub kind: String,
}

pub fn source_without_annotations(annotated_source: &str) -> String {
    let (source, _) = parse_annotated_source(annotated_source);
    source
}

pub fn assert_annotated_errors(annotated_source: &str, actual_errors: Vec<ActualError>) {
    assert_annotated_errors_with_filename(annotated_source, "test.styx", actual_errors);
}

pub fn assert_annotated_errors_with_filename(
    annotated_source: &str,
    filename: &'static str,
    actual_errors: Vec<ActualError>,
) {
    let (source, expected_errors) = parse_annotated_source(annotated_source);

    let actual_error_positions: Vec<_> = actual_errors
        .iter()
        .enumerate()
        .filter_map(|(idx, e)| span_to_line_col(&source, e.span.clone()).map(|pos| (idx, e, pos)))
        .collect();

    let mut unmatched_expected = Vec::new();
    let mut matched_actual = vec![false; actual_errors.len()];

    for expected in &expected_errors {
        let mut found = false;

        for (idx, actual, (line, start, end)) in &actual_error_positions {
            if matched_actual[*idx] {
                continue;
            }

            if *line == expected.line
                && *start == expected.start
                && *end == expected.end
                && actual.kind == expected.kind
            {
                matched_actual[*idx] = true;
                found = true;
                break;
            }
        }

        if !found {
            unmatched_expected.push(expected.clone());
        }
    }

    let unmatched_actual: Vec<_> = actual_errors
        .iter()
        .enumerate()
        .filter(|(i, _)| !matched_actual[*i])
        .map(|(_, err)| err.clone())
        .collect();

    if unmatched_expected.is_empty() && unmatched_actual.is_empty() {
        return;
    }

    let mut msg = String::new();
    msg.push('\n');

    let expected_labels: Vec<_> = expected_errors
        .iter()
        .filter_map(|expected| {
            let start_offset = line_col_to_offset(&source, expected.line, expected.start)?;
            let end_offset = line_col_to_offset(&source, expected.line, expected.end)?;
            let range = safe_range(start_offset, end_offset, source.len());
            let is_unmatched = unmatched_expected.iter().any(|e| {
                e.line == expected.line
                    && e.start == expected.start
                    && e.end == expected.end
                    && e.kind == expected.kind
            });
            let color = if is_unmatched {
                Color::Red
            } else {
                Color::Yellow
            };
            Some((range, expected.kind.clone(), color))
        })
        .collect();

    let actual_labels: Vec<_> = actual_errors
        .iter()
        .map(|actual| {
            let range = safe_range(actual.span.start, actual.span.end, source.len());
            let is_unmatched = unmatched_actual.iter().any(|e| {
                e.span.start == actual.span.start
                    && e.span.end == actual.span.end
                    && e.kind == actual.kind
            });
            let color = if is_unmatched {
                Color::Green
            } else {
                Color::Cyan
            };
            (range, actual.kind.clone(), color)
        })
        .collect();

    if let Some(report) = build_report(
        "EXPECTED ERRORS (from test annotations)",
        filename,
        expected_labels,
        &source,
    ) {
        msg.push_str(&report);
    }

    if let Some(report) = build_report(
        "ACTUAL ERRORS (from parser)",
        filename,
        actual_labels,
        &source,
    ) {
        msg.push('\n');
        msg.push_str(&report);
    }

    panic!("{}", msg);
}

fn build_report(
    title: &str,
    filename: &'static str,
    labels: Vec<(Range<usize>, String, Color)>,
    source: &str,
) -> Option<String> {
    let (first_range, _, _) = labels.first()?;
    let mut report = Report::build(ReportKind::Error, (filename, first_range.clone()))
        .with_message(title)
        .with_config(
            Config::default()
                .with_color(true)
                .with_compact(false)
                .with_char_set(CharSet::Unicode),
        );

    for (range, message, color) in labels {
        report = report.with_label(
            Label::new((filename, range))
                .with_message(message)
                .with_color(color),
        );
    }

    let mut output = Vec::new();
    report
        .finish()
        .write((filename, Source::from(source)), &mut output)
        .ok()?;
    String::from_utf8(output).ok()
}

fn parse_annotated_source(annotated_source: &str) -> (String, Vec<ExpectedError>) {
    let mut source_lines = Vec::new();
    let mut expected_errors = Vec::new();

    let lines: Vec<&str> = annotated_source.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        if line.trim_start().starts_with('^') {
            parse_annotation_line(
                line,
                source_lines.len().saturating_sub(1),
                &mut expected_errors,
            );
            i += 1;
        } else {
            source_lines.push(line);
            i += 1;

            while i < lines.len() {
                let next_line = lines[i];
                if next_line.trim_start().starts_with('^') {
                    parse_annotation_line(next_line, source_lines.len() - 1, &mut expected_errors);
                    i += 1;
                } else {
                    break;
                }
            }
        }
    }

    (source_lines.join("\n"), expected_errors)
}

fn parse_annotation_line(line: &str, line_index: usize, expected: &mut Vec<ExpectedError>) {
    let trimmed = line.trim_start();
    let caret_start = line.len().saturating_sub(trimmed.len());
    let caret_count = trimmed.chars().take_while(|&c| c == '^').count();
    let after_carets = &trimmed[caret_count..].trim_start();
    let error_kind_name = after_carets.split_whitespace().next().unwrap_or("");

    if !error_kind_name.is_empty() && caret_count > 0 {
        expected.push(ExpectedError {
            line: line_index,
            start: caret_start,
            end: caret_start + caret_count,
            kind: error_kind_name.to_string(),
        });
    }
}

fn span_to_line_col(source: &str, span: Range<usize>) -> Option<(usize, usize, usize)> {
    let mut line_start = 0;
    for (line_idx, line) in source.lines().enumerate() {
        let line_end = line_start + line.len();
        // Include the newline character in this line's range for span matching
        // This allows spans that point to the newline to be attributed to this line
        let line_end_with_newline = line_end + 1;
        let span_start = span.start;
        let span_end = span.end;

        if span_start >= line_start && span_start < line_end_with_newline {
            let col_start = span_start - line_start;
            // Allow col_end to extend to line.len() + 1 to cover the newline position
            let col_end = (span_end - line_start).min(line.len() + 1);
            return Some((line_idx, col_start, col_end));
        }
        line_start = line_end + 1;
    }
    None
}

fn line_col_to_offset(source: &str, line: usize, col: usize) -> Option<usize> {
    let mut line_start = 0;
    for (idx, text) in source.lines().enumerate() {
        if idx == line {
            return Some(line_start + col.min(text.len()));
        }
        line_start += text.len() + 1;
    }
    None
}

fn safe_range(start: usize, end: usize, source_len: usize) -> Range<usize> {
    let max_len = source_len.max(1);
    let start = start.min(max_len);
    let mut end = end.min(max_len);
    if end <= start {
        end = (start + 1).min(max_len);
    }
    start..end
}
