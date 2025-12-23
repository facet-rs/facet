//! Pretty-printing support for paths using facet-pretty and miette.
//!
//! This module provides rich error rendering that shows the type structure
//! with the error location highlighted using miette's diagnostic rendering.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use facet_core::Shape;
use facet_pretty::{FormattedShape, PathSegment as PrettyPathSegment, format_shape_with_spans};
use miette::{Diagnostic, LabeledSpan, NamedSource, Report, SourceSpan};

use crate::{Path, PathStep};

/// A diagnostic error that shows the type structure with the error location highlighted.
#[derive(Debug)]
pub struct PathDiagnostic {
    /// The error message
    message: String,
    /// The source code (formatted type definition)
    source_code: NamedSource<String>,
    /// The span to highlight
    span: SourceSpan,
    /// Optional help text
    help: Option<String>,
}

impl core::fmt::Display for PathDiagnostic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PathDiagnostic {}

impl Diagnostic for PathDiagnostic {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source_code)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(core::iter::once(LabeledSpan::new_with_span(
            Some("error here".to_string()),
            self.span,
        ))))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn core::fmt::Display + 'a>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h.as_str()) as Box<dyn core::fmt::Display>)
    }
}

impl Path {
    /// Create a miette diagnostic that shows the type structure with the error location highlighted.
    ///
    /// This provides rich terminal output with:
    /// - The formatted type definition as source code
    /// - The error location underlined with a label
    /// - Optional help text
    pub fn to_diagnostic(
        &self,
        shape: &'static Shape,
        message: impl Into<String>,
        help: Option<String>,
    ) -> PathDiagnostic {
        let FormattedShape { text, spans } = format_shape_with_spans(shape);

        // Convert our path to facet-pretty's path format
        let pretty_path = self.to_pretty_path(shape);

        // Find the span for our path
        let span = if let Some(field_span) = spans.get(&pretty_path) {
            // Use the value span (the type that caused the error)
            SourceSpan::new(
                field_span.value.0.into(),
                field_span.value.1 - field_span.value.0,
            )
        } else {
            // Fallback: highlight the whole thing
            SourceSpan::new(0.into(), text.len())
        };

        PathDiagnostic {
            message: message.into(),
            source_code: NamedSource::new(shape.type_identifier, text),
            span,
            help,
        }
    }

    /// Format this path with rich pretty-printing, showing the type structure
    /// with the error location highlighted.
    ///
    /// Returns the formatted diagnostic as a string for display.
    pub fn format_pretty(
        &self,
        shape: &'static Shape,
        message: impl Into<String>,
        help: Option<String>,
    ) -> String {
        let diagnostic = self.to_diagnostic(shape, message, help);
        let report = Report::new(diagnostic);
        format!("{:?}", report)
    }

    /// Convert this path to facet-pretty's PathSegment format
    fn to_pretty_path(&self, shape: &'static Shape) -> Vec<PrettyPathSegment> {
        use facet_core::{Def, StructKind, Type, UserType};

        let mut result = Vec::new();
        let mut current_shape = shape;

        for step in self.steps() {
            match step {
                PathStep::Field(idx) => {
                    let idx = *idx as usize;
                    if let Type::User(UserType::Struct(sd)) = current_shape.ty {
                        if let Some(field) = sd.fields.get(idx) {
                            result.push(PrettyPathSegment::Field(Cow::Borrowed(field.name)));
                            current_shape = field.shape();
                        }
                    } else if let Type::User(UserType::Enum(ed)) = current_shape.ty {
                        // For enum variant fields, we need the variant context
                        // This is handled by the Variant step before this
                        if let Some(last) = result.last() {
                            if let PrettyPathSegment::Variant(variant_name) = last {
                                // Find the variant
                                if let Some(variant) =
                                    ed.variants.iter().find(|v| v.name == variant_name.as_ref())
                                {
                                    if let Some(field) = variant.data.fields.get(idx) {
                                        result.push(PrettyPathSegment::Field(Cow::Borrowed(
                                            field.name,
                                        )));
                                        current_shape = field.shape();
                                    }
                                }
                            }
                        }
                    }
                }
                PathStep::Index(idx) => {
                    result.push(PrettyPathSegment::Index(*idx as usize));
                    // Update current_shape to element type
                    match current_shape.def {
                        Def::List(ld) => current_shape = ld.t(),
                        Def::Array(ad) => current_shape = ad.t(),
                        Def::Slice(sd) => current_shape = sd.t(),
                        _ => {}
                    }
                }
                PathStep::Variant(idx) => {
                    let idx = *idx as usize;
                    if let Type::User(UserType::Enum(ed)) = current_shape.ty {
                        if let Some(variant) = ed.variants.get(idx) {
                            result.push(PrettyPathSegment::Variant(Cow::Borrowed(variant.name)));
                            // For struct variants, we stay at the enum shape
                            // The next Field step will handle the variant's field
                            if variant.data.kind != StructKind::Unit {
                                // Could update to first field's shape if needed
                            }
                        }
                    }
                }
                PathStep::MapKey => {
                    result.push(PrettyPathSegment::Key(Cow::Borrowed("<key>")));
                    if let Def::Map(md) = current_shape.def {
                        current_shape = md.k();
                    }
                }
                PathStep::MapValue => {
                    // Map values don't have a direct PathSegment equivalent
                    // We could use Key with a special marker
                    if let Def::Map(md) = current_shape.def {
                        current_shape = md.v();
                    }
                }
                PathStep::OptionSome => {
                    if let Def::Option(od) = current_shape.def {
                        current_shape = od.t();
                    }
                }
                PathStep::Deref => {
                    if let Def::Pointer(pd) = current_shape.def {
                        if let Some(pointee) = pd.pointee() {
                            current_shape = pointee;
                        }
                    }
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn test_diagnostic_for_struct_field() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
            max_retries: u8,
            enabled: bool,
        }

        let mut path = Path::new();
        path.push(PathStep::Field(1)); // max_retries

        let output = path.format_pretty(
            Config::SHAPE,
            "unsupported scalar type",
            Some("consider using a different type".to_string()),
        );

        // The output should contain the type name and field
        assert!(output.contains("Config"), "Should mention type name");
        assert!(
            output.contains("max_retries") || output.contains("u8"),
            "Should highlight the field or its type"
        );
    }

    #[test]
    fn test_diagnostic_for_nested_type() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Outer {
            name: String,
            inner: Inner,
        }

        let mut path = Path::new();
        path.push(PathStep::Field(1)); // inner

        let diagnostic = path.to_diagnostic(Outer::SHAPE, "nested type error", None);

        assert_eq!(diagnostic.message, "nested type error");
    }
}
