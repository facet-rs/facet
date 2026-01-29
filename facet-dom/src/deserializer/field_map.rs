//! Precomputed field lookup for struct deserialization.

use std::borrow::Cow;
use std::collections::HashMap;

use facet_core::{Def, Field, StructKind, StructType, Type, UserType};

use crate::naming::{apply_rename_all, dom_key};
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
    /// Whether the parent field is an `Option<Struct>` (requires begin_some())
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
    /// Whether the parent field is an `Option<Struct>`
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
    /// Fields marked with `xml::elements` or `html::elements`, keyed by expected element name.
    /// Each field collects child elements matching its singularized name (or rename).
    pub elements_fields: HashMap<String, FieldInfo>,
    /// The field marked with `xml::attribute` as a catch-all (collects all unmatched attribute values)
    pub attributes_field: Option<FieldInfo>,
    /// The field marked with `xml::text` (collects text content)
    pub text_field: Option<FieldInfo>,
    /// The field marked with `xml::tag` or `html::tag` (captures element tag name)
    pub tag_field: Option<FieldInfo>,
    /// The field marked with `xml::doctype` (captures DOCTYPE declaration)
    pub doctype_field: Option<FieldInfo>,
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
    /// Catch-all elements field - matches any tag name (for item types with xml::tag field)
    pub catch_all_elements_field: Option<FieldInfo>,
}

/// Compute the effective DOM key for a field, considering `rename_all` from the parent type.
///
/// Priority:
/// 1. Explicit field rename (field.rename) - use as-is
/// 2. Parent type's rename_all - apply transformation to field.name
/// 3. Default lowerCamelCase conversion via dom_key
fn field_dom_key<'a>(
    field_name: &'a str,
    field_rename: Option<&'a str>,
    rename_all: Option<&str>,
) -> Cow<'a, str> {
    if let Some(rename) = field_rename {
        // Explicit rename takes precedence
        Cow::Borrowed(rename)
    } else if let Some(rename_all) = rename_all {
        // Apply rename_all transformation
        Cow::Owned(apply_rename_all(field_name, rename_all))
    } else {
        // Default: lowerCamelCase
        dom_key(field_name, None)
    }
}

impl StructFieldMap {
    /// Build the field map from a struct definition.
    ///
    /// The `ns_all` parameter is the default namespace for element fields that don't
    /// have an explicit `xml::ns` attribute. When set, fields without `xml::ns` will
    /// inherit this namespace.
    ///
    /// The `rename_all` parameter, when set, applies a naming transformation to all
    /// fields that don't have explicit renames. This is used to propagate `rename_all`
    /// from parent enums to their struct variant fields.
    ///
    /// The `format_ns` parameter is the format namespace (e.g., "xml") used to resolve
    /// format-specific proxies on item types.
    pub fn new(
        struct_def: &'static StructType,
        ns_all: Option<&'static str>,
        rename_all: Option<&'static str>,
        format_ns: Option<&'static str>,
    ) -> Self {
        let mut attribute_fields: HashMap<String, Vec<FieldInfo>> = HashMap::new();
        let mut element_fields: HashMap<String, Vec<FieldInfo>> = HashMap::new();
        let mut elements_fields: HashMap<String, FieldInfo> = HashMap::new();
        let mut attributes_field = None;
        let mut text_field = None;
        let mut tag_field = None;
        let mut doctype_field = None;
        let mut other_field = None;
        let mut flattened_children: HashMap<String, Vec<FlattenedChildInfo>> = HashMap::new();
        let mut flattened_attributes: HashMap<String, Vec<FlattenedChildInfo>> = HashMap::new();
        let mut flattened_enum: Option<FlattenedEnumInfo> = None;
        let mut flattened_maps: Vec<FieldInfo> = Vec::new();
        let mut flattened_attr_maps: Vec<FieldInfo> = Vec::new();
        let mut nested_flattened_attr_maps: Vec<NestedFlattenedMapInfo> = Vec::new();
        let mut has_flatten = false;
        let mut catch_all_elements_field: Option<FieldInfo> = None;

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
            //   - element name uses rename if present, else rename_all transformation, else lowerCamelCase
            // For list fields, this is the repeated item element name (flat, no wrapper)
            let element_key = field_dom_key(field.name, field.rename, rename_all);

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
                    // Named attribute: uses rename > rename_all > lowerCamelCase
                    let attr_key = field_dom_key(field.name, field.rename, rename_all);
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
            } else if field.is_elements() {
                // xml::elements or html::elements - collect child elements by name
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    is_tuple,
                    namespace,
                };
                // Key priority:
                // 1. Item type has xml::tag field - catch-all (matches any element)
                // 2. Explicit field rename - single key
                // 3. Item type is enum OR has a proxy that is an enum - register each variant name
                // 4. Item type's rename (from #[facet(rename = "...")] on the item type)
                // 5. Singularized field name
                if item_type_has_tag_field(shape) {
                    // Item type has xml::tag field - this is a catch-all that matches any element
                    catch_all_elements_field = Some(info);
                } else if let Some(rename) = field.rename {
                    // Explicit field rename - single key
                    elements_fields.insert(rename.to_string(), info);
                } else if let Some(enum_def) =
                    get_item_type_enum(shape).or_else(|| get_item_type_proxy_enum(shape, format_ns))
                {
                    // Item type is an enum (or has a proxy that is an enum) - register each variant name
                    // Match the same logic as deserialize_enum: rename.is_some() uses
                    // effective_name(), otherwise apply to_element_name() for lowerCamelCase
                    for variant in enum_def.variants.iter() {
                        let variant_key: Cow<'_, str> = if variant.rename.is_some() {
                            Cow::Borrowed(variant.effective_name())
                        } else {
                            dom_key(variant.name, None)
                        };
                        elements_fields.insert(variant_key.into_owned(), info.clone());
                    }
                } else if let Some(item_rename) = get_item_type_rename(shape) {
                    // Item type has a rename attribute
                    elements_fields.insert(item_rename.to_string(), info);
                } else if let Some(item_element_name) = get_item_type_default_element_name(shape) {
                    // Use item type's name as element name (e.g., Vec<SomeInteger> matches <someInteger>)
                    elements_fields.insert(item_element_name, info);
                } else {
                    // Fallback to singularized field name (with rename_all if present)
                    let element_key =
                        singularize(&field_dom_key(field.name, None, rename_all));
                    elements_fields.insert(element_key, info);
                };
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
            } else if field.is_doctype() {
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    is_set,
                    is_tuple,
                    namespace,
                };
                doctype_field = Some(info);
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
            elements_fields,
            attributes_field,
            text_field,
            tag_field,
            doctype_field,
            other_field,
            tuple_fields,
            flattened_children,
            flattened_attributes,
            flattened_enum,
            flattened_maps,
            flattened_attr_maps,
            nested_flattened_attr_maps,
            has_flatten,
            catch_all_elements_field,
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
/// Handles direct structs and `Option<Struct>`.
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

/// Get the item shape for a collection field.
/// Returns the inner element type for Vec, Set, Slice, Array, and smart pointers to these.
fn get_item_shape(shape: &facet_core::Shape) -> Option<&'static facet_core::Shape> {
    match &shape.def {
        Def::List(list_def) => Some(list_def.t()),
        Def::Set(set_def) => Some(set_def.t()),
        Def::Slice(slice_def) => Some(slice_def.t()),
        Def::Array(array_def) => Some(array_def.t()),
        Def::Pointer(ptr_def) => {
            // Look through smart pointers like Arc<[T]>
            ptr_def.pointee().and_then(|inner| match &inner.def {
                Def::List(list_def) => Some(list_def.t()),
                Def::Set(set_def) => Some(set_def.t()),
                Def::Slice(slice_def) => Some(slice_def.t()),
                _ => None,
            })
        }
        _ => None,
    }
}

/// Get the item type's enum definition for a collection field.
/// For `Vec<MyEnum>`, returns `Some(&EnumType)`.
/// Returns `None` if the field is not a collection or the item type is not an enum.
fn get_item_type_enum(shape: &facet_core::Shape) -> Option<&'static facet_core::EnumType> {
    let item_shape = get_item_shape(shape)?;

    // Check if the item type is an enum
    match &item_shape.ty {
        Type::User(UserType::Enum(enum_def)) => Some(enum_def),
        _ => None,
    }
}

/// Get the item type's proxy enum definition for a collection field.
/// For `Vec<Type>` where `Type` has `#[facet(xml::proxy = TypeProxy)]` and `TypeProxy` is an enum,
/// returns `Some(&EnumType)`.
/// Returns `None` if the field is not a collection, or the item type has no xml::proxy,
/// or the proxy is not an enum.
fn get_item_type_proxy_enum(
    shape: &facet_core::Shape,
    format_ns: Option<&'static str>,
) -> Option<&'static facet_core::EnumType> {
    let item_shape = get_item_shape(shape)?;

    // Check if the item type has a proxy
    let proxy_def = item_shape.effective_proxy(format_ns)?;
    let proxy_shape = proxy_def.shape;

    // Check if the proxy type is an enum
    match &proxy_shape.ty {
        Type::User(UserType::Enum(enum_def)) => Some(enum_def),
        _ => None,
    }
}

/// Get the item type's rename attribute for a collection field.
/// For `Vec<Container>` where `Container` has `#[facet(rename = "Object")]`, returns `Some("Object")`.
/// Returns `None` if the field is not a collection or the item type has no rename.
pub(crate) fn get_item_type_rename(shape: &facet_core::Shape) -> Option<&'static str> {
    // Get the item shape for collections
    let item_shape = match &shape.def {
        Def::List(list_def) => Some(list_def.t()),
        Def::Set(set_def) => Some(set_def.t()),
        Def::Slice(slice_def) => Some(slice_def.t()),
        Def::Array(array_def) => Some(array_def.t()),
        Def::Pointer(ptr_def) => {
            // Look through smart pointers like Arc<[T]>
            ptr_def.pointee().and_then(|inner| match &inner.def {
                Def::List(list_def) => Some(list_def.t()),
                Def::Set(set_def) => Some(set_def.t()),
                Def::Slice(slice_def) => Some(slice_def.t()),
                _ => None,
            })
        }
        _ => None,
    }?;

    // Check if the item type has a rename attribute
    item_shape.get_builtin_attr_value::<&str>("rename")
}

/// Get the default element name for a collection's item type.
///
/// For `Vec<SomeInteger>`, this returns `"someInteger"` (the type name in lowerCamelCase).
/// This is used when no explicit rename is specified on either the field or the item type.
pub(crate) fn get_item_type_default_element_name(shape: &facet_core::Shape) -> Option<String> {
    // Get the item shape for collections
    let item_shape = match &shape.def {
        Def::List(list_def) => Some(list_def.t()),
        Def::Set(set_def) => Some(set_def.t()),
        Def::Slice(slice_def) => Some(slice_def.t()),
        Def::Array(array_def) => Some(array_def.t()),
        Def::Pointer(ptr_def) => {
            // Look through smart pointers like Arc<[T]>
            ptr_def.pointee().and_then(|inner| match &inner.def {
                Def::List(list_def) => Some(list_def.t()),
                Def::Set(set_def) => Some(set_def.t()),
                Def::Slice(slice_def) => Some(slice_def.t()),
                _ => None,
            })
        }
        _ => None,
    }?;

    // Use the item type's type_identifier, converted to element name format
    Some(crate::naming::to_element_name(item_shape.type_identifier).into_owned())
}

/// Check if the item type of a collection has an `xml::tag` or `html::tag` field.
/// This indicates the type can capture any element tag name (catch-all).
/// For `Vec<Element>` where `Element` has `#[facet(xml::tag)]`, returns `true`.
fn item_type_has_tag_field(shape: &facet_core::Shape) -> bool {
    // Get the item shape for collections
    let item_shape = match &shape.def {
        Def::List(list_def) => Some(list_def.t()),
        Def::Set(set_def) => Some(set_def.t()),
        Def::Slice(slice_def) => Some(slice_def.t()),
        Def::Array(array_def) => Some(array_def.t()),
        Def::Pointer(ptr_def) => {
            // Look through smart pointers like Arc<[T]>
            ptr_def.pointee().and_then(|inner| match &inner.def {
                Def::List(list_def) => Some(list_def.t()),
                Def::Set(set_def) => Some(set_def.t()),
                Def::Slice(slice_def) => Some(slice_def.t()),
                _ => None,
            })
        }
        _ => None,
    };

    let Some(item_shape) = item_shape else {
        return false;
    };

    // Check if the item type is a struct with an xml::tag or html::tag field
    if let Type::User(UserType::Struct(struct_def)) = &item_shape.ty {
        for field in struct_def.fields.iter() {
            if field.is_tag() {
                return true;
            }
        }
    }

    false
}
