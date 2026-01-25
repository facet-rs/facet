use alloc::borrow::Cow;
use facet::Facet;

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
#[derive(Facet)]
#[repr(u8)]
pub enum SchemaError {
    /// Top-level shape must be a struct.
    TopLevelNotStruct {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
    },
    /// A field was not annotated with any args attribute (positional/named/subcommand/config).
    MissingArgsAnnotation {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
        field: &'static str,
    },

    /// More than one field marked as `#[facet(args::subcommand)]` at the same level.
    MultipleSubcommandFields {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
        field: &'static str,
    },
    /// `#[facet(args::subcommand)]` used on a non-enum field.
    SubcommandOnNonEnum {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
        field: &'static str,
    },
    /// `#[facet(args::counted)]` used on a non-integer type.
    CountedOnNonInteger {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
        field: &'static str,
    },
    /// `#[facet(args::short)]` used on a positional-only argument.
    ShortOnPositional {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
        field: &'static str,
    },
    /// `#[facet(args::env_prefix)]` used without `#[facet(args::config)]`.
    EnvPrefixWithoutConfig {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
        field: &'static str,
    },
    /// Duplicate CLI flag name or short at the same level.
    ConflictingFlagNames {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
        name: String,
    },
    /// Unsupported leaf type (non-scalar, non-enum).
    UnsupportedLeafType {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
    },
    /// Config field must be a struct.
    ConfigFieldMustBeStruct {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
    },
    /// More than one field marked as `#[facet(args::config)]`.
    MultipleConfigFields {
        #[facet(opaque)]
        ctx: SchemaErrorContext,
        field: &'static str,
    },
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

impl SchemaError {
    fn ctx(&self) -> &SchemaErrorContext {
        match self {
            SchemaError::TopLevelNotStruct { ctx } => ctx,
            SchemaError::MissingArgsAnnotation { ctx, .. } => ctx,
            SchemaError::MultipleSubcommandFields { ctx, .. } => ctx,
            SchemaError::SubcommandOnNonEnum { ctx, .. } => ctx,
            SchemaError::CountedOnNonInteger { ctx, .. } => ctx,
            SchemaError::ShortOnPositional { ctx, .. } => ctx,
            SchemaError::EnvPrefixWithoutConfig { ctx, .. } => ctx,
            SchemaError::ConflictingFlagNames { ctx, .. } => ctx,
            SchemaError::UnsupportedLeafType { ctx } => ctx,
            SchemaError::ConfigFieldMustBeStruct { ctx } => ctx,
            SchemaError::MultipleConfigFields { ctx, .. } => ctx,
        }
    }
}

impl Diagnostic for SchemaError {
    fn code(&self) -> &'static str {
        match self {
            SchemaError::TopLevelNotStruct { .. } => "schema::top_level_not_struct",
            SchemaError::MissingArgsAnnotation { .. } => "schema::missing_args_annotation",
            SchemaError::MultipleSubcommandFields { .. } => "schema::multiple_subcommand_fields",
            SchemaError::SubcommandOnNonEnum { .. } => "schema::subcommand_on_non_enum",
            SchemaError::CountedOnNonInteger { .. } => "schema::counted_on_non_integer",
            SchemaError::ShortOnPositional { .. } => "schema::short_on_positional",
            SchemaError::EnvPrefixWithoutConfig { .. } => "schema::env_prefix_without_config",
            SchemaError::ConflictingFlagNames { .. } => "schema::conflicting_flag_names",
            SchemaError::UnsupportedLeafType { .. } => "schema::unsupported_leaf_type",
            SchemaError::ConfigFieldMustBeStruct { .. } => "schema::config_field_must_be_struct",
            SchemaError::MultipleConfigFields { .. } => "schema::multiple_config_fields",
        }
    }

    fn label(&self) -> Cow<'static, str> {
        match self {
            SchemaError::TopLevelNotStruct { .. } => {
                Cow::Borrowed("top-level shape must be a struct")
            }
            SchemaError::MissingArgsAnnotation { field, .. } => Cow::Owned(format!(
                "field `{field}` is missing a #[facet(args::...)] annotation"
            )),
            SchemaError::MultipleSubcommandFields { .. } => Cow::Borrowed(
                "only one field may be marked with #[facet(args::subcommand)] at this level",
            ),
            SchemaError::SubcommandOnNonEnum { field, .. } => Cow::Owned(format!(
                "field `{field}` marked as subcommand must be an enum"
            )),
            SchemaError::CountedOnNonInteger { field, .. } => Cow::Owned(format!(
                "field `{field}` marked as counted must be an integer"
            )),
            SchemaError::ShortOnPositional { field, .. } => Cow::Owned(format!(
                "field `{field}` is positional and cannot have a short flag"
            )),
            SchemaError::EnvPrefixWithoutConfig { field, .. } => Cow::Owned(format!(
                "field `{field}` uses args::env_prefix without args::config"
            )),
            SchemaError::ConflictingFlagNames { name, .. } => {
                Cow::Owned(format!("duplicate flag name `{name}` at this level"))
            }
            SchemaError::UnsupportedLeafType { .. } => Cow::Borrowed("unsupported leaf type"),
            SchemaError::ConfigFieldMustBeStruct { .. } => {
                Cow::Borrowed("config field must be a struct")
            }
            SchemaError::MultipleConfigFields { field, .. } => Cow::Owned(format!(
                "multiple config fields (already saw another before `{field}`)"
            )),
        }
    }

    fn sources(&self) -> Vec<SourceBundle> {
        let ctx = self.ctx();
        let formatted = format_shape_with_spans(ctx.shape);
        vec![SourceBundle {
            id: SourceId::Schema,
            name: Some(Cow::Borrowed("schema definition")),
            text: Cow::Owned(formatted.text),
        }]
    }

    fn labels(&self) -> Vec<LabelSpec> {
        let ctx = self.ctx();
        let formatted = format_shape_with_spans(ctx.shape);
        let path = schema_path_to_segments(&ctx.path);
        let span = formatted
            .spans
            .get(&path)
            .map(|span| span.key.0..span.value.1)
            .or_else(|| formatted.type_name_span.map(|(start, end)| start..end));

        match span {
            Some(span) => {
                let mut labels = Vec::new();

                let mut def_end_span = None;
                if let Some(type_name_span) = formatted.type_name_span {
                    let type_label_span = type_name_span.0..type_name_span.1;
                    let def_end = formatted.text[type_name_span.1..]
                        .find('}')
                        .map(|offset| type_name_span.1 + offset)
                        .unwrap_or_else(|| formatted.text.len().saturating_sub(1));
                    let def_end_end = (def_end + 1).min(formatted.text.len());
                    def_end_span = Some(def_end..def_end_end);

                    let source_label = ctx
                        .shape
                        .source_file
                        .zip(ctx.shape.source_line)
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

                let message = self.label();
                labels.push(LabelSpec {
                    source: SourceId::Schema,
                    span,
                    message,
                    is_primary: true,
                    color: Some(ColorHint::Red),
                });

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
        let sources = self.sources();
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
