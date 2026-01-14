//! Precomputed field lookup for struct deserialization.

use std::collections::HashMap;

use facet_core::{Def, Field, StructKind, StructType, Type, UserType};

use crate::naming::dom_key;
use facet_singularize::singularize;

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
    /// True if this field is a tuple type (i32, String, bool)
    pub is_tuple: bool,
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

/// Info about a flattened map that's nested inside another flattened struct.
/// E.g., struct A { #[facet(flatten)] b: B } where B { #[facet(flatten)] extra: HashMap }
#[derive(Clone)]
pub(crate) struct NestedFlattenedMapInfo {
    /// Index of the flattened parent field in the outer struct (e.g., `b` in A)
    pub parent_idx: usize,
    /// Index of the map field within the flattened struct (e.g., `extra` in B)
    pub child_idx: usize,
    /// Info about the map field
    pub child_info: FieldInfo,
    /// Whether the parent field is an Option<Struct>
    pub parent_is_option: bool,
}

/// Precomputed field lookup map for a struct.
///
/// This separates "what fields does this struct have" from the parsing loop,
/// making the code cleaner and avoiding repeated linear scans.
pub(crate) struct StructFieldMap {
    /// Fields marked with `xml::attribute`, keyed by lowerCamelCase name or rename.
    /// Multiple fields can have the same name if they have different namespace constraints.
    attribute_fields: HashMap<String, Vec<FieldInfo>>,
    /// Fields that are child elements, keyed by lowerCamelCase name or rename.
    /// Multiple fields can have the same name if they have different namespace constraints.
    element_fields: HashMap<String, Vec<FieldInfo>>,
    /// The field marked with `xml::elements` (collects all unmatched children)
    pub elements_field: Option<FieldInfo>,
    /// The field marked with `xml::attribute` as a catch-all (collects all unmatched attribute values)
    pub attributes_field: Option<FieldInfo>,
    /// The field marked with `xml::text` (collects text content)
    pub text_field: Option<FieldInfo>,
    /// The field marked with `xml::tag` or `html::tag` (captures element tag name)
    pub tag_field: Option<FieldInfo>,
    /// The field marked with `#[facet(other)]` (fallback when root doesn't match)
    pub other_field: Option<FieldInfo>,
    /// For tuple structs: fields in order for positional matching.
    /// Uses `<item>` elements matched by position.
    pub tuple_fields: Option<Vec<FieldInfo>>,
    /// Flattened child fields - child fields from flattened structs that appear as siblings.
    /// Keyed by the child's lowerCamelCase element name or rename.
    flattened_children: HashMap<String, Vec<FlattenedChildInfo>>,
    /// Flattened attribute fields - attribute fields from flattened structs.
    /// Keyed by the lowerCamelCase attribute name or rename.
    flattened_attributes: HashMap<String, Vec<FlattenedChildInfo>>,
    /// Flattened enum field - enum variants match against child elements directly.
    /// Only one flattened enum is supported per struct.
    pub flattened_enum: Option<FlattenedEnumInfo>,
    /// Flattened map fields - capture unknown elements as key-value pairs.
    /// Multiple flattened maps are supported; first match wins.
    pub flattened_maps: Vec<FieldInfo>,
    /// Flattened attribute map fields - capture unknown attributes as key-value pairs.
    /// Multiple are supported; first match wins.
    pub flattened_attr_maps: Vec<FieldInfo>,
    /// Nested flattened attribute map fields (inside another flattened struct) - capture unknown attributes.
    pub nested_flattened_attr_maps: Vec<NestedFlattenedMapInfo>,
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
        let mut attribute_fields: HashMap<String, Vec<FieldInfo>> = HashMap::new();
        let mut element_fields: HashMap<String, Vec<FieldInfo>> = HashMap::new();
        let mut elements_field = None;
        let mut attributes_field = None;
        let mut text_field = None;
        let mut tag_field = None;
        let mut other_field = None;
        let mut flattened_children: HashMap<String, Vec<FlattenedChildInfo>> = HashMap::new();
        let mut flattened_attributes: HashMap<String, Vec<FlattenedChildInfo>> = HashMap::new();
        let mut flattened_enum: Option<FlattenedEnumInfo> = None;
        let mut flattened_maps: Vec<FieldInfo> = Vec::new();
        let mut flattened_attr_maps: Vec<FieldInfo> = Vec::new();
        let mut nested_flattened_attr_maps: Vec<NestedFlattenedMapInfo> = Vec::new();
        let mut has_flatten = false;

        for (idx, field) in struct_def.fields.iter().enumerate() {
            // Check if this field is flattened
            if field.is_flattened() {
                has_flatten = true;

                // Check if the parent field is Option<Struct>
                let parent_is_option = matches!(field.shape().def, Def::Option(_));

                // Check if this is a flattened enum
                if is_flattened_enum(field) {
                    let shape = field.shape();
                    let (is_list, is_array, is_set, is_tuple) = classify_sequence_shape(shape);
                    let namespace: Option<&'static str> = field
                        .get_attr(Some("xml"), "ns")
                        .and_then(|attr| attr.get_as::<&str>().copied());

                    flattened_enum = Some(FlattenedEnumInfo {
                        field_idx: idx,
                        field_info: FieldInfo {
                            idx,
                            field,
                            is_list,
                            is_array,
                            is_set,
                            is_tuple,
                            namespace,
                        },
                    });
                    continue;
                }

                // Get the inner struct's fields
                if let Some(inner_struct_def) = get_flattened_struct_def(field) {
                    for (child_idx, child_field) in inner_struct_def.fields.iter().enumerate() {
                        // Check if this child field is itself a flattened map
                        // (e.g., #[facet(flatten)] extra: HashMap<String, String>)
                        if child_field.is_flattened() && is_flattened_map(child_field) {
                            let namespace: Option<&'static str> = child_field
                                .get_attr(Some("xml"), "ns")
                                .and_then(|attr| attr.get_as::<&str>().copied());

                            let info = FieldInfo {
                                idx: child_idx,
                                field: child_field,
                                is_list: false,
                                is_array: false,
                                is_set: false,
                                is_tuple: false,
                                namespace,
                            };

                            // Register as nested flattened map info
                            let nested_info = NestedFlattenedMapInfo {
                                parent_idx: idx,
                                child_idx,
                                child_info: info.clone(),
                                parent_is_option,
                            };
                            nested_flattened_attr_maps.push(nested_info);
                            continue;
                        }

                        let child_shape = child_field.shape();
                        let (is_list, is_array, is_set, is_tuple) =
                            classify_sequence_shape(child_shape);
                        let namespace: Option<&'static str> = child_field
                            .get_attr(Some("xml"), "ns")
                            .and_then(|attr| attr.get_as::<&str>().copied());
                        // Compute child key: rename (as-is) or lowerCamelCase(name)
                        let child_key = dom_key(child_field.name, child_field.rename);

                        let child_info = FieldInfo {
                            idx: child_idx,
                            field: child_field,
                            is_list,
                            is_array,
                            is_set,
                            is_tuple,
                            namespace,
                        };

                        let flattened_child = FlattenedChildInfo {
                            parent_idx: idx,
                            child_idx,
                            child_info,
                            parent_is_option,
                        };

                        // Determine if this is an attribute field or an element field
                        let is_attribute = child_field.is_attribute();

                        if is_attribute {
                            // Register as flattened attribute
                            flattened_attributes
                                .entry(child_key.clone().into_owned())
                                .or_default()
                                .push(flattened_child.clone());

                            // Also register alias if present
                            if let Some(alias) = child_field.alias {
                                flattened_attributes
                                    .entry(alias.to_string())
                                    .or_default()
                                    .push(flattened_child);
                            }
                        } else {
                            // Register as flattened element
                            flattened_children
                                .entry(child_key.clone().into_owned())
                                .or_default()
                                .push(flattened_child.clone());

                            // For list/set fields without explicit rename, also register singularized form
                            // (but not for tuples - they use the field name directly)
                            if (is_list || is_set) && !is_tuple && child_field.rename.is_none() {
                                let singular_key = singularize(&child_key);
                                if singular_key != *child_key {
                                    flattened_children
                                        .entry(singular_key)
                                        .or_default()
                                        .push(flattened_child.clone());
                                }
                            }

                            // Also register alias if present
                            if let Some(alias) = child_field.alias {
                                flattened_children
                                    .entry(alias.to_string())
                                    .or_default()
                                    .push(flattened_child);
                            }
                        }
                    }
                } else if is_flattened_map(field) {
                    // Flattened map - captures unknown elements AND attributes as key-value pairs
                    let _shape = field.shape();
                    let namespace: Option<&'static str> = field
                        .get_attr(Some("xml"), "ns")
                        .and_then(|attr| attr.get_as::<&str>().copied());

                    let info = FieldInfo {
                        idx,
                        field,
                        is_list: false,
                        is_array: false,
                        is_set: false,
                        is_tuple: false,
                        namespace,
                    };
                    // Add to both element and attribute capture lists
                    flattened_maps.push(info.clone());
                    flattened_attr_maps.push(info);
                }
                continue; // Don't register the flattened field itself as an element
            }

            // Check if this field is a list, array, set, or tuple type
            // Need to look through pointers (Arc<[T]>, Box<[T]>, etc.)
            let shape = field.shape();
            let (is_list, is_array, is_set, is_tuple) = classify_sequence_shape(shape);

            // Extract namespace from xml::ns attribute if present
            let namespace: Option<&'static str> = field
                .get_attr(Some("xml"), "ns")
                .and_then(|attr| attr.get_as::<&str>().copied());

            // For all fields (list or not):
            //   - element name uses rename if present, else lowerCamelCase(field.name)
            // For list fields, this is the repeated item element name (flat, no wrapper)
            let element_key = dom_key(field.name, field.rename);

            if field.is_attribute() {
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    is_tuple,
                    namespace,
                };
                // Check if this is a catch-all for attribute values (Vec/Set without rename)
                if (is_list || is_set) && field.rename.is_none() {
                    attributes_field = Some(info);
                } else {
                    // Named attribute: uses rename or lowerCamelCase(field.name)
                    let attr_key = dom_key(field.name, field.rename);
                    attribute_fields
                        .entry(attr_key.into_owned())
                        .or_default()
                        .push(info.clone());

                    // Also register alias if present (aliases are used as-is, no conversion)
                    if let Some(alias) = field.alias {
                        attribute_fields
                            .entry(alias.to_string())
                            .or_default()
                            .push(info);
                    }
                }
            } else if field.is_elements() && field.rename.is_none() {
                // xml::elements without rename = catch-all for unmatched children
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    is_tuple,
                    namespace,
                };
                elements_field = Some(info);
            } else if field.is_text() {
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    is_tuple,
                    namespace,
                };
                text_field = Some(info);
            } else if field.is_tag() {
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    is_tuple,
                    namespace,
                };
                tag_field = Some(info);
            } else {
                // Check if this field is marked as "other" - if so, register it as the fallback
                // for tag mismatches, but ALSO register it as a normal element field so it
                // can match when the tag name is correct (e.g., <body> matches body field)
                if field.is_other() {
                    let info = FieldInfo {
                        idx,
                        field,
                        is_list,
                        is_array,
                        is_set,
                        is_tuple,
                        namespace,
                    };
                    other_field = Some(info);
                }
                // FALL THROUGH to register as element field
                // Default: unmarked fields and explicit xml::element fields are child elements
                // Apply ns_all to elements without explicit namespace
                let effective_namespace = namespace.or(ns_all);
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    is_tuple,
                    namespace: effective_namespace,
                };
                element_fields
                    .entry(element_key.clone().into_owned())
                    .or_default()
                    .push(info.clone());

                // For list/set fields without explicit rename, also register the singularized form
                // e.g., field "tracks" (Vec<T>) also matches element <track>
                // (but not for tuples - they use the field name directly)
                if (is_list || is_set) && !is_tuple && field.rename.is_none() {
                    let singular_key = singularize(&element_key);
                    // Only register if singularization actually changed the name
                    if singular_key != element_key {
                        element_fields
                            .entry(singular_key)
                            .or_default()
                            .push(info.clone());
                    }
                }

                // Also register alias if present (aliases are used as-is, no conversion)
                if let Some(alias) = field.alias {
                    element_fields
                        .entry(alias.to_string())
                        .or_default()
                        .push(info);
                }
            }
        }

        // For tuple structs, build positional field list
        let tuple_fields = if matches!(struct_def.kind, StructKind::TupleStruct | StructKind::Tuple)
        {
            let fields: Vec<FieldInfo> = struct_def
                .fields
                .iter()
                .enumerate()
                .map(|(idx, field)| {
                    let shape = field.shape();
                    let (is_list, is_array, is_set, is_tuple) = classify_sequence_shape(shape);
                    FieldInfo {
                        idx,
                        field,
                        is_list,
                        is_array,
                        is_set,
                        is_tuple,
                        namespace: None,
                    }
                })
                .collect();
            Some(fields)
        } else {
            None
        };

        Self {
            attribute_fields,
            element_fields,
            elements_field,
            attributes_field,
            text_field,
            tag_field,
            other_field,
            tuple_fields,
            flattened_children,
            flattened_attributes,
            flattened_enum,
            flattened_maps,
            flattened_attr_maps,
            nested_flattened_attr_maps,
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
        self.attribute_fields.get(name).and_then(|fields| {
            // First try to find an exact namespace match
            let exact_match = fields
                .iter()
                .find(|info| info.namespace.is_some() && info.namespace == namespace);
            if exact_match.is_some() {
                return exact_match;
            }
            // Fall back to a field with no namespace constraint
            fields.iter().find(|info| info.namespace.is_none())
        })
    }

    /// Find an element field by tag name and namespace.
    ///
    /// Returns `Some` if the name matches AND the namespace matches:
    /// - If the field has no namespace constraint, it matches any namespace
    /// - If the field has a namespace constraint, the incoming namespace must match exactly
    ///
    /// When multiple fields have the same name, prefers exact namespace match over wildcard.
    pub fn find_element(&self, tag: &str, namespace: Option<&str>) -> Option<&FieldInfo> {
        self.element_fields.get(tag).and_then(|fields| {
            // First try to find an exact namespace match
            let exact_match = fields
                .iter()
                .find(|info| info.namespace.is_some() && info.namespace == namespace);
            if exact_match.is_some() {
                return exact_match;
            }
            // Fall back to a field with no namespace constraint
            fields.iter().find(|info| info.namespace.is_none())
        })
    }

    /// Find a flattened child field by tag name and namespace.
    ///
    /// Returns `Some` if the name matches a child field from a flattened struct.
    pub fn find_flattened_child(
        &self,
        tag: &str,
        namespace: Option<&str>,
    ) -> Option<&FlattenedChildInfo> {
        self.flattened_children.get(tag).and_then(|children| {
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
        })
    }

    /// Find a flattened attribute field by name and namespace.
    ///
    /// Returns `Some` if the name matches an attribute field from a flattened struct.
    pub fn find_flattened_attribute(
        &self,
        name: &str,
        namespace: Option<&str>,
    ) -> Option<&FlattenedChildInfo> {
        self.flattened_attributes.get(name).and_then(|children| {
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
        })
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

    // Check for Vec<Enum> (List containing enum)
    if let Def::List(list_def) = &shape.def {
        let item_shape = list_def.t();
        if matches!(&item_shape.ty, Type::User(UserType::Enum(_))) {
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

/// Classify a shape as list, array, set, tuple, or neither. Returns (is_list, is_array, is_set, is_tuple).
/// Lists are Vec, slices. Arrays are [T; N]. Sets are HashSet, BTreeSet. Tuples are (T, U, V).
/// Looks through pointers.
fn classify_sequence_shape(shape: &facet_core::Shape) -> (bool, bool, bool, bool) {
    match &shape.def {
        Def::List(_) | Def::Slice(_) => (true, false, false, false),
        Def::Array(_) => (false, true, false, false),
        Def::Set(_) => (false, false, true, false),
        Def::Pointer(ptr_def) => {
            // Look through Arc<[T]>, Box<[T]>, Rc<[T]>, etc.
            ptr_def
                .pointee()
                .map(classify_sequence_shape)
                .unwrap_or((false, false, false, false))
        }
        _ => {
            // Check for tuple types
            if let Type::User(UserType::Struct(struct_def)) = &shape.ty
                && struct_def.kind == StructKind::Tuple
            {
                return (false, false, false, true);
            }
            (false, false, false, false)
        }
    }
}
