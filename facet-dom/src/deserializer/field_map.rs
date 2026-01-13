//! Precomputed field lookup for struct deserialization.

use std::collections::HashMap;

use facet_core::{Def, Field, StructKind, StructType};

use crate::tracing_macros::{trace, trace_span};

/// Info about a field in a struct for deserialization purposes.
#[derive(Clone)]
pub(crate) struct FieldInfo {
    pub idx: usize,
    #[allow(dead_code)]
    pub field: &'static Field,
    /// True if this field is a list type (Vec, etc.) - NOT an array
    pub is_list: bool,
    /// True if this field is a fixed-size array [T; N]
    pub is_array: bool,
    /// The namespace URI this field must match (from `xml::ns` attribute), if any.
    pub namespace: Option<&'static str>,
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
}

impl StructFieldMap {
    /// Build the field map from a struct definition.
    pub fn new(struct_def: &'static StructType) -> Self {
        trace_span!("StructFieldMap::new");

        let mut attribute_fields: HashMap<&'static str, Vec<FieldInfo>> = HashMap::new();
        let mut element_fields: HashMap<&'static str, Vec<FieldInfo>> = HashMap::new();
        let mut elements_field = None;
        let mut text_field = None;

        for (idx, field) in struct_def.fields.iter().enumerate() {
            // Check if this field is a list type or array
            // Need to look through pointers (Arc<[T]>, Box<[T]>, etc.)
            let shape = field.shape();
            let (is_list, is_array) = classify_sequence_shape(shape);

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
                    namespace,
                };
                text_field = Some(info);
            } else {
                // Default: unmarked fields and explicit xml::element fields are child elements
                trace!(idx, field_name = %field.name, field_rename = ?field.rename, key = %element_key, is_list, is_array, namespace = ?namespace, "found element field");
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    is_array,
                    namespace,
                };
                element_fields.entry(element_key).or_default().push(info);
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
                    let (is_list, is_array) = classify_sequence_shape(shape);
                    FieldInfo {
                        idx,
                        field,
                        is_list,
                        is_array,
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

/// Classify a shape as list, array, or neither. Returns (is_list, is_array).
/// Lists are Vec, slices. Arrays are [T; N]. Looks through pointers.
fn classify_sequence_shape(shape: &facet_core::Shape) -> (bool, bool) {
    match &shape.def {
        Def::List(_) | Def::Slice(_) => (true, false),
        Def::Array(_) => (false, true),
        Def::Pointer(ptr_def) => {
            // Look through Arc<[T]>, Box<[T]>, Rc<[T]>, etc.
            ptr_def
                .pointee()
                .map(classify_sequence_shape)
                .unwrap_or((false, false))
        }
        _ => (false, false),
    }
}
