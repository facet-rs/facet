//! Build a Layout from a Diff.
//!
//! This module converts a `Diff<'mem, 'facet>` into a `Layout` that can be rendered.
//!
//! # Architecture
//!
//! The build process walks the Diff tree while simultaneously navigating the original
//! `from` and `to` Peek values. This allows us to:
//! - Look up unchanged field values from the original structs
//! - Decide whether to show unchanged fields or collapse them
//!
//! The Diff itself only stores what changed - the original Peeks provide context.

use std::borrow::Cow;

use tracing::debug;

use facet_core::{Def, NumericType, PrimitiveType, Shape, StructKind, TextualType, Type, UserType};
use facet_reflect::Peek;
use indextree::{Arena, NodeId};

use super::{
    Attr, DiffFlavor, ElementChange, FormatArena, FormattedValue, Layout, LayoutNode, ValueType,
    group_changed_attrs,
};
use crate::{Diff, ReplaceGroup, Updates, UpdatesGroup, Value};

/// Get the display name for a shape, respecting the `rename` attribute.
fn get_shape_display_name(shape: &Shape) -> &'static str {
    if let Some(renamed) = shape.get_builtin_attr_value::<&str>("rename") {
        return renamed;
    }
    shape.type_identifier
}

/// Check if a shape has any XML namespace attributes (ns_all, rename in xml namespace, etc.)
/// Shapes without XML attributes are "proxy types" - Rust implementation details
/// that wouldn't exist in actual XML output.
fn shape_has_xml_attrs(shape: &Shape) -> bool {
    shape.attributes.iter().any(|attr| attr.ns == Some("xml"))
}

/// Get display name for XML output, prefixing proxy types with `@`.
/// Proxy types are structs without XML namespace attributes - they're Rust
/// implementation details (like PathData) that represent something that would
/// be different in actual XML (like a string attribute).
fn get_xml_display_name(shape: &Shape) -> Cow<'static, str> {
    let base_name = get_shape_display_name(shape);

    // Check if this is a struct without XML attributes (a proxy type)
    if let Type::User(UserType::Struct(_)) = shape.ty
        && !shape_has_xml_attrs(shape)
    {
        return Cow::Owned(format!("@{}", base_name));
    }

    Cow::Borrowed(base_name)
}

/// Get the display name for an enum variant, respecting the `rename` attribute.
fn get_variant_display_name(variant: &facet_core::Variant) -> &'static str {
    if let Some(attr) = variant.get_builtin_attr("rename")
        && let Some(renamed) = attr.get_as::<&'static str>()
    {
        return renamed;
    }
    variant.name
}

/// Check if a value should be skipped in diff output.
///
/// Returns true for "falsy" values like `Option::None`, empty strings, empty vecs, etc.
/// This is used to avoid cluttering diff output with unchanged `None` fields.
fn should_skip_falsy(peek: Peek<'_, '_>) -> bool {
    let shape = peek.shape();
    // Check if the type has a truthiness function (Option<T>, Vec<T>, str, etc.)
    if let Some(truthy_fn) = shape.truthiness_fn() {
        // If the value is falsy, skip it
        let is_truthy = unsafe { truthy_fn(peek.data()) };
        let should_skip = !is_truthy;
        debug!(
            type_id = %shape.type_identifier,
            is_truthy,
            should_skip,
            "should_skip_falsy check"
        );
        return should_skip;
    }
    false
}

/// Determine the type of a value for coloring purposes.
fn determine_value_type(peek: Peek<'_, '_>) -> ValueType {
    let shape = peek.shape();

    // Check the Def first for special types like Option
    if let Def::Option(_) = shape.def {
        // Check if it's None
        if let Ok(opt) = peek.into_option() {
            if opt.is_none() {
                return ValueType::Null;
            }
            // If Some, recurse to get inner type
            if let Some(inner) = opt.value() {
                return determine_value_type(inner);
            }
        }
        return ValueType::Other;
    }

    // Check the Type for primitives
    match shape.ty {
        Type::Primitive(p) => match p {
            PrimitiveType::Boolean => ValueType::Boolean,
            PrimitiveType::Numeric(NumericType::Integer { .. })
            | PrimitiveType::Numeric(NumericType::Float) => ValueType::Number,
            PrimitiveType::Textual(TextualType::Char)
            | PrimitiveType::Textual(TextualType::Str) => ValueType::String,
            PrimitiveType::Never => ValueType::Null,
        },
        _ => ValueType::Other,
    }
}

/// Options for building a layout from a diff.
#[derive(Clone, Debug)]
pub struct BuildOptions {
    /// Maximum line width for attribute grouping.
    pub max_line_width: usize,
    /// Maximum number of unchanged fields to show inline.
    /// If more than this many unchanged fields exist, collapse to "N unchanged".
    pub max_unchanged_fields: usize,
    /// Minimum run length to collapse unchanged sequence elements.
    pub collapse_threshold: usize,
    /// Precision for formatting floating-point numbers.
    /// If set, floats are formatted with this many decimal places.
    /// Useful when using float tolerance in comparisons.
    pub float_precision: Option<usize>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            max_line_width: 80,
            max_unchanged_fields: 5,
            collapse_threshold: 3,
            float_precision: None,
        }
    }
}

impl BuildOptions {
    /// Set the float precision for formatting.
    ///
    /// When set, all floating-point numbers will be formatted with this many
    /// decimal places. This is useful when using float tolerance in comparisons
    /// to ensure the display matches the tolerance level.
    pub fn with_float_precision(mut self, precision: usize) -> Self {
        self.float_precision = Some(precision);
        self
    }
}

/// Build a Layout from a Diff.
///
/// This is the main entry point for converting a diff into a renderable layout.
///
/// # Arguments
///
/// * `diff` - The diff to render
/// * `from` - The original "from" value (for looking up unchanged fields)
/// * `to` - The original "to" value (for looking up unchanged fields)
/// * `opts` - Build options
/// * `flavor` - The output flavor (Rust, JSON, XML)
pub fn build_layout<'mem, 'facet, F: DiffFlavor>(
    diff: &Diff<'mem, 'facet>,
    from: Peek<'mem, 'facet>,
    to: Peek<'mem, 'facet>,
    opts: &BuildOptions,
    flavor: &F,
) -> Layout {
    let mut builder = LayoutBuilder::new(opts.clone(), flavor);
    let root_id = builder.build(diff, Some(from), Some(to));
    builder.finish(root_id)
}

/// Internal builder state.
struct LayoutBuilder<'f, F: DiffFlavor> {
    /// Arena for formatted strings.
    strings: FormatArena,
    /// Arena for layout nodes.
    tree: Arena<LayoutNode>,
    /// Build options.
    opts: BuildOptions,
    /// Output flavor for formatting.
    flavor: &'f F,
}

impl<'f, F: DiffFlavor> LayoutBuilder<'f, F> {
    fn new(opts: BuildOptions, flavor: &'f F) -> Self {
        Self {
            strings: FormatArena::new(),
            tree: Arena::new(),
            opts,
            flavor,
        }
    }

    /// Build the layout from a diff, with optional context Peeks.
    fn build<'mem, 'facet>(
        &mut self,
        diff: &Diff<'mem, 'facet>,
        from: Option<Peek<'mem, 'facet>>,
        to: Option<Peek<'mem, 'facet>>,
    ) -> NodeId {
        self.build_diff(diff, from, to, ElementChange::None)
    }

    /// Build a node from a diff with a given element change type.
    fn build_diff<'mem, 'facet>(
        &mut self,
        diff: &Diff<'mem, 'facet>,
        from: Option<Peek<'mem, 'facet>>,
        to: Option<Peek<'mem, 'facet>>,
        change: ElementChange,
    ) -> NodeId {
        match diff {
            Diff::Equal { value } => {
                // For equal values, render as unchanged text
                if let Some(peek) = value {
                    self.build_peek(*peek, ElementChange::None)
                } else {
                    // No value available, create a placeholder
                    let (span, width) = self.strings.push_str("(equal)");
                    let value = FormattedValue::new(span, width);
                    self.tree.new_node(LayoutNode::Text {
                        value,
                        change: ElementChange::None,
                    })
                }
            }
            Diff::Replace { from, to } => {
                // Create a container element with deleted and inserted children
                let root = self.tree.new_node(LayoutNode::element("_replace"));

                let from_node = self.build_peek(*from, ElementChange::Deleted);
                let to_node = self.build_peek(*to, ElementChange::Inserted);

                root.append(from_node, &mut self.tree);
                root.append(to_node, &mut self.tree);

                root
            }
            Diff::User {
                from: from_shape,
                to: _to_shape,
                variant,
                value,
            } => {
                // Handle Option<T> transparently - don't create an <Option> element wrapper
                // Option is a Rust implementation detail that shouldn't leak into XML diff output
                if matches!(from_shape.def, Def::Option(_))
                    && let Value::Tuple { updates } = value
                {
                    // Unwrap from/to to get inner Option values
                    let inner_from =
                        from.and_then(|p| p.into_option().ok().and_then(|opt| opt.value()));
                    let inner_to =
                        to.and_then(|p| p.into_option().ok().and_then(|opt| opt.value()));

                    // Build updates without an Option wrapper
                    // Use a transparent container that just holds the children
                    return self.build_tuple_transparent(updates, inner_from, inner_to, change);
                }

                // Handle enum variants transparently - use variant name as tag
                // This makes enums like SvgNode::Path render as <path> not <SvgNode><Path>
                if let Some(variant_name) = *variant
                    && let Type::User(UserType::Enum(enum_ty)) = from_shape.ty
                {
                    // Look up the variant to get the rename attribute
                    let tag =
                        if let Some(v) = enum_ty.variants.iter().find(|v| v.name == variant_name) {
                            Cow::Borrowed(get_variant_display_name(v))
                        } else {
                            Cow::Borrowed(variant_name)
                        };
                    debug!(
                        tag = tag.as_ref(),
                        variant_name, "Diff::User enum variant - using variant tag"
                    );

                    // For tuple variants (newtypes), make them transparent
                    if let Value::Tuple { updates } = value {
                        // Unwrap from/to to get inner enum values
                        let inner_from = from.and_then(|p| {
                            p.into_enum().ok().and_then(|e| e.field(0).ok().flatten())
                        });
                        let inner_to = to.and_then(|p| {
                            p.into_enum().ok().and_then(|e| e.field(0).ok().flatten())
                        });

                        // Build the inner content with the variant tag
                        return self
                            .build_enum_tuple_variant(tag, updates, inner_from, inner_to, change);
                    }

                    // For struct variants, use the variant tag directly
                    if let Value::Struct {
                        updates,
                        deletions,
                        insertions,
                        unchanged,
                    } = value
                    {
                        return self.build_struct(
                            tag, None, updates, deletions, insertions, unchanged, from, to, change,
                        );
                    }
                }

                // Get type name for the tag, respecting `rename` attribute
                // Use get_xml_display_name to prefix proxy types with `@`
                let tag = get_xml_display_name(from_shape);
                debug!(tag = tag.as_ref(), variant = ?variant, value_type = ?std::mem::discriminant(value), "Diff::User");

                match value {
                    Value::Struct {
                        updates,
                        deletions,
                        insertions,
                        unchanged,
                    } => self.build_struct(
                        tag, *variant, updates, deletions, insertions, unchanged, from, to, change,
                    ),
                    Value::Tuple { updates } => {
                        debug!(tag = tag.as_ref(), "Value::Tuple - building tuple");
                        self.build_tuple(tag, *variant, updates, from, to, change)
                    }
                }
            }
            Diff::Sequence {
                from: _seq_shape_from,
                to: _seq_shape_to,
                updates,
            } => {
                // Get item type from the from/to Peek values passed to build_diff
                let item_type = from
                    .and_then(|p| p.into_list_like().ok())
                    .and_then(|list| list.iter().next())
                    .or_else(|| {
                        to.and_then(|p| p.into_list_like().ok())
                            .and_then(|list| list.iter().next())
                    })
                    .map(|item| get_shape_display_name(item.shape()))
                    .unwrap_or("item");
                self.build_sequence(updates, change, item_type)
            }
        }
    }

    /// Build a node from a Peek value.
    fn build_peek(&mut self, peek: Peek<'_, '_>, change: ElementChange) -> NodeId {
        let shape = peek.shape();
        debug!(
            type_id = %shape.type_identifier,
            def = ?shape.def,
            change = ?change,
            "build_peek"
        );

        // Check if this is a struct we can recurse into
        match (shape.def, shape.ty) {
            // Handle Option<T> by unwrapping to the inner value
            (Def::Option(_), _) => {
                if let Ok(opt) = peek.into_option()
                    && let Some(inner) = opt.value()
                {
                    // Recurse into the inner value
                    return self.build_peek(inner, change);
                }
                // None - render as null text
                let (span, width) = self.strings.push_str("null");
                return self.tree.new_node(LayoutNode::Text {
                    value: FormattedValue::with_type(span, width, ValueType::Null),
                    change,
                });
            }
            (_, Type::User(UserType::Struct(ty))) if ty.kind == StructKind::Struct => {
                // Build as element with fields as attributes
                if let Ok(struct_peek) = peek.into_struct() {
                    let tag = get_xml_display_name(shape);
                    let mut attrs = Vec::new();

                    for (i, field) in ty.fields.iter().enumerate() {
                        if let Ok(field_value) = struct_peek.field(i) {
                            // Skip falsy values (e.g., Option::None)
                            if should_skip_falsy(field_value) {
                                continue;
                            }
                            let formatted_value = self.format_peek(field_value);
                            let attr = match change {
                                ElementChange::None => {
                                    Attr::unchanged(field.name, field.name.len(), formatted_value)
                                }
                                ElementChange::Deleted => {
                                    Attr::deleted(field.name, field.name.len(), formatted_value)
                                }
                                ElementChange::Inserted => {
                                    Attr::inserted(field.name, field.name.len(), formatted_value)
                                }
                                ElementChange::MovedFrom | ElementChange::MovedTo => {
                                    // For moved elements, show fields as unchanged
                                    Attr::unchanged(field.name, field.name.len(), formatted_value)
                                }
                            };
                            attrs.push(attr);
                        }
                    }

                    let changed_groups = group_changed_attrs(&attrs, self.opts.max_line_width, 0);

                    return self.tree.new_node(LayoutNode::Element {
                        tag,
                        field_name: None,
                        attrs,
                        changed_groups,
                        change,
                    });
                }
            }
            (_, Type::User(UserType::Enum(_))) => {
                // Build enum as element with variant name as tag (respecting rename attribute)
                debug!(type_id = %shape.type_identifier, "processing enum");
                if let Ok(enum_peek) = peek.into_enum()
                    && let Ok(variant) = enum_peek.active_variant()
                {
                    let tag_str = get_variant_display_name(variant);
                    let fields = &variant.data.fields;
                    debug!(
                        variant_name = tag_str,
                        fields_count = fields.len(),
                        "enum variant"
                    );

                    // If variant has fields, build as element with those fields
                    if !fields.is_empty() {
                        // Check for newtype pattern: single field with same-named inner type
                        // e.g., `Circle(Circle)` where we want to show Circle's fields directly
                        if fields.len() == 1
                            && let Ok(Some(inner_value)) = enum_peek.field(0)
                        {
                            let inner_shape = inner_value.shape();
                            // If it's a struct, recurse into it but use the variant name
                            if let Type::User(UserType::Struct(s)) = inner_shape.ty
                                && s.kind == StructKind::Struct
                                && let Ok(struct_peek) = inner_value.into_struct()
                            {
                                let mut attrs = Vec::new();

                                for (i, field) in s.fields.iter().enumerate() {
                                    if let Ok(field_value) = struct_peek.field(i) {
                                        // Skip falsy values (e.g., Option::None)
                                        if should_skip_falsy(field_value) {
                                            continue;
                                        }
                                        let formatted_value = self.format_peek(field_value);
                                        let attr = match change {
                                            ElementChange::None => Attr::unchanged(
                                                field.name,
                                                field.name.len(),
                                                formatted_value,
                                            ),
                                            ElementChange::Deleted => Attr::deleted(
                                                field.name,
                                                field.name.len(),
                                                formatted_value,
                                            ),
                                            ElementChange::Inserted => Attr::inserted(
                                                field.name,
                                                field.name.len(),
                                                formatted_value,
                                            ),
                                            ElementChange::MovedFrom | ElementChange::MovedTo => {
                                                Attr::unchanged(
                                                    field.name,
                                                    field.name.len(),
                                                    formatted_value,
                                                )
                                            }
                                        };
                                        attrs.push(attr);
                                    }
                                }

                                let changed_groups =
                                    group_changed_attrs(&attrs, self.opts.max_line_width, 0);

                                return self.tree.new_node(LayoutNode::Element {
                                    tag: Cow::Borrowed(tag_str),
                                    field_name: None,
                                    attrs,
                                    changed_groups,
                                    change,
                                });
                            }
                        }

                        // General case: show variant fields directly
                        let mut attrs = Vec::new();

                        for (i, field) in fields.iter().enumerate() {
                            if let Ok(Some(field_value)) = enum_peek.field(i) {
                                // Skip falsy values (e.g., Option::None)
                                if should_skip_falsy(field_value) {
                                    continue;
                                }
                                let formatted_value = self.format_peek(field_value);
                                let attr = match change {
                                    ElementChange::None => Attr::unchanged(
                                        field.name,
                                        field.name.len(),
                                        formatted_value,
                                    ),
                                    ElementChange::Deleted => {
                                        Attr::deleted(field.name, field.name.len(), formatted_value)
                                    }
                                    ElementChange::Inserted => Attr::inserted(
                                        field.name,
                                        field.name.len(),
                                        formatted_value,
                                    ),
                                    ElementChange::MovedFrom | ElementChange::MovedTo => {
                                        Attr::unchanged(
                                            field.name,
                                            field.name.len(),
                                            formatted_value,
                                        )
                                    }
                                };
                                attrs.push(attr);
                            }
                        }

                        let changed_groups =
                            group_changed_attrs(&attrs, self.opts.max_line_width, 0);

                        return self.tree.new_node(LayoutNode::Element {
                            tag: Cow::Borrowed(tag_str),
                            field_name: None,
                            attrs,
                            changed_groups,
                            change,
                        });
                    } else {
                        // Unit variant - just show the variant name as text
                        let (span, width) = self.strings.push_str(tag_str);
                        return self.tree.new_node(LayoutNode::Text {
                            value: FormattedValue::new(span, width),
                            change,
                        });
                    }
                }
            }
            _ => {}
        }

        // Default: format as text
        let formatted = self.format_peek(peek);
        self.tree.new_node(LayoutNode::Text {
            value: formatted,
            change,
        })
    }

    /// Build a struct diff as an element with attributes.
    #[allow(clippy::too_many_arguments)]
    fn build_struct<'mem, 'facet>(
        &mut self,
        tag: Cow<'static, str>,
        variant: Option<&'static str>,
        updates: &std::collections::HashMap<Cow<'static, str>, Diff<'mem, 'facet>>,
        deletions: &std::collections::HashMap<Cow<'static, str>, Peek<'mem, 'facet>>,
        insertions: &std::collections::HashMap<Cow<'static, str>, Peek<'mem, 'facet>>,
        unchanged: &std::collections::HashSet<Cow<'static, str>>,
        from: Option<Peek<'mem, 'facet>>,
        to: Option<Peek<'mem, 'facet>>,
        change: ElementChange,
    ) -> NodeId {
        let element_tag = tag;

        // If there's a variant, we should indicate it somehow.
        // TODO: LayoutNode::Element should have an optional variant: Option<&'static str>
        if variant.is_some() {
            // For now, just use the tag
        }

        let mut attrs = Vec::new();
        let mut child_nodes = Vec::new();

        // Handle unchanged fields - try to get values from the original Peek
        debug!(
            unchanged_count = unchanged.len(),
            updates_count = updates.len(),
            deletions_count = deletions.len(),
            insertions_count = insertions.len(),
            unchanged_fields = ?unchanged.iter().collect::<Vec<_>>(),
            updates_fields = ?updates.keys().collect::<Vec<_>>(),
            "build_struct"
        );
        if !unchanged.is_empty() {
            let unchanged_count = unchanged.len();

            if unchanged_count <= self.opts.max_unchanged_fields {
                // Show unchanged fields with their values (if we have the original Peek)
                if let Some(from_peek) = from {
                    if let Ok(struct_peek) = from_peek.into_struct() {
                        let mut sorted_unchanged: Vec<_> = unchanged.iter().collect();
                        sorted_unchanged.sort();

                        for field_name in sorted_unchanged {
                            if let Ok(field_value) = struct_peek.field_by_name(field_name) {
                                let field_shape = field_value.shape();
                                debug!(
                                    field_name = %field_name,
                                    field_type = %field_shape.type_identifier,
                                    "processing unchanged field"
                                );
                                // Skip falsy values (e.g., Option::None) in unchanged fields
                                if should_skip_falsy(field_value) {
                                    debug!(field_name = %field_name, "skipping falsy field");
                                    continue;
                                }
                                let formatted = self.format_peek(field_value);
                                let name_width = field_name.len();
                                let attr =
                                    Attr::unchanged(field_name.clone(), name_width, formatted);
                                attrs.push(attr);
                            }
                        }
                    }
                } else {
                    // No original Peek available - add a collapsed placeholder
                    // We'll handle this after building the element
                }
            }
            // If more than max_unchanged_fields, we'll add a collapsed node as a child
        }

        // Process updates - these become changed attributes or nested children
        let mut sorted_updates: Vec<_> = updates.iter().collect();
        sorted_updates.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (field_name, field_diff) in sorted_updates {
            // Navigate into the field in from/to Peeks for nested context
            let field_from = from.and_then(|p| {
                p.into_struct()
                    .ok()
                    .and_then(|s| s.field_by_name(field_name).ok())
            });
            let field_to = to.and_then(|p| {
                p.into_struct()
                    .ok()
                    .and_then(|s| s.field_by_name(field_name).ok())
            });

            match field_diff {
                Diff::Replace { from, to } => {
                    // Check if this is a complex type that should be built as children
                    let from_shape = from.shape();
                    let is_complex = match from_shape.ty {
                        Type::User(UserType::Enum(_)) => true,
                        Type::User(UserType::Struct(s)) if s.kind == StructKind::Struct => true,
                        _ => false,
                    };

                    if is_complex {
                        // Build from/to as separate child elements
                        let from_node = self.build_peek(*from, ElementChange::Deleted);
                        let to_node = self.build_peek(*to, ElementChange::Inserted);

                        // Set field name on both nodes
                        if let Cow::Borrowed(name) = field_name {
                            if let Some(node) = self.tree.get_mut(from_node)
                                && let LayoutNode::Element { field_name, .. } = node.get_mut()
                            {
                                *field_name = Some(name);
                            }
                            if let Some(node) = self.tree.get_mut(to_node)
                                && let LayoutNode::Element { field_name, .. } = node.get_mut()
                            {
                                *field_name = Some(name);
                            }
                        }

                        child_nodes.push(from_node);
                        child_nodes.push(to_node);
                    } else {
                        // Scalar replacement - show as changed attribute
                        let old_value = self.format_peek(*from);
                        let new_value = self.format_peek(*to);
                        let name_width = field_name.len();
                        let attr =
                            Attr::changed(field_name.clone(), name_width, old_value, new_value);
                        attrs.push(attr);
                    }
                }
                // Handle Option<scalar> as attribute changes, not children
                Diff::User {
                    from: shape,
                    value: Value::Tuple { .. },
                    ..
                } if matches!(shape.def, Def::Option(_)) => {
                    // Check if we can get scalar values from the Option
                    if let (Some(from_peek), Some(to_peek)) = (field_from, field_to) {
                        // Unwrap Option to get inner values
                        let inner_from = from_peek.into_option().ok().and_then(|opt| opt.value());
                        let inner_to = to_peek.into_option().ok().and_then(|opt| opt.value());

                        if let (Some(from_val), Some(to_val)) = (inner_from, inner_to) {
                            // Check if inner type is scalar (not struct/enum)
                            let is_scalar = match from_val.shape().ty {
                                Type::User(UserType::Enum(_)) => false,
                                Type::User(UserType::Struct(s)) if s.kind == StructKind::Struct => {
                                    false
                                }
                                _ => true,
                            };

                            if is_scalar {
                                // Treat as scalar attribute change
                                let old_value = self.format_peek(from_val);
                                let new_value = self.format_peek(to_val);
                                let name_width = field_name.len();
                                let attr = Attr::changed(
                                    field_name.clone(),
                                    name_width,
                                    old_value,
                                    new_value,
                                );
                                attrs.push(attr);
                                continue;
                            }
                        }
                    }

                    // Fall through to child handling if not a simple scalar Option
                    let child =
                        self.build_diff(field_diff, field_from, field_to, ElementChange::None);
                    if let Cow::Borrowed(name) = field_name
                        && let Some(node) = self.tree.get_mut(child)
                    {
                        match node.get_mut() {
                            LayoutNode::Element { field_name, .. } => {
                                *field_name = Some(name);
                            }
                            LayoutNode::Sequence { field_name, .. } => {
                                *field_name = Some(name);
                            }
                            _ => {}
                        }
                    }
                    child_nodes.push(child);
                }
                // Handle single-field wrapper structs (like SvgStyle) as inline attributes
                // instead of nested child elements
                Diff::User {
                    from: inner_shape,
                    value:
                        Value::Struct {
                            updates: inner_updates,
                            deletions: inner_deletions,
                            insertions: inner_insertions,
                            unchanged: inner_unchanged,
                        },
                    ..
                } if inner_updates.len() == 1
                    && inner_deletions.is_empty()
                    && inner_insertions.is_empty()
                    && inner_unchanged.is_empty() =>
                {
                    // Single-field struct with one update - check if it's a scalar change
                    let (inner_field_name, inner_field_diff) = inner_updates.iter().next().unwrap();

                    // Check if the inner field's change is a scalar Replace
                    if let Diff::Replace {
                        from: inner_from,
                        to: inner_to,
                    } = inner_field_diff
                    {
                        // Check if inner type is scalar (not struct/enum)
                        let is_scalar = match inner_from.shape().ty {
                            Type::User(UserType::Enum(_)) => false,
                            Type::User(UserType::Struct(s)) if s.kind == StructKind::Struct => {
                                false
                            }
                            _ => true,
                        };

                        if is_scalar {
                            // Inline as attribute change using the parent field name
                            debug!(
                                field_name = %field_name,
                                inner_type = %inner_shape.type_identifier,
                                inner_field = %inner_field_name,
                                "inlining single-field wrapper as attribute"
                            );
                            let old_value = self.format_peek(*inner_from);
                            let new_value = self.format_peek(*inner_to);
                            let name_width = field_name.len();
                            let attr =
                                Attr::changed(field_name.clone(), name_width, old_value, new_value);
                            attrs.push(attr);
                            continue;
                        }
                    }

                    // Fall through to default child handling
                    let child =
                        self.build_diff(field_diff, field_from, field_to, ElementChange::None);
                    if let Cow::Borrowed(name) = field_name
                        && let Some(node) = self.tree.get_mut(child)
                    {
                        match node.get_mut() {
                            LayoutNode::Element { field_name, .. } => {
                                *field_name = Some(name);
                            }
                            LayoutNode::Sequence { field_name, .. } => {
                                *field_name = Some(name);
                            }
                            _ => {}
                        }
                    }
                    child_nodes.push(child);
                }
                _ => {
                    // Nested diff - build as child element or sequence
                    let child =
                        self.build_diff(field_diff, field_from, field_to, ElementChange::None);

                    // Set the field name on the child (only for borrowed names for now)
                    // TODO: Support owned field names for nested elements
                    if let Cow::Borrowed(name) = field_name
                        && let Some(node) = self.tree.get_mut(child)
                    {
                        match node.get_mut() {
                            LayoutNode::Element { field_name, .. } => {
                                *field_name = Some(name);
                            }
                            LayoutNode::Sequence { field_name, .. } => {
                                *field_name = Some(name);
                            }
                            _ => {}
                        }
                    }

                    child_nodes.push(child);
                }
            }
        }

        // Process deletions
        let mut sorted_deletions: Vec<_> = deletions.iter().collect();
        sorted_deletions.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (field_name, value) in sorted_deletions {
            let formatted = self.format_peek(*value);
            let name_width = field_name.len();
            let attr = Attr::deleted(field_name.clone(), name_width, formatted);
            attrs.push(attr);
        }

        // Process insertions
        let mut sorted_insertions: Vec<_> = insertions.iter().collect();
        sorted_insertions.sort_by(|(a, _), (b, _)| a.cmp(b));

        for (field_name, value) in sorted_insertions {
            let formatted = self.format_peek(*value);
            let name_width = field_name.len();
            let attr = Attr::inserted(field_name.clone(), name_width, formatted);
            attrs.push(attr);
        }

        // Group changed attributes for alignment
        let changed_groups = group_changed_attrs(&attrs, self.opts.max_line_width, 0);

        // Create the element node
        let node = self.tree.new_node(LayoutNode::Element {
            tag: element_tag,
            field_name: None, // Will be set by parent if this is a struct field
            attrs,
            changed_groups,
            change,
        });

        // Add children
        for child in child_nodes {
            node.append(child, &mut self.tree);
        }

        // Add collapsed unchanged fields indicator if needed
        let unchanged_count = unchanged.len();
        if unchanged_count > self.opts.max_unchanged_fields
            || (unchanged_count > 0 && from.is_none())
        {
            let collapsed = self.tree.new_node(LayoutNode::collapsed(unchanged_count));
            node.append(collapsed, &mut self.tree);
        }

        node
    }

    /// Build a tuple diff.
    fn build_tuple<'mem, 'facet>(
        &mut self,
        tag: Cow<'static, str>,
        variant: Option<&'static str>,
        updates: &Updates<'mem, 'facet>,
        _from: Option<Peek<'mem, 'facet>>,
        _to: Option<Peek<'mem, 'facet>>,
        change: ElementChange,
    ) -> NodeId {
        // Same variant issue as build_struct
        if variant.is_some() {
            // TODO: LayoutNode::Element should support variant display
        }

        // Create element for the tuple
        let node = self.tree.new_node(LayoutNode::Element {
            tag,
            field_name: None,
            attrs: Vec::new(),
            changed_groups: Vec::new(),
            change,
        });

        // Build children from updates (tuple items don't have specific type names)
        self.build_updates_children(node, updates, "item");

        node
    }

    /// Build a tuple diff without a wrapper element (for transparent types like Option).
    ///
    /// This builds the updates directly without creating a containing element.
    /// If there's a single child, returns it directly. Otherwise returns
    /// a transparent wrapper element.
    fn build_tuple_transparent<'mem, 'facet>(
        &mut self,
        updates: &Updates<'mem, 'facet>,
        _from: Option<Peek<'mem, 'facet>>,
        _to: Option<Peek<'mem, 'facet>>,
        change: ElementChange,
    ) -> NodeId {
        // Create a temporary container to collect children
        let temp = self.tree.new_node(LayoutNode::Element {
            tag: Cow::Borrowed("_transparent"),
            field_name: None,
            attrs: Vec::new(),
            changed_groups: Vec::new(),
            change,
        });

        // Build children into the temporary container
        self.build_updates_children(temp, updates, "item");

        // Check how many children we have
        let children: Vec<_> = temp.children(&self.tree).collect();

        if children.len() == 1 {
            // Single child - detach it from temp and return it directly
            let child = children[0];
            child.detach(&mut self.tree);
            // Remove the temporary node
            temp.remove(&mut self.tree);
            child
        } else {
            // Multiple children or none - return the container
            // (it will render as transparent due to the "_transparent" tag)
            temp
        }
    }

    /// Build an enum tuple variant (newtype pattern) with the variant tag.
    ///
    /// For enums like `SvgNode::Path(Path)`, this:
    /// 1. Uses the variant's renamed tag (e.g., "path") as the element name
    /// 2. Extracts the inner struct's fields as element attributes
    ///
    /// This makes enum variants transparent in the diff output.
    fn build_enum_tuple_variant<'mem, 'facet>(
        &mut self,
        tag: Cow<'static, str>,
        updates: &Updates<'mem, 'facet>,
        inner_from: Option<Peek<'mem, 'facet>>,
        inner_to: Option<Peek<'mem, 'facet>>,
        change: ElementChange,
    ) -> NodeId {
        // Check if this is a single-element tuple (newtype pattern)
        // For newtype variants, the updates should contain a single diff for the inner value
        let interspersed = &updates.0;

        // Check for a single replacement (1 removal + 1 addition) in the first update group
        // This handles cases where the inner struct is fully replaced
        if let Some(update_group) = &interspersed.first {
            let group_interspersed = &update_group.0;

            // Check the first ReplaceGroup for a single replacement
            if let Some(replace_group) = &group_interspersed.first
                && replace_group.removals.len() == 1
                && replace_group.additions.len() == 1
            {
                let from = replace_group.removals[0];
                let to = replace_group.additions[0];

                // Compare fields and only show those that actually differ
                let mut attrs = Vec::new();

                if let (Ok(from_struct), Ok(to_struct)) = (from.into_struct(), to.into_struct())
                    && let Type::User(UserType::Struct(ty)) = from.shape().ty
                {
                    for (i, field) in ty.fields.iter().enumerate() {
                        let from_value = from_struct.field(i).ok();
                        let to_value = to_struct.field(i).ok();

                        match (from_value, to_value) {
                            (Some(fv), Some(tv)) => {
                                // Both present - compare formatted values
                                let from_formatted = self.format_peek(fv);
                                let to_formatted = self.format_peek(tv);

                                if self.strings.get(from_formatted.span)
                                    != self.strings.get(to_formatted.span)
                                {
                                    // Values differ - show as changed
                                    attrs.push(Attr::changed(
                                        Cow::Borrowed(field.name),
                                        field.name.len(),
                                        from_formatted,
                                        to_formatted,
                                    ));
                                } else {
                                    // Values same - show as unchanged (if not falsy)
                                    if !should_skip_falsy(fv) {
                                        attrs.push(Attr::unchanged(
                                            Cow::Borrowed(field.name),
                                            field.name.len(),
                                            from_formatted,
                                        ));
                                    }
                                }
                            }
                            (Some(fv), None) => {
                                // Only in from - deleted
                                if !should_skip_falsy(fv) {
                                    let formatted = self.format_peek(fv);
                                    attrs.push(Attr::deleted(
                                        Cow::Borrowed(field.name),
                                        field.name.len(),
                                        formatted,
                                    ));
                                }
                            }
                            (None, Some(tv)) => {
                                // Only in to - inserted
                                if !should_skip_falsy(tv) {
                                    let formatted = self.format_peek(tv);
                                    attrs.push(Attr::inserted(
                                        Cow::Borrowed(field.name),
                                        field.name.len(),
                                        formatted,
                                    ));
                                }
                            }
                            (None, None) => {
                                // Neither present - skip
                            }
                        }
                    }
                }

                let changed_groups = group_changed_attrs(&attrs, self.opts.max_line_width, 0);

                return self.tree.new_node(LayoutNode::Element {
                    tag,
                    field_name: None,
                    attrs,
                    changed_groups,
                    change,
                });
            }
        }

        // Try to find the single nested diff
        let single_diff = {
            let mut found_diff: Option<&Diff<'mem, 'facet>> = None;

            // Check first update group
            if let Some(update_group) = &interspersed.first {
                let group_interspersed = &update_group.0;

                // Check for nested diffs in the first group
                if let Some(diffs) = &group_interspersed.last
                    && diffs.len() == 1
                    && found_diff.is_none()
                {
                    found_diff = Some(&diffs[0]);
                }
                for (diffs, _replace) in &group_interspersed.values {
                    if diffs.len() == 1 && found_diff.is_none() {
                        found_diff = Some(&diffs[0]);
                    }
                }
            }

            found_diff
        };

        // If we have a single nested diff, handle it with our variant tag
        if let Some(diff) = single_diff {
            match diff {
                Diff::User {
                    value:
                        Value::Struct {
                            updates,
                            deletions,
                            insertions,
                            unchanged,
                        },
                    ..
                } => {
                    // Build the struct with our variant tag
                    return self.build_struct(
                        tag.clone(),
                        None,
                        updates,
                        deletions,
                        insertions,
                        unchanged,
                        inner_from,
                        inner_to,
                        change,
                    );
                }
                Diff::Replace { from, to } => {
                    // For replacements, show both values as attributes with change markers
                    // This handles cases where the inner struct is fully different
                    let mut attrs = Vec::new();

                    // Build attrs from the "from" struct (deleted)
                    if let Ok(struct_peek) = from.into_struct()
                        && let Type::User(UserType::Struct(ty)) = from.shape().ty
                    {
                        for (i, field) in ty.fields.iter().enumerate() {
                            if let Ok(field_value) = struct_peek.field(i) {
                                if should_skip_falsy(field_value) {
                                    continue;
                                }
                                let formatted = self.format_peek(field_value);
                                attrs.push(Attr::deleted(
                                    Cow::Borrowed(field.name),
                                    field.name.len(),
                                    formatted,
                                ));
                            }
                        }
                    }

                    // Build attrs from the "to" struct (inserted)
                    if let Ok(struct_peek) = to.into_struct()
                        && let Type::User(UserType::Struct(ty)) = to.shape().ty
                    {
                        for (i, field) in ty.fields.iter().enumerate() {
                            if let Ok(field_value) = struct_peek.field(i) {
                                if should_skip_falsy(field_value) {
                                    continue;
                                }
                                let formatted = self.format_peek(field_value);
                                attrs.push(Attr::inserted(
                                    Cow::Borrowed(field.name),
                                    field.name.len(),
                                    formatted,
                                ));
                            }
                        }
                    }

                    let changed_groups = group_changed_attrs(&attrs, self.opts.max_line_width, 0);

                    return self.tree.new_node(LayoutNode::Element {
                        tag: tag.clone(),
                        field_name: None,
                        attrs,
                        changed_groups,
                        change,
                    });
                }
                _ => {}
            }
        }

        // Fallback: create element with tag and build children normally
        let node = self.tree.new_node(LayoutNode::Element {
            tag,
            field_name: None,
            attrs: Vec::new(),
            changed_groups: Vec::new(),
            change,
        });

        // Build children from updates
        self.build_updates_children(node, updates, "item");

        node
    }

    /// Build a sequence diff.
    fn build_sequence(
        &mut self,
        updates: &Updates<'_, '_>,
        change: ElementChange,
        item_type: &'static str,
    ) -> NodeId {
        // Create sequence node with item type info
        let node = self.tree.new_node(LayoutNode::Sequence {
            change,
            item_type,
            field_name: None,
        });

        // Build children from updates
        self.build_updates_children(node, updates, item_type);

        node
    }

    /// Build children from an Updates structure and append to parent.
    ///
    /// This groups consecutive items by their change type (unchanged, deleted, inserted)
    /// and renders them on single lines with optional collapsing for long runs.
    /// Nested diffs (struct items with internal changes) are built as full child nodes.
    fn build_updates_children(
        &mut self,
        parent: NodeId,
        updates: &Updates<'_, '_>,
        _item_type: &'static str,
    ) {
        // Collect simple items (adds/removes) and nested diffs separately
        let mut items: Vec<(Peek<'_, '_>, ElementChange)> = Vec::new();
        let mut nested_diffs: Vec<&Diff<'_, '_>> = Vec::new();

        let interspersed = &updates.0;

        // Process first update group if present
        if let Some(update_group) = &interspersed.first {
            self.collect_updates_group_items(&mut items, &mut nested_diffs, update_group);
        }

        // Process interleaved (unchanged, update) pairs
        for (unchanged_items, update_group) in &interspersed.values {
            // Add unchanged items
            for item in unchanged_items {
                items.push((*item, ElementChange::None));
            }

            self.collect_updates_group_items(&mut items, &mut nested_diffs, update_group);
        }

        // Process trailing unchanged items
        if let Some(unchanged_items) = &interspersed.last {
            for item in unchanged_items {
                items.push((*item, ElementChange::None));
            }
        }

        tracing::debug!(
            items_count = items.len(),
            nested_diffs_count = nested_diffs.len(),
            "collected sequence items"
        );

        // Build nested diffs as full child nodes (struct items with internal changes)
        for diff in nested_diffs {
            debug!(diff_type = ?std::mem::discriminant(diff), "building nested diff");
            // Get from/to Peek from the diff for context
            let (from_peek, to_peek) = match diff {
                Diff::User { .. } => {
                    // For User diffs, we need the actual Peek values
                    // The diff contains the shapes but we need to find the corresponding Peeks
                    // For now, pass None - the build_diff will use the shape info
                    (None, None)
                }
                Diff::Replace { from, to } => (Some(*from), Some(*to)),
                _ => (None, None),
            };
            let child = self.build_diff(diff, from_peek, to_peek, ElementChange::None);
            parent.append(child, &mut self.tree);
        }

        // Render simple items (unchanged, adds, removes)
        for (item_peek, item_change) in items {
            let child = self.build_peek(item_peek, item_change);
            parent.append(child, &mut self.tree);
        }
    }

    /// Collect items from an UpdatesGroup into the items list.
    /// Also returns nested diffs that need to be built as full child nodes.
    fn collect_updates_group_items<'a, 'mem: 'a, 'facet: 'a>(
        &self,
        items: &mut Vec<(Peek<'mem, 'facet>, ElementChange)>,
        nested_diffs: &mut Vec<&'a Diff<'mem, 'facet>>,
        group: &'a UpdatesGroup<'mem, 'facet>,
    ) {
        let interspersed = &group.0;

        // Process first replace group if present
        if let Some(replace) = &interspersed.first {
            self.collect_replace_group_items(items, replace);
        }

        // Process interleaved (diffs, replace) pairs
        for (diffs, replace) in &interspersed.values {
            // Collect nested diffs - these are struct items with internal changes
            for diff in diffs {
                nested_diffs.push(diff);
            }
            self.collect_replace_group_items(items, replace);
        }

        // Process trailing diffs (if any)
        if let Some(diffs) = &interspersed.last {
            for diff in diffs {
                nested_diffs.push(diff);
            }
        }
    }

    /// Collect items from a ReplaceGroup into the items list.
    fn collect_replace_group_items<'a, 'mem: 'a, 'facet: 'a>(
        &self,
        items: &mut Vec<(Peek<'mem, 'facet>, ElementChange)>,
        group: &'a ReplaceGroup<'mem, 'facet>,
    ) {
        // Add removals as deleted
        for removal in &group.removals {
            items.push((*removal, ElementChange::Deleted));
        }

        // Add additions as inserted
        for addition in &group.additions {
            items.push((*addition, ElementChange::Inserted));
        }
    }

    /// Format a Peek value into the arena using the flavor.
    fn format_peek(&mut self, peek: Peek<'_, '_>) -> FormattedValue {
        let shape = peek.shape();
        debug!(
            type_id = %shape.type_identifier,
            def = ?shape.def,
            "format_peek"
        );

        // Unwrap Option types to format the inner value
        if let Def::Option(_) = shape.def
            && let Ok(opt) = peek.into_option()
        {
            if let Some(inner) = opt.value() {
                return self.format_peek(inner);
            }
            // None - format as null
            let (span, width) = self.strings.push_str("null");
            return FormattedValue::with_type(span, width, ValueType::Null);
        }

        // Handle float formatting with precision if configured
        if let Some(precision) = self.opts.float_precision
            && let Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) = shape.ty
        {
            // Try f64 first, then f32
            if let Ok(v) = peek.get::<f64>() {
                let formatted = format!("{:.prec$}", v, prec = precision);
                // Trim trailing zeros and decimal point for cleaner output
                let formatted = formatted.trim_end_matches('0').trim_end_matches('.');
                let (span, width) = self.strings.push_str(formatted);
                return FormattedValue::with_type(span, width, ValueType::Number);
            }
            if let Ok(v) = peek.get::<f32>() {
                let formatted = format!("{:.prec$}", v, prec = precision);
                let formatted = formatted.trim_end_matches('0').trim_end_matches('.');
                let (span, width) = self.strings.push_str(formatted);
                return FormattedValue::with_type(span, width, ValueType::Number);
            }
        }

        let (span, width) = self.strings.format(|w| self.flavor.format_value(peek, w));
        let value_type = determine_value_type(peek);
        FormattedValue::with_type(span, width, value_type)
    }

    /// Finish building and return the Layout.
    fn finish(self, root: NodeId) -> Layout {
        Layout {
            strings: self.strings,
            tree: self.tree,
            root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::render::{RenderOptions, render_to_string};
    use crate::layout::{RustFlavor, XmlFlavor};

    #[test]
    fn test_build_equal_diff() {
        let value = 42i32;
        let peek = Peek::new(&value);
        let diff = Diff::Equal { value: Some(peek) };

        let layout = build_layout(&diff, peek, peek, &BuildOptions::default(), &RustFlavor);

        // Should produce a single text node
        let root = layout.get(layout.root).unwrap();
        assert!(matches!(root, LayoutNode::Text { .. }));
    }

    #[test]
    fn test_build_replace_diff() {
        let from = 10i32;
        let to = 20i32;
        let diff = Diff::Replace {
            from: Peek::new(&from),
            to: Peek::new(&to),
        };

        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
            &RustFlavor,
        );

        // Should produce an element with two children
        let root = layout.get(layout.root).unwrap();
        match root {
            LayoutNode::Element { tag, .. } => assert_eq!(tag.as_ref(), "_replace"),
            _ => panic!("expected Element node"),
        }

        let children: Vec<_> = layout.children(layout.root).collect();
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn test_build_and_render_replace() {
        let from = 10i32;
        let to = 20i32;
        let diff = Diff::Replace {
            from: Peek::new(&from),
            to: Peek::new(&to),
        };

        let layout = build_layout(
            &diff,
            Peek::new(&from),
            Peek::new(&to),
            &BuildOptions::default(),
            &RustFlavor,
        );
        let output = render_to_string(&layout, &RenderOptions::plain(), &XmlFlavor);

        // Should contain both values with appropriate markers
        assert!(
            output.contains("10"),
            "output should contain old value: {}",
            output
        );
        assert!(
            output.contains("20"),
            "output should contain new value: {}",
            output
        );
    }

    #[test]
    fn test_build_options_default() {
        let opts = BuildOptions::default();
        assert_eq!(opts.max_line_width, 80);
        assert_eq!(opts.max_unchanged_fields, 5);
        assert_eq!(opts.collapse_threshold, 3);
    }
}
