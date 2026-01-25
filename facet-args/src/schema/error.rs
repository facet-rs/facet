use alloc::borrow::Cow;

use ariadne::{Color, Label, Report, ReportKind, Source};
use facet_core::Shape;
use facet_error as _;
use facet_pretty::{PathSegment, format_shape_with_spans};

use crate::{
    diagnostics::{ColorHint, Diagnostic, LabelSpec, SourceBundle, SourceId},
    path::Path,
};

/// The struct passed into facet_args::builder has some problems: some fields are not
/// annotated, etc.
pub struct SchemaError {
    ctx: SchemaErrorContext,
    code: &'static str,
    label: Cow<'static, str>,
    /// For ConflictingFlagNames: the other location that conflicts.
    other_ctx: Option<SchemaErrorContext>,
    /// For ConflictingFlagNames: the conflicting flag name.
    conflicting_name: Option<String>,
}

impl SchemaError {
    fn new(
        ctx: SchemaErrorContext,
        code: &'static str,
        label: impl Into<Cow<'static, str>>,
    ) -> Self {
        Self {
            ctx,
            code,
            label: label.into(),
            other_ctx: None,
            conflicting_name: None,
        }
    }

    pub fn top_level_not_struct(ctx: SchemaErrorContext) -> Self {
        Self::new(
            ctx,
            "schema::top_level_not_struct",
            "top-level shape must be a struct",
        )
    }

    pub fn missing_args_annotation(ctx: SchemaErrorContext, field: &'static str) -> Self {
        Self::new(
            ctx,
            "schema::missing_args_annotation",
            format!("field `{field}` is missing a #[facet(args::...)] annotation"),
        )
    }

    pub fn multiple_subcommand_fields(ctx: SchemaErrorContext, _field: &'static str) -> Self {
        Self::new(
            ctx,
            "schema::multiple_subcommand_fields",
            "only one field may be marked with #[facet(args::subcommand)] at this level",
        )
    }

    pub fn subcommand_on_non_enum(ctx: SchemaErrorContext, field: &'static str) -> Self {
        Self::new(
            ctx,
            "schema::subcommand_on_non_enum",
            format!("field `{field}` marked as subcommand must be an enum"),
        )
    }

    pub fn counted_on_non_integer(ctx: SchemaErrorContext, field: &'static str) -> Self {
        Self::new(
            ctx,
            "schema::counted_on_non_integer",
            format!("field `{field}` marked as counted must be an integer"),
        )
    }

    pub fn short_on_positional(ctx: SchemaErrorContext, field: &'static str) -> Self {
        Self::new(
            ctx,
            "schema::short_on_positional",
            format!("field `{field}` is positional and cannot have a short flag"),
        )
    }

    pub fn env_prefix_without_config(ctx: SchemaErrorContext, field: &'static str) -> Self {
        Self::new(
            ctx,
            "schema::env_prefix_without_config",
            format!("field `{field}` uses args::env_prefix without args::config"),
        )
    }

    pub fn conflicting_flag_names(
        ctx: SchemaErrorContext,
        other_ctx: SchemaErrorContext,
        name: String,
    ) -> Self {
        Self {
            ctx,
            code: "schema::conflicting_flag_names",
            label: Cow::Owned(format!("duplicate flag name `{name}` at this level")),
            other_ctx: Some(other_ctx),
            conflicting_name: Some(name),
        }
    }

    pub fn unsupported_leaf_type(ctx: SchemaErrorContext) -> Self {
        Self::new(
            ctx,
            "schema::unsupported_leaf_type",
            "unsupported leaf type",
        )
    }

    pub fn config_field_must_be_struct(ctx: SchemaErrorContext) -> Self {
        Self::new(
            ctx,
            "schema::config_field_must_be_struct",
            "config field must be a struct",
        )
    }

    pub fn multiple_config_fields(ctx: SchemaErrorContext, field: &'static str) -> Self {
        Self::new(
            ctx,
            "schema::multiple_config_fields",
            format!("multiple config fields (already saw another before `{field}`)"),
        )
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
    pub(crate) fn root(shape: &'static Shape) -> Self {
        Self {
            shape,
            path: Vec::new(),
        }
    }

    pub(crate) fn with_field(&self, field: &'static str) -> Self {
        let mut path = self.path.clone();
        path.push(field.to_string());
        Self {
            shape: self.shape,
            path,
        }
    }

    pub(crate) fn with_variant(&self, variant: impl Into<String>) -> Self {
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
        self.code
    }

    fn label(&self) -> Cow<'static, str> {
        self.label.clone()
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

                // Handle the "other" label for conflicting flag names
                let mut other_label = None;
                if let (Some(other_ctx), Some(name)) = (&self.other_ctx, &self.conflicting_name) {
                    let other_path = schema_path_to_segments(&other_ctx.path);
                    let other_span = formatted
                        .spans
                        .get(&other_path)
                        .map(|span| span.key.0..span.value.1)
                        .or_else(|| formatted.type_name_span.map(|(start, end)| start..end));

                    if let Some(other_span) = other_span
                        && other_span != span
                    {
                        other_label = Some((other_span, format!("also uses flag `{name}`")));
                    }
                }

                let message = self.label();
                labels.push(LabelSpec {
                    source: SourceId::Schema,
                    span,
                    message,
                    is_primary: true,
                    color: Some(ColorHint::Red),
                });

                if let Some((other_span, other_message)) = other_label {
                    labels.push(LabelSpec {
                        source: SourceId::Schema,
                        span: other_span,
                        message: Cow::Owned(other_message),
                        is_primary: false,
                        color: Some(ColorHint::Red),
                    });
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

        let mut builder = Report::build(ReportKind::Error, primary_span.clone())
            .with_code(self.code())
            .with_message(self.label());

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
