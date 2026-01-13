//! Precomputed field lookup for struct deserialization.

use std::collections::HashMap;

use facet_core::{Def, Field, StructType};

use crate::tracing_macros::{trace, trace_span};

/// Info about a field in a struct for deserialization purposes.
#[derive(Clone)]
pub(crate) struct FieldInfo {
    pub idx: usize,
    #[allow(dead_code)]
    pub field: &'static Field,
    /// True if this field is a list type (Vec, etc.)
    pub is_list: bool,
    /// For list fields, the element name for each item (from rename, or "item" default)
    pub item_element_name: Option<String>,
}

/// Precomputed field lookup map for a struct.
///
/// This separates "what fields does this struct have" from the parsing loop,
/// making the code cleaner and avoiding repeated linear scans.
pub(crate) struct StructFieldMap {
    /// Fields marked with `xml::attribute`, keyed by name/rename
    attribute_fields: HashMap<String, FieldInfo>,
    /// Fields that are child elements, keyed by name/rename
    element_fields: HashMap<String, FieldInfo>,
    /// The field marked with `xml::elements` (collects all unmatched children)
    pub elements_field: Option<FieldInfo>,
    /// The field marked with `xml::text` (collects text content)
    pub text_field: Option<FieldInfo>,
}

impl StructFieldMap {
    /// Build the field map from a struct definition.
    pub fn new(struct_def: &'static StructType) -> Self {
        trace_span!("StructFieldMap::new");

        let mut attribute_fields = HashMap::new();
        let mut element_fields = HashMap::new();
        let mut elements_field = None;
        let mut text_field = None;

        for (idx, field) in struct_def.fields.iter().enumerate() {
            // Check if this field is a list type
            let shape = field.shape();
            let is_list = matches!(&shape.def, Def::List(_));

            // For list fields:
            //   - wrapper element uses field name
            //   - item elements use rename (or "item" default)
            // For non-list fields:
            //   - element uses rename if present, else field name
            let (element_key, item_element_name) = if is_list {
                // List field: wrapper is field name, items are rename or "item"
                let wrapper_name = field.name.to_string();
                let item_name = field
                    .rename
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "item".to_string());
                (wrapper_name, Some(item_name))
            } else {
                // Non-list field: use rename if present, else field name
                let name = field.rename.unwrap_or(field.name).to_string();
                (name, None)
            };

            if field.is_attribute() {
                // Attributes always use rename or field name directly
                let attr_key = field.rename.unwrap_or(field.name).to_string();
                trace!(idx, field_name = %field.name, key = %attr_key, "found attribute field");
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    item_element_name,
                };
                attribute_fields.insert(attr_key, info);
            } else if field.is_elements() {
                trace!(idx, field_name = %field.name, "found elements collection field");
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    item_element_name,
                };
                elements_field = Some(info);
            } else if field.is_text() {
                trace!(idx, field_name = %field.name, "found text field");
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    item_element_name,
                };
                text_field = Some(info);
            } else {
                // Default: unmarked fields and explicit xml::element fields are child elements
                trace!(idx, field_name = %field.name, field_rename = ?field.rename, key = %element_key, is_list, item_element_name = ?item_element_name, "found element field");
                let info = FieldInfo {
                    idx,
                    field,
                    is_list,
                    item_element_name,
                };
                element_fields.insert(element_key, info);
            }
        }

        trace!(
            attribute_count = attribute_fields.len(),
            element_count = element_fields.len(),
            has_elements = elements_field.is_some(),
            has_text = text_field.is_some(),
            "field map built"
        );

        Self {
            attribute_fields,
            element_fields,
            elements_field,
            text_field,
        }
    }

    /// Find an attribute field by name (exact match).
    pub fn find_attribute(&self, name: &str) -> Option<&FieldInfo> {
        let result = self.attribute_fields.get(name);
        trace!(name, found = result.is_some(), "find_attribute");
        result
    }

    /// Find an element field by tag name (exact match).
    pub fn find_element(&self, tag: &str) -> Option<&FieldInfo> {
        let result = self.element_fields.get(tag);
        trace!(tag, found = result.is_some(), "find_element");
        result
    }
}
