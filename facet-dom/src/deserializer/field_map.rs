//! Precomputed field lookup for struct deserialization.

use std::collections::HashMap;

use facet_core::{Def, Field, StructKind, StructType, Type, UserType};

use crate::tracing_macros::{trace, trace_span};

/// Info about a field in a struct for deserialization purposes.
#[derive(Clone)]
pub(crate) struct FieldInfo {
    pub idx: usize,
    #[allow(dead_code)]
    pub field: &'static Field,
    /// True if this field is a list type (Vec, etc.) - NOT an array or set
    pub is_list: bool,
    /// True if this field is a fixed-size array [T; N]
    pub is_array: bool,
    /// True if this field is a set type (HashSet, BTreeSet, etc.)
    pub is_set: bool,
    /// The namespace URI this field must match (from `xml::ns` attribute), if any.
    pub namespace: Option<&'static str>,
}

/// Info about a flattened child field - a field inside a flattened struct that
/// appears as a sibling in the XML.
#[derive(Clone)]
pub(crate) struct FlattenedChildInfo {
    /// Index of the flattened parent field in the outer struct
    pub parent_idx: usize,
    /// Index of the child field within the flattened struct
    pub child_idx: usize,
    /// Info about the child field (is_list, is_array, etc.)
    pub child_info: FieldInfo,
    /// Whether the parent field is an Option<Struct> (requires begin_some())
    pub parent_is_option: bool,
}

/// Info about a flattened enum field.
#[derive(Clone)]
pub(crate) struct FlattenedEnumInfo {
    /// Index of the flattened enum field in the outer struct
    pub field_idx: usize,
    /// The field info (kept for potential future use)
    #[allow(dead_code)]
    pub field_info: FieldInfo,
}

/// Precomputed field lookup map for a struct.
///
/// This separates "what fields does this struct have" from the parsing loop,
/// making the code cleaner and avoiding repeated linear scans.
pub(crate) struct StructFieldMap {
    /// Fields marked with `xml::attribute`, keyed by name/rename.
    /// Multiple fields can have the same name if they have different namespace constraints.
    attribute_fields: HashMap<&'static str, Vec<FieldInfo>>,
    /// Fields that are child elements, keyed by name/rename.
    /// Multiple fields can have the same name if they have different namespace constraints.
    element_fields: HashMap<&'static str, Vec<FieldInfo>>,
    /// The field marked with `xml::elements` (collects all unmatched children)
    pub elements_field: Option<FieldInfo>,
    /// The field marked with `xml::text` (collects text content)
    pub text_field: Option<FieldInfo>,
    /// For tuple structs: fields in order for positional matching.
    /// Uses `<item>` elements matched by position.
    pub tuple_fields: Option<Vec<FieldInfo>>,
    /// Flattened child fields - child fields from flattened structs that appear as siblings.
    /// Keyed by the child's element name (rename or field name).
    flattened_children: HashMap<&'static str, Vec<FlattenedChildInfo>>,
    /// Flattened enum field - enum variants match against child elements directly.
    /// Only one flattened enum is supported per struct.
    pub flattened_enum: Option<FlattenedEnumInfo>,
    /// Flattened map fields - capture unknown elements as key-value pairs.
    /// Multiple flattened maps are supported; first match wins.
    pub flattened_maps: Vec<FieldInfo>,
    /// Whether this struct has any flattened fields (requires deferred mode)
    pub has_flatten: bool,
}

impl StructFieldMap {
    /// Build the field map from a struct definition.
    ///
    /// The `ns_all` parameter is the default namespace for element fields that don't
    /// have an explicit `xml::ns` attribute. When set, fields without `xml::ns` will
    /// inherit this namespace.
    pub fn new(struct_def: &'static StructType, ns_all: Option<&'static str>) -> Self {
        trace_span!("StructFieldMap::new");

        let mut attribute_fields: HashMap<&'static str, Vec<FieldInfo>> = HashMap::new();
        let mut element_fields: HashMap<&'static str, Vec<FieldInfo>> = HashMap::new();
        let mut elements_field = None;
        let mut text_field = None;
        let mut flattened_children: HashMap<&'static str, Vec<FlattenedChildInfo>> = HashMap::new();
        let mut flattened_enum: Option<FlattenedEnumInfo> = None;
        let mut flattened_maps: Vec<FieldInfo> = Vec::new();
        let mut has_flatten = false;

        for (idx, field) in struct_def.fields.iter().enumerate() {
            // Check if this field is flattened
            if field.is_flattened() {
                has_flatten = true;
                trace!(idx, field_name = %field.name, "found flattened field");

                // Check if the parent field is Option<Struct>
                let parent_is_option = matches!(field.shape().def, Def::Option(_));

                // Check if this is a flattened enum
                if is_flattened_enum(field) {
                    let shape = field.shape();
                    let (is_list, is_array, is_set) = classify_sequence_shape(shape);
                    let namespace: Option<&'static str> = field
                        .get_attr(Some("xml"), "ns")
                        .and_then(|attr| attr.get_as::<&str>().copied());

                    trace!(idx, field_name = %field.name, "found flattened enum field");
                    flattened_enum = Some(FlattenedEnumInfo {
                        field_idx: idx,
                        field_info: FieldInfo {
                            idx,
                            field,
                            is_list,
                            is_array,
                            is_set,
                            namespace,
                        },
                    });
                    continue;
                }

                // Get the inner struct's fields
                if let Some(inner_struct_def) = get_flattened_struct_def(field) {
                    for (child_idx, child_field) in inner_struct_def.fields.iter().enumerate() {
                        let child_shape = child_field.shape();
                        let (is_list, is_array, is_set) = classify_sequence_shape(child_shape);
                        let namespace: Option<&'static str> = child_field
                            .get_attr(Some("xml"), "ns")
                            .and_then(|attr| attr.get_as::<&str>().copied());
                        let child_key = child_field.rename.unwrap_or(child_field.name);

                        let child_info = FieldInfo {
                            idx: child_idx,
                            field: child_field,
                            is_list,
                            is_array,
                            is_set,
                            namespace,
                        };

                        let flattened_child = FlattenedChildInfo {
                            parent_idx: idx,
                            child_idx,
                            child_info,
                            parent_is_option,
                        };

                        trace!(
                            parent_idx = idx,
                            parent_name = %field.name,
                            child_idx,
                            child_name = %child_field.name,
                            child_key = %child_key,
                            parent_is_option,
                            "registering flattened child"
                        );

                        flattened_children
                            .entry(child_key)
                            .or_default()
                            .push(flattened_child.clone());

                        // Also register alias if present
                        if let Some(alias) = child_field.alias {
                            trace!(
                                parent_idx = idx,
                                child_idx,
                                alias = %alias,
                                "registering flattened child alias"
                            );
                            flattened_children
                                .entry(alias)
                                .or_default()
                                .push(flattened_child);
                        }
                    }
                } else if is_flattened_map(field) {
                    // Flattened map - captures unknown elements as key-value pairs
                    let _shape = field.shape();
                    let namespace: Option<&'static str> = field
                        .get_attr(Some("xml"), "ns")
                        .and_then(|attr| attr.get_as::<&str>().copied());

                    trace!(idx, field_name = %field.name, "found flattened map field");
                    flattened_maps.push(FieldInfo {
                        idx,
                        field,
                        is_list: false,
                        is_array: false,
                        is_set: false,
                        namespace,
                    });
                }
                continue; // Don't register the flattened field itself as an element
            }

            // Check if this field is a list, array, or set type
            // Need to look through pointers (Arc<[T]>, Box<[T]>, etc.)
            let shape = field.shape();
            let (is_list, is_array, is_set) = classify_sequence_shape(shape);

            // Extract namespace from xml::ns attribute if present
            let namespace: Option<&'static str> = field
                .get_attr(Some("xml"), "ns")
                .and_then(|attr| attr.get_as::<&str>().copied());

            // For all fields (list or not):
            //   - element name uses rename if present, else field name
            // For list fields, this is the repeated item element name (flat, no wrapper)
            let element_key = field.rename.unwrap_or(field.name);

            if field.is_attribute() {
                // Attributes always use rename or field name directly
                let attr_key = field.rename.unwrap_or(field.name);
                trace!(idx, field_name = %field.name, key = %attr_key, namespace = ?namespace, "found attribute field");
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    namespace,
                };
                attribute_fields.entry(attr_key).or_default().push(info);
            } else if field.is_elements() {
                trace!(idx, field_name = %field.name, "found elements collection field");
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    namespace,
                };
                elements_field = Some(info);
            } else if field.is_text() {
                trace!(idx, field_name = %field.name, "found text field");
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    namespace,
                };
                text_field = Some(info);
            } else {
                // Default: unmarked fields and explicit xml::element fields are child elements
                // Apply ns_all to elements without explicit namespace
                let effective_namespace = namespace.or(ns_all);
                trace!(idx, field_name = %field.name, field_rename = ?field.rename, key = %element_key, alias = ?field.alias, is_list, is_array, is_set, namespace = ?effective_namespace, "found element field");
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    namespace: effective_namespace,
                };
                element_fields
                    .entry(element_key)
                    .or_default()
                    .push(info.clone());

                // Also register alias if present
                if let Some(alias) = field.alias {
                    trace!(idx, field_name = %field.name, alias = %alias, "registering alias for element field");
                    element_fields.entry(alias).or_default().push(info);
                }
            }
        }

        // For tuple structs, build positional field list
        let tuple_fields = if matches!(struct_def.kind, StructKind::TupleStruct | StructKind::Tuple)
        {
            trace!(
                field_count = struct_def.fields.len(),
                "building tuple field list"
            );
            let fields: Vec<FieldInfo> = struct_def
                .fields
                .iter()
                .enumerate()
                .map(|(idx, field)| {
                    let shape = field.shape();
                    let (is_list, is_array, is_set) = classify_sequence_shape(shape);
                    FieldInfo {
                        idx,
                        field,
                        is_list,
                        is_array,
                        is_set,
                        namespace: None,
                    }
                })
                .collect();
            Some(fields)
        } else {
            None
        };

        trace!(
            attribute_count = attribute_fields.len(),
            element_count = element_fields.len(),
            flattened_count = flattened_children.len(),
            has_flattened_enum = flattened_enum.is_some(),
            flattened_maps_count = flattened_maps.len(),
            has_flatten,
            has_elements = elements_field.is_some(),
            has_text = text_field.is_some(),
            is_tuple = tuple_fields.is_some(),
            "field map built"
        );

        Self {
            attribute_fields,
            element_fields,
            elements_field,
            text_field,
            tuple_fields,
            flattened_children,
            flattened_enum,
            flattened_maps,
            has_flatten,
        }
    }

    /// Find an attribute field by name and namespace.
    ///
    /// Returns `Some` if the name matches AND the namespace matches:
    /// - If the field has no namespace constraint, it matches any namespace
    /// - If the field has a namespace constraint, the incoming namespace must match exactly
    ///
    /// When multiple fields have the same name, prefers exact namespace match over wildcard.
    pub fn find_attribute(&self, name: &str, namespace: Option<&str>) -> Option<&FieldInfo> {
        let result = self.attribute_fields.get(name).and_then(|fields| {
            // First try to find an exact namespace match
            let exact_match = fields
                .iter()
                .find(|info| info.namespace.is_some() && info.namespace == namespace);
            if exact_match.is_some() {
                return exact_match;
            }
            // Fall back to a field with no namespace constraint
            fields.iter().find(|info| info.namespace.is_none())
        });
        trace!(name, ?namespace, found = result.is_some(), "find_attribute");
        result
    }

    /// Find an element field by tag name and namespace.
    ///
    /// Returns `Some` if the name matches AND the namespace matches:
    /// - If the field has no namespace constraint, it matches any namespace
    /// - If the field has a namespace constraint, the incoming namespace must match exactly
    ///
    /// When multiple fields have the same name, prefers exact namespace match over wildcard.
    pub fn find_element(&self, tag: &str, namespace: Option<&str>) -> Option<&FieldInfo> {
        let result = self.element_fields.get(tag).and_then(|fields| {
            // First try to find an exact namespace match
            let exact_match = fields
                .iter()
                .find(|info| info.namespace.is_some() && info.namespace == namespace);
            if exact_match.is_some() {
                return exact_match;
            }
            // Fall back to a field with no namespace constraint
            fields.iter().find(|info| info.namespace.is_none())
        });
        trace!(tag, ?namespace, found = result.is_some(), "find_element");
        result
    }

    /// Find a flattened child field by tag name and namespace.
    ///
    /// Returns `Some` if the name matches a child field from a flattened struct.
    pub fn find_flattened_child(
        &self,
        tag: &str,
        namespace: Option<&str>,
    ) -> Option<&FlattenedChildInfo> {
        let result = self.flattened_children.get(tag).and_then(|children| {
            // First try to find an exact namespace match
            let exact_match = children.iter().find(|info| {
                info.child_info.namespace.is_some() && info.child_info.namespace == namespace
            });
            if exact_match.is_some() {
                return exact_match;
            }
            // Fall back to a field with no namespace constraint
            children
                .iter()
                .find(|info| info.child_info.namespace.is_none())
        });
        trace!(
            tag,
            ?namespace,
            found = result.is_some(),
            "find_flattened_child"
        );
        result
    }

    /// Get a tuple field by position index.
    /// Returns None if this is not a tuple struct or if the index is out of bounds.
    pub fn get_tuple_field(&self, index: usize) -> Option<&FieldInfo> {
        self.tuple_fields
            .as_ref()
            .and_then(|fields| fields.get(index))
    }

    /// Returns true if this is a tuple struct (fields matched by position).
    pub fn is_tuple(&self) -> bool {
        self.tuple_fields.is_some()
    }
}

/// Check if a flattened field is an enum type.
fn is_flattened_enum(field: &'static Field) -> bool {
    let shape = field.shape();

    // Check for direct enum
    if matches!(&shape.ty, Type::User(UserType::Enum(_))) {
        return true;
    }

    // Check for Option<Enum>
    if let Def::Option(option_def) = &shape.def {
        let inner_shape = option_def.t();
        if matches!(&inner_shape.ty, Type::User(UserType::Enum(_))) {
            return true;
        }
    }

    false
}

/// Get the inner struct definition from a flattened field.
/// Handles direct structs and Option<Struct>.
fn get_flattened_struct_def(field: &'static Field) -> Option<&'static StructType> {
    let shape = field.shape();

    // Check for direct struct
    if let Type::User(UserType::Struct(struct_def)) = &shape.ty {
        return Some(struct_def);
    }

    // Check for Option<Struct>
    if let Def::Option(option_def) = &shape.def {
        let inner_shape = option_def.t();
        if let Type::User(UserType::Struct(struct_def)) = &inner_shape.ty {
            return Some(struct_def);
        }
    }

    None
}

/// Check if a flattened field is a map type (HashMap, BTreeMap, etc.)
fn is_flattened_map(field: &'static Field) -> bool {
    let shape = field.shape();

    // Check for direct map
    if matches!(&shape.def, Def::Map(_)) {
        return true;
    }

    // Check for Option<Map>
    if let Def::Option(option_def) = &shape.def {
        let inner_shape = option_def.t();
        if matches!(&inner_shape.def, Def::Map(_)) {
            return true;
        }
    }

    false
}

/// Classify a shape as list, array, set, or neither. Returns (is_list, is_array, is_set).
/// Lists are Vec, slices. Arrays are [T; N]. Sets are HashSet, BTreeSet. Looks through pointers.
fn classify_sequence_shape(shape: &facet_core::Shape) -> (bool, bool, bool) {
    match &shape.def {
        Def::List(_) | Def::Slice(_) => (true, false, false),
        Def::Array(_) => (false, true, false),
        Def::Set(_) => (false, false, true),
        Def::Pointer(ptr_def) => {
            // Look through Arc<[T]>, Box<[T]>, Rc<[T]>, etc.
            ptr_def
                .pointee()
                .map(classify_sequence_shape)
                .unwrap_or((false, false, false))
        }
        _ => (false, false, false),
    }
}
