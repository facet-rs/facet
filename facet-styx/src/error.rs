//! Error types for Styx parsing.

use facet_format::{DeserializeError, DeserializeErrorKind};
use margin::{
    Annotation, AnnotationRole, Diagnostics, Note, NoteKind, Report, Severity, Source, SourceId,
    Span,
};
use margin_term::{ColorLevel, GlyphMode, HyperlinkMode, TerminalCapabilities};

/// Convert a `facet_reflect::Span` to byte offsets clamped to the source.
fn reflect_span_to_offsets(span: &facet_reflect::Span, source: &str) -> (usize, usize) {
    let start = clamp_to_char_boundary(source, span.offset as usize);
    let end = clamp_to_char_boundary(source, start.saturating_add(span.len as usize));
    (start, end.max(start))
}

fn clamp_to_char_boundary(source: &str, mut offset: usize) -> usize {
    offset = offset.min(source.len());
    while offset > 0 && !source.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn error_offsets(err: &DeserializeError, source: &str) -> (usize, usize) {
    err.span
        .as_ref()
        .map(|span| reflect_span_to_offsets(span, source))
        .unwrap_or((0, source.len()))
}

/// Trait for rendering errors as source diagnostics.
pub trait RenderError {
    /// Render this error with source context.
    fn render(&self, filename: &str, source: &str) -> String;

    /// Write the error report to a writer.
    fn write_report<W: std::io::Write>(&self, filename: &str, source: &str, writer: W);
}

/// Rendering support for `DeserializeError`.
impl RenderError for DeserializeError {
    fn render(&self, filename: &str, source: &str) -> String {
        render_deserialize_error(self, filename, source)
    }

    fn write_report<W: std::io::Write>(&self, filename: &str, source: &str, mut writer: W) {
        let _ = writer.write_all(self.render(filename, source).as_bytes());
    }
}

fn render_deserialize_error(err: &DeserializeError, filename: &str, source: &str) -> String {
    let diagnostics = build_deserialize_error_diagnostics(err, filename, source);
    let capabilities = TerminalCapabilities {
        width: 100,
        glyph_mode: GlyphMode::Unicode,
        color_level: if std::env::var_os("NO_COLOR").is_some() {
            ColorLevel::None
        } else {
            ColorLevel::Ansi16
        },
        hyperlink_mode: HyperlinkMode::None,
        tab_width: 4,
    };

    match margin_term::render(&diagnostics, capabilities) {
        Ok(rendered) => rendered,
        Err(_) => format!("Error: {}\n", err),
    }
}

fn build_deserialize_error_diagnostics(
    err: &DeserializeError,
    filename: &str,
    source: &str,
) -> Diagnostics {
    let source_id = SourceId(filename.to_string());
    let (start, end) = error_offsets(err, source);

    let mut notes = Vec::new();
    let (title, label) = match &err.kind {
        DeserializeErrorKind::MissingField {
            field,
            container_shape,
        } => {
            notes.push(Note {
                kind: NoteKind::Help,
                text: format!("add `{field} <value>`"),
            });
            (
                format!("missing required field `{field}`"),
                format!("required by {container_shape}"),
            )
        }
        DeserializeErrorKind::UnknownField { field, suggestion } => {
            if let Some(suggestion) = suggestion {
                notes.push(Note {
                    kind: NoteKind::Help,
                    text: format!("did you mean `{suggestion}`?"),
                });
            }
            (
                format!("unknown field `{field}`"),
                "unknown field".to_string(),
            )
        }
        DeserializeErrorKind::TypeMismatch { expected, got } => (
            format!("type mismatch: expected {expected}"),
            format!("got {got}"),
        ),
        DeserializeErrorKind::Reflect { kind, context } => {
            if !context.is_empty() {
                notes.push(Note {
                    kind: NoteKind::Note,
                    text: format!("while {context}"),
                });
            }
            (kind.to_string(), "error here".to_string())
        }
        DeserializeErrorKind::UnexpectedEof { expected } => (
            "unexpected end of input".to_string(),
            format!("expected {expected}"),
        ),
        DeserializeErrorKind::Unsupported { message } => (
            format!("unsupported: {message}"),
            "unsupported here".to_string(),
        ),
        DeserializeErrorKind::CannotBorrow { reason } => {
            (reason.to_string(), "cannot borrow here".to_string())
        }
        DeserializeErrorKind::UnexpectedToken { got, expected } => (
            format!("unexpected token `{got}`"),
            format!("expected {expected}"),
        ),
        DeserializeErrorKind::InvalidValue { message } => (
            format!("invalid value: {message}"),
            "invalid value".to_string(),
        ),
        _ => (err.kind.to_string(), "error here".to_string()),
    };

    if let Some(path) = err.path.as_ref() {
        notes.push(Note {
            kind: NoteKind::Note,
            text: format!("at path: {path}"),
        });
    }

    Diagnostics {
        sources: vec![Source {
            id: source_id.clone(),
            name: filename.to_string(),
            hyperlink: None,
            text: source.to_string(),
        }],
        reports: vec![Report {
            severity: Severity::Error,
            title,
            annotations: vec![Annotation {
                spans: vec![Span::new(source_id.0.as_str(), start, end)],
                role: AnnotationRole::PrimaryLabel,
                syntax_class: None,
                message: Some(label),
                priority: 100,
            }],
            notes,
            sections: Vec::new(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Facet, Debug)]
    struct Person {
        name: String,
        age: u32,
    }

    #[test]
    fn test_missing_field_diagnostic() {
        let source = "name Alice";
        let result: Result<Person, _> = crate::from_str(source);
        let err = result.unwrap_err();

        crate::assert_snapshot_stripped!(RenderError::render(&err, "test.styx", source));

        assert!(!err.render("test.styx", source).contains("Path {"));
        assert!(!err.render("test.styx", source).contains("Shape {"));
    }

    #[test]
    fn test_invalid_scalar_diagnostic() {
        let source = "name Alice\nage notanumber";
        let result: Result<Person, _> = crate::from_str(source);
        let err = result.unwrap_err();

        crate::assert_snapshot_stripped!(err.render("test.styx", source));
    }

    #[test]
    fn test_unknown_field_diagnostic() {
        #[derive(Facet, Debug)]
        #[facet(deny_unknown_fields)]
        struct Strict {
            name: String,
        }

        let source = "name Alice\nunknown_field value";
        let result: Result<Strict, _> = crate::from_str(source);
        let err = result.unwrap_err();

        crate::assert_snapshot_stripped!(err.render("test.styx", source));
    }
}
