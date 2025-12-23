//! Pretty-printing support for paths using facet-pretty and miette.
//!
//! This module provides rich error rendering that shows the type structure
//! with the error location highlighted using miette's diagnostic rendering.

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;

use facet_core::{Def, Shape, Type, UserType};
use facet_pretty::{FormattedShape, PathSegment as PrettyPathSegment, format_shape_with_spans};
use miette::{Diagnostic, LabeledSpan, NamedSource, Report, SourceSpan};

use crate::{Path, PathStep};

/// A single type in the diagnostic chain, showing one step in the path.
#[derive(Debug)]
struct TypeDiagnostic {
    /// The formatted type definition
    source_code: NamedSource<String>,
    /// The span to highlight (the field/variant leading to the error)
    span: SourceSpan,
    /// Label for this span
    label: String,
}

/// A diagnostic error that shows the full type hierarchy with each step highlighted.
#[derive(Debug)]
pub struct PathDiagnostic {
    /// The error message
    message: String,
    /// Chain of types from root to leaf, each with their relevant span highlighted
    type_chain: Vec<TypeDiagnostic>,
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
        // Return the first (root) type's source
        self.type_chain
            .first()
            .map(|t| &t.source_code as &dyn miette::SourceCode)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        // Return labels for the first type
        self.type_chain.first().map(|t| {
            Box::new(core::iter::once(LabeledSpan::new_with_span(
                Some(t.label.clone()),
                t.span,
            ))) as Box<dyn Iterator<Item = LabeledSpan> + '_>
        })
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        if self.type_chain.len() <= 1 {
            return None;
        }

        // Create related diagnostics for each subsequent type in the chain
        Some(Box::new(
            self.type_chain[1..].iter().map(|t| t as &dyn Diagnostic),
        ))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn core::fmt::Display + 'a>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h.as_str()) as Box<dyn core::fmt::Display>)
    }
}

// Implement Diagnostic for TypeDiagnostic so it can be used as a related diagnostic
impl core::fmt::Display for TypeDiagnostic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "in type")
    }
}

impl std::error::Error for TypeDiagnostic {}

impl Diagnostic for TypeDiagnostic {
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        Some(&self.source_code)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(core::iter::once(LabeledSpan::new_with_span(
            Some(self.label.clone()),
            self.span,
        ))))
    }
}

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
                        if let Some(PrettyPathSegment::Variant(variant_name)) = local_path.last() {
                            if let Some(variant) =
                                ed.variants.iter().find(|v| v.name == variant_name.as_ref())
                            {
                                if let Some(field) = variant.data.fields.get(idx) {
                                    local_path
                                        .push(PrettyPathSegment::Field(Cow::Borrowed(field.name)));
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
                    if let Type::User(UserType::Enum(ed)) = current_shape.ty {
                        if let Some(variant) = ed.variants.get(idx) {
                            local_path
                                .push(PrettyPathSegment::Variant(Cow::Borrowed(variant.name)));
                        }
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
                    if let Def::Pointer(pd) = current_shape.def {
                        if let Some(pointee) = pd.pointee() {
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
    /// - Each type in the path shown as a separate code block
    /// - The relevant field/variant highlighted in each type
    /// - Optional help text
    pub fn to_diagnostic(
        &self,
        shape: &'static Shape,
        message: impl Into<String>,
        help: Option<String>,
    ) -> PathDiagnostic {
        let segments = self.collect_type_segments(shape);

        let mut type_chain = Vec::with_capacity(segments.len());

        // Iterate in reverse: show the leaf (where the error is) first,
        // then show the path back to the root
        for (i, segment) in segments.iter().rev().enumerate() {
            let FormattedShape { text, spans } = format_shape_with_spans(segment.shape);

            // Determine the label for this segment
            let is_first = i == 0;
            let label = if is_first {
                "error here".to_string()
            } else {
                "via this field".to_string()
            };

            // Find the span for the local path
            let span = if let Some(field_span) = spans.get(&segment.local_path) {
                SourceSpan::new(
                    field_span.value.0.into(),
                    field_span.value.1 - field_span.value.0,
                )
            } else {
                // Fallback: highlight the whole type
                SourceSpan::new(0.into(), text.len())
            };

            type_chain.push(TypeDiagnostic {
                source_code: NamedSource::new(segment.shape.type_identifier, text),
                span,
                label,
            });
        }

        PathDiagnostic {
            message: message.into(),
            type_chain,
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
