//! Tree-based serializer for DOM documents.
//!
//! This module provides a serializer trait and shared logic for serializing
//! facet types to tree-based formats like XML and HTML.

mod write_scalar;

pub use write_scalar::{ScalarBuffer, WriteScalar};

extern crate alloc;

use std::io::Write;

/// A function that formats a floating-point number to a writer.
///
/// This is used to customize how `f32` and `f64` values are serialized.
/// The function receives the value (as `f64`, with `f32` values upcast) and
/// a writer to write the formatted output to.
pub type FloatFormatter = fn(f64, &mut dyn Write) -> std::io::Result<()>;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Debug;

use facet_core::{Def, StructKind};
use facet_reflect::{HasFields as _, Peek, ReflectError};

use crate::naming::to_element_name;
use crate::trace;

/// Low-level serializer interface for DOM-based formats (XML, HTML).
///
/// This trait provides callbacks for tree structure events. The shared
/// serializer logic walks facet types and calls these methods.
pub trait DomSerializer {
    /// Format-specific error type.
    type Error: Debug;

    /// Begin an element with the given tag name.
    ///
    /// Followed by zero or more `attribute` calls, then `children_start`.
    fn element_start(&mut self, tag: &str, namespace: Option<&str>) -> Result<(), Self::Error>;

    /// Emit an attribute on the current element.
    ///
    /// Only valid between `element_start` and `children_start`.
    /// The value is passed as a `Peek` so the serializer can format it directly
    /// without intermediate allocations.
    fn attribute(
        &mut self,
        name: &str,
        value: Peek<'_, '_>,
        namespace: Option<&str>,
    ) -> Result<(), Self::Error>;

    /// Start the children section of the current element.
    fn children_start(&mut self) -> Result<(), Self::Error>;

    /// End the children section.
    fn children_end(&mut self) -> Result<(), Self::Error>;

    /// End the current element.
    fn element_end(&mut self, tag: &str) -> Result<(), Self::Error>;

    /// Emit text content.
    fn text(&mut self, content: &str) -> Result<(), Self::Error>;

    /// Emit a comment (usually for debugging or special content).
    fn comment(&mut self, _content: &str) -> Result<(), Self::Error> {
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Metadata hooks
    // ─────────────────────────────────────────────────────────────────────────

    /// Provide struct/container metadata before serializing.
    ///
    /// This allows extracting container-level attributes like xml::ns_all.
    fn struct_metadata(&mut self, _shape: &facet_core::Shape) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Provide field metadata before serializing a field.
    ///
    /// This allows extracting field-level attributes like xml::attribute,
    /// xml::text, xml::ns, etc.
    fn field_metadata(&mut self, _field: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Provide variant metadata before serializing an enum variant.
    fn variant_metadata(
        &mut self,
        _variant: &'static facet_core::Variant,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Field type hints
    // ─────────────────────────────────────────────────────────────────────────

    /// Check if the current field should be serialized as an attribute.
    fn is_attribute_field(&self) -> bool {
        false
    }

    /// Check if the current field should be serialized as text content.
    fn is_text_field(&self) -> bool {
        false
    }

    /// Check if the current field is an "elements" list (no wrapper element).
    fn is_elements_field(&self) -> bool {
        false
    }

    /// Check if the current field is a "tag" field (stores the element's tag name).
    fn is_tag_field(&self) -> bool {
        false
    }

    /// Clear field-related state after a field is serialized.
    fn clear_field_state(&mut self) {}

    // ─────────────────────────────────────────────────────────────────────────
    // Value formatting
    // ─────────────────────────────────────────────────────────────────────────

    /// Format a floating-point value as a string.
    ///
    /// Override this to provide custom float formatting (e.g., fixed decimal places).
    /// The default implementation uses `Display`. The value is passed as f64
    /// (f32 values are upcast).
    fn format_float(&self, value: f64) -> String {
        value.to_string()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Option handling
    // ─────────────────────────────────────────────────────────────────────────

    /// Called when serializing `None`. DOM formats typically skip the field entirely.
    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Error produced by the DOM serializer.
#[derive(Debug)]
pub enum DomSerializeError<E: Debug> {
    /// Format backend error.
    Backend(E),
    /// Reflection failed while traversing the value.
    Reflect(ReflectError),
    /// Value can't be represented by the DOM serializer.
    Unsupported(Cow<'static, str>),
}

impl<E: Debug> core::fmt::Display for DomSerializeError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DomSerializeError::Backend(_) => f.write_str("DOM serializer error"),
            DomSerializeError::Reflect(err) => write!(f, "{err}"),
            DomSerializeError::Unsupported(msg) => f.write_str(msg.as_ref()),
        }
    }
}

impl<E: Debug + 'static> std::error::Error for DomSerializeError<E> {}

/// Serialize a value using the DOM serializer.
pub fn serialize<S>(
    serializer: &mut S,
    value: Peek<'_, '_>,
) -> Result<(), DomSerializeError<S::Error>>
where
    S: DomSerializer,
{
    serialize_value(serializer, value, None)
}

/// Internal: serialize a value, optionally with an element name.
fn serialize_value<S>(
    serializer: &mut S,
    value: Peek<'_, '_>,
    element_name: Option<&str>,
) -> Result<(), DomSerializeError<S::Error>>
where
    S: DomSerializer,
{
    // Dereference smart pointers
    let value = deref_if_pointer(value);
    let value = value.innermost_peek();

    // Check for container-level proxy
    if value.shape().proxy.is_some() {
        return serialize_via_proxy(serializer, value, element_name);
    }

    // Handle scalars
    if let Some(s) = value_to_string(value, serializer) {
        if let Some(tag) = element_name {
            serializer
                .element_start(tag, None)
                .map_err(DomSerializeError::Backend)?;
            serializer
                .children_start()
                .map_err(DomSerializeError::Backend)?;
            serializer.text(&s).map_err(DomSerializeError::Backend)?;
            serializer
                .children_end()
                .map_err(DomSerializeError::Backend)?;
            serializer
                .element_end(tag)
                .map_err(DomSerializeError::Backend)?;
        } else {
            serializer.text(&s).map_err(DomSerializeError::Backend)?;
        }
        return Ok(());
    }

    // Handle Option<T>
    if let Ok(opt) = value.into_option() {
        return match opt.value() {
            Some(inner) => serialize_value(serializer, inner, element_name),
            None => serializer
                .serialize_none()
                .map_err(DomSerializeError::Backend),
        };
    }

    // Handle lists/arrays
    // Flat list model: each item uses the field's element name (no wrapper element)
    if let Def::List(_) | Def::Array(_) | Def::Slice(_) = value.shape().def {
        let list = value.into_list_like().map_err(DomSerializeError::Reflect)?;

        for item in list.iter() {
            // Use the field's element name for each item (flat list)
            serialize_value(serializer, item, element_name)?;
        }

        return Ok(());
    }

    // Handle maps
    if let Ok(map) = value.into_map() {
        if let Some(tag) = element_name {
            serializer
                .element_start(tag, None)
                .map_err(DomSerializeError::Backend)?;
            serializer
                .children_start()
                .map_err(DomSerializeError::Backend)?;
        }

        for (key, val) in map.iter() {
            let key_str = if let Some(s) = key.as_str() {
                Cow::Borrowed(s)
            } else {
                Cow::Owned(alloc::format!("{}", key))
            };
            serialize_value(serializer, val, Some(&key_str))?;
        }

        if let Some(tag) = element_name {
            serializer
                .children_end()
                .map_err(DomSerializeError::Backend)?;
            serializer
                .element_end(tag)
                .map_err(DomSerializeError::Backend)?;
        }

        return Ok(());
    }

    // Handle sets
    // Flat set model: each item uses the field's element name (no wrapper element)
    // Same as lists for consistency
    if let Ok(set) = value.into_set() {
        for item in set.iter() {
            // Use the field's element name for each item (flat set)
            serialize_value(serializer, item, element_name)?;
        }

        return Ok(());
    }

    // Handle structs
    if let Ok(struct_) = value.into_struct() {
        let kind = struct_.ty().kind;

        // For tuples, serialize as a flat sequence (like Vec)
        // Each tuple field becomes a sibling element with the same tag name
        if kind == StructKind::Tuple || kind == StructKind::TupleStruct {
            for (_field_item, field_value) in struct_.fields_for_serialize() {
                serialize_value(serializer, field_value, element_name)?;
            }
            return Ok(());
        }

        // Regular struct
        trace!(type_id = %value.shape().type_identifier, "serializing struct");
        serializer
            .struct_metadata(value.shape())
            .map_err(DomSerializeError::Backend)?;

        // Collect fields first to check for tag field
        let fields: Vec<_> = struct_.fields_for_serialize().collect();

        // Find the tag field if present (html::tag or xml::tag)
        let tag_field_value: Option<String> = {
            let mut result = None;
            for (field_item, field_value) in &fields {
                serializer
                    .field_metadata(field_item)
                    .map_err(DomSerializeError::Backend)?;
                if serializer.is_tag_field() {
                    // Extract the string value from the tag field
                    if let Some(s) = field_value.as_str() {
                        result = Some(s.to_string());
                    } else if let Some(s) = value_to_string(*field_value, serializer) {
                        result = Some(s);
                    }
                }
                serializer.clear_field_state();
            }
            result
        };

        // Determine element name: tag field value > provided name > shape rename > type identifier (lowerCamelCase)
        let tag: Cow<'_, str> = if let Some(ref tag_value) = tag_field_value {
            Cow::Owned(tag_value.clone())
        } else if let Some(name) = element_name {
            Cow::Borrowed(name)
        } else if let Some(rename) = value.shape().get_builtin_attr_value::<&str>("rename") {
            Cow::Borrowed(rename)
        } else {
            // No explicit name - apply lowerCamelCase to type identifier
            to_element_name(value.shape().type_identifier)
        };
        trace!(tag = %tag, "element_start");

        serializer
            .element_start(&tag, None)
            .map_err(DomSerializeError::Backend)?;

        // Fields were already collected above when checking for tag field
        trace!(field_count = fields.len(), "collected fields for serialize");

        // First pass: emit attributes
        for (field_item, field_value) in &fields {
            trace!(field_name = %field_item.name, "processing field for attributes");
            serializer
                .field_metadata(field_item)
                .map_err(DomSerializeError::Backend)?;

            let is_attr = serializer.is_attribute_field();
            trace!(field_name = %field_item.name, is_attribute = is_attr, "field_metadata result");

            if is_attr {
                trace!(field_name = %field_item.name, "attribute field");
                // Compute attribute name: rename > lowerCamelCase(field.name)
                // BUT for flattened map entries (field is None), use the key as-is
                let attr_name = if let Some(field) = field_item.field {
                    field
                        .rename
                        .map(Cow::Borrowed)
                        .unwrap_or_else(|| to_element_name(&field_item.name))
                } else {
                    // Flattened map entry - preserve the key exactly as stored
                    field_item.name.clone()
                };

                // Check for field-level proxy
                if let Some(field) = field_item.field {
                    if field.proxy().is_some() {
                        match field_value.custom_serialization(field) {
                            Ok(proxy_peek) => {
                                serializer
                                    .attribute(&attr_name, proxy_peek.as_peek(), None)
                                    .map_err(DomSerializeError::Backend)?;
                            }
                            Err(e) => {
                                return Err(DomSerializeError::Reflect(e));
                            }
                        }
                    } else {
                        serializer
                            .attribute(&attr_name, *field_value, None)
                            .map_err(DomSerializeError::Backend)?;
                    }
                } else {
                    serializer
                        .attribute(&attr_name, *field_value, None)
                        .map_err(DomSerializeError::Backend)?;
                }
                serializer.clear_field_state();
            }
        }

        trace!("children_start");
        serializer
            .children_start()
            .map_err(DomSerializeError::Backend)?;

        // Second pass: emit child elements and text
        for (field_item, field_value) in &fields {
            serializer
                .field_metadata(field_item)
                .map_err(DomSerializeError::Backend)?;

            if serializer.is_attribute_field() {
                serializer.clear_field_state();
                continue;
            }

            // Skip tag fields - the value was already used as the element name
            if serializer.is_tag_field() {
                serializer.clear_field_state();
                continue;
            }

            if serializer.is_text_field() {
                if let Some(s) = value_to_string(*field_value, serializer) {
                    serializer.text(&s).map_err(DomSerializeError::Backend)?;
                }
                serializer.clear_field_state();
                continue;
            }

            // For xml::elements, serialize items directly (they determine their own element names)
            // Exception: if the field has an explicit rename, use that name for each item
            let is_elements = serializer.is_elements_field();
            let explicit_rename = field_item.field.and_then(|f| f.rename);

            // For flattened fields (flatten on Vec<Enum>), the FieldsForSerializeIter
            // already yields each enum item as a separate field with the variant name.
            // We should use that name directly (set in field_item.name/rename).
            let is_flattened = field_item.flattened;

            // Check if this is a text variant from a flattened enum (html::text or xml::text)
            // Text variants should be serialized as raw text without element wrapping
            if field_item.is_text_variant {
                if let Some(s) = value_to_string(*field_value, serializer) {
                    serializer.text(&s).map_err(DomSerializeError::Backend)?;
                }
                serializer.clear_field_state();
                continue;
            }

            // Compute field element name: rename > lowerCamelCase(field.name)
            let field_element_name: Option<Cow<'_, str>> =
                if is_elements && explicit_rename.is_none() {
                    None // Items determine their own element names
                } else if is_flattened {
                    // Flattened field - use the variant's effective name
                    Some(to_element_name(field_item.effective_name()))
                } else if let Some(rename) = explicit_rename {
                    // Use the explicit rename value as-is
                    Some(Cow::Borrowed(rename))
                } else {
                    // Apply lowerCamelCase to field name
                    Some(to_element_name(&field_item.name))
                };

            // Check for field-level proxy
            if let Some(field) = field_item.field {
                if field.proxy().is_some() {
                    // Use custom_serialization for field-level proxy
                    match field_value.custom_serialization(field) {
                        Ok(proxy_peek) => {
                            serialize_value(
                                serializer,
                                proxy_peek.as_peek(),
                                field_element_name.as_deref(),
                            )?;
                        }
                        Err(e) => {
                            return Err(DomSerializeError::Reflect(e));
                        }
                    }
                } else {
                    serialize_value(serializer, *field_value, field_element_name.as_deref())?;
                }
            } else {
                serialize_value(serializer, *field_value, field_element_name.as_deref())?;
            }

            serializer.clear_field_state();
        }

        serializer
            .children_end()
            .map_err(DomSerializeError::Backend)?;
        serializer
            .element_end(&tag)
            .map_err(DomSerializeError::Backend)?;

        return Ok(());
    }

    // Handle enums
    if let Ok(enum_) = value.into_enum() {
        let variant = enum_.active_variant().map_err(|_| {
            DomSerializeError::Unsupported(Cow::Borrowed("opaque enum layout is unsupported"))
        })?;

        serializer
            .variant_metadata(variant)
            .map_err(DomSerializeError::Backend)?;

        let untagged = value.shape().is_untagged();
        let tag_attr = value.shape().get_tag_attr();
        let content_attr = value.shape().get_content_attr();

        // Unit variant
        if variant.data.kind == StructKind::Unit {
            let variant_name: Cow<'_, str> = variant
                .get_builtin_attr("rename")
                .and_then(|a| a.get_as::<&str>().copied())
                .map(Cow::Borrowed)
                .unwrap_or_else(|| to_element_name(variant.name));

            if untagged {
                serializer
                    .text(&variant_name)
                    .map_err(DomSerializeError::Backend)?;
            } else if let Some(tag) = element_name {
                serializer
                    .element_start(tag, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .text(&variant_name)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_end()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .element_end(tag)
                    .map_err(DomSerializeError::Backend)?;
            } else {
                serializer
                    .text(&variant_name)
                    .map_err(DomSerializeError::Backend)?;
            }
            return Ok(());
        }

        // Newtype variant (single unnamed field)
        if variant.data.kind == StructKind::TupleStruct && variant.data.fields.len() == 1 {
            let inner = enum_
                .fields_for_serialize()
                .next()
                .map(|(_, v)| v)
                .ok_or_else(|| {
                    DomSerializeError::Unsupported(Cow::Borrowed("newtype variant missing field"))
                })?;

            // Text variant (html::text or xml::text) - emit as plain text, no element wrapper
            if variant.is_text() {
                if let Some(s) = value_to_string(inner, serializer) {
                    serializer.text(&s).map_err(DomSerializeError::Backend)?;
                }
                return Ok(());
            }

            if untagged {
                return serialize_value(serializer, inner, element_name);
            }

            let variant_name: Cow<'_, str> = variant
                .get_builtin_attr("rename")
                .and_then(|a| a.get_as::<&str>().copied())
                .map(Cow::Borrowed)
                .unwrap_or_else(|| to_element_name(variant.name));

            // Externally tagged: <Variant>inner</Variant>
            if let Some(outer_tag) = element_name {
                serializer
                    .element_start(outer_tag, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;
            }

            serialize_value(serializer, inner, Some(&variant_name))?;

            if let Some(outer_tag) = element_name {
                serializer
                    .children_end()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .element_end(outer_tag)
                    .map_err(DomSerializeError::Backend)?;
            }

            return Ok(());
        }

        // Struct variant
        let variant_name: Cow<'_, str> = variant
            .get_builtin_attr("rename")
            .and_then(|a| a.get_as::<&str>().copied())
            .map(Cow::Borrowed)
            .unwrap_or_else(|| to_element_name(variant.name));

        match (tag_attr, content_attr) {
            // Internally tagged
            (Some(tag_key), None) => {
                let tag = element_name.unwrap_or("value");
                serializer
                    .element_start(tag, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;

                // Emit tag field
                serializer
                    .element_start(tag_key, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .text(&variant_name)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_end()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .element_end(tag_key)
                    .map_err(DomSerializeError::Backend)?;

                // Emit variant fields
                for (field_item, field_value) in enum_.fields_for_serialize() {
                    serialize_value(serializer, field_value, Some(&field_item.name))?;
                }

                serializer
                    .children_end()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .element_end(tag)
                    .map_err(DomSerializeError::Backend)?;
            }

            // Adjacently tagged
            (Some(tag_key), Some(content_key)) => {
                let tag = element_name.unwrap_or("value");
                serializer
                    .element_start(tag, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;

                // Emit tag
                serializer
                    .element_start(tag_key, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .text(&variant_name)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_end()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .element_end(tag_key)
                    .map_err(DomSerializeError::Backend)?;

                // Emit content
                serializer
                    .element_start(content_key, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;
                for (field_item, field_value) in enum_.fields_for_serialize() {
                    serialize_value(serializer, field_value, Some(&field_item.name))?;
                }
                serializer
                    .children_end()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .element_end(content_key)
                    .map_err(DomSerializeError::Backend)?;

                serializer
                    .children_end()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .element_end(tag)
                    .map_err(DomSerializeError::Backend)?;
            }

            // Externally tagged (default) or untagged
            _ => {
                if untagged {
                    // Serialize just the variant content
                    let tag = element_name.unwrap_or("value");
                    serializer
                        .element_start(tag, None)
                        .map_err(DomSerializeError::Backend)?;
                    serializer
                        .children_start()
                        .map_err(DomSerializeError::Backend)?;
                    for (field_item, field_value) in enum_.fields_for_serialize() {
                        serialize_value(serializer, field_value, Some(&field_item.name))?;
                    }
                    serializer
                        .children_end()
                        .map_err(DomSerializeError::Backend)?;
                    serializer
                        .element_end(tag)
                        .map_err(DomSerializeError::Backend)?;
                } else {
                    // Externally tagged: <outer><Variant>...</Variant></outer>
                    if let Some(outer_tag) = element_name {
                        serializer
                            .element_start(outer_tag, None)
                            .map_err(DomSerializeError::Backend)?;
                        serializer
                            .children_start()
                            .map_err(DomSerializeError::Backend)?;
                    }

                    serializer
                        .element_start(&variant_name, None)
                        .map_err(DomSerializeError::Backend)?;
                    serializer
                        .children_start()
                        .map_err(DomSerializeError::Backend)?;
                    for (field_item, field_value) in enum_.fields_for_serialize() {
                        serialize_value(serializer, field_value, Some(&field_item.name))?;
                    }
                    serializer
                        .children_end()
                        .map_err(DomSerializeError::Backend)?;
                    serializer
                        .element_end(&variant_name)
                        .map_err(DomSerializeError::Backend)?;

                    if let Some(outer_tag) = element_name {
                        serializer
                            .children_end()
                            .map_err(DomSerializeError::Backend)?;
                        serializer
                            .element_end(outer_tag)
                            .map_err(DomSerializeError::Backend)?;
                    }
                }
            }
        }

        return Ok(());
    }

    Err(DomSerializeError::Unsupported(Cow::Owned(alloc::format!(
        "unsupported type: {:?}",
        value.shape().def
    ))))
}

/// Serialize through a proxy type.
fn serialize_via_proxy<S>(
    serializer: &mut S,
    value: Peek<'_, '_>,
    element_name: Option<&str>,
) -> Result<(), DomSerializeError<S::Error>>
where
    S: DomSerializer,
{
    // Use the high-level API that handles allocation and conversion
    let owned_peek = value
        .custom_serialization_from_shape()
        .map_err(DomSerializeError::Reflect)?;

    match owned_peek {
        Some(proxy_peek) => {
            // proxy_peek is an OwnedPeek that will auto-deallocate on drop
            serialize_value(serializer, proxy_peek.as_peek(), element_name)
        }
        None => {
            // No proxy on shape - this shouldn't happen since we checked proxy exists
            Err(DomSerializeError::Unsupported(Cow::Borrowed(
                "proxy serialization failed: no proxy on shape",
            )))
        }
    }
}

/// Dereference smart pointers (Box, Arc, Rc) to get the inner value.
fn deref_if_pointer<'mem, 'facet>(value: Peek<'mem, 'facet>) -> Peek<'mem, 'facet> {
    if let Ok(ptr) = value.into_pointer()
        && let Some(inner) = ptr.borrow_inner()
    {
        return deref_if_pointer(inner);
    }
    value
}

/// Convert a value to a string if it's a scalar type.
fn value_to_string<S: DomSerializer>(value: Peek<'_, '_>, serializer: &S) -> Option<String> {
    use facet_core::ScalarType;

    // Handle Option<T> by unwrapping if Some, returning None if None
    if let Def::Option(_) = &value.shape().def
        && let Ok(opt) = value.into_option()
    {
        return match opt.value() {
            Some(inner) => value_to_string(inner, serializer),
            None => None,
        };
    }

    if let Some(scalar_type) = value.scalar_type() {
        let s = match scalar_type {
            ScalarType::Unit => return Some("null".into()),
            ScalarType::Bool => if *value.get::<bool>().ok()? {
                "true"
            } else {
                "false"
            }
            .into(),
            ScalarType::Char => value.get::<char>().ok()?.to_string(),
            ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
                value.as_str()?.to_string()
            }
            ScalarType::F32 => serializer.format_float(*value.get::<f32>().ok()? as f64),
            ScalarType::F64 => serializer.format_float(*value.get::<f64>().ok()?),
            ScalarType::U8 => value.get::<u8>().ok()?.to_string(),
            ScalarType::U16 => value.get::<u16>().ok()?.to_string(),
            ScalarType::U32 => value.get::<u32>().ok()?.to_string(),
            ScalarType::U64 => value.get::<u64>().ok()?.to_string(),
            ScalarType::U128 => value.get::<u128>().ok()?.to_string(),
            ScalarType::USize => value.get::<usize>().ok()?.to_string(),
            ScalarType::I8 => value.get::<i8>().ok()?.to_string(),
            ScalarType::I16 => value.get::<i16>().ok()?.to_string(),
            ScalarType::I32 => value.get::<i32>().ok()?.to_string(),
            ScalarType::I64 => value.get::<i64>().ok()?.to_string(),
            ScalarType::I128 => value.get::<i128>().ok()?.to_string(),
            ScalarType::ISize => value.get::<isize>().ok()?.to_string(),
            #[cfg(feature = "net")]
            ScalarType::IpAddr => value.get::<core::net::IpAddr>().ok()?.to_string(),
            #[cfg(feature = "net")]
            ScalarType::Ipv4Addr => value.get::<core::net::Ipv4Addr>().ok()?.to_string(),
            #[cfg(feature = "net")]
            ScalarType::Ipv6Addr => value.get::<core::net::Ipv6Addr>().ok()?.to_string(),
            #[cfg(feature = "net")]
            ScalarType::SocketAddr => value.get::<core::net::SocketAddr>().ok()?.to_string(),
            _ => return None,
        };
        return Some(s);
    }

    // Try Display for Def::Scalar types (SmolStr, etc.)
    if matches!(value.shape().def, Def::Scalar) && value.shape().vtable.has_display() {
        return Some(alloc::format!("{}", value));
    }

    None
}
