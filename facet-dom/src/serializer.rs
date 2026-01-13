//! Tree-based serializer for DOM documents.
//!
//! This module provides a serializer trait and shared logic for serializing
//! facet types to tree-based formats like XML and HTML.

extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Debug;

use facet_core::{Def, StructKind};
use facet_reflect::{HasFields as _, Peek, ReflectError};

use crate::tracing_macros::trace;

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
    fn attribute(
        &mut self,
        name: &str,
        value: &str,
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

    /// Clear field-related state after a field is serialized.
    fn clear_field_state(&mut self) {}

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
    if let Some(s) = value_to_string(value) {
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
    if let Def::List(_) | Def::Array(_) | Def::Slice(_) = value.shape().def {
        let list = value.into_list_like().map_err(DomSerializeError::Reflect)?;

        // If we have an element name, wrap the list in it
        if let Some(tag) = element_name
            && !serializer.is_elements_field()
        {
            serializer
                .element_start(tag, None)
                .map_err(DomSerializeError::Backend)?;
            serializer
                .children_start()
                .map_err(DomSerializeError::Backend)?;
        }

        for item in list.iter() {
            // Each item gets wrapped in an <item> element (or type name for xml::elements)
            serialize_value(serializer, item, Some("item"))?;
        }

        if let Some(tag) = element_name
            && !serializer.is_elements_field()
        {
            serializer
                .children_end()
                .map_err(DomSerializeError::Backend)?;
            serializer
                .element_end(tag)
                .map_err(DomSerializeError::Backend)?;
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
    if let Ok(set) = value.into_set() {
        if let Some(tag) = element_name {
            serializer
                .element_start(tag, None)
                .map_err(DomSerializeError::Backend)?;
            serializer
                .children_start()
                .map_err(DomSerializeError::Backend)?;
        }

        for item in set.iter() {
            serialize_value(serializer, item, Some("item"))?;
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

    // Handle structs
    if let Ok(struct_) = value.into_struct() {
        let kind = struct_.ty().kind;

        // For tuples, serialize as a sequence
        if kind == StructKind::Tuple || kind == StructKind::TupleStruct {
            if let Some(tag) = element_name {
                serializer
                    .element_start(tag, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;
            }

            for (_field_item, field_value) in struct_.fields_for_serialize() {
                serialize_value(serializer, field_value, Some("item"))?;
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

        // Regular struct
        trace!(type_id = %value.shape().type_identifier, "serializing struct");
        serializer
            .struct_metadata(value.shape())
            .map_err(DomSerializeError::Backend)?;

        // Determine element name from shape or provided name
        let tag = element_name.map(Cow::Borrowed).unwrap_or_else(|| {
            Cow::Borrowed(
                value
                    .shape()
                    .get_builtin_attr_value::<&str>("rename")
                    .unwrap_or(value.shape().type_identifier),
            )
        });
        trace!(tag = %tag, "element_start");

        serializer
            .element_start(&tag, None)
            .map_err(DomSerializeError::Backend)?;

        // Collect fields, separate attributes from children
        let fields: Vec<_> = struct_.fields_for_serialize().collect();
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
                let string_value = value_to_string(*field_value);
                trace!(field_name = %field_item.name, value = ?string_value, "attribute field");
                if let Some(s) = string_value {
                    serializer
                        .attribute(&field_item.name, &s, None)
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

            if serializer.is_text_field() {
                if let Some(s) = value_to_string(*field_value) {
                    serializer.text(&s).map_err(DomSerializeError::Backend)?;
                }
                serializer.clear_field_state();
                continue;
            }

            // Check for field-level proxy
            if let Some(field) = field_item.field {
                if field.proxy().is_some() {
                    // Use custom_serialization for field-level proxy
                    match field_value.custom_serialization(field) {
                        Ok(proxy_peek) => {
                            serialize_value(
                                serializer,
                                proxy_peek.as_peek(),
                                Some(&field_item.name),
                            )?;
                        }
                        Err(e) => {
                            return Err(DomSerializeError::Reflect(e));
                        }
                    }
                } else {
                    serialize_value(serializer, *field_value, Some(&field_item.name))?;
                }
            } else {
                serialize_value(serializer, *field_value, Some(&field_item.name))?;
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
            let variant_name = variant
                .get_builtin_attr("rename")
                .and_then(|a| a.get_as::<&str>().copied())
                .unwrap_or(variant.name);

            if untagged {
                serializer
                    .text(variant_name)
                    .map_err(DomSerializeError::Backend)?;
            } else if let Some(tag) = element_name {
                serializer
                    .element_start(tag, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .text(variant_name)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_end()
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .element_end(tag)
                    .map_err(DomSerializeError::Backend)?;
            } else {
                serializer
                    .text(variant_name)
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

            if untagged {
                return serialize_value(serializer, inner, element_name);
            }

            let variant_name = variant
                .get_builtin_attr("rename")
                .and_then(|a| a.get_as::<&str>().copied())
                .unwrap_or(variant.name);

            // Externally tagged: <Variant>inner</Variant>
            if let Some(outer_tag) = element_name {
                serializer
                    .element_start(outer_tag, None)
                    .map_err(DomSerializeError::Backend)?;
                serializer
                    .children_start()
                    .map_err(DomSerializeError::Backend)?;
            }

            serialize_value(serializer, inner, Some(variant_name))?;

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
        let variant_name = variant
            .get_builtin_attr("rename")
            .and_then(|a| a.get_as::<&str>().copied())
            .unwrap_or(variant.name);

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
                    .text(variant_name)
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
                    .text(variant_name)
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
                        .element_start(variant_name, None)
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
                        .element_end(variant_name)
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
fn value_to_string(value: Peek<'_, '_>) -> Option<String> {
    use facet_core::ScalarType;

    // Handle Option<T> by unwrapping if Some, returning None if None
    if let Def::Option(_) = &value.shape().def {
        if let Ok(opt) = value.into_option() {
            return match opt.value() {
                Some(inner) => value_to_string(inner),
                None => None,
            };
        }
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
            ScalarType::F32 => value.get::<f32>().ok()?.to_string(),
            ScalarType::F64 => value.get::<f64>().ok()?.to_string(),
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
