use alloc::borrow::Cow;

use ariadne::{Color, Label, Report, ReportKind, Source};
use facet_core::Shape;
use facet_error as _;
use facet_pretty::{PathSegment, format_shape_with_spans};

use crate::{
    diagnostics::{ColorHint, Diagnostic, LabelSpec, SourceBundle, SourceId},
    path::Path,
};

/// A secondary label to display alongside the primary error location.
#[derive(Clone, Debug)]
pub struct SecondaryLabel {
    /// Context pointing to the secondary location.
    pub ctx: SchemaErrorContext,
    /// Message to display at this location.
    pub message: Cow<'static, str>,
}

/// The struct passed into facet_args::builder has some problems: some fields are not
/// annotated, etc.
pub struct SchemaError {
    /// Primary error context.
    pub ctx: SchemaErrorContext,
    /// Primary error message.
    pub message: Cow<'static, str>,
    /// Additional labeled locations to highlight.
    pub secondary_labels: Vec<SecondaryLabel>,
}

impl SchemaError {
    /// Create a simple error with just a context and message.
    pub fn new(ctx: SchemaErrorContext, message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            ctx,
            message: message.into(),
            secondary_labels: Vec::new(),
        }
    }

    /// Add a secondary label to this error.
    pub fn with_label(
        mut self,
        ctx: SchemaErrorContext,
        message: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.secondary_labels.push(SecondaryLabel {
            ctx,
            message: message.into(),
        });
        self
    }
}

/// Context for schema errors, retained for late diagnostic formatting.
#[derive(Clone, Debug)]
pub struct SchemaErrorContext {
    /// Shape where the error occurred (root for formatting).
    pub shape: &'static Shape,
    /// Path to the offending node.
    pub path: Path,
}

impl SchemaErrorContext {
    pub fn root(shape: &'static Shape) -> Self {
        Self {
            shape,
            path: Vec::new(),
        }
    }

    pub fn with_field(&self, field: &'static str) -> Self {
        let mut path = self.path.clone();
        path.push(field.to_string());
        Self {
            shape: self.shape,
            path,
        }
    }

    pub fn with_variant(&self, variant: impl Into<String>) -> Self {
        let mut path = self.path.clone();
        path.push(variant.into());
        Self {
            shape: self.shape,
            path,
        }
    }
}

fn schema_path_to_segments(path: &Path) -> Vec<PathSegment> {
    path.iter()
        .map(|segment| PathSegment::Field(Cow::Owned(segment.clone())))
        .collect()
}

impl Diagnostic for SchemaError {
    fn code(&self) -> &'static str {
        "schema"
    }

    fn label(&self) -> Cow<'static, str> {
        self.message.clone()
    }

    fn sources(&self) -> Vec<SourceBundle> {
        let formatted = format_shape_with_spans(self.ctx.shape);
        vec![SourceBundle {
            id: SourceId::Schema,
            name: Some(Cow::Borrowed("schema definition")),
            text: Cow::Owned(formatted.text),
        }]
    }

    fn labels(&self) -> Vec<LabelSpec> {
        let formatted = format_shape_with_spans(self.ctx.shape);
        let path = schema_path_to_segments(&self.ctx.path);
        let span = formatted
            .spans
            .get(&path)
            .map(|span| span.key.0..span.value.1)
            .or_else(|| formatted.type_name_span.map(|(start, end)| start..end));

        match span {
            Some(span) => {
                let mut labels = Vec::new();

                let def_end_span = formatted.type_end_span.map(|(start, end)| start..end);
                if let Some(type_name_span) = formatted.type_name_span {
                    let type_label_span = type_name_span.0..type_name_span.1;

                    let source_label = self
                        .ctx
                        .shape
                        .source_file
                        .zip(self.ctx.shape.source_line)
                        .map(|(file, line)| format!("defined at {file}:{line}"))
                        .unwrap_or_else(|| {
                            "definition location unavailable (enable facet/doc)".to_string()
                        });

                    labels.push(LabelSpec {
                        source: SourceId::Schema,
                        span: type_label_span,
                        message: Cow::Owned(source_label),
                        is_primary: false,
                        color: Some(ColorHint::Blue),
                    });
                }

                // Primary label
                labels.push(LabelSpec {
                    source: SourceId::Schema,
                    span: span.clone(),
                    message: self.message.clone(),
                    is_primary: true,
                    color: Some(ColorHint::Red),
                });

                // Secondary labels
                for secondary in &self.secondary_labels {
                    let secondary_path = schema_path_to_segments(&secondary.ctx.path);
                    let secondary_span = formatted
                        .spans
                        .get(&secondary_path)
                        .map(|s| s.key.0..s.value.1)
                        .or_else(|| formatted.type_name_span.map(|(start, end)| start..end));

                    if let Some(secondary_span) = secondary_span
                        && secondary_span != span
                    {
                        labels.push(LabelSpec {
                            source: SourceId::Schema,
                            span: secondary_span,
                            message: secondary.message.clone(),
                            is_primary: false,
                            color: Some(ColorHint::Red),
                        });
                    }
                }

                if let Some(def_end_span) = def_end_span {
                    labels.push(LabelSpec {
                        source: SourceId::Schema,
                        span: def_end_span,
                        message: Cow::Borrowed("end of definition"),
                        is_primary: false,
                        color: Some(ColorHint::Blue),
                    });
                }

                labels
            }
            None => Vec::new(),
        }
    }
}

fn color_from_hint(hint: ColorHint) -> Color {
    match hint {
        ColorHint::Red => Color::Red,
        ColorHint::Yellow => Color::Yellow,
        ColorHint::Blue => Color::Blue,
        ColorHint::Cyan => Color::Cyan,
        ColorHint::Green => Color::Green,
    }
}

impl SchemaError {
    fn to_ariadne_report(&self) -> Report<'static, core::ops::Range<usize>> {
        let labels = self.labels();
        let primary_span = labels
            .iter()
            .find(|label| label.is_primary)
            .map(|label| label.span.clone())
            .unwrap_or(0..0);

        let mut builder =
            Report::build(ReportKind::Error, primary_span.clone()).with_message(self.label());

        for label in labels {
            if label.source != SourceId::Schema {
                continue;
            }
            let mut ar_label = Label::new(label.span).with_message(label.message);
            if let Some(color) = label.color {
                ar_label = ar_label.with_color(color_from_hint(color));
            }
            builder = builder.with_label(ar_label);
        }

        if let Some(help) = self.help() {
            builder = builder.with_help(help.to_string());
        }

        for note in self.notes() {
            builder = builder.with_note(note.to_string());
        }

        builder.finish()
    }

    fn to_ariadne_string(&self) -> String {
        let sources = self.sources();
        let source_text = sources
            .first()
            .map(|source| source.text.as_ref())
            .unwrap_or("");
        let source = Source::from(source_text);

        let mut buf = Vec::new();
        self.to_ariadne_report()
            .write(source, &mut buf)
            .expect("write to Vec failed");
        String::from_utf8(buf).expect("ariadne output is valid UTF-8")
    }
}

impl core::error::Error for SchemaError {}

impl core::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_ariadne_string())
    }
}

impl core::fmt::Debug for SchemaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_ariadne_string())
    }
}
