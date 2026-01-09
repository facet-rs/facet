//! Pretty-printing support for paths using facet-pretty and miette.
//!
//! This module provides rich error rendering that shows the type structure
//! with the error location highlighted using miette's diagnostic rendering.
//!
//! # Syntax Highlighting
//!
//! Call [`install_highlighter`] at program startup to enable Rust syntax
//! highlighting in diagnostic output via arborium.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use facet_core::{Def, Shape, Type, UserType};
use facet_pretty::{
    FormattedShape, PathSegment as PrettyPathSegment, ShapeFormatConfig,
    format_shape_with_spans_and_config,
};
use miette::{Diagnostic, LabeledSpan, NamedSource, Report, SourceSpan};

use crate::{Path, PathStep};

/// Install the arborium syntax highlighter for miette diagnostics.
///
/// Call this once at program startup to enable Rust syntax highlighting
/// in pretty error output. Without this, code snippets appear without colors.
///
/// # Example
///
/// ```
/// facet_path::pretty::install_highlighter();
/// // ... rest of your program
/// ```
pub fn install_highlighter() {
    let _ = miette_arborium::install_global();
}

/// A single type diagnostic - one source with labels pointing to it.
#[derive(Debug)]
struct TypeDiagnostic {
    /// Message for this type in the chain (e.g., "in type `Foo`")
    message: String,
    /// The source code (formatted type definition)
    source: NamedSource<String>,
    /// Labels pointing to spans in the source
    labels: Vec<LabeledSpan>,
}

impl core::fmt::Display for TypeDiagnostic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TypeDiagnostic {}

/// A diagnostic error that shows the full type hierarchy with each step highlighted.
/// Each type in the chain is shown as a separate related error.
#[derive(Debug)]
pub struct PathDiagnostic {
    /// The primary error message
    message: String,
    /// The source code for the leaf type (where the error occurred)
    source: NamedSource<String>,
    /// Labels pointing to spans in the leaf type
    labels: Vec<LabeledSpan>,
    /// Optional help text
    help: Option<String>,
    /// Related diagnostics showing the path through parent types
    related: Vec<TypeDiagnostic>,
}

impl core::fmt::Display for PathDiagnostic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PathDiagnostic {}

/// A step in the path through types, recording the shape and local path at each user type.
struct PathSegment {
    /// The shape at this point in the traversal
    shape: &'static Shape,
    /// The path within this shape (field names, variant names)
    local_path: Vec<PrettyPathSegment>,
}

/// Check if a shape is a "real" user type (struct/enum) that we should show,
/// as opposed to a container type (Option, Vec, Map) which wraps other types.
fn is_displayable_user_type(shape: &Shape) -> bool {
    // Container types (Option, List, Map, Array) shouldn't get their own diagnostic blocks
    // even if they happen to be implemented as UserType::Enum (like Option with NPO)
    match shape.def {
        Def::Option(_) | Def::List(_) | Def::Map(_) | Def::Array(_) | Def::Slice(_) => false,
        _ => matches!(
            shape.ty,
            Type::User(UserType::Struct(_) | UserType::Enum(_))
        ),
    }
}

impl Path {
    /// Collect all user types traversed by this path, with local paths within each.
    ///
    /// For a path like `items[0].type_info` through `Container { items: Vec<Item> }`,
    /// this returns:
    /// - `(Container, [Field("items")])`
    /// - `(Item, [Field("type_info")])`
    fn collect_type_segments(&self, root_shape: &'static Shape) -> Vec<PathSegment> {
        let mut segments: Vec<PathSegment> = Vec::new();
        let mut current_shape = root_shape;
        let mut local_path: Vec<PrettyPathSegment> = Vec::new();

        // Start with the root type
        let mut current_segment_shape = root_shape;

        for step in self.steps() {
            match step {
                PathStep::Field(idx) => {
                    let idx = *idx as usize;
                    if let Type::User(UserType::Struct(sd)) = current_shape.ty {
                        if let Some(field) = sd.fields.get(idx) {
                            local_path.push(PrettyPathSegment::Field(Cow::Borrowed(field.name)));
                            current_shape = field.shape();

                            // If we entered a new displayable user type, save current segment and start new one
                            if is_displayable_user_type(current_shape) {
                                segments.push(PathSegment {
                                    shape: current_segment_shape,
                                    local_path: core::mem::take(&mut local_path),
                                });
                                current_segment_shape = current_shape;
                            }
                        }
                    } else if let Type::User(UserType::Enum(ed)) = current_shape.ty {
                        // For enum variant fields, we need the variant context from local_path
                        if let Some(PrettyPathSegment::Variant(variant_name)) = local_path.last()
                            && let Some(variant) =
                                ed.variants.iter().find(|v| v.name == variant_name.as_ref())
                            && let Some(field) = variant.data.fields.get(idx)
                        {
                            local_path.push(PrettyPathSegment::Field(Cow::Borrowed(field.name)));
                            current_shape = field.shape();

                            if is_displayable_user_type(current_shape) {
                                segments.push(PathSegment {
                                    shape: current_segment_shape,
                                    local_path: core::mem::take(&mut local_path),
                                });
                                current_segment_shape = current_shape;
                            }
                        }
                    }
                }
                PathStep::Index(_idx) => {
                    // Indices go through containers - update current_shape
                    match current_shape.def {
                        Def::List(ld) => {
                            current_shape = ld.t();
                            if is_displayable_user_type(current_shape) {
                                segments.push(PathSegment {
                                    shape: current_segment_shape,
                                    local_path: core::mem::take(&mut local_path),
                                });
                                current_segment_shape = current_shape;
                            }
                        }
                        Def::Array(ad) => {
                            current_shape = ad.t();
                            if is_displayable_user_type(current_shape) {
                                segments.push(PathSegment {
                                    shape: current_segment_shape,
                                    local_path: core::mem::take(&mut local_path),
                                });
                                current_segment_shape = current_shape;
                            }
                        }
                        Def::Slice(sd) => {
                            current_shape = sd.t();
                            if is_displayable_user_type(current_shape) {
                                segments.push(PathSegment {
                                    shape: current_segment_shape,
                                    local_path: core::mem::take(&mut local_path),
                                });
                                current_segment_shape = current_shape;
                            }
                        }
                        _ => {}
                    }
                }
                PathStep::Variant(idx) => {
                    let idx = *idx as usize;
                    if let Type::User(UserType::Enum(ed)) = current_shape.ty
                        && let Some(variant) = ed.variants.get(idx)
                    {
                        local_path.push(PrettyPathSegment::Variant(Cow::Borrowed(variant.name)));
                    }
                }
                PathStep::MapKey => {
                    if let Def::Map(md) = current_shape.def {
                        current_shape = md.k();
                        if is_displayable_user_type(current_shape) {
                            segments.push(PathSegment {
                                shape: current_segment_shape,
                                local_path: core::mem::take(&mut local_path),
                            });
                            current_segment_shape = current_shape;
                        }
                    }
                }
                PathStep::MapValue => {
                    if let Def::Map(md) = current_shape.def {
                        current_shape = md.v();
                        if is_displayable_user_type(current_shape) {
                            segments.push(PathSegment {
                                shape: current_segment_shape,
                                local_path: core::mem::take(&mut local_path),
                            });
                            current_segment_shape = current_shape;
                        }
                    }
                }
                PathStep::OptionSome => {
                    if let Def::Option(od) = current_shape.def {
                        current_shape = od.t();
                        if is_displayable_user_type(current_shape) {
                            segments.push(PathSegment {
                                shape: current_segment_shape,
                                local_path: core::mem::take(&mut local_path),
                            });
                            current_segment_shape = current_shape;
                        }
                    }
                }
                PathStep::Deref => {
                    if let Def::Pointer(pd) = current_shape.def
                        && let Some(pointee) = pd.pointee()
                    {
                        current_shape = pointee;
                        if is_displayable_user_type(current_shape) {
                            segments.push(PathSegment {
                                shape: current_segment_shape,
                                local_path: core::mem::take(&mut local_path),
                            });
                            current_segment_shape = current_shape;
                        }
                    }
                }
            }
        }

        // Add the final segment only if it has meaningful content
        // (i.e., it has a local path pointing to something, or it's a displayable user type)
        if !local_path.is_empty() || is_displayable_user_type(current_segment_shape) {
            segments.push(PathSegment {
                shape: current_segment_shape,
                local_path,
            });
        }

        segments
    }

    /// Create a miette diagnostic that shows the full type hierarchy with each step highlighted.
    ///
    /// This provides rich terminal output with:
    /// - The leaf type (where the error occurred) as the primary diagnostic
    /// - Parent types shown as related diagnostics
    /// - The relevant field/variant highlighted in each type
    /// - Optional help text
    ///
    /// The `leaf_field` parameter optionally specifies a field name within the leaf type
    /// to highlight. This is useful for "missing field" errors where the path points to
    /// the struct, but we want to highlight the specific missing field.
    pub fn to_diagnostic(
        &self,
        shape: &'static Shape,
        message: impl Into<String>,
        help: Option<String>,
        leaf_field: Option<&'static str>,
    ) -> PathDiagnostic {
        let segments = self.collect_type_segments(shape);
        let message = message.into();

        // Build diagnostics for each segment, leaf first
        let mut diagnostics: Vec<(NamedSource<String>, Vec<LabeledSpan>, String)> = Vec::new();

        // Config: show third-party attrs (like #[facet(kdl::argument)]) but don't expand nested types
        let config = ShapeFormatConfig::new()
            .with_third_party_attrs()
            .without_nested_types();

        for (i, segment) in segments.iter().rev().enumerate() {
            let FormattedShape {
                text,
                spans,
                type_name_span,
            } = format_shape_with_spans_and_config(segment.shape, &config);

            // Use .rs extension so miette-arborium can detect Rust syntax for highlighting
            let source_name = alloc::format!("{}.rs", segment.shape.type_identifier);
            let source = NamedSource::new(source_name, text.clone());

            let mut labels = Vec::new();

            // Add type name underline (no label text)
            if let Some((start, end)) = type_name_span {
                let type_span = SourceSpan::new(start.into(), end - start);
                labels.push(LabeledSpan::new_with_span(None, type_span));
            }

            // Add field/value span with label
            let is_first = i == 0;
            let label_text = if is_first {
                "as requested here"
            } else {
                "via this field"
            };

            // For the leaf segment (first in reversed order), if leaf_field is specified,
            // create a path that includes that field to highlight it directly
            let lookup_path = if let (true, Some(field)) = (is_first, leaf_field) {
                let mut path = segment.local_path.clone();
                path.push(PrettyPathSegment::Field(Cow::Borrowed(field)));
                path
            } else {
                segment.local_path.clone()
            };

            let field_span = if let Some(field_span) = spans.get(&lookup_path) {
                // Use the key span (field name) rather than value span (type)
                SourceSpan::new(field_span.key.0.into(), field_span.key.1 - field_span.key.0)
            } else {
                // Fallback: highlight the whole type
                SourceSpan::new(0.into(), text.len())
            };
            labels.push(LabeledSpan::new_with_span(
                Some(label_text.to_string()),
                field_span,
            ));

            // Message for this diagnostic
            let diag_message = if is_first {
                message.clone()
            } else {
                alloc::format!("in type `{}`", segment.shape.type_identifier)
            };

            diagnostics.push((source, labels, diag_message));
        }

        // First diagnostic becomes the primary, rest become related
        let (source, labels, _primary_message) = diagnostics.remove(0);
        let related: Vec<TypeDiagnostic> = diagnostics
            .into_iter()
            .map(|(source, labels, msg)| TypeDiagnostic {
                message: msg,
                source,
                labels,
            })
            .collect();

        PathDiagnostic {
            message,
            source,
            labels,
            help,
            related,
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
        self.format_pretty_impl(shape, message, help, true)
    }

    /// Format with explicit color control (for testing)
    pub fn format_pretty_no_color(
        &self,
        shape: &'static Shape,
        message: impl Into<String>,
        help: Option<String>,
    ) -> String {
        self.format_pretty_impl(shape, message, help, false)
    }

    fn format_pretty_impl(
        &self,
        shape: &'static Shape,
        message: impl Into<String>,
        help: Option<String>,
        use_color: bool,
    ) -> String {
        use miette::{GraphicalReportHandler, GraphicalTheme};

        let diagnostic = self.to_diagnostic(shape, message, help, None);

        if use_color {
            let report = Report::new(diagnostic);
            format!("{:?}", report)
        } else {
            // Use GraphicalReportHandler with Unicode but no ANSI colors
            let mut output = String::new();
            let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor());
            handler.render_report(&mut output, &diagnostic).unwrap();
            output
        }
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
        path.push(PathStep::Field(0)); // value

        let output = path.format_pretty(Outer::SHAPE, "nested type error", None);

        // Should show both Outer and Inner
        assert!(output.contains("Outer"), "Should show Outer type: {output}");
        assert!(output.contains("Inner"), "Should show Inner type: {output}");
        assert!(
            output.contains("inner"),
            "Should highlight inner field: {output}"
        );
        assert!(
            output.contains("value"),
            "Should highlight value field: {output}"
        );
    }

    #[test]
    fn test_collect_segments_through_vec() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Item {
            id: u32,
            type_info: u64,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Container {
            items: Vec<Item>,
        }

        // Path: items[0].type_info
        let mut path = Path::new();
        path.push(PathStep::Field(0)); // items
        path.push(PathStep::Index(0)); // [0]
        path.push(PathStep::Field(1)); // type_info

        let segments = path.collect_type_segments(Container::SHAPE);

        // Should have 2 segments: Container and Item
        assert_eq!(
            segments.len(),
            2,
            "Expected 2 segments, got {}",
            segments.len()
        );

        assert_eq!(segments[0].shape.type_identifier, "Container");
        assert_eq!(segments[0].local_path.len(), 1);
        assert!(
            matches!(&segments[0].local_path[0], PrettyPathSegment::Field(name) if name == "items")
        );

        assert_eq!(segments[1].shape.type_identifier, "Item");
        assert_eq!(segments[1].local_path.len(), 1);
        assert!(
            matches!(&segments[1].local_path[0], PrettyPathSegment::Field(name) if name == "type_info")
        );
    }

    #[test]
    fn test_collect_segments_through_option() {
        #[derive(Facet)]
        #[allow(dead_code)]
        struct Inner {
            value: i32,
        }

        #[derive(Facet)]
        #[allow(dead_code)]
        struct Config {
            name: String,
            inner: Option<Inner>,
        }

        // Path: inner.Some.value
        let mut path = Path::new();
        path.push(PathStep::Field(1)); // inner
        path.push(PathStep::OptionSome); // Some
        path.push(PathStep::Field(0)); // value

        let segments = path.collect_type_segments(Config::SHAPE);

        // Should have 2 segments: Config and Inner
        assert_eq!(
            segments.len(),
            2,
            "Expected 2 segments, got {}",
            segments.len()
        );

        assert_eq!(segments[0].shape.type_identifier, "Config");
        assert_eq!(segments[1].shape.type_identifier, "Inner");
    }
}
