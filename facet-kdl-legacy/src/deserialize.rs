//! KDL deserialization implementation.

use std::borrow::Cow;
use std::mem;

use facet_core::{
    Def, EnumType, Facet, Field, NumericType, PrimitiveType, Shape, ShapeLayout, StructType, Type,
    UserType,
};
use facet_reflect::{Partial, is_spanned_shape};
use facet_solver::{
    FieldPath, KeyResult, MatchResult, PathSegment, Resolution, ResolutionHandle, SatisfyResult,
    Schema, Solver,
};
use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};
use miette::SourceSpan;

use crate::error::{KdlError, KdlErrorKind};
use crate::serialize::kebab_to_pascal;

pub(crate) type Result<T> = std::result::Result<T, KdlError>;

/// Extension trait for Field to check KDL-specific child attributes.
///
/// KDL supports both the builtin `#[facet(child)]` attribute and the
/// KDL-specific `#[facet(kdl::child)]` attribute for marking child fields.
pub(crate) trait KdlFieldExt {
    /// Returns true if this field is a child field (either via builtin or kdl::child).
    fn is_kdl_child(&self) -> bool;
}

impl KdlFieldExt for Field {
    fn is_kdl_child(&self) -> bool {
        self.is_child() || self.has_attr(Some("kdl"), "child")
    }
}

/// Extension trait for checking kdl::children attribute
pub(crate) trait KdlChildrenFieldExt {
    /// Returns true if this field has the kdl::children attribute
    fn is_kdl_children(&self) -> bool;

    /// Returns the custom node name from `#[facet(kdl::children = "...")]`
    /// if specified, otherwise None.
    fn kdl_children_node_name(&self) -> Option<&'static str>;
}

impl KdlChildrenFieldExt for Field {
    fn is_kdl_children(&self) -> bool {
        self.has_attr(Some("kdl"), "children")
    }

    fn kdl_children_node_name(&self) -> Option<&'static str> {
        // Get the kdl::children attribute and extract the Option<&'static str> value
        self.get_attr(Some("kdl"), "children").and_then(|attr| {
            // Use the typed accessor to get the Attr enum value
            attr.get_as::<crate::Attr>()
                .and_then(|kdl_attr| match kdl_attr {
                    crate::Attr::Children(opt) => *opt,
                    _ => None,
                })
        })
    }
}

/// Check if a shape is an enum type and return its definition if so.
fn get_enum_type(shape: &Shape) -> Option<EnumType> {
    match &shape.ty {
        Type::User(UserType::Enum(enum_type)) => Some(*enum_type),
        _ => None,
    }
}

/// Find a variant in an enum type that matches the given name.
/// Returns a 'static reference since `EnumType.variants` is `&'static [Variant]`.
fn find_variant_by_name(enum_type: &EnumType, name: &str) -> Option<&'static facet_core::Variant> {
    enum_type.variants.iter().find(|v| v.name == name)
}

/// Check if a node name matches a `kdl::children` field.
///
/// If `custom_node_name` is provided (from `#[facet(kdl::children(node_name = "..."))]`),
/// that is used for exact matching.
///
/// Otherwise, uses `facet_singularize` to check if the node name is the singular form
/// of the field name. For example:
/// - "dependency" matches "dependencies"
/// - "child" matches "children"
/// - "box" matches "boxes"
///
/// This handles irregular plurals (children, people, mice, etc.) as well as
/// standard plural rules (-s, -es, -ies, -ves).
fn node_name_matches_children_field(
    node_name: &str,
    field_name: &str,
    custom_node_name: Option<&str>,
) -> bool {
    if let Some(expected) = custom_node_name {
        // Exact match with the custom node name
        node_name == expected
    } else {
        // Use singularization to match node name to field name
        facet_singularize::is_singular_of(node_name, field_name)
    }
}

/// Result of finding a property field, possibly inside a flattened struct
enum PropertyFieldMatch {
    /// Property field found directly on the struct
    Direct {
        field_name: &'static str,
        /// The field definition (for accessing vtable.deserialize_with)
        field: &'static Field,
    },
    /// Property field found inside a flattened struct
    Flattened {
        /// The flattened field name on the parent struct
        flattened_field_name: &'static str,
        /// The property field name inside the flattened struct
        property_field_name: &'static str,
        /// The inner property field definition (for accessing vtable.deserialize_with)
        inner_field: &'static Field,
    },
}

/// Find a property field by name, checking both direct fields and flattened struct fields.
fn find_property_field(
    fields: &'static [Field],
    property_name: &str,
) -> Option<PropertyFieldMatch> {
    // First check direct fields
    for field in fields {
        if field.has_attr(Some("kdl"), "property") && field.name == property_name {
            return Some(PropertyFieldMatch::Direct {
                field_name: field.name,
                field,
            });
        }
    }

    // Then check flattened struct fields
    for field in fields {
        if field.is_flattened() {
            let field_shape = field.shape();
            if let Type::User(UserType::Struct(struct_def)) = &field_shape.ty {
                for inner_field in struct_def.fields {
                    if inner_field.has_attr(Some("kdl"), "property")
                        && inner_field.name == property_name
                    {
                        return Some(PropertyFieldMatch::Flattened {
                            flattened_field_name: field.name,
                            property_field_name: inner_field.name,
                            inner_field,
                        });
                    }
                }
            }
        }
    }

    None
}

/// Check if a struct type has any flattened fields.
/// When flattened fields exist, we use the solver for proper path resolution and
/// to handle missing optional fields via `missing_optional_fields()`.
fn has_flatten(fields: &[Field]) -> bool {
    fields.iter().any(|f| f.is_flattened())
}

/// An entry in the open paths stack, tracking both the path segment and
/// whether we entered an Option wrapper for this segment.
#[derive(Debug, Clone)]
struct OpenPathEntry {
    segment: PathSegment,
    /// True if we called begin_some() after opening this field
    entered_option: bool,
}

/// Result of matching a KDL node to a field
enum FieldMatchResult {
    /// Node matched a #[facet(child)] field by exact name
    ExactChild(&'static str),
    /// Node matched an enum variant within a #[facet(child)] field
    EnumVariant {
        field_name: &'static str,
        variant_name: &'static str,
        variant_data: StructType,
    },
    /// Node matched a #[facet(children)] container
    ChildrenContainer {
        field_name: &'static str,
        field_index: usize,
    },
}

/// Tracks the state of a children container (list, map, or set)
#[derive(Clone, Copy, PartialEq, Eq)]
enum ChildrenContainerState {
    /// Not currently in a children container
    None,
    /// In a list container (`Vec<T>`) for a specific field
    List { field_index: usize },
    /// In a map container (`HashMap<K, V>` or `BTreeMap<K, V>`) for a specific field
    Map { field_index: usize },
    /// In a set container (`HashSet<T>` or `BTreeSet<T>`) for a specific field
    Set { field_index: usize },
}

impl ChildrenContainerState {
    /// Returns the field index if we're in a container, None otherwise
    fn field_index(&self) -> Option<usize> {
        match self {
            ChildrenContainerState::None => None,
            ChildrenContainerState::List { field_index }
            | ChildrenContainerState::Map { field_index }
            | ChildrenContainerState::Set { field_index } => Some(*field_index),
        }
    }
}

#[allow(dead_code)]
struct KdlDeserializer<'input> {
    kdl: &'input str,
}

impl<'input, 'facet> KdlDeserializer<'input> {
    /// Create an error with source code attached for diagnostics.
    fn err(&self, kind: impl Into<KdlErrorKind>) -> KdlError {
        KdlError::new(kind).with_source(self.kdl.to_string())
    }

    /// Create an error with source code and span attached for diagnostics.
    fn err_at(&self, kind: impl Into<KdlErrorKind>, span: impl Into<SourceSpan>) -> KdlError {
        KdlError::new(kind)
            .with_source(self.kdl.to_string())
            .with_span(span)
    }

    fn from_str<T: Facet<'facet>>(kdl: &'input str) -> Result<T> {
        log::trace!("Entering `from_str` method");

        let document: KdlDocument = kdl.parse()?;
        log::trace!("KDL parsed");

        let partial = Partial::alloc::<T>().expect("failed to allocate");
        let shape = partial.shape();
        log::trace!("Allocated WIP for type {shape}");

        let partial = Self { kdl }.deserialize_toplevel_document(partial, document)?;

        let heap_value = partial.build()?;
        log::trace!("WIP fully built");
        log::trace!("Type of WIP unerased");

        let value = heap_value.materialize()?;
        Ok(value)
    }

    fn deserialize_toplevel_document(
        &mut self,
        partial: Partial<'facet>,
        document: KdlDocument,
    ) -> Result<Partial<'facet>> {
        log::trace!("Entering `deserialize_toplevel_document` method");

        // Check that the target type is a struct with child/children fields
        if let Type::User(UserType::Struct(struct_def)) = &partial.shape().ty {
            log::trace!("Document `Partial` is a struct: {struct_def:#?}");
            let is_valid_toplevel = struct_def
                .fields
                .iter()
                .all(|field| field.is_kdl_child() || field.has_attr(Some("kdl"), "children"));
            log::trace!("WIP represents a valid top-level: {is_valid_toplevel}");

            if is_valid_toplevel {
                return self.deserialize_document(partial, document);
            } else {
                return Err(KdlErrorKind::InvalidDocumentShape(&partial.shape().def).into());
            }
        }

        // Fall back to the def system for backward compatibility
        let def = partial.shape().def;
        match def {
            Def::List(_) => Err(KdlErrorKind::UnsupportedShape(
                "top-level list not yet supported; use a struct with #[facet(children)]".into(),
            )
            .into()),
            _ => Err(KdlErrorKind::InvalidDocumentShape(&partial.shape().def).into()),
        }
    }

    fn deserialize_document(
        &mut self,
        partial: Partial<'facet>,
        document: KdlDocument,
    ) -> Result<Partial<'facet>> {
        self.deserialize_document_with_fields(partial, document, None)
    }

    fn deserialize_document_with_fields(
        &mut self,
        partial: Partial<'facet>,
        mut document: KdlDocument,
        override_fields: Option<&[Field]>,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        log::trace!(
            "Entering `deserialize_document` method at {}",
            partial.path()
        );

        let document_shape = partial.shape();

        let mut children_container_state = ChildrenContainerState::None;

        for node in document.nodes_mut().drain(..) {
            // log::trace!("Processing node: {node:#?}");
            partial = self.deserialize_node_with_fields(
                partial,
                node,
                document_shape,
                &mut children_container_state,
                override_fields,
            )?;
        }

        if children_container_state != ChildrenContainerState::None {
            partial = partial.end()?;
        }

        // Set defaults for any unset child fields that have the DEFAULT flag
        // This handles optional child nodes that weren't present in the document
        let fields: &[Field] = if let Some(fields) = override_fields {
            fields
        } else if let Type::User(UserType::Struct(struct_def)) = document_shape.ty {
            struct_def.fields
        } else {
            &[]
        };

        for (idx, field) in fields.iter().enumerate() {
            // Handle both kdl::child and kdl::children fields
            if (field.is_kdl_child() || field.is_kdl_children())
                && !partial.is_field_set(idx)?
                && (field.has_default() || field.should_skip_deserializing())
            {
                log::trace!("Setting default for unset child field: {}", field.name);
                partial = partial.set_nth_field_to_default(idx)?;
            }
        }

        log::trace!(
            "Exiting `deserialize_document` method at {}",
            partial.path()
        );

        Ok(partial)
    }

    fn deserialize_node_with_fields(
        &mut self,
        partial: Partial<'facet>,
        mut node: KdlNode,
        document_shape: &Shape,
        children_container_state: &mut ChildrenContainerState,
        override_fields: Option<&[Field]>,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        log::trace!("Entering `deserialize_node` method at {}", partial.path());

        // Track whether we found an enum variant to select after beginning the field
        // Also track the variant's StructType for property matching
        let mut enum_variant_to_select: Option<(&str, StructType)> = None;

        // Helper closure to find and process matching fields
        let find_matching_field = |fields: &[Field]| -> Option<FieldMatchResult> {
            // First, try to match by exact field name with CHILD flag
            if let Some(child_field) = fields
                .iter()
                .find(|field| field.is_kdl_child() && field.name == node.name().value())
            {
                return Some(FieldMatchResult::ExactChild(child_field.name));
            }

            // Second, try to match by enum variant name
            if let Some((child_field, variant)) = fields
                .iter()
                .filter(|field| field.is_kdl_child())
                .find_map(|field| {
                    let field_shape = field.shape();
                    if let Some(enum_type) = get_enum_type(field_shape)
                        && let Some(variant) = find_variant_by_name(&enum_type, node.name().value())
                    {
                        return Some((field, variant));
                    }
                    None
                })
            {
                return Some(FieldMatchResult::EnumVariant {
                    field_name: child_field.name,
                    variant_name: variant.name,
                    variant_data: variant.data,
                });
            }

            // Third, try to match as a children container element
            // Collect all fields with kdl::children attribute
            let children_fields: Vec<_> = fields
                .iter()
                .enumerate()
                .filter(|(_, field)| field.has_attr(Some("kdl"), "children"))
                .collect();

            match children_fields.len() {
                0 => None,
                1 => {
                    // Single children field: check if it has a custom node name.
                    // If it does, match by name. If not, use it as a catch-all.
                    let (idx, field) = children_fields[0];
                    let custom_node_name = field.kdl_children_node_name();

                    if custom_node_name.is_some() || fields.len() > 1 {
                        // If there's a custom name, or there are other (non-children) fields,
                        // we need to be selective about what nodes match this field.
                        if node_name_matches_children_field(
                            node.name().value(),
                            field.name,
                            custom_node_name,
                        ) {
                            Some(FieldMatchResult::ChildrenContainer {
                                field_name: field.name,
                                field_index: idx,
                            })
                        } else {
                            None
                        }
                    } else {
                        // No custom name and this is the only field: catch-all behavior
                        Some(FieldMatchResult::ChildrenContainer {
                            field_name: field.name,
                            field_index: idx,
                        })
                    }
                }
                _ => {
                    // Multiple children fields: match by node name (singular-to-plural)
                    // e.g., "dependency" matches field "dependencies"
                    // If the field has a custom node_name, use that for exact matching.
                    children_fields
                        .into_iter()
                        .find(|(_, field)| {
                            let custom_node_name = field.kdl_children_node_name();
                            node_name_matches_children_field(
                                node.name().value(),
                                field.name,
                                custom_node_name,
                            )
                        })
                        .map(|(idx, field)| FieldMatchResult::ChildrenContainer {
                            field_name: field.name,
                            field_index: idx,
                        })
                }
            }
        };

        // Use override_fields if provided, otherwise get fields from document_shape
        let fields: &[Field] = if let Some(fields) = override_fields {
            fields
        } else {
            match document_shape.ty {
                Type::User(UserType::Struct(struct_def)) => struct_def.fields,
                ty => {
                    log::debug!("deserialize_node with unexpected shape: {ty}");
                    return Err(KdlErrorKind::UnsupportedShape(format!(
                        "expected struct, got {ty}"
                    ))
                    .into());
                }
            }
        };

        match find_matching_field(fields) {
            Some(FieldMatchResult::ExactChild(field_name)) => {
                log::trace!("Node matched expected child {field_name}");
                if *children_container_state != ChildrenContainerState::None {
                    partial = partial.end()?;
                    *children_container_state = ChildrenContainerState::None;
                }
                partial = partial.begin_field(field_name)?;
            }
            Some(FieldMatchResult::EnumVariant {
                field_name,
                variant_name,
                variant_data,
            }) => {
                log::trace!("Node matched enum variant {variant_name} of field {field_name}");
                if *children_container_state != ChildrenContainerState::None {
                    partial = partial.end()?;
                    *children_container_state = ChildrenContainerState::None;
                }
                partial = partial.begin_field(field_name)?;
                enum_variant_to_select = Some((variant_name, variant_data));
            }
            Some(FieldMatchResult::ChildrenContainer {
                field_name,
                field_index,
            }) => {
                log::trace!("Node matched children container for field {field_name}");

                // Get the field shape to determine if it's a List or Map
                let children_field = &fields[field_index];
                let field_shape = children_field.shape();

                // Check if we need to open a new container:
                // 1. We're not in any container, or
                // 2. We're in a container for a different field (switching fields)
                let current_field = children_container_state.field_index();
                let need_new_container =
                    current_field.is_none() || current_field != Some(field_index);

                if need_new_container {
                    // Close the previous container if we were in one
                    if *children_container_state != ChildrenContainerState::None {
                        partial = partial.end()?;
                        *children_container_state = ChildrenContainerState::None;
                    }

                    // For children containers, we allow reopening because nodes
                    // can be intermixed in the KDL document (e.g., dependency, sample, dependency)
                    // So we don't check is_field_set here - we'll continue adding to the existing list
                    partial = partial.begin_field(field_name)?;

                    // Check if it's a Map, Set, or List type
                    match field_shape.def {
                        Def::Map(_) => {
                            partial = partial.begin_map()?;
                            *children_container_state = ChildrenContainerState::Map { field_index };
                        }
                        Def::Set(_) => {
                            partial = partial.begin_set()?;
                            *children_container_state = ChildrenContainerState::Set { field_index };
                        }
                        _ => {
                            partial = partial.begin_list()?;
                            *children_container_state =
                                ChildrenContainerState::List { field_index };
                        }
                    }
                }

                match *children_container_state {
                    ChildrenContainerState::Map { .. } => {
                        // For maps, use node name as key
                        partial = partial.begin_key()?;
                        let key_str = node.name().value().to_string();
                        // For types with parse_from_str (like Utf8PathBuf), use that
                        if partial.shape().vtable.has_parse() {
                            partial = partial.parse_from_str(&key_str)?;
                        } else if partial.shape().inner.is_some() {
                            // For other transparent types, use begin_inner
                            partial = partial.begin_inner()?;
                            partial = partial.set(key_str)?;
                            partial = partial.end()?;
                        } else {
                            partial = partial.set(key_str)?;
                        }
                        partial = partial.end()?;
                        partial = partial.begin_value()?;

                        // Check if the value type is a simple type (not a struct)
                        // If so, deserialize the first argument directly as the value
                        let value_shape = partial.shape();
                        let is_struct = matches!(value_shape.ty, Type::User(UserType::Struct(_)));

                        if !is_struct {
                            // Value is a simple type, get the first argument
                            if let Some(mut entry) = node.entries_mut().drain(..).next()
                                && entry.name().is_none()
                            {
                                // It's an argument (not a property)
                                let entry_span = entry.span();
                                let value = mem::replace(entry.value_mut(), KdlValue::Null);
                                partial =
                                    self.deserialize_value(partial, value, Some(entry_span))?;
                                partial = partial.end()?; // end value
                                return Ok(partial);
                            }
                            return Err(KdlErrorKind::NoMatchingArgument.into());
                        }
                        // For struct values, continue with normal processing below
                    }
                    ChildrenContainerState::List { .. } => {
                        partial = partial.begin_list_item()?;

                        // After beginning the list item, check if it's an enum type
                        if let Some(enum_type) = get_enum_type(partial.shape())
                            && let Some(variant) =
                                find_variant_by_name(&enum_type, node.name().value())
                        {
                            log::trace!(
                                "List item is enum, matched variant {} for node {}",
                                variant.name,
                                node.name().value()
                            );
                            enum_variant_to_select = Some((variant.name, variant.data));
                        }
                    }
                    ChildrenContainerState::Set { .. } => {
                        partial = partial.begin_set_item()?;

                        // After beginning the set item, check if it's an enum type
                        if let Some(enum_type) = get_enum_type(partial.shape())
                            && let Some(variant) =
                                find_variant_by_name(&enum_type, node.name().value())
                        {
                            log::trace!(
                                "Set item is enum, matched variant {} for node {}",
                                variant.name,
                                node.name().value()
                            );
                            enum_variant_to_select = Some((variant.name, variant.data));
                        }
                    }
                    ChildrenContainerState::None => unreachable!(),
                }
            }
            None => {
                // Unknown child node
                if document_shape.has_deny_unknown_fields_attr() {
                    log::debug!("No fields for child {} (deny_unknown_fields)", node.name());
                    for field in fields {
                        log::debug!("field {}\tattributes {:?}", field.name, field.attributes);
                    }
                    return Err(
                        KdlErrorKind::NoMatchingField(node.name().value().to_string()).into(),
                    );
                }
                // Skip unknown child node
                log::trace!("Skipping unknown child node '{}'", node.name().value());
                return Ok(partial);
            }
        }

        // Handle Option wrapper - if the current shape is Option<T>, begin building Some(T)
        // so that we can deserialize into the inner type
        let mut entered_option = false;
        if let Def::Option(_) = partial.shape().def {
            log::trace!("Field is Option<T>, calling begin_some()");
            log::trace!(
                "DEBUG: Field is Option<T>, calling begin_some() at path={}",
                partial.path()
            );
            partial = partial.begin_some()?;
            log::trace!(
                "DEBUG: After begin_some() at path={}, shape={}",
                partial.path(),
                partial.shape()
            );
            entered_option = true;
        }

        // Handle Pointer wrapper - if the current shape is Box<T>, Arc<T>, etc., enter the pointer
        let mut entered_pointer = false;
        if let Def::Pointer(ptr_def) = partial.shape().def {
            log::trace!(
                "Field is Pointer type ({:?}), calling begin_smart_ptr()",
                ptr_def.known
            );
            partial = partial.begin_smart_ptr()?;
            entered_pointer = true;
        }

        // If we matched an enum variant by node name, select it now and capture its fields
        let variant_fields: Option<&[Field]> =
            if let Some((variant_name, variant_data)) = enum_variant_to_select {
                log::trace!("Selecting enum variant: {variant_name}");
                partial = partial.select_variant_named(variant_name)?;
                Some(variant_data.fields)
            } else {
                None
            };
        log::trace!("New def: {:#?}", partial.shape().def);

        // Get the fields for property/argument matching
        // For enum variants, use the variant's fields; otherwise use the struct's fields
        let fields_for_matching: &[Field] = if let Some(fields) = variant_fields {
            fields
        } else if let Type::User(UserType::Struct(struct_def)) = partial.shape().ty {
            struct_def.fields
        } else {
            &[]
        };

        // Handle kdl::node_name attribute (stores the node name into a field)
        if let Some(node_name_field) = fields_for_matching
            .iter()
            .find(|field| field.has_attr(Some("kdl"), "node_name"))
        {
            let field_shape = node_name_field.shape();
            if is_spanned_shape(field_shape) {
                // Deserialize as Spanned<String>
                partial = partial.begin_field(node_name_field.name)?;
                partial = partial.begin_field("value")?;
                partial = partial.set(node.name().value().to_string())?;
                partial = partial.end()?;
                partial = partial.begin_field("span")?;
                let node_name_span = node.name().span();
                partial = partial.set_field("offset", node_name_span.offset())?;
                partial = partial.set_field("len", node_name_span.len())?;
                partial = partial.end()?;
                partial = partial.end()?;
            } else {
                partial =
                    partial.set_field(node_name_field.name, node.name().value().to_string())?;
            }
        }

        // Check if we need solver-based deserialization (any flattened fields)
        // Using the solver for all flatten cases ensures proper path resolution and
        // automatic initialization of missing optional fields via missing_optional_fields().
        //
        // Note: We could also use the solver for unselected enum variants (property-based
        // disambiguation), but this requires facet-solver to support extracting fields from
        // enum variant data, which is not yet implemented.
        let deny_unknown_fields = partial.shape().has_deny_unknown_fields_attr();

        log::trace!(
            "DEBUG: has_flatten={} for fields_for_matching, path={}, shape={}, shape.ty={:?}",
            has_flatten(fields_for_matching),
            partial.path(),
            partial.shape(),
            partial.shape().ty
        );
        // Use solver when we have flattened fields OR an enum that needs variant
        // disambiguation (presence/shape-based).
        // BUT: if we already matched a variant by node name (variant_fields is Some),
        // we don't need solver disambiguation - the node name already told us which variant.
        let is_enum = matches!(partial.shape().ty, Type::User(UserType::Enum(_)));
        let needs_enum_disambiguation = is_enum && variant_fields.is_none();
        if has_flatten(fields_for_matching) || needs_enum_disambiguation {
            // Use solver-based deserialization for flattened fields
            log::trace!(" Using solver-based deserialization");
            partial = self.deserialize_entries_with_solver(
                partial,
                &mut node,
                fields_for_matching,
                deny_unknown_fields,
                has_flatten(fields_for_matching),
            )?;
        } else {
            log::trace!(" Using standard deserialization path");
            // Use standard deserialization path
            let mut in_entry_arguments_list = false;
            // Track which flattened fields are currently open (we're inside them setting properties)
            let mut open_flattened_field: Option<&'static str> = None;

            let entries: Vec<_> = node.entries_mut().drain(..).collect();
            log::trace!(" Processing {} entries", entries.len());
            for entry in entries {
                log::trace!("Processing entry: {entry:?}");
                log::trace!(
                    "DEBUG: Processing entry: {:?}, path before={}",
                    entry,
                    partial.path()
                );

                partial = self.deserialize_entry(
                    partial,
                    entry,
                    fields_for_matching,
                    &mut in_entry_arguments_list,
                    &mut open_flattened_field,
                    deny_unknown_fields,
                )?;
                log::trace!(" After entry, path={}", partial.path());
            }

            if in_entry_arguments_list {
                partial = partial.end()?;
            }

            // End any open flattened field before processing children
            if let Some(flattened_name) = open_flattened_field.take() {
                log::trace!("Ending open flattened field: {flattened_name}");
                partial = partial.end()?;
            }
        }

        if let Some(children) = node.children_mut().take() {
            // Pass the fields_for_matching so child nodes can be matched correctly
            // This is especially important for enum variants where partial.shape() is the enum
            partial = self.deserialize_document_with_fields(
                partial,
                children,
                Some(fields_for_matching),
            )?;
        }

        // Set defaults for any unset fields that have the DEFAULT flag or skip attribute
        // Note: Option<T> fields are NOT implicitly optional - they require an explicit
        // value (use #null in KDL for None). Use #[facet(default)] to make a field optional.
        for (idx, field) in fields_for_matching.iter().enumerate() {
            if !partial.is_field_set(idx)?
                && (field.has_default() || field.should_skip_deserializing())
            {
                log::trace!("Setting default for unset field: {}", field.name);
                partial = partial.set_nth_field_to_default(idx)?;
            }
        }

        // End the inner struct/enum
        log::trace!(
            "About to end() inner struct/enum at path={}, entered_option={}, entered_pointer={}",
            partial.path(),
            entered_option,
            entered_pointer
        );
        log::trace!(
            "DEBUG: About to end() inner struct/enum at path={}, entered_option={}, entered_pointer={}, shape={}, frame_count={}",
            partial.path(),
            entered_option,
            entered_pointer,
            partial.shape(),
            partial.frame_count()
        );
        partial = partial.end()?;

        // If we entered a Pointer, end that too
        if entered_pointer {
            log::trace!("About to end() pointer at path={}", partial.path());
            partial = partial.end()?;
        }

        // If we entered an Option, end that too
        if entered_option {
            log::trace!("About to end() option at path={}", partial.path());
            partial = partial.end()?;
        }

        log::trace!(
            "Exiting `deserialize_node` method at path={}",
            partial.path()
        );

        Ok(partial)
    }

    fn deserialize_entry(
        &mut self,
        partial: Partial<'facet>,
        mut entry: KdlEntry,
        fields: &'static [Field],
        in_entry_arguments_list: &mut bool,
        open_flattened_field: &mut Option<&'static str>,
        deny_unknown_fields: bool,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        log::trace!("Entering `deserialize_entry` method at {}", partial.path());

        if let Some(name) = entry.name() {
            // property - check direct fields and flattened struct fields
            match find_property_field(fields, name.value()) {
                Some(PropertyFieldMatch::Direct { field_name, field }) => {
                    // If we have an open flattened field, close it first
                    if let Some(flattened_name) = open_flattened_field.take() {
                        log::trace!(
                            "Closing flattened field {flattened_name} before direct property"
                        );
                        partial = partial.end()?;
                    }
                    partial = partial.begin_field(field_name)?;

                    // Check for custom deserialization
                    let entry_span = entry.span();
                    let value = mem::replace(entry.value_mut(), KdlValue::Null);
                    if field.proxy_convert_in_fn().is_some() {
                        partial = partial.begin_custom_deserialization()?;
                        partial = self.deserialize_value(partial, value, Some(entry_span))?;
                        partial = partial.end()?; // Calls deserialize_with function
                    } else {
                        partial = self.deserialize_value(partial, value, Some(entry_span))?;
                    }
                    partial = partial.end()?; // end field
                    log::trace!("Exiting `deserialize_entry` method (direct property)");
                    Ok(partial)
                }
                Some(PropertyFieldMatch::Flattened {
                    flattened_field_name,
                    property_field_name,
                    inner_field,
                }) => {
                    // Check if we need to switch to a different flattened field
                    if let Some(current) = *open_flattened_field {
                        if current != flattened_field_name {
                            // Close the current one and open the new one
                            log::trace!(
                                "Switching from flattened field {current} to {flattened_field_name}"
                            );
                            partial = partial.end()?;
                            partial = partial.begin_field(flattened_field_name)?;
                            *open_flattened_field = Some(flattened_field_name);
                        }
                        // else: same flattened field, already open
                    } else {
                        // No flattened field open, open this one
                        partial = partial.begin_field(flattened_field_name)?;
                        *open_flattened_field = Some(flattened_field_name);
                    }
                    // Now set the property inside the flattened struct
                    partial = partial.begin_field(property_field_name)?;
                    let entry_span = entry.span();
                    let value = mem::replace(entry.value_mut(), KdlValue::Null);
                    // Check for custom deserialization on the inner field
                    if inner_field.proxy_convert_in_fn().is_some() {
                        partial = partial.begin_custom_deserialization()?;
                        partial = self.deserialize_value(partial, value, Some(entry_span))?;
                        partial = partial.end()?; // Calls deserialize_with function
                    } else {
                        partial = self.deserialize_value(partial, value, Some(entry_span))?;
                    }
                    partial = partial.end()?; // end property field (but keep flattened field open)
                    log::trace!("Exiting `deserialize_entry` method (flattened property)");
                    Ok(partial)
                }
                None => {
                    // Unknown property
                    if deny_unknown_fields {
                        let expected: Vec<&'static str> = fields
                            .iter()
                            .filter(|f| f.has_attr(Some("kdl"), "property"))
                            .map(|f| f.name)
                            .collect();
                        let name_span = name.span();
                        return Err(self.err_at(
                            KdlErrorKind::UnknownProperty {
                                property: name.value().to_string(),
                                expected,
                            },
                            (name_span.offset(), name_span.len()),
                        ));
                    }
                    // Skip unknown property
                    log::trace!("Skipping unknown property '{}'", name.value());
                    Ok(partial)
                }
            }
        } else {
            // argument
            // Track the field for potential deserialize_with (None for list items)
            let argument_field: Option<&Field>;

            if let Some((_, next_arg_field)) = fields.iter().enumerate().find(|(index, field)| {
                field.has_attr(Some("kdl"), "argument")
                    && partial.is_field_set(*index).ok() == Some(false)
            }) {
                if *in_entry_arguments_list {
                    return Err(KdlErrorKind::UnexpectedArgument.into());
                }
                partial = partial.begin_field(next_arg_field.name)?;
                argument_field = Some(next_arg_field);
            } else if let Some((args_field_index, args_field)) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.has_attr(Some("kdl"), "arguments"))
            {
                if !*in_entry_arguments_list {
                    if partial.is_field_set(args_field_index)? {
                        return Err(KdlErrorKind::UnsupportedShape(
                            "cannot reopen arguments list that was already completed".into(),
                        )
                        .into());
                    }
                    partial = partial.begin_field(args_field.name)?;
                    partial = partial.begin_list()?;
                    *in_entry_arguments_list = true;
                }
                partial = partial.begin_list_item()?;
                // For list items, deserialize_with doesn't apply to the container
                // (it would be on the element type, but we don't have that reference here)
                argument_field = None;
            } else {
                log::debug!("No fields for argument");
                for field in fields {
                    log::debug!(
                        "field {}\tattributes {:?}\tis_field_set {:?}",
                        field.name,
                        field.attributes,
                        partial.is_field_set(field.offset)
                    );
                }
                return Err(KdlErrorKind::NoMatchingArgument.into());
            }

            let entry_span = entry.span();
            let value = mem::replace(entry.value_mut(), KdlValue::Null);

            // Check for custom deserialization on the argument field
            if let Some(field) = argument_field {
                if field.proxy_convert_in_fn().is_some() {
                    partial = partial.begin_custom_deserialization()?;
                    partial = self.deserialize_value(partial, value, Some(entry_span))?;
                    partial = partial.end()?; // Calls deserialize_with function
                } else {
                    partial = self.deserialize_value(partial, value, Some(entry_span))?;
                }
            } else {
                // List item or no field reference - just deserialize directly
                partial = self.deserialize_value(partial, value, Some(entry_span))?;
            }
            partial = partial.end()?;

            log::trace!("Exiting `deserialize_entry` method (argument)");
            Ok(partial)
        }
    }

    /// Deserialize node entries using the solver for flattened enum disambiguation.
    ///
    /// This method uses the Solver to process properties one at a time,
    /// deferring values when the path is ambiguous and replaying them after disambiguation.
    ///
    /// This approach uses the Solver API which supports both key-based and value-based
    /// type disambiguation. When multiple enum variants have the same field name but
    /// different types (e.g., u8 vs u16), the solver checks which types the actual
    /// KDL value can fit into.
    fn deserialize_entries_with_solver(
        &mut self,
        partial: Partial<'facet>,
        node: &mut KdlNode,
        fields: &[Field],
        deny_unknown_fields: bool,
        has_flatten: bool,
    ) -> Result<Partial<'facet>> {
        use std::collections::BTreeSet;

        let mut partial = partial;
        log::trace!(
            "Entering `deserialize_entries_with_solver` at {}",
            partial.path()
        );

        // Build schema from the current shape
        let schema = Schema::build(partial.shape())?;
        log::trace!(
            "Built schema with {} resolutions",
            schema.resolutions().len()
        );
        let resolutions = schema.resolutions();

        // Create the new Solver (supports value-based disambiguation)
        let mut solver = Solver::new(&schema);

        // Helper to start deferred mode once.
        let start_deferred =
            |partial: Partial<'facet>, res: &Resolution| -> Result<Partial<'facet>> {
                let mut partial = partial;
                if has_flatten && !partial.is_deferred() {
                    partial = partial.begin_deferred(res.clone())?;
                }
                Ok(partial)
            };

        // If this shape has flatten fields and only one resolution, we can
        // enter deferred mode immediately to handle interleaved fields/children
        // without extra buffering.
        if has_flatten && resolutions.len() == 1 {
            partial = start_deferred(partial, &resolutions[0])?;
        }

        // Check for KDL type annotation for explicit variant disambiguation
        // e.g., `(Http)source "download" url="..."` would hint at the Http variant
        // Also supports kebab-case: `(http-source)source ...` matches HttpSource
        // Extract variant name early to avoid borrow conflicts later
        let type_annotation_variant: Option<String> = node.ty().map(|ty| ty.value().to_string());
        if let Some(ref variant_name) = type_annotation_variant {
            log::trace!("Node has type annotation '{variant_name}', hinting solver at variant");

            // Try exact match first, then kebab-to-pascal conversion
            let matched = if solver.hint_variant(variant_name) {
                true
            } else {
                // Try converting kebab-case to PascalCase
                let pascal_name = kebab_to_pascal(variant_name);
                if pascal_name != *variant_name && solver.hint_variant(&pascal_name) {
                    log::trace!(
                        "Matched via kebab-to-pascal conversion: '{variant_name}' -> '{pascal_name}'"
                    );
                    true
                } else {
                    false
                }
            };

            if matched {
                log::trace!(
                    "Type annotation '{}' matched {} candidate(s)",
                    variant_name,
                    solver.candidates().len()
                );
                // Also mark the variant name as "seen" so finish() doesn't report it as missing
                // We need to find the static variant name from the remaining candidates
                if let Some(handle) = solver.candidates().first() {
                    let resolution = handle.resolution();
                    for vs in resolution.variant_selections() {
                        // Check both exact match and kebab conversion
                        if vs.variant_name == variant_name.as_str()
                            || vs.variant_name == kebab_to_pascal(variant_name)
                        {
                            // Use the static string from the resolution
                            solver.mark_seen(vs.variant_name);
                            log::trace!(
                                "Marked variant '{}' as seen via type annotation",
                                vs.variant_name
                            );
                            break;
                        }
                    }
                }
            } else {
                log::trace!("Type annotation '{variant_name}' did not match any variant, ignoring");
            }
        }

        // Pre-register argument fields with the solver (they're always present)
        // This is important because the solver's finish() method checks required fields
        for field in fields {
            if field.has_attr(Some("kdl"), "argument") || field.has_attr(Some("kdl"), "arguments") {
                let _ = solver.see_key(field.name); // Inform solver about argument fields
            }
        }

        // Track navigation state - each entry tracks the path segment and whether we entered an Option
        let mut open_paths: Vec<OpenPathEntry> = Vec::new();

        // Process arguments first (they don't go through property path resolution)
        let mut argument_index = 0;
        let argument_fields: Vec<_> = fields
            .iter()
            .filter(|f| f.has_attr(Some("kdl"), "argument"))
            .collect();

        let mut in_arguments_list = false;
        let arguments_field = fields.iter().find(|f| f.has_attr(Some("kdl"), "arguments"));

        // Separate arguments from properties
        let mut arguments: Vec<KdlEntry> = Vec::new();
        let mut properties: Vec<KdlEntry> = Vec::new();
        let mut property_names: Vec<String> = Vec::new();

        for entry in node.entries_mut().drain(..) {
            if let Some(name) = entry.name() {
                property_names.push(name.value().to_string());
                properties.push(entry);
            } else {
                arguments.push(entry);
            }
        }

        // Phase 1: Process all properties through the solver
        // The solver supports value-based disambiguation for same-named fields with different types
        let mut resolved_resolution: Option<ResolutionHandle<'_>> = None;

        for (idx, prop_name) in property_names.iter().enumerate() {
            // If already resolved, skip solver interaction
            if resolved_resolution.is_some() {
                continue;
            }

            let result = solver.see_key(prop_name);
            log::trace!("Solver result for '{prop_name}': {result:?}");

            match result {
                KeyResult::Solved(handle) => {
                    let resolution = handle.resolution();
                    // Disambiguated by key alone
                    log::trace!("Solved to resolution: {}", resolution.describe());
                    resolved_resolution = Some(handle);
                    partial = start_deferred(partial, resolution)?;
                }
                KeyResult::Unambiguous { shape: _ } => {
                    // All candidates agree on the type - continue
                    log::trace!("Unambiguous type for '{prop_name}'");
                }
                KeyResult::Ambiguous {
                    fields: ambiguous_fields,
                } => {
                    // Different types for this field across candidates!
                    // Check which types the actual value can fit into
                    // Note: ambiguous_fields is Vec<(&FieldInfo, u64)> where u64 is specificity score
                    log::trace!(
                        "Ambiguous types for '{}': {:?}",
                        prop_name,
                        ambiguous_fields
                            .iter()
                            .map(|(f, _)| f.value_shape.type_identifier)
                            .collect::<Vec<_>>()
                    );

                    let value = properties[idx].value();
                    let mut satisfied_shapes: Vec<&'static Shape> = ambiguous_fields
                        .iter()
                        .filter(|(f, _)| kdl_value_fits_shape(value, f.value_shape))
                        .map(|(f, _)| f.value_shape)
                        .collect();

                    // Pick the tightest type(s) - e.g., u8 over u16 when both fit
                    // This prefers more constrained types for better type safety
                    if satisfied_shapes.len() > 1 {
                        let min_tightness = satisfied_shapes
                            .iter()
                            .map(|s| shape_tightness(s))
                            .min()
                            .unwrap_or(0);
                        satisfied_shapes.retain(|s| shape_tightness(s) == min_tightness);
                    }

                    // For integer values, prefer integer types over float types
                    // (e.g., i64 over f64 when both are 8 bytes)
                    if satisfied_shapes.len() > 1 && matches!(value, KdlValue::Integer(_)) {
                        let has_integer_type = satisfied_shapes.iter().any(|s| {
                            matches!(
                                s.ty,
                                Type::Primitive(PrimitiveType::Numeric(
                                    NumericType::Integer { .. }
                                ))
                            )
                        });
                        if has_integer_type {
                            satisfied_shapes.retain(|s| {
                                matches!(
                                    s.ty,
                                    Type::Primitive(PrimitiveType::Numeric(
                                        NumericType::Integer { .. }
                                    ))
                                )
                            });
                        }
                    }

                    log::trace!(
                        "Value {:?} satisfies tightest types: {:?}",
                        value,
                        satisfied_shapes
                            .iter()
                            .map(|s| s.type_identifier)
                            .collect::<Vec<_>>()
                    );

                    // Use satisfy_at_path to check only THIS specific field, not all fields
                    // This is crucial because other fields might share the same type
                    match solver.satisfy_at_path(&[prop_name.as_str()], &satisfied_shapes) {
                        SatisfyResult::Solved(handle) => {
                            let resolution = handle.resolution();
                            log::trace!(
                                "Value disambiguation solved to: {}",
                                resolution.describe()
                            );
                            resolved_resolution = Some(handle);
                            partial = start_deferred(partial, resolution)?;
                        }
                        SatisfyResult::Continue => {
                            // Still multiple candidates, keep going
                        }
                        SatisfyResult::NoMatch => {
                            return Err(KdlErrorKind::InvalidValueForShape(format!(
                                "value {value:?} doesn't fit any candidate type for field '{prop_name}'"
                            ))
                            .into());
                        }
                    }
                }
                KeyResult::Unknown => {
                    if deny_unknown_fields {
                        // Collect expected property fields for the error message
                        let expected: Vec<&'static str> = fields
                            .iter()
                            .filter(|f| f.has_attr(Some("kdl"), "property"))
                            .map(|f| f.name)
                            .collect();
                        // Get span from the property entry
                        let prop_span = properties[idx].name().map(|n| n.span());
                        let err = KdlErrorKind::UnknownProperty {
                            property: prop_name.clone(),
                            expected,
                        };
                        return Err(if let Some(span) = prop_span {
                            self.err_at(err, (span.offset(), span.len()))
                        } else {
                            self.err(err)
                        });
                    }
                    // Skip unknown property
                    log::trace!("Skipping unknown property '{prop_name}'");
                }
            }
        }

        // Phase 1b: Process child nodes through the solver for nested disambiguation
        // This handles cases like #[facet(child)] fields where the discriminating
        // information is in nested child nodes rather than top-level properties.
        if resolved_resolution.is_none()
            && let Some(children) = node.children()
        {
            for child_node in children.nodes() {
                if resolved_resolution.is_some() {
                    break;
                }

                let child_name = child_node.name().value();
                log::trace!("Probing child node '{child_name}' for solver");

                // Tell solver we saw this child node
                let result = solver.probe_key(&[], child_name);
                log::trace!("Solver probe_key result for child '{child_name}': {result:?}");

                match result {
                    KeyResult::Solved(handle) => {
                        let resolution = handle.resolution();
                        log::trace!(
                            "Child node '{}' solved to: {}",
                            child_name,
                            resolution.describe()
                        );
                        resolved_resolution = Some(handle);
                        partial = start_deferred(partial, resolution)?;
                    }
                    KeyResult::Unambiguous { .. } | KeyResult::Unknown => {
                        // Continue - either all agree or this child isn't tracked
                    }
                    KeyResult::Ambiguous { .. } => {
                        // Need to look deeper - check properties inside this child
                        log::trace!(
                            "Child '{child_name}' is ambiguous, checking nested properties"
                        );
                    }
                }

                // Process properties inside this child node for deeper disambiguation
                if resolved_resolution.is_none() {
                    for entry in child_node.entries() {
                        if let Some(prop_name_ident) = entry.name() {
                            let prop_name = prop_name_ident.value();
                            let path: Vec<&str> = vec![child_name];

                            log::trace!("Probing nested property '{child_name}.{prop_name}'");
                            let result = solver.probe_key(&path, prop_name);
                            log::trace!(
                                "Solver probe_key result for '{child_name}.{prop_name}': {result:?}"
                            );

                            match result {
                                KeyResult::Solved(handle) => {
                                    let resolution = handle.resolution();
                                    log::trace!(
                                        "Nested property solved to: {}",
                                        resolution.describe()
                                    );
                                    resolved_resolution = Some(handle);
                                    break;
                                }
                                KeyResult::Ambiguous { .. } => {
                                    // Different types at this nested path - use value-based disambiguation
                                    let full_path: Vec<&str> = vec![child_name, prop_name];
                                    let shapes = solver.get_shapes_at_path(&full_path);
                                    log::trace!(
                                        "Ambiguous nested types at {:?}: {:?}",
                                        full_path,
                                        shapes
                                            .iter()
                                            .map(|s| s.type_identifier)
                                            .collect::<Vec<_>>()
                                    );

                                    let value = entry.value();
                                    let mut satisfied_shapes: Vec<&'static Shape> = shapes
                                        .into_iter()
                                        .filter(|s| kdl_value_fits_shape(value, s))
                                        .collect();

                                    // Pick tightest types
                                    if satisfied_shapes.len() > 1 {
                                        let min_tightness = satisfied_shapes
                                            .iter()
                                            .map(|s| shape_tightness(s))
                                            .min()
                                            .unwrap_or(0);
                                        satisfied_shapes
                                            .retain(|s| shape_tightness(s) == min_tightness);
                                    }

                                    log::trace!(
                                        "Value {:?} satisfies tightest nested types: {:?}",
                                        value,
                                        satisfied_shapes
                                            .iter()
                                            .map(|s| s.type_identifier)
                                            .collect::<Vec<_>>()
                                    );

                                    match solver.satisfy_at_path(&full_path, &satisfied_shapes) {
                                        SatisfyResult::Solved(handle) => {
                                            let resolution = handle.resolution();
                                            log::trace!(
                                                "Nested value disambiguation solved to: {}",
                                                resolution.describe()
                                            );
                                            resolved_resolution = Some(handle);
                                            partial = start_deferred(partial, resolution)?;
                                            break;
                                        }
                                        SatisfyResult::Continue => {
                                            // Still ambiguous, continue
                                        }
                                        SatisfyResult::NoMatch => {
                                            return Err(KdlErrorKind::InvalidValueForShape(format!(
                                                    "value {value:?} doesn't fit any candidate type for nested field '{child_name}.{prop_name}'"
                                                ))
                                                .into());
                                        }
                                    }
                                }
                                KeyResult::Unambiguous { .. } | KeyResult::Unknown => {
                                    // Continue
                                }
                            }
                        }
                    }
                }
            }
        }

        // Check for truly ambiguous resolutions before finishing
        // If multiple candidates remain with identical field types AND all required fields
        // satisfied, error (truly ambiguous)
        let remaining_candidates = solver.candidates();
        if remaining_candidates.len() > 1 {
            // Include both properties and argument fields in seen set
            let mut seen_props: std::collections::BTreeSet<Cow<'_, str>> = property_names
                .iter()
                .map(|s| Cow::Borrowed(s.as_str()))
                .collect();
            for field in fields {
                if field.has_attr(Some("kdl"), "argument")
                    || field.has_attr(Some("kdl"), "arguments")
                {
                    seen_props.insert(Cow::Borrowed(field.name));
                }
            }

            // Filter to only viable candidates (all required fields satisfied)
            let viable_candidates: Vec<_> = remaining_candidates
                .iter()
                .filter(|handle| {
                    let resolution = handle.resolution();
                    // Check if this resolution matches (not NoMatch = has all required fields)
                    !matches!(resolution.matches(&seen_props), MatchResult::NoMatch { .. })
                })
                .collect();

            if viable_candidates.len() > 1 {
                // Check if all viable candidates have identical types for all seen props
                let first = viable_candidates[0].resolution();
                let first_types: Vec<_> = seen_props
                    .iter()
                    .filter_map(|key| first.field(key).map(|f| f.value_shape))
                    .collect();

                let all_identical = viable_candidates[1..].iter().all(|handle| {
                    let resolution = handle.resolution();
                    seen_props
                        .iter()
                        .filter_map(|key| resolution.field(key).map(|f| f.value_shape))
                        .zip(first_types.iter())
                        .all(|(a, b)| std::ptr::eq(a, *b))
                });

                if all_identical {
                    let candidates: Vec<_> = viable_candidates
                        .iter()
                        .map(|handle| handle.resolution().describe())
                        .collect();
                    // Build a proper SolverError::Ambiguous
                    return Err(self.err(KdlErrorKind::Solver(
                        facet_solver::SolverError::Ambiguous {
                            candidates,
                            disambiguating_fields: Vec::new(), // Truly ambiguous - no disambiguating fields
                        },
                    )));
                }
            }
        }

        // Finish solving - this checks for ambiguity and missing required fields
        let final_handle = match resolved_resolution {
            Some(handle) => handle,
            None => {
                // Call finish to get the resolution or error - pass through full error
                solver
                    .finish()
                    .map_err(|e| self.err(KdlErrorKind::Solver(e)))?
            }
        };

        let final_resolution = final_handle.resolution();
        partial = start_deferred(partial, final_resolution)?;

        log::trace!("Final resolution: {}", final_resolution.describe());

        // Phase 2: Deserialize all properties using resolved paths from the final resolution
        // Process properties in input order; deferred materialization makes re-entry safe.
        for idx in 0..property_names.len() {
            let prop_name = &property_names[idx];
            let field_info = final_resolution
                .field(prop_name)
                .ok_or_else(|| KdlErrorKind::NoMatchingProperty(prop_name.clone()))?;

            let entry = &mut properties[idx];
            partial = self.close_paths_to(partial, &mut open_paths, &field_info.path)?;
            // Always enter new Options for actual property values
            (partial, _) = self.open_path_to(partial, &mut open_paths, &field_info.path, true)?;

            let entry_span = entry.span();
            let value = mem::replace(entry.value_mut(), KdlValue::Null);

            // Check for custom deserialization via partial.parent_field()
            let has_custom_deser = partial
                .parent_field()
                .map(|f| f.proxy_convert_in_fn().is_some())
                .unwrap_or(false);

            if has_custom_deser {
                partial = partial.begin_custom_deserialization()?;
                partial = self.deserialize_value(partial, value, Some(entry_span))?;
                partial = partial.end()?; // Calls deserialize_with function
            } else {
                partial = self.deserialize_value(partial, value, Some(entry_span))?;
            }
            partial = partial.end()?;
        }

        // Initialize missing optional fields BEFORE closing all paths
        // This is crucial: we need to set defaults while parent structs are still open,
        // otherwise partial.end() will fail because required fields aren't initialized.
        //
        // However, we DON'T want to enter new Option<T> fields just to set defaults,
        // as that would turn None into Some(default). So we pass enter_new_options=false.
        // When we encounter a field inside an unopened Option<T>, we track the Option field
        // so we can set it to None later.
        let mut seen_keys: BTreeSet<Cow<'_, str>> = property_names
            .iter()
            .map(|s| Cow::Borrowed(s.as_str()))
            .collect();
        let mut skipped_option_fields: std::collections::HashSet<&'static str> =
            std::collections::HashSet::new();
        log::trace!(" Processing missing_optional_fields");
        for field_info in final_resolution.missing_optional_fields(&seen_keys) {
            log::trace!(
                "DEBUG: Missing optional field: {} (CHILD={})",
                field_info.serialized_name,
                field_info.field.is_kdl_child()
            );
            // Skip child fields - they are handled later in child node processing
            // We only want to set defaults for property fields here
            if field_info.field.is_kdl_child() {
                log::trace!(
                    "Skipping child field '{}' - will be handled in child node processing",
                    field_info.serialized_name
                );
                log::trace!(
                    "DEBUG: Skipping CHILD field '{}' in missing_optional_fields",
                    field_info.serialized_name
                );
                continue;
            }
            log::trace!(
                "DEBUG: Processing non-CHILD missing optional field '{}'",
                field_info.serialized_name
            );

            log::trace!(
                "Initializing missing optional field '{}' at path {:?}",
                field_info.serialized_name,
                field_info.path
            );

            // Navigate to the field (may need to open intermediate structs)
            partial = self.close_paths_to(partial, &mut open_paths, &field_info.path)?;
            // Don't enter new Options - if this field is under an unopened Option<T>,
            // skip it and record the Option field so we can set it to None
            let option_field_name;
            (partial, option_field_name) =
                self.open_path_to(partial, &mut open_paths, &field_info.path, false)?;
            if let Some(option_field_name) = option_field_name {
                log::trace!(
                    "Skipping missing optional field '{}' - inside unopened Option field '{}'",
                    field_info.serialized_name,
                    option_field_name
                );
                skipped_option_fields.insert(option_field_name);
                continue;
            }
            partial = partial.set_default()?;
            partial = partial.end()?; // End the field itself
        }
        log::trace!(" Finished processing missing_optional_fields loop");

        // Set any skipped Option<T> fields to None
        log::trace!(
            "DEBUG: About to set skipped_option_fields to None, count={}",
            skipped_option_fields.len()
        );
        for option_field_name in skipped_option_fields {
            log::trace!("Setting skipped Option field '{option_field_name}' to None");
            log::trace!("DEBUG: Setting skipped Option field '{option_field_name}' to None");
            // Close all open paths first (we're at the root level for these fields)
            partial = self.close_paths_to(partial, &mut open_paths, &FieldPath::empty())?;
            partial = partial.begin_field(option_field_name)?;
            partial = partial.set_default()?; // This sets Option<T> to None
            partial = partial.end()?;
        }
        log::trace!(" Done setting skipped option fields");

        log::trace!(
            "DEBUG: About to process child nodes, node.children() = {:?}, open_paths len={}",
            node.children(),
            open_paths.len()
        );

        // Process child nodes using solver resolution
        // IMPORTANT: Process children BEFORE closing paths, because child fields may belong
        // to currently-open nested structs (e.g., `cache` is a field of LocalBackend which
        // is currently open via the `backend.Local` path)
        if let Some(mut children) = node.children_mut().take() {
            log::trace!(
                "DEBUG: Processing children. Solver config fields: {:?}",
                final_resolution.fields().keys().collect::<Vec<_>>()
            );
            // Process children in the order they appear; deferred mode handles interleaving.
            let mut child_nodes: Vec<KdlNode> = children.nodes_mut().drain(..).collect();
            for mut child_node in child_nodes.drain(..) {
                let child_name = child_node.name().value().to_string();
                log::trace!("DEBUG: Looking for child '{child_name}' in solver resolution");

                // Look up the child field in the solver's resolution
                if let Some(field_info) = final_resolution.field(&child_name)
                    && field_info.field.is_kdl_child()
                {
                    log::trace!(
                        "Processing child node '{}' via solver path {:?}",
                        child_name,
                        field_info.path
                    );
                    log::trace!(
                        "DEBUG: Processing child node '{}' via solver path {:?}",
                        child_name,
                        field_info.path
                    );

                    // Record that we've seen this child field - important for variant selection
                    // check later (variants selected via child paths, not just properties)
                    // Use the serialized_name from field_info since it's 'static
                    seen_keys.insert(Cow::Borrowed(field_info.serialized_name));

                    // First close paths to the common prefix with the target field
                    // This handles cases like: we're inside `connection` (a flatten struct)
                    // but `logging` is a sibling field at the parent level
                    partial = self.close_paths_to(partial, &mut open_paths, &field_info.path)?;

                    // Navigate to the field using its path
                    // Don't enter new options here - we handle Option wrapping ourselves
                    (partial, _) =
                        self.open_path_to(partial, &mut open_paths, &field_info.path, false)?;

                    // Handle Option wrapper
                    let mut entered_option = false;
                    if let Def::Option(_) = partial.shape().def {
                        log::trace!("Child field is Option<T>, calling begin_some()");
                        partial = partial.begin_some()?;
                        entered_option = true;
                    }

                    // Deserialize the child node's entries into the struct
                    if let Type::User(UserType::Struct(struct_def)) = partial.shape().ty {
                        let deny_unknown = partial.shape().has_deny_unknown_fields_attr();
                        let mut in_entry_arguments_list = false;
                        let mut open_flattened_field: Option<&'static str> = None;

                        for entry in child_node.entries_mut().drain(..) {
                            partial = self.deserialize_entry(
                                partial,
                                entry,
                                struct_def.fields,
                                &mut in_entry_arguments_list,
                                &mut open_flattened_field,
                                deny_unknown,
                            )?;
                        }

                        if open_flattened_field.is_some() {
                            partial = partial.end()?;
                        }

                        // Set defaults for unset fields
                        for (idx, field) in struct_def.fields.iter().enumerate() {
                            if !partial.is_field_set(idx)?
                                && (field.has_default() || field.should_skip_deserializing())
                            {
                                partial = partial.set_nth_field_to_default(idx)?;
                            }
                        }
                    }

                    // End the struct
                    partial = partial.end()?;

                    // End the Option if we entered one
                    if entered_option {
                        partial = partial.end()?;
                    }

                    continue;
                }

                // Fall back to original field matching for non-solver child fields
                // (direct child fields on the parent struct)
                log::trace!(
                    "Child node '{child_name}' not found in solver resolution, using field matching"
                );

                // Find matching field in the original fields
                if let Some(child_field) = fields
                    .iter()
                    .find(|field| field.is_kdl_child() && field.name == child_name.as_str())
                {
                    partial = partial.begin_field(child_field.name)?;
                    let _field_shape = child_field.shape();

                    // Handle Option wrapper
                    let mut entered_option = false;
                    if let Def::Option(_) = partial.shape().def {
                        partial = partial.begin_some()?;
                        entered_option = true;
                    }

                    // Deserialize the child node's entries
                    if let Type::User(UserType::Struct(struct_def)) = partial.shape().ty {
                        let deny_unknown = partial.shape().has_deny_unknown_fields_attr();
                        let mut in_entry_arguments_list = false;
                        let mut open_flattened_field: Option<&'static str> = None;

                        for entry in child_node.entries_mut().drain(..) {
                            partial = self.deserialize_entry(
                                partial,
                                entry,
                                struct_def.fields,
                                &mut in_entry_arguments_list,
                                &mut open_flattened_field,
                                deny_unknown,
                            )?;
                        }

                        if open_flattened_field.is_some() {
                            partial = partial.end()?;
                        }

                        for (idx, field) in struct_def.fields.iter().enumerate() {
                            if !partial.is_field_set(idx)?
                                && (field.has_default() || field.should_skip_deserializing())
                            {
                                partial = partial.set_nth_field_to_default(idx)?;
                            }
                        }
                    }

                    partial = partial.end()?;
                    if entered_option {
                        partial = partial.end()?;
                    }
                } else {
                    // Check for enum variant matching
                    if let Some((child_field, variant)) = fields
                        .iter()
                        .filter(|field| field.is_kdl_child())
                        .find_map(|field| {
                            let field_shape = field.shape();
                            if let Some(enum_type) = get_enum_type(field_shape)
                                && let Some(variant) = find_variant_by_name(&enum_type, &child_name)
                            {
                                return Some((field, variant));
                            }
                            None
                        })
                    {
                        partial = partial.begin_field(child_field.name)?;
                        partial = partial.select_variant_named(variant.name)?;

                        // Deserialize variant's struct fields
                        if let Type::User(UserType::Struct(struct_def)) = &partial.shape().ty {
                            let deny_unknown = partial.shape().has_deny_unknown_fields_attr();
                            let mut in_entry_arguments_list = false;
                            let mut open_flattened_field: Option<&'static str> = None;

                            for entry in child_node.entries_mut().drain(..) {
                                partial = self.deserialize_entry(
                                    partial,
                                    entry,
                                    struct_def.fields,
                                    &mut in_entry_arguments_list,
                                    &mut open_flattened_field,
                                    deny_unknown,
                                )?;
                            }

                            if open_flattened_field.is_some() {
                                partial = partial.end()?;
                            }

                            for (idx, field) in struct_def.fields.iter().enumerate() {
                                if !partial.is_field_set(idx)?
                                    && (field.has_default() || field.should_skip_deserializing())
                                {
                                    partial = partial.set_nth_field_to_default(idx)?;
                                }
                            }
                        }

                        partial = partial.end()?; // End variant/struct
                        partial = partial.end()?; // End field
                    } else {
                        log::warn!("Unknown child node '{child_name}', skipping");
                    }
                }
            }
        }

        // Set defaults for missing optional child fields
        // We skipped these earlier in missing_optional_fields, so handle them now
        for field_info in final_resolution.missing_optional_fields(&seen_keys) {
            if !field_info.field.is_kdl_child() {
                continue;
            }
            log::trace!(
                "Setting default for missing optional child field '{}'",
                field_info.serialized_name
            );
            // Close paths and navigate to the field
            partial = self.close_paths_to(partial, &mut open_paths, &field_info.path)?;
            (partial, _) = self.open_path_to(partial, &mut open_paths, &field_info.path, false)?;
            partial = partial.set_default()?;
            partial = partial.end()?;
        }

        // Close all paths after processing child nodes
        log::trace!("DEBUG: About to close paths after children, open_paths={open_paths:?}");
        partial = self.close_paths_to(partial, &mut open_paths, &FieldPath::empty())?;
        log::trace!(" Closed all paths, partial.path()={}", partial.path());

        // Initialize any flattened enum variants that weren't already selected via property paths.
        // This handles unit variants (like `Stdout`) that have no properties - we still need to
        // select the variant in the Partial to initialize the field.
        log::trace!(
            "DEBUG: About to check variant selections, partial.path()={}, partial.shape()={}",
            partial.path(),
            partial.shape()
        );
        for vs in final_resolution.variant_selections() {
            log::trace!(
                "Checking variant selection: {} at {:?}",
                vs.variant_name,
                vs.path
            );
            log::trace!(
                "DEBUG: Checking variant selection: {} at {:?}",
                vs.variant_name,
                vs.path
            );

            // Build a synthetic FieldPath for just the enum field (without the variant segment)
            // The path in VariantSelection includes the field, so we use it directly
            // but we need to open the field and select the variant

            // Check if this variant was already initialized by property navigation
            // by checking if we've seen any properties with a path that goes through this variant
            log::trace!(" seen_keys = {seen_keys:?}");
            let variant_already_selected = seen_keys.iter().any(|key| {
                if let Some(field_info) = final_resolution.field(key) {
                    log::trace!(
                        "DEBUG: Checking field '{}' path {:?} for variant '{}'",
                        key,
                        field_info.path,
                        vs.variant_name
                    );
                    // Check if this field's path goes through this variant selection
                    field_info.path.segments().iter().any(
                        |seg| matches!(seg, PathSegment::Variant(_, vn) if *vn == vs.variant_name),
                    )
                } else {
                    false
                }
            });
            log::trace!("DEBUG: variant_already_selected = {variant_already_selected}");

            if !variant_already_selected {
                log::trace!(
                    "Selecting unit variant '{}' at field '{}'",
                    vs.variant_name,
                    vs.path
                        .segments()
                        .last()
                        .map(|s| match s {
                            PathSegment::Field(n) => *n,
                            PathSegment::Variant(n, _) => *n,
                        })
                        .unwrap_or("?")
                );

                // Navigate to the enum field and select the variant
                // The path in VariantSelection is to the field (e.g., FieldPath(output))
                // We need to begin that field and select the variant
                for seg in vs.path.segments() {
                    match seg {
                        PathSegment::Field(name) => {
                            partial = partial.begin_field(name)?;
                        }
                        PathSegment::Variant(_, variant_name) => {
                            partial = partial.select_variant_named(variant_name)?;
                        }
                    }
                }
                // Now select the variant
                partial = partial.select_variant_named(vs.variant_name)?;
                // For unit variants, just end immediately (no fields to set)
                partial = partial.end()?;
            }
        }

        // Now close all property paths before handling arguments
        log::trace!(
            "DEBUG: About to close_all_paths before arguments, open_paths len={}",
            open_paths.len()
        );
        partial = self.close_all_paths(partial, &mut open_paths)?;
        log::trace!(
            "DEBUG: After close_all_paths, partial.path()={}",
            partial.path()
        );

        // Now process arguments
        log::trace!(
            "DEBUG: Processing {} arguments, argument_fields len={}",
            arguments.len(),
            argument_fields.len()
        );
        for entry in arguments {
            if argument_index < argument_fields.len() {
                // Single argument field
                if in_arguments_list {
                    return Err(KdlErrorKind::UnexpectedArgument.into());
                }
                let arg_field = argument_fields[argument_index];
                partial = partial.begin_field(arg_field.name)?;
                let entry_span = entry.span();
                let mut entry = entry;
                let value = mem::replace(entry.value_mut(), KdlValue::Null);
                partial = self.deserialize_value(partial, value, Some(entry_span))?;
                partial = partial.end()?;
                argument_index += 1;
            } else if let Some(args_field) = arguments_field {
                // Arguments list
                if !in_arguments_list {
                    partial = partial.begin_field(args_field.name)?;
                    partial = partial.begin_list()?;
                    in_arguments_list = true;
                }
                partial = partial.begin_list_item()?;
                let entry_span = entry.span();
                let mut entry = entry;
                let value = mem::replace(entry.value_mut(), KdlValue::Null);
                partial = self.deserialize_value(partial, value, Some(entry_span))?;
                partial = partial.end()?; // End list item
            } else {
                return Err(KdlErrorKind::NoMatchingArgument.into());
            }
        }

        // Close arguments list if open
        if in_arguments_list {
            partial = partial.end()?; // End list
            partial = partial.end()?; // End field
        }

        log::trace!("Exiting `deserialize_entries_with_solver`");

        if partial.is_deferred() {
            partial = partial.finish_deferred()?;
        }
        Ok(partial)
    }

    /// Deserialize a node's content into the current shape (for solver-based child processing).
    /// This is called when we've already navigated to the correct field position.
    #[allow(dead_code)]
    fn deserialize_node_inner(
        &mut self,
        partial: Partial<'facet>,
        mut node: KdlNode,
        _target_shape: &Shape,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        log::trace!("deserialize_node_inner: shape = {:?}", partial.shape().ty);

        // Handle Option wrapper
        let mut entered_option = false;
        if let Def::Option(_) = partial.shape().def {
            log::trace!("Field is Option<T>, calling begin_some()");
            partial = partial.begin_some()?;
            entered_option = true;
        }

        // Get fields from current shape
        let fields: &[Field] = if let Type::User(UserType::Struct(struct_def)) = partial.shape().ty
        {
            struct_def.fields
        } else {
            &[]
        };

        // Process entries (arguments and properties)
        let mut in_entry_arguments_list = false;
        let mut open_flattened_field: Option<&'static str> = None;
        let deny_unknown_fields = partial.shape().has_deny_unknown_fields_attr();

        for entry in node.entries_mut().drain(..) {
            log::trace!("Processing entry in node_inner: {entry:?}");
            partial = self.deserialize_entry(
                partial,
                entry,
                fields,
                &mut in_entry_arguments_list,
                &mut open_flattened_field,
                deny_unknown_fields,
            )?;
        }

        if in_entry_arguments_list {
            partial = partial.end()?;
        }

        if let Some(flattened_name) = open_flattened_field.take() {
            log::trace!("Ending open flattened field: {flattened_name}");
            partial = partial.end()?;
        }

        // Process nested children
        if let Some(children) = node.children_mut().take() {
            partial = self.deserialize_document_with_fields(partial, children, Some(fields))?;
        }

        // Set defaults for unset fields
        for (idx, field) in fields.iter().enumerate() {
            if !partial.is_field_set(idx)?
                && (field.has_default() || field.should_skip_deserializing())
            {
                log::trace!("Setting default for unset field: {}", field.name);
                partial = partial.set_nth_field_to_default(idx)?;
            }
        }

        // Note: we do NOT call partial.end() here because:
        // - The caller (open_path_to) already called begin_field for this struct
        // - The caller will handle closing it

        // End Option if we entered one
        if entered_option {
            partial = partial.end()?;
        }

        Ok(partial)
    }

    /// Close paths from the current open state back to the common prefix with target.
    fn close_paths_to(
        &self,
        partial: Partial<'facet>,
        open_paths: &mut Vec<OpenPathEntry>,
        target: &FieldPath,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        let target_segments = target.segments();

        // Find common prefix length
        let common_len = open_paths
            .iter()
            .zip(target_segments.iter())
            .take_while(|(entry, seg)| entry.segment == **seg)
            .count();

        // Close segments beyond common prefix
        while open_paths.len() > common_len {
            let entry = open_paths.pop();
            if let Some(entry) = entry {
                match &entry.segment {
                    PathSegment::Field(_) => {
                        // If we entered an Option for this field, close it first
                        if entry.entered_option {
                            partial = partial.end()?; // Close the Some wrapper
                            log::trace!("Closed Option wrapper, depth now {}", open_paths.len());
                        }
                        partial = partial.end()?; // Close the field itself
                        log::trace!("Closed field segment, depth now {}", open_paths.len());
                    }
                    PathSegment::Variant(_, _) => {
                        // Variant segments do NOT push a frame - select_variant_named only
                        // updates the tracker on the current frame. So we don't call end() here.
                        log::trace!(
                            "Skipped closing variant segment (no frame pushed), depth now {}",
                            open_paths.len()
                        );
                    }
                }
            }
        }

        Ok(partial)
    }

    /// Open path segments from current state to target (excluding the final field).
    ///
    /// If `enter_new_options` is false, this will return `Ok(Some(field_name))` if it would need to
    /// enter a new `Option<T>` field that isn't already open, where field_name is the name of the
    /// Option field that was encountered. This is used when initializing missing optional fields -
    /// we don't want to enter a new `Option<T>` just to set defaults, as that would turn None into
    /// Some(default).
    ///
    /// Returns `Ok(None)` if the path was fully opened.
    fn open_path_to(
        &self,
        partial: Partial<'facet>,
        open_paths: &mut Vec<OpenPathEntry>,
        target: &FieldPath,
        enter_new_options: bool,
    ) -> Result<(Partial<'facet>, Option<&'static str>)> {
        let mut partial = partial;
        let target_segments = target.segments();

        // The last segment is the actual field we're setting - don't open it as a struct
        let segments_to_open = if target_segments.is_empty() {
            &[]
        } else {
            &target_segments[..target_segments.len() - 1]
        };

        // Open segments we don't have yet
        for (i, segment) in segments_to_open.iter().enumerate() {
            if i >= open_paths.len() {
                match segment {
                    PathSegment::Field(name) => {
                        // Check if this field is an Option BEFORE opening it
                        // by looking at the field definition in the current struct
                        if !enter_new_options
                            && let Type::User(UserType::Struct(struct_def)) = partial.shape().ty
                            && let Some(field) = struct_def.fields.iter().find(|f| f.name == *name)
                        {
                            let field_shape = field.shape();
                            if matches!(field_shape.def, Def::Option(_)) {
                                log::trace!(
                                    "Field {name} is Option<T>, not entering (enter_new_options=false)"
                                );
                                return Ok((partial, Some(name)));
                            }
                        }
                        log::trace!("Opening field: {name}");
                        partial = partial.begin_field(name)?;
                        // Handle Option wrapper - if the field is Option<T>, call begin_some()
                        // to unwrap it so we can access fields inside T
                        let entered_option = if let Def::Option(_) = partial.shape().def {
                            if !enter_new_options {
                                // This shouldn't happen anymore since we check above,
                                // but keep as safety net
                                log::trace!(
                                    "Field {name} is Option<T> but enter_new_options=false, backing out"
                                );
                                partial = partial.end()?; // Close the field we just opened
                                return Ok((partial, Some(name)));
                            }
                            log::trace!("Field {name} is Option<T>, calling begin_some()");
                            partial = partial.begin_some()?;
                            true
                        } else {
                            false
                        };
                        open_paths.push(OpenPathEntry {
                            segment: segment.clone(),
                            entered_option,
                        });
                    }
                    PathSegment::Variant(_field_name, variant_name) => {
                        // Variant segment: the field was already entered by a preceding
                        // Field segment, so we just need to select the variant
                        log::trace!("Selecting variant: {variant_name}");
                        partial = partial.select_variant_named(variant_name)?;
                        open_paths.push(OpenPathEntry {
                            segment: segment.clone(),
                            entered_option: false,
                        });
                    }
                }
            }
        }

        // Now begin the final field (the property itself)
        if let Some(last_segment) = target_segments.last() {
            match last_segment {
                PathSegment::Field(name) => {
                    log::trace!("Beginning final field: {name}");
                    partial = partial.begin_field(name)?;
                }
                PathSegment::Variant(_field_name, variant_name) => {
                    // Unlikely for the final segment to be a variant, but handle it
                    log::trace!("Selecting final variant: {variant_name}");
                    partial = partial.select_variant_named(variant_name)?;
                }
            }
        }

        Ok((partial, None))
    }

    /// Close all open paths.
    fn close_all_paths(
        &self,
        partial: Partial<'facet>,
        open_paths: &mut Vec<OpenPathEntry>,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        while !open_paths.is_empty() {
            let entry = open_paths.pop();
            if let Some(entry) = entry {
                // Only call end() for Field segments - Variant segments don't push a frame
                if let PathSegment::Field(_) = entry.segment {
                    // If we entered an Option for this field, close it first
                    if entry.entered_option {
                        partial = partial.end()?; // Close the Some wrapper
                        log::trace!("Closed Option wrapper, depth now {}", open_paths.len());
                    }
                    partial = partial.end()?;
                    log::trace!("Closed field segment, depth now {}", open_paths.len());
                } else {
                    log::trace!(
                        "Skipped closing variant segment, depth now {}",
                        open_paths.len()
                    );
                }
            }
        }
        Ok(partial)
    }

    #[allow(clippy::only_used_in_recursion)]
    fn deserialize_value(
        &mut self,
        partial: Partial<'facet>,
        value: KdlValue,
        span: Option<SourceSpan>,
    ) -> Result<Partial<'facet>> {
        let mut partial = partial;
        log::trace!("Entering `deserialize_value` method at {}", partial.path());

        log::trace!("Parsing {:?} into {}", &value, partial.path());

        // Check if we're deserializing into Spanned<T>
        if is_spanned_shape(partial.shape()) {
            log::trace!("Detected Spanned<T> wrapper at {}", partial.path());

            // Deserialize the inner value into the `value` field
            partial = partial.begin_field("value")?;
            partial = self.deserialize_value(partial, value, None)?; // No span for inner value
            partial = partial.end()?;

            // Set the span field - SourceSpan stores offset and length
            partial = partial.begin_field("span")?;
            if let Some(ss) = span {
                partial = partial.set_field("offset", ss.offset())?;
                partial = partial.set_field("len", ss.len())?;
            } else {
                // No span available, use defaults (0, 0)
                partial = partial.set_field("offset", 0usize)?;
                partial = partial.set_field("len", 0usize)?;
            }
            partial = partial.end()?;

            log::trace!("Exiting `deserialize_value` method (Spanned path)");
            return Ok(partial);
        }

        // Handle Option<T> - either set to None (for null) or unwrap and recurse
        if let Def::Option(_) = partial.shape().def {
            if value == KdlValue::Null {
                partial = partial.set_default()?;
                log::trace!("Exiting `deserialize_value` method (Option None)");
                return Ok(partial);
            } else {
                partial = partial.begin_some()?;
                // Recurse to handle the inner type (which might be Spanned<T>, etc.)
                partial = self.deserialize_value(partial, value, span)?;
                partial = partial.end()?;
                log::trace!("Exiting `deserialize_value` method (Option Some)");
                return Ok(partial);
            }
        }

        // Handle Pointer types (Box<T>, Arc<T>, Rc<T>, etc.)
        if let Def::Pointer(ptr_def) = partial.shape().def {
            log::trace!(
                "Field is Pointer type ({:?}), calling begin_smart_ptr()",
                ptr_def.known
            );
            partial = partial.begin_smart_ptr()?;
            // Recurse to handle the inner type
            partial = self.deserialize_value(partial, value, span)?;
            partial = partial.end()?;
            log::trace!("Exiting `deserialize_value` method (Pointer)");
            return Ok(partial);
        }

        // Handle transparent/inner wrapper types (like Utf8PathBuf, newtype wrappers, etc.)
        // These should deserialize as their inner type, UNLESS they have parse_from_str
        // (like Utf8PathBuf which can parse directly from a string)
        if partial.shape().inner.is_some() && !partial.shape().vtable.has_parse() {
            log::trace!(
                "Field has inner type, using begin_inner() for {}",
                partial.shape().type_identifier
            );
            partial = partial.begin_inner()?;
            partial = self.deserialize_value(partial, value, span)?;
            partial = partial.end()?;
            log::trace!("Exiting `deserialize_value` method (inner/transparent)");
            return Ok(partial);
        }

        // For scalars, handle primitive values directly
        if !matches!(partial.shape().def, Def::Scalar) {
            return Err(
                KdlErrorKind::UnsupportedValueDef(format!("{:?}", partial.shape().def)).into(),
            );
        }

        match value {
            KdlValue::String(string) => {
                // Try parse_from_str first if the type supports it (e.g., Utf8PathBuf, chrono types)
                if partial.shape().vtable.has_parse() {
                    partial = partial.parse_from_str(&string)?;
                } else {
                    partial = partial.set(string)?;
                }
            }
            KdlValue::Integer(integer) => {
                let size = match partial.shape().layout {
                    ShapeLayout::Sized(layout) => layout.size(),
                    ShapeLayout::Unsized => {
                        return Err(KdlErrorKind::InvalidValueForShape(
                            "cannot assign integer to unsized type".into(),
                        )
                        .into());
                    }
                };
                let ty = match partial.shape().ty {
                    Type::Primitive(PrimitiveType::Numeric(ty)) => ty,
                    _ => {
                        return Err(KdlErrorKind::InvalidValueForShape(
                            "integer value requires numeric type".into(),
                        )
                        .into());
                    }
                };
                match (ty, size) {
                    // Unsigned integers
                    (NumericType::Integer { signed: false }, 1) => {
                        partial = partial.set(integer as u8)?
                    }
                    (NumericType::Integer { signed: false }, 2) => {
                        partial = partial.set(integer as u16)?
                    }
                    (NumericType::Integer { signed: false }, 4) => {
                        partial = partial.set(integer as u32)?
                    }
                    (NumericType::Integer { signed: false }, 8) => {
                        partial = partial.set(integer as u64)?
                    }
                    (NumericType::Integer { signed: false }, 16) => {
                        partial = partial.set(integer as u128)?
                    }
                    // Signed integers
                    (NumericType::Integer { signed: true }, 1) => {
                        partial = partial.set(integer as i8)?
                    }
                    (NumericType::Integer { signed: true }, 2) => {
                        partial = partial.set(integer as i16)?
                    }
                    (NumericType::Integer { signed: true }, 4) => {
                        partial = partial.set(integer as i32)?
                    }
                    (NumericType::Integer { signed: true }, 8) => {
                        partial = partial.set(integer as i64)?
                    }
                    (NumericType::Integer { signed: true }, 16) => {
                        partial = partial.set(integer)?
                    } // already i128
                    // Floats from integer literals
                    (NumericType::Float, 4) => partial = partial.set(integer as f32)?,
                    (NumericType::Float, 8) => partial = partial.set(integer as f64)?,
                    _ => {
                        return Err(KdlErrorKind::InvalidValueForShape(format!(
                            "unhandled numeric type: {ty:?} with size {size}"
                        ))
                        .into());
                    }
                };
            }
            KdlValue::Float(float) => {
                let size = match partial.shape().layout {
                    ShapeLayout::Sized(layout) => layout.size(),
                    ShapeLayout::Unsized => {
                        return Err(KdlErrorKind::InvalidValueForShape(
                            "cannot assign float to unsized type".into(),
                        )
                        .into());
                    }
                };
                match size {
                    4 => partial = partial.set(float as f32)?,
                    8 => partial = partial.set(float)?, // already f64
                    _ => {
                        return Err(KdlErrorKind::InvalidValueForShape(format!(
                            "unhandled float size: {size}"
                        ))
                        .into());
                    }
                };
            }
            KdlValue::Bool(bool) => {
                partial = partial.set(bool)?;
            }
            KdlValue::Null => {
                // Null should have been handled by Option above
                return Err(KdlErrorKind::InvalidValueForShape(
                    "null value only valid for Option types".into(),
                )
                .into());
            }
        };

        log::trace!("Exiting `deserialize_value` method");

        Ok(partial)
    }
}

/// Get the "tightness" score of a shape for disambiguation.
/// Lower score = tighter/more specific type = preferred.
///
/// For integers: smaller byte size is tighter (u8 < u16 < u32 < u64)
/// For floats: f32 < f64
/// For other types: equal (0)
fn shape_tightness(shape: &Shape) -> usize {
    match shape.layout {
        ShapeLayout::Sized(layout) => layout.size(),
        ShapeLayout::Unsized => usize::MAX,
    }
}

/// Check if a KDL value can be deserialized into the given shape.
///
/// This is used for value-based type disambiguation when multiple enum variants
/// have the same field name but different types (e.g., u8 vs u16).
fn kdl_value_fits_shape(value: &KdlValue, shape: &'static Shape) -> bool {
    // Unwrap Option types to check the inner type
    let inner_shape = match shape.def {
        Def::Option(opt) => opt.t,
        _ => shape,
    };

    match value {
        KdlValue::String(_) => {
            // Strings fit String type
            inner_shape.type_identifier == "String" || inner_shape.type_identifier == "&str"
        }
        KdlValue::Integer(n) => {
            // Check if this integer fits in the target numeric type
            let size = match inner_shape.layout {
                ShapeLayout::Sized(layout) => layout.size(),
                ShapeLayout::Unsized => return false,
            };
            match inner_shape.ty {
                Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: false })) => {
                    match size {
                        1 => *n >= 0 && *n <= u8::MAX as i128,
                        2 => *n >= 0 && *n <= u16::MAX as i128,
                        4 => *n >= 0 && *n <= u32::MAX as i128,
                        8 => *n >= 0 && *n <= u64::MAX as i128,
                        16 => *n >= 0, // u128 - any non-negative i128 fits
                        _ => false,
                    }
                }
                Type::Primitive(PrimitiveType::Numeric(NumericType::Integer { signed: true })) => {
                    match size {
                        1 => *n >= i8::MIN as i128 && *n <= i8::MAX as i128,
                        2 => *n >= i16::MIN as i128 && *n <= i16::MAX as i128,
                        4 => *n >= i32::MIN as i128 && *n <= i32::MAX as i128,
                        8 => *n >= i64::MIN as i128 && *n <= i64::MAX as i128,
                        16 => true, // i128 - any i128 fits
                        _ => false,
                    }
                }
                Type::Primitive(PrimitiveType::Numeric(NumericType::Float)) => {
                    // Integers can be coerced to floats
                    true
                }
                _ => false,
            }
        }
        KdlValue::Float(_) => {
            // Floats fit float types
            matches!(
                inner_shape.ty,
                Type::Primitive(PrimitiveType::Numeric(NumericType::Float))
            )
        }
        KdlValue::Bool(_) => {
            // Booleans fit bool type
            inner_shape.type_identifier == "bool"
        }
        KdlValue::Null => {
            // Null fits Option types
            matches!(shape.def, Def::Option(_))
        }
    }
}

/// Deserialize a value of type `T` from a KDL string.
///
/// Returns a [`KdlError`] if the input KDL is invalid or doesn't match `T`.
///
/// # Example
/// ```
/// # use facet::Facet;
/// # use facet_kdl_legacy as kdl;
/// # use facet_kdl_legacy::from_str;
/// #[derive(Facet, Debug, PartialEq)]
/// struct Config {
///     #[facet(kdl::child)]
///     server: Server,
/// }
///
/// #[derive(Facet, Debug, PartialEq)]
/// struct Server {
///     #[facet(kdl::argument)]
///     host: String,
///     #[facet(kdl::property)]
///     port: u16,
/// }
///
/// # fn main() -> Result<(), facet_kdl_legacy::KdlError> {
/// let config: Config = from_str(r#"server "localhost" port=8080"#)?;
/// assert_eq!(config.server.host, "localhost");
/// assert_eq!(config.server.port, 8080);
/// # Ok(())
/// # }
/// ```
pub fn from_str<'input, 'facet: 'shape, 'shape, T>(kdl: &'input str) -> Result<T>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!("Entering `from_str` function");

    KdlDeserializer::from_str(kdl)
}

/// Deserialize a KDL string into an owned type.
///
/// This variant does not require the input to outlive the result, making it
/// suitable for deserializing from temporary buffers (e.g., HTTP request bodies).
///
/// Types containing `&str` fields cannot be deserialized with this function;
/// use `String` or `Cow<str>` instead.
pub fn from_str_owned<T: Facet<'static>>(kdl: &str) -> Result<T> {
    log::trace!("Entering `from_str_owned` function");

    KdlDeserializer::from_str(kdl)
}
