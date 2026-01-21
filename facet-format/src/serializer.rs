extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use core::fmt::Debug;
use core::fmt::Write as _;

use facet_core::{Def, DynDateTimeKind, DynValueKind, ScalarType, Shape, StructKind, Type};
use facet_reflect::{HasFields as _, Peek, ReflectError};

use crate::ScalarValue;

/// Field ordering preference for serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldOrdering {
    /// Fields are serialized in declaration order (default).
    #[default]
    Declaration,
}

/// How struct fields should be serialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StructFieldMode {
    /// Serialize fields with names/keys (default for text formats).
    #[default]
    Named,
    /// Serialize fields in declaration order without names (binary formats).
    Unnamed,
}

/// How map-like values should be serialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapEncoding {
    /// Serialize maps as objects/structs with string keys.
    #[default]
    Struct,
    /// Serialize maps as key/value pairs (binary formats).
    Pairs,
}

/// How enum variants should be serialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnumVariantEncoding {
    /// Serialize enums using tag/field-name strategies (default for text formats).
    #[default]
    Tagged,
    /// Serialize enums using a numeric variant index followed by fields (binary formats).
    Index,
}

/// How dynamic values (e.g. `facet_value::Value`) should be encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DynamicValueEncoding {
    /// Use the format's native self-describing encoding (default for JSON, MsgPack, etc.).
    #[default]
    SelfDescribing,
    /// Use an explicit type tag before the dynamic value payload (binary formats).
    Tagged,
}

/// Tag describing the concrete payload type for a dynamic value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicValueTag {
    /// Null value.
    Null,
    /// Boolean value.
    Bool,
    /// Signed 64-bit integer.
    I64,
    /// Unsigned 64-bit integer.
    U64,
    /// 64-bit float.
    F64,
    /// UTF-8 string.
    String,
    /// Raw bytes.
    Bytes,
    /// Sequence/array.
    Array,
    /// Object/map.
    Object,
    /// Date/time value (encoded as string for tagged formats).
    DateTime,
}

/// Low-level serializer interface implemented by each format backend.
///
/// This is intentionally event-ish: the shared serializer logic owns traversal
/// (struct/enum/seq decisions), while formats own representation details.
pub trait FormatSerializer {
    /// Format-specific error type.
    type Error: Debug;

    /// Begin a map/object/struct.
    fn begin_struct(&mut self) -> Result<(), Self::Error>;
    /// Emit a field key within a struct.
    fn field_key(&mut self, key: &str) -> Result<(), Self::Error>;
    /// Emit a rich field key with optional tag and documentation.
    ///
    /// This is called when serializing map keys that have been extracted from
    /// metadata containers (like `ObjectKey` with tag support).
    ///
    /// Default implementation ignores tag and doc, just emits the name.
    /// Formats that support tags (like Styx) should override this.
    fn emit_field_key(&mut self, key: &crate::FieldKey<'_>) -> Result<(), Self::Error> {
        // Default: ignore tag and doc, just emit the name (empty string if None)
        let name = key.name.as_deref().unwrap_or("");
        self.field_key(name)
    }
    /// End a map/object/struct.
    fn end_struct(&mut self) -> Result<(), Self::Error>;

    /// Begin a sequence/array.
    fn begin_seq(&mut self) -> Result<(), Self::Error>;
    /// End a sequence/array.
    fn end_seq(&mut self) -> Result<(), Self::Error>;

    /// Emit a scalar value.
    fn scalar(&mut self, scalar: ScalarValue<'_>) -> Result<(), Self::Error>;

    /// Optional: Provide field metadata before field_key is called.
    /// Default implementation does nothing.
    fn field_metadata(&mut self, _field: &facet_reflect::FieldItem) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Optional: Provide field metadata with access to the field value.
    ///
    /// This is called before `field_key` and allows formats to inspect the field value
    /// for metadata. This is particularly useful for metadata containers like `Documented<T>`
    /// where doc comments are stored in the value, not the field definition.
    ///
    /// If this returns `Ok(true)`, the field key has been written and `field_key` will be skipped.
    /// If this returns `Ok(false)`, normal field_key handling continues.
    ///
    /// Default implementation does nothing and returns `Ok(false)`.
    fn field_metadata_with_value(
        &mut self,
        _field: &facet_reflect::FieldItem,
        _value: Peek<'_, '_>,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }

    /// Optional: Provide struct/enum type metadata when beginning to serialize it.
    /// Default implementation does nothing.
    fn struct_metadata(&mut self, _shape: &facet_core::Shape) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Optional: Provide variant metadata before serializing an enum variant.
    /// Default implementation does nothing.
    fn variant_metadata(
        &mut self,
        _variant: &'static facet_core::Variant,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Serialize a metadata container value.
    ///
    /// Metadata containers (structs with `#[facet(metadata_container)]`) have exactly
    /// one non-metadata field (the actual value) and one or more metadata fields
    /// (like doc comments or source spans).
    ///
    /// Formats that support metadata can override this to emit metadata in the
    /// appropriate position. For example, Styx emits doc comments before the value:
    ///
    /// ```text
    /// /// The port to listen on
    /// port 8080
    /// ```
    ///
    /// The format is responsible for:
    /// 1. Extracting metadata fields (use `field.metadata_kind()` to identify them)
    /// 2. Emitting metadata in the appropriate position
    /// 3. Serializing the non-metadata field value
    ///
    /// Returns `Ok(true)` if handled, `Ok(false)` to fall back to default transparent
    /// serialization (which just serializes the non-metadata field).
    fn serialize_metadata_container(
        &mut self,
        _container: &facet_reflect::PeekStruct<'_, '_>,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }

    /// Preferred field ordering for this format.
    /// Default is declaration order.
    fn preferred_field_order(&self) -> FieldOrdering {
        FieldOrdering::Declaration
    }

    /// Preferred struct field mode for this format.
    fn struct_field_mode(&self) -> StructFieldMode {
        StructFieldMode::Named
    }

    /// Preferred map encoding for this format.
    fn map_encoding(&self) -> MapEncoding {
        MapEncoding::Struct
    }

    /// Preferred enum variant encoding for this format.
    fn enum_variant_encoding(&self) -> EnumVariantEncoding {
        EnumVariantEncoding::Tagged
    }

    /// Whether this format is self-describing (includes type information).
    ///
    /// Self-describing formats (JSON, YAML, TOML) can deserialize without hints
    /// and treat newtypes transparently. Non-self-describing formats (ASN.1,
    /// postcard, msgpack) require structural hints and wrap newtypes.
    ///
    /// Default is `true` for text-based formats.
    fn is_self_describing(&self) -> bool {
        true
    }

    /// Preferred dynamic value encoding for this format.
    fn dynamic_value_encoding(&self) -> DynamicValueEncoding {
        DynamicValueEncoding::SelfDescribing
    }

    /// Returns the shape of the format's raw capture type for serialization.
    ///
    /// When serializing a value whose shape matches this, the serializer will
    /// extract the inner string and call [`FormatSerializer::raw_scalar`] instead of normal
    /// serialization.
    fn raw_serialize_shape(&self) -> Option<&'static facet_core::Shape> {
        None
    }

    /// Emit a raw scalar value (for RawJson, etc.) without any encoding/escaping.
    ///
    /// The content is the format-specific raw representation that should be
    /// output directly.
    fn raw_scalar(&mut self, content: &str) -> Result<(), Self::Error> {
        // Default: treat as a regular string (formats should override this)
        self.scalar(ScalarValue::Str(Cow::Borrowed(content)))
    }

    /// Serialize an opaque scalar type with a format-specific encoding.
    ///
    /// Returns `Ok(true)` if handled, `Ok(false)` to fall back to standard logic.
    fn serialize_opaque_scalar(
        &mut self,
        _shape: &'static facet_core::Shape,
        _value: Peek<'_, '_>,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }

    /// Emit a dynamic value type tag.
    ///
    /// Formats that use [`DynamicValueEncoding::Tagged`] should override this.
    /// Self-describing formats can ignore it.
    fn dynamic_value_tag(&mut self, _tag: DynamicValueTag) -> Result<(), Self::Error> {
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Binary format support methods
    //
    // The following methods enable proper serialization for binary formats like
    // postcard that need length prefixes, type-precise encoding, and explicit
    // discriminants. All have default implementations for backward compatibility
    // with existing text-format serializers.
    // ─────────────────────────────────────────────────────────────────────────

    /// Begin a sequence with known length.
    ///
    /// Binary formats (postcard, msgpack) can use this to write a length prefix
    /// before the elements. Text formats can ignore the length and just call
    /// `begin_seq()`.
    ///
    /// Default: delegates to `begin_seq()`.
    fn begin_seq_with_len(&mut self, _len: usize) -> Result<(), Self::Error> {
        self.begin_seq()
    }

    /// Begin serializing a map with known length.
    ///
    /// Default: delegates to `begin_struct()` for formats that encode maps as objects.
    fn begin_map_with_len(&mut self, _len: usize) -> Result<(), Self::Error> {
        self.begin_struct()
    }

    /// End a map/object/struct.
    ///
    /// Default: delegates to `end_struct()`.
    fn end_map(&mut self) -> Result<(), Self::Error> {
        self.end_struct()
    }

    /// Serialize a map key in `MapEncoding::Struct` mode.
    ///
    /// This is called for each map key when using struct encoding. The default
    /// implementation converts the key to a string (via `as_str()` or `Display`)
    /// and calls `field_key()`.
    ///
    /// Formats can override this to handle special key types differently.
    /// For example, Styx overrides this to serialize `Option::None` as `@`.
    ///
    /// Returns `Ok(true)` if handled, `Ok(false)` to use the default behavior.
    fn serialize_map_key(&mut self, _key: Peek<'_, '_>) -> Result<bool, Self::Error> {
        Ok(false)
    }

    /// Serialize a scalar with full type information.
    ///
    /// Binary formats need to encode different integer sizes differently:
    /// - postcard: u8 as raw byte, u16+ as varint, signed use zigzag
    /// - msgpack: different tags for different sizes
    ///
    /// Text formats can ignore the type and use the normalized `ScalarValue`.
    ///
    /// Default: normalizes to `ScalarValue` and calls `scalar()`.
    fn typed_scalar(
        &mut self,
        scalar_type: ScalarType,
        value: Peek<'_, '_>,
    ) -> Result<(), Self::Error> {
        // Default implementation: normalize to ScalarValue and call scalar()
        let scalar = match scalar_type {
            ScalarType::Unit => ScalarValue::Null,
            ScalarType::Bool => ScalarValue::Bool(*value.get::<bool>().unwrap()),
            ScalarType::Char => ScalarValue::Char(*value.get::<char>().unwrap()),
            ScalarType::Str | ScalarType::String | ScalarType::CowStr => {
                ScalarValue::Str(Cow::Borrowed(value.as_str().unwrap()))
            }
            ScalarType::F32 => ScalarValue::F64(*value.get::<f32>().unwrap() as f64),
            ScalarType::F64 => ScalarValue::F64(*value.get::<f64>().unwrap()),
            ScalarType::U8 => ScalarValue::U64(*value.get::<u8>().unwrap() as u64),
            ScalarType::U16 => ScalarValue::U64(*value.get::<u16>().unwrap() as u64),
            ScalarType::U32 => ScalarValue::U64(*value.get::<u32>().unwrap() as u64),
            ScalarType::U64 => ScalarValue::U64(*value.get::<u64>().unwrap()),
            ScalarType::U128 => {
                let n = *value.get::<u128>().unwrap();
                ScalarValue::Str(Cow::Owned(alloc::string::ToString::to_string(&n)))
            }
            ScalarType::USize => ScalarValue::U64(*value.get::<usize>().unwrap() as u64),
            ScalarType::I8 => ScalarValue::I64(*value.get::<i8>().unwrap() as i64),
            ScalarType::I16 => ScalarValue::I64(*value.get::<i16>().unwrap() as i64),
            ScalarType::I32 => ScalarValue::I64(*value.get::<i32>().unwrap() as i64),
            ScalarType::I64 => ScalarValue::I64(*value.get::<i64>().unwrap()),
            ScalarType::I128 => {
                let n = *value.get::<i128>().unwrap();
                ScalarValue::Str(Cow::Owned(alloc::string::ToString::to_string(&n)))
            }
            ScalarType::ISize => ScalarValue::I64(*value.get::<isize>().unwrap() as i64),
            #[cfg(feature = "net")]
            ScalarType::IpAddr => {
                let addr = *value.get::<core::net::IpAddr>().unwrap();
                ScalarValue::Str(Cow::Owned(alloc::string::ToString::to_string(&addr)))
            }
            #[cfg(feature = "net")]
            ScalarType::Ipv4Addr => {
                let addr = *value.get::<core::net::Ipv4Addr>().unwrap();
                ScalarValue::Str(Cow::Owned(alloc::string::ToString::to_string(&addr)))
            }
            #[cfg(feature = "net")]
            ScalarType::Ipv6Addr => {
                let addr = *value.get::<core::net::Ipv6Addr>().unwrap();
                ScalarValue::Str(Cow::Owned(alloc::string::ToString::to_string(&addr)))
            }
            #[cfg(feature = "net")]
            ScalarType::SocketAddr => {
                let addr = *value.get::<core::net::SocketAddr>().unwrap();
                ScalarValue::Str(Cow::Owned(alloc::string::ToString::to_string(&addr)))
            }
            _ => {
                // For unknown scalar types, try to get a string representation
                if let Some(s) = value.as_str() {
                    ScalarValue::Str(Cow::Borrowed(s))
                } else {
                    ScalarValue::Null
                }
            }
        };
        self.scalar(scalar)
    }

    /// Begin serializing `Option::Some(value)`.
    ///
    /// Binary formats like postcard write a `0x01` discriminant byte here.
    /// Text formats typically don't need a prefix (they just serialize the value).
    ///
    /// Default: no-op (text formats).
    fn begin_option_some(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Serialize `Option::None`.
    ///
    /// Binary formats like postcard write a `0x00` discriminant byte.
    /// Text formats typically emit `null`.
    ///
    /// Default: emits `ScalarValue::Null`.
    fn serialize_none(&mut self) -> Result<(), Self::Error> {
        self.scalar(ScalarValue::Null)
    }

    /// Begin an enum variant with its index and name.
    ///
    /// Binary formats like postcard write the variant index as a varint.
    /// Text formats typically use the variant name as a key or value.
    ///
    /// This is called for externally tagged enums before the variant payload.
    /// For untagged enums, this is not called.
    ///
    /// Default: no-op (text formats handle variants via field_key/scalar).
    fn begin_enum_variant(
        &mut self,
        _variant_index: usize,
        _variant_name: &'static str,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Write a tag for an externally-tagged enum variant.
    ///
    /// Formats like Styx that use `@tag` syntax for enum variants should override
    /// this to write their tag and return `Ok(true)`. The shared serializer will
    /// then call the appropriate payload serialization method.
    ///
    /// If this returns `Ok(false)` (the default), the shared serializer uses
    /// the standard externally-tagged representation: `{ "variant_name": payload }`.
    ///
    /// When returning `Ok(true)`:
    /// - For unit variants, nothing more is written
    /// - For newtype variants, the payload is serialized directly after
    /// - For struct variants, begin_struct_after_tag is called for the payload
    fn write_variant_tag(&mut self, _variant_name: &str) -> Result<bool, Self::Error> {
        Ok(false)
    }

    /// Begin a struct directly after a variant tag (no separator).
    ///
    /// Called after `write_variant_tag` returns `Ok(true)` for struct variants.
    /// Formats should write `{` without any preceding space/separator.
    ///
    /// Default: calls `begin_struct()`.
    fn begin_struct_after_tag(&mut self) -> Result<(), Self::Error> {
        self.begin_struct()
    }

    /// Begin a sequence directly after a variant tag (no separator).
    ///
    /// Called after `write_variant_tag` returns `Ok(true)` for tuple variants.
    /// Formats should write `(` or `[` without any preceding space/separator.
    ///
    /// Default: calls `begin_seq()`.
    fn begin_seq_after_tag(&mut self) -> Result<(), Self::Error> {
        self.begin_seq()
    }

    /// Serialize a byte sequence (`Vec<u8>`, `&[u8]`, etc.) in bulk.
    ///
    /// For binary formats like postcard that store byte sequences as raw bytes
    /// (varint length followed by raw data), this allows bulk writing instead
    /// of element-by-element serialization.
    ///
    /// If the serializer handles this, it should write the bytes directly and
    /// return `Ok(true)`. If it doesn't support this optimization, it should
    /// return `Ok(false)` and the serializer will fall back to element-by-element
    /// serialization.
    ///
    /// Returns `Ok(true)` if handled (bytes were written), `Ok(false)` otherwise.
    fn serialize_byte_sequence(&mut self, _bytes: &[u8]) -> Result<bool, Self::Error> {
        // Default: not supported, fall back to element-by-element
        Ok(false)
    }

    /// Serialize a fixed-size byte array (`[u8; N]`) in bulk.
    ///
    /// Unlike `serialize_byte_sequence`, this does NOT write a length prefix
    /// since the array size is known from the type.
    ///
    /// Returns `Ok(true)` if handled (bytes were written), `Ok(false)` otherwise.
    fn serialize_byte_array(&mut self, _bytes: &[u8]) -> Result<bool, Self::Error> {
        // Default: not supported, fall back to element-by-element
        Ok(false)
    }

    /// Returns the format namespace for format-specific proxy resolution.
    ///
    /// When a field or container has format-specific proxies (e.g., `#[facet(xml::proxy = XmlProxy)]`),
    /// this namespace is used to look up the appropriate proxy. If no namespace is returned,
    /// only the format-agnostic proxy (`#[facet(proxy = ...)]`) is considered.
    ///
    /// Examples:
    /// - XML serializer should return `Some("xml")`
    /// - JSON serializer should return `Some("json")`
    ///
    /// Default: returns `None` (only format-agnostic proxies are used).
    fn format_namespace(&self) -> Option<&'static str> {
        None
    }
}

/// Error produced by the shared serializer.
#[derive(Debug)]
pub enum SerializeError<E: Debug> {
    /// Format backend error.
    Backend(E),
    /// Reflection failed while traversing the value.
    Reflect(ReflectError),
    /// Value can't be represented by the shared serializer.
    Unsupported(Cow<'static, str>),
    /// Internal invariant violation.
    Internal(Cow<'static, str>),
}

impl<E: Debug> core::fmt::Display for SerializeError<E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SerializeError::Backend(_) => f.write_str("format serializer error"),
            SerializeError::Reflect(err) => write!(f, "{err}"),
            SerializeError::Unsupported(msg) => f.write_str(msg.as_ref()),
            SerializeError::Internal(msg) => f.write_str(msg.as_ref()),
        }
    }
}

impl<E: Debug> std::error::Error for SerializeError<E> {}

/// Serialize a root value using the shared traversal logic.
pub fn serialize_root<'mem, 'facet, S>(
    serializer: &mut S,
    value: Peek<'mem, 'facet>,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    shared_serialize(serializer, value)
}

/// Helper to sort fields according to format preference (currently a no-op).
fn sort_fields_if_needed<'mem, 'facet, S>(
    _serializer: &S,
    _fields: &mut alloc::vec::Vec<(facet_reflect::FieldItem, Peek<'mem, 'facet>)>,
) where
    S: FormatSerializer,
{
    // Currently only Declaration order is supported, which preserves the original order.
}

fn shared_serialize<'mem, 'facet, S>(
    serializer: &mut S,
    value: Peek<'mem, 'facet>,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    // Dereference pointers (Box, Arc, etc.) to get the underlying value
    let value = deref_if_pointer(value);

    // Check for raw serialization type (e.g., RawJson) BEFORE innermost_peek
    // because innermost_peek might unwrap the type if it has .inner set
    if serializer.raw_serialize_shape() == Some(value.shape()) {
        // RawJson is a tuple struct with a single Cow<str> field
        // Get the inner Cow<str> value via as_str on the inner.
        // Use fields_for_binary_serialize to access the field directly without skip logic.
        if let Ok(struct_) = value.into_struct()
            && let Some((_field_item, inner_value)) = struct_.fields_for_binary_serialize().next()
            && let Some(s) = inner_value.as_str()
        {
            return serializer.raw_scalar(s).map_err(SerializeError::Backend);
        }
        // If we get here, the raw shape matched but extraction failed
        // This shouldn't happen for properly implemented raw types
        return Err(SerializeError::Unsupported(Cow::Borrowed(
            "raw capture type matched but could not extract inner string",
        )));
    }

    if serializer
        .serialize_opaque_scalar(value.shape(), value)
        .map_err(SerializeError::Backend)?
    {
        return Ok(());
    }

    let value = value.innermost_peek();

    // Check for metadata containers - serialize transparently through the inner value
    // Metadata containers have exactly one non-metadata field that carries the actual value,
    // while other fields (like doc comments) are metadata for formats that support it.
    if value.shape().is_metadata_container()
        && let Ok(struct_) = value.into_struct()
    {
        // Let the format handle metadata containers if it wants to (e.g., emit doc comments)
        if serializer
            .serialize_metadata_container(&struct_)
            .map_err(SerializeError::Backend)?
        {
            return Ok(());
        }
        // Default: serialize transparently through the non-metadata field
        for (field, field_value) in struct_.fields() {
            if !field.is_metadata() {
                return shared_serialize(serializer, field_value);
            }
        }
    }

    // Check for container-level proxy - serialize through the proxy type
    // Format-specific proxies take precedence over format-agnostic proxies
    if let Some(proxy_def) = value.shape().effective_proxy(serializer.format_namespace()) {
        return serialize_via_proxy(serializer, value, proxy_def);
    }

    // Use typed_scalar for scalars - allows binary formats to encode precisely
    if let Some(scalar_type) = value.scalar_type() {
        return serializer
            .typed_scalar(scalar_type, value)
            .map_err(SerializeError::Backend);
    }

    // Fallback for Def::Scalar types with Display trait (e.g., SmolStr, SmartString, CompactString)
    // These are string-like types that should serialize as strings
    if matches!(value.shape().def, Def::Scalar) && value.shape().vtable.has_display() {
        use alloc::string::ToString;
        let formatted = value.to_string();
        return serializer
            .scalar(ScalarValue::Str(Cow::Owned(formatted)))
            .map_err(SerializeError::Backend);
    }

    // Handle Option<T> - use begin_option_some/serialize_none for binary formats
    if let Ok(opt) = value.into_option() {
        return match opt.value() {
            Some(inner) => {
                serializer
                    .begin_option_some()
                    .map_err(SerializeError::Backend)?;
                shared_serialize(serializer, inner)
            }
            None => serializer.serialize_none().map_err(SerializeError::Backend),
        };
    }

    if let Ok(result) = value.into_result() {
        let (variant_index, variant_name, inner) = if result.is_ok() {
            (
                0,
                "Ok",
                result.ok().ok_or(SerializeError::Internal(Cow::Borrowed(
                    "result reported Ok but value was missing",
                )))?,
            )
        } else {
            (
                1,
                "Err",
                result.err().ok_or(SerializeError::Internal(Cow::Borrowed(
                    "result reported Err but value was missing",
                )))?,
            )
        };

        if serializer.enum_variant_encoding() == EnumVariantEncoding::Index {
            serializer
                .begin_enum_variant(variant_index, variant_name)
                .map_err(SerializeError::Backend)?;
            return shared_serialize(serializer, inner);
        }

        // Externally tagged representation for non-index encodings.
        serializer.begin_struct().map_err(SerializeError::Backend)?;
        serializer
            .field_key(variant_name)
            .map_err(SerializeError::Backend)?;
        shared_serialize(serializer, inner)?;
        serializer.end_struct().map_err(SerializeError::Backend)?;
        return Ok(());
    }

    if let Ok(dynamic) = value.into_dynamic_value() {
        return serialize_dynamic_value(serializer, dynamic);
    }

    match value.shape().def {
        facet_core::Def::List(_) | facet_core::Def::Array(_) | facet_core::Def::Slice(_) => {
            let list = value.into_list_like().map_err(SerializeError::Reflect)?;
            let len = list.len();

            // Check if this is a byte sequence - if so, try bulk serialization
            if let Some(bytes) = list.as_bytes() {
                let handled = match value.shape().def {
                    // Arrays have no length prefix
                    facet_core::Def::Array(_) => serializer
                        .serialize_byte_array(bytes)
                        .map_err(SerializeError::Backend)?,
                    // Lists and slices have a length prefix
                    _ => serializer
                        .serialize_byte_sequence(bytes)
                        .map_err(SerializeError::Backend)?,
                };
                if handled {
                    return Ok(());
                }
            }
            // Fall through to element-by-element if not handled

            match value.shape().def {
                facet_core::Def::Array(_) => {
                    serializer.begin_seq().map_err(SerializeError::Backend)?
                }
                _ => serializer
                    .begin_seq_with_len(len)
                    .map_err(SerializeError::Backend)?,
            };
            for item in list.iter() {
                shared_serialize(serializer, item)?;
            }
            serializer.end_seq().map_err(SerializeError::Backend)?;
            return Ok(());
        }
        _ => {}
    }

    if let Ok(map) = value.into_map() {
        let len = map.len();
        match serializer.map_encoding() {
            MapEncoding::Pairs => {
                serializer
                    .begin_map_with_len(len)
                    .map_err(SerializeError::Backend)?;
                for (key, val) in map.iter() {
                    shared_serialize(serializer, key)?;
                    shared_serialize(serializer, val)?;
                }
                serializer.end_map().map_err(SerializeError::Backend)?;
            }
            MapEncoding::Struct => {
                serializer.begin_struct().map_err(SerializeError::Backend)?;
                for (key, val) in map.iter() {
                    // Let format handle special key types first
                    if !serializer
                        .serialize_map_key(key)
                        .map_err(SerializeError::Backend)?
                    {
                        // Default: convert the key to a string for the field name
                        let key_str = if let Some(s) = key.as_str() {
                            Cow::Borrowed(s)
                        } else {
                            // For non-string keys, use Display format (not Debug, which adds quotes)
                            Cow::Owned(alloc::format!("{}", key))
                        };
                        serializer
                            .field_key(&key_str)
                            .map_err(SerializeError::Backend)?;
                    }
                    shared_serialize(serializer, val)?;
                }
                serializer.end_struct().map_err(SerializeError::Backend)?;
            }
        }
        return Ok(());
    }

    if let Ok(set) = value.into_set() {
        // Use begin_seq_with_len for binary formats that need length prefixes
        let len = set.len();
        serializer
            .begin_seq_with_len(len)
            .map_err(SerializeError::Backend)?;
        for item in set.iter() {
            shared_serialize(serializer, item)?;
        }
        serializer.end_seq().map_err(SerializeError::Backend)?;
        return Ok(());
    }

    if let Ok(struct_) = value.into_struct() {
        let kind = struct_.ty().kind;
        let field_mode = serializer.struct_field_mode();

        if kind == StructKind::Tuple || kind == StructKind::TupleStruct {
            // Special case: transparent newtypes (marked with #[facet(transparent)] or
            // #[repr(transparent)]) serialize as their inner value without wrapping.
            let fields: alloc::vec::Vec<_> = struct_.fields_for_binary_serialize().collect();
            let is_transparent = value.shape().is_transparent() && fields.len() == 1;

            if is_transparent {
                // Serialize the single field directly without array wrapper
                let (field_item, field_value) = &fields[0];
                if let Some(proxy_def) = field_item
                    .field
                    .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                {
                    serialize_via_proxy(serializer, *field_value, proxy_def)?;
                } else {
                    shared_serialize(serializer, *field_value)?;
                }
            } else {
                // Serialize tuples as arrays without length prefixes.
                // Tuples are positional, so use binary serialize to avoid skip predicates.
                serializer.begin_seq().map_err(SerializeError::Backend)?;
                for (field_item, field_value) in fields {
                    // Check for field-level proxy
                    if let Some(proxy_def) = field_item
                        .field
                        .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                    {
                        serialize_via_proxy(serializer, field_value, proxy_def)?;
                    } else {
                        shared_serialize(serializer, field_value)?;
                    }
                }
                serializer.end_seq().map_err(SerializeError::Backend)?;
            }
        } else {
            // Regular structs as objects
            serializer
                .struct_metadata(value.shape())
                .map_err(SerializeError::Backend)?;
            serializer.begin_struct().map_err(SerializeError::Backend)?;

            // Collect fields and sort according to format preference.
            // For binary formats (Unnamed), use fields_for_binary_serialize to avoid
            // skip predicates that would break positional deserialization.
            let mut fields: alloc::vec::Vec<_> = if field_mode == StructFieldMode::Unnamed {
                struct_.fields_for_binary_serialize().collect()
            } else {
                struct_.fields_for_serialize().collect()
            };

            sort_fields_if_needed(serializer, &mut fields);

            for (field_item, field_value) in fields {
                // Check for flattened internally-tagged enum
                // For these, we need to emit the tag field and flatten the content
                if field_item.flattened
                    && let Some(field) = field_item.field
                    && let shape = field.shape()
                    && let Some(tag_key) = shape.get_tag_attr()
                    && shape.get_content_attr().is_none()
                {
                    // Internally-tagged: has tag but no content attribute
                    let variant_name = field_item.effective_name();

                    // Emit the tag field
                    if field_mode == StructFieldMode::Named {
                        serializer
                            .field_key(tag_key)
                            .map_err(SerializeError::Backend)?;
                    }
                    serializer
                        .scalar(ScalarValue::Str(Cow::Borrowed(variant_name)))
                        .map_err(SerializeError::Backend)?;

                    // Emit the content fields (flattened)
                    // For newtype variants, field_value is the inner struct
                    // For struct variants, field_value is the enum itself
                    if let Ok(inner_struct) = field_value.into_struct() {
                        // Newtype variant - iterate inner struct's fields
                        for (inner_item, inner_value) in inner_struct.fields_for_serialize() {
                            if field_mode == StructFieldMode::Named {
                                serializer
                                    .field_key(inner_item.effective_name())
                                    .map_err(SerializeError::Backend)?;
                            }
                            if let Some(proxy_def) = inner_item
                                .field
                                .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                            {
                                serialize_via_proxy(serializer, inner_value, proxy_def)?;
                            } else {
                                shared_serialize(serializer, inner_value)?;
                            }
                        }
                    } else if let Ok(enum_peek) = field_value.into_enum() {
                        // Struct variant - iterate enum's fields (the variant's fields)
                        for (inner_item, inner_value) in enum_peek.fields_for_serialize() {
                            if field_mode == StructFieldMode::Named {
                                serializer
                                    .field_key(inner_item.effective_name())
                                    .map_err(SerializeError::Backend)?;
                            }
                            if let Some(proxy_def) = inner_item
                                .field
                                .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                            {
                                serialize_via_proxy(serializer, inner_value, proxy_def)?;
                            } else {
                                shared_serialize(serializer, inner_value)?;
                            }
                        }
                    } else if matches!(field_value.shape().ty, Type::Primitive(_)) {
                        // Scalar/primitive payload (e.g., A(i32))
                        // Internally-tagged enums cannot flatten scalar payloads -
                        // there's no field name to use for the value.
                        // Use #[facet(content = "...")] for adjacently-tagged representation.
                        return Err(SerializeError::Unsupported(
                            "internally-tagged enum with scalar newtype payload cannot be \
                             flattened; use #[facet(content = \"...\")] for adjacently-tagged \
                             representation"
                                .into(),
                        ));
                    }
                    // Unit variants have no content fields to emit
                    continue;
                }

                // Let format handle field metadata with value access (for metadata containers)
                let key_written = serializer
                    .field_metadata_with_value(&field_item, field_value)
                    .map_err(SerializeError::Backend)?;
                if !key_written {
                    serializer
                        .field_metadata(&field_item)
                        .map_err(SerializeError::Backend)?;
                    if field_mode == StructFieldMode::Named {
                        serializer
                            .field_key(field_item.effective_name())
                            .map_err(SerializeError::Backend)?;
                    }
                }
                // Check for field-level proxy
                if let Some(proxy_def) = field_item
                    .field
                    .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                {
                    serialize_via_proxy(serializer, field_value, proxy_def)?;
                } else {
                    shared_serialize(serializer, field_value)?;
                }
            }
            serializer.end_struct().map_err(SerializeError::Backend)?;
        }
        return Ok(());
    }

    if let Ok(enum_) = value.into_enum() {
        let variant = enum_.active_variant().map_err(|_| {
            SerializeError::Unsupported(Cow::Borrowed("opaque enum layout is unsupported"))
        })?;

        // Notify format of the variant being serialized (for xml::elements support)
        serializer
            .variant_metadata(variant)
            .map_err(SerializeError::Backend)?;

        if serializer.enum_variant_encoding() == EnumVariantEncoding::Index {
            let variant_index = enum_.variant_index().map_err(|_| {
                SerializeError::Unsupported(Cow::Borrowed("opaque enum layout is unsupported"))
            })?;
            serializer
                .begin_enum_variant(variant_index, variant.name)
                .map_err(SerializeError::Backend)?;

            match variant.data.kind {
                StructKind::Unit => return Ok(()),
                StructKind::TupleStruct | StructKind::Tuple | StructKind::Struct => {
                    // Index encoding is for binary formats - use binary serialize
                    // to avoid skip predicates that would break positional deserialization.
                    for (field_item, field_value) in enum_.fields_for_binary_serialize() {
                        if let Some(proxy_def) = field_item
                            .field
                            .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                        {
                            serialize_via_proxy(serializer, field_value, proxy_def)?;
                        } else {
                            shared_serialize(serializer, field_value)?;
                        }
                    }
                    return Ok(());
                }
            }
        }

        let numeric = value.shape().is_numeric();
        let untagged = value.shape().is_untagged();
        let tag = value.shape().get_tag_attr();
        let content = value.shape().get_content_attr();

        if numeric {
            return serialize_numeric_enum(serializer, variant);
        }
        if untagged {
            return serialize_untagged_enum(serializer, enum_, variant);
        }

        match (tag, content) {
            (Some(tag_key), None) => {
                // Internally tagged.
                serializer.begin_struct().map_err(SerializeError::Backend)?;
                serializer
                    .field_key(tag_key)
                    .map_err(SerializeError::Backend)?;
                serializer
                    .scalar(ScalarValue::Str(Cow::Borrowed(variant.effective_name())))
                    .map_err(SerializeError::Backend)?;

                let field_mode = serializer.struct_field_mode();
                match variant.data.kind {
                    StructKind::Unit => {}
                    StructKind::Struct => {
                        let mut fields: alloc::vec::Vec<_> =
                            if field_mode == StructFieldMode::Unnamed {
                                enum_.fields_for_binary_serialize().collect()
                            } else {
                                enum_.fields_for_serialize().collect()
                            };
                        sort_fields_if_needed(serializer, &mut fields);
                        for (field_item, field_value) in fields {
                            serializer
                                .field_metadata(&field_item)
                                .map_err(SerializeError::Backend)?;
                            if field_mode == StructFieldMode::Named {
                                serializer
                                    .field_key(field_item.effective_name())
                                    .map_err(SerializeError::Backend)?;
                            }
                            // Check for field-level proxy
                            if let Some(proxy_def) = field_item
                                .field
                                .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                            {
                                serialize_via_proxy(serializer, field_value, proxy_def)?;
                            } else {
                                shared_serialize(serializer, field_value)?;
                            }
                        }
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        return Err(SerializeError::Unsupported(Cow::Borrowed(
                            "internally tagged tuple variants are not supported",
                        )));
                    }
                }

                serializer.end_struct().map_err(SerializeError::Backend)?;
                return Ok(());
            }
            (Some(tag_key), Some(content_key)) => {
                // Adjacently tagged.
                let field_mode = serializer.struct_field_mode();
                serializer.begin_struct().map_err(SerializeError::Backend)?;
                serializer
                    .field_key(tag_key)
                    .map_err(SerializeError::Backend)?;
                serializer
                    .scalar(ScalarValue::Str(Cow::Borrowed(variant.effective_name())))
                    .map_err(SerializeError::Backend)?;

                match variant.data.kind {
                    StructKind::Unit => {
                        // Unit variants with adjacent tagging omit the content field.
                    }
                    StructKind::Struct => {
                        serializer
                            .field_key(content_key)
                            .map_err(SerializeError::Backend)?;
                        serializer.begin_struct().map_err(SerializeError::Backend)?;
                        let mut fields: alloc::vec::Vec<_> =
                            if field_mode == StructFieldMode::Unnamed {
                                enum_.fields_for_binary_serialize().collect()
                            } else {
                                enum_.fields_for_serialize().collect()
                            };
                        sort_fields_if_needed(serializer, &mut fields);
                        for (field_item, field_value) in fields {
                            serializer
                                .field_metadata(&field_item)
                                .map_err(SerializeError::Backend)?;
                            if field_mode == StructFieldMode::Named {
                                serializer
                                    .field_key(field_item.effective_name())
                                    .map_err(SerializeError::Backend)?;
                            }
                            // Check for field-level proxy
                            if let Some(proxy_def) = field_item
                                .field
                                .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                            {
                                serialize_via_proxy(serializer, field_value, proxy_def)?;
                            } else {
                                shared_serialize(serializer, field_value)?;
                            }
                        }
                        serializer.end_struct().map_err(SerializeError::Backend)?;
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        serializer
                            .field_key(content_key)
                            .map_err(SerializeError::Backend)?;

                        let field_count = variant.data.fields.len();
                        if field_count == 1 {
                            let inner = enum_
                                .field(0)
                                .map_err(|_| {
                                    SerializeError::Internal(Cow::Borrowed(
                                        "variant field lookup failed",
                                    ))
                                })?
                                .ok_or(SerializeError::Internal(Cow::Borrowed(
                                    "variant reported 1 field but field(0) returned None",
                                )))?;
                            shared_serialize(serializer, inner)?;
                        } else {
                            serializer.begin_seq().map_err(SerializeError::Backend)?;
                            for idx in 0..field_count {
                                let inner = enum_
                                    .field(idx)
                                    .map_err(|_| {
                                        SerializeError::Internal(Cow::Borrowed(
                                            "variant field lookup failed",
                                        ))
                                    })?
                                    .ok_or(SerializeError::Internal(Cow::Borrowed(
                                        "variant field missing while iterating tuple fields",
                                    )))?;
                                shared_serialize(serializer, inner)?;
                            }
                            serializer.end_seq().map_err(SerializeError::Backend)?;
                        }
                    }
                }

                serializer.end_struct().map_err(SerializeError::Backend)?;
                return Ok(());
            }
            (None, Some(_)) => {
                return Err(SerializeError::Unsupported(Cow::Borrowed(
                    "adjacent content key set without tag key",
                )));
            }
            (None, None) => {}
        }

        // Externally tagged (default).
        // For #[facet(other)] variants with a #[facet(metadata = "tag")] field,
        // use the field's value as the tag name instead of the variant name.
        trace!(
            variant_name = variant.name,
            is_other = variant.is_other(),
            "serializing enum variant"
        );
        let field_mode = serializer.struct_field_mode();
        let tag_name: Cow<'_, str> = if variant.is_other() {
            // Look for a tag field in the variant's data.
            // For binary formats, use binary serialize to avoid skip predicates.
            let mut tag_value: Option<Cow<'_, str>> = None;
            let fields_iter: alloc::boxed::Box<
                dyn Iterator<Item = (facet_reflect::FieldItem, facet_reflect::Peek<'_, '_>)>,
            > = if field_mode == StructFieldMode::Unnamed {
                alloc::boxed::Box::new(enum_.fields_for_binary_serialize())
            } else {
                alloc::boxed::Box::new(enum_.fields_for_serialize())
            };
            for (field_item, field_value) in fields_iter {
                if let Some(field) = field_item.field {
                    trace!(
                        field_name = field.name,
                        is_variant_tag = field.is_variant_tag(),
                        "checking field for tag"
                    );
                    if field.is_variant_tag() {
                        // Extract the tag value - handle Option<String>
                        if let Ok(opt) = field_value.into_option()
                            && let Some(inner) = opt.value()
                            && let Some(s) = inner.as_str()
                        {
                            tag_value = Some(Cow::Borrowed(s));
                        }
                        break;
                    }
                }
            }
            tag_value.unwrap_or_else(|| Cow::Borrowed(variant.effective_name()))
        } else {
            Cow::Borrowed(variant.effective_name())
        };

        // First, check if the format wants to handle this with tag syntax (e.g., Styx's @tag)
        let use_tag_syntax = serializer
            .write_variant_tag(&tag_name)
            .map_err(SerializeError::Backend)?;

        if use_tag_syntax {
            // Format wrote the tag, now serialize the payload appropriately
            return match variant.data.kind {
                StructKind::Unit => {
                    // Nothing more to write for unit variants
                    Ok(())
                }
                StructKind::TupleStruct | StructKind::Tuple => {
                    let field_count = variant.data.fields.len();
                    if field_count == 1 {
                        // Newtype variant - serialize payload directly
                        let inner = enum_
                            .field(0)
                            .map_err(|_| {
                                SerializeError::Internal(Cow::Borrowed(
                                    "variant field lookup failed",
                                ))
                            })?
                            .ok_or(SerializeError::Internal(Cow::Borrowed(
                                "variant reported 1 field but field(0) returned None",
                            )))?;
                        shared_serialize(serializer, inner)?;
                    } else {
                        // Multi-field tuple - use sequence after tag
                        serializer
                            .begin_seq_after_tag()
                            .map_err(SerializeError::Backend)?;
                        for idx in 0..field_count {
                            let inner = enum_
                                .field(idx)
                                .map_err(|_| {
                                    SerializeError::Internal(Cow::Borrowed(
                                        "variant field lookup failed",
                                    ))
                                })?
                                .ok_or(SerializeError::Internal(Cow::Borrowed(
                                    "variant field missing while iterating tuple fields",
                                )))?;
                            shared_serialize(serializer, inner)?;
                        }
                        serializer.end_seq().map_err(SerializeError::Backend)?;
                    }
                    Ok(())
                }
                StructKind::Struct => {
                    // Struct variant - use struct after tag
                    // For #[facet(other)] variants, skip metadata fields (like tag) from payload
                    let is_other = variant.is_other();
                    let fields_iter: alloc::boxed::Box<
                        dyn Iterator<
                            Item = (facet_reflect::FieldItem, facet_reflect::Peek<'_, '_>),
                        >,
                    > = if field_mode == StructFieldMode::Unnamed {
                        alloc::boxed::Box::new(enum_.fields_for_binary_serialize())
                    } else {
                        alloc::boxed::Box::new(enum_.fields_for_serialize())
                    };
                    let mut fields: alloc::vec::Vec<_> = fields_iter
                        .filter(|(field_item, _)| {
                            if is_other {
                                // Skip metadata fields and tag fields for other variants
                                field_item
                                    .field
                                    .map(|f| f.metadata_kind().is_none() && !f.is_variant_tag())
                                    .unwrap_or(true)
                            } else {
                                true
                            }
                        })
                        .collect();

                    // If no non-metadata fields remain, don't emit a struct at all
                    if fields.is_empty() {
                        return Ok(());
                    }

                    serializer
                        .begin_struct_after_tag()
                        .map_err(SerializeError::Backend)?;
                    sort_fields_if_needed(serializer, &mut fields);
                    let field_mode = serializer.struct_field_mode();
                    for (field_item, field_value) in fields {
                        serializer
                            .field_metadata(&field_item)
                            .map_err(SerializeError::Backend)?;
                        if field_mode == StructFieldMode::Named {
                            serializer
                                .field_key(field_item.effective_name())
                                .map_err(SerializeError::Backend)?;
                        }
                        if let Some(proxy_def) = field_item
                            .field
                            .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                        {
                            serialize_via_proxy(serializer, field_value, proxy_def)?;
                        } else {
                            shared_serialize(serializer, field_value)?;
                        }
                    }
                    serializer.end_struct().map_err(SerializeError::Backend)?;
                    Ok(())
                }
            };
        }

        // Standard externally tagged representation: { "variant_name": payload }
        return match variant.data.kind {
            StructKind::Unit => {
                serializer
                    .scalar(ScalarValue::Str(Cow::Borrowed(variant.effective_name())))
                    .map_err(SerializeError::Backend)?;
                Ok(())
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                serializer.begin_struct().map_err(SerializeError::Backend)?;
                serializer
                    .field_key(variant.effective_name())
                    .map_err(SerializeError::Backend)?;

                let field_count = variant.data.fields.len();
                if field_count == 1 {
                    let inner = enum_
                        .field(0)
                        .map_err(|_| {
                            SerializeError::Internal(Cow::Borrowed("variant field lookup failed"))
                        })?
                        .ok_or(SerializeError::Internal(Cow::Borrowed(
                            "variant reported 1 field but field(0) returned None",
                        )))?;
                    shared_serialize(serializer, inner)?;
                } else {
                    serializer.begin_seq().map_err(SerializeError::Backend)?;
                    for idx in 0..field_count {
                        let inner = enum_
                            .field(idx)
                            .map_err(|_| {
                                SerializeError::Internal(Cow::Borrowed(
                                    "variant field lookup failed",
                                ))
                            })?
                            .ok_or(SerializeError::Internal(Cow::Borrowed(
                                "variant field missing while iterating tuple fields",
                            )))?;
                        shared_serialize(serializer, inner)?;
                    }
                    serializer.end_seq().map_err(SerializeError::Backend)?;
                }

                serializer.end_struct().map_err(SerializeError::Backend)?;
                Ok(())
            }
            StructKind::Struct => {
                let field_mode = serializer.struct_field_mode();
                serializer.begin_struct().map_err(SerializeError::Backend)?;
                serializer
                    .field_key(variant.effective_name())
                    .map_err(SerializeError::Backend)?;

                serializer.begin_struct().map_err(SerializeError::Backend)?;
                let mut fields: alloc::vec::Vec<_> = if field_mode == StructFieldMode::Unnamed {
                    enum_.fields_for_binary_serialize().collect()
                } else {
                    enum_.fields_for_serialize().collect()
                };
                sort_fields_if_needed(serializer, &mut fields);
                for (field_item, field_value) in fields {
                    serializer
                        .field_metadata(&field_item)
                        .map_err(SerializeError::Backend)?;
                    if field_mode == StructFieldMode::Named {
                        serializer
                            .field_key(field_item.effective_name())
                            .map_err(SerializeError::Backend)?;
                    }
                    // Check for field-level proxy
                    if let Some(proxy_def) = field_item
                        .field
                        .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                    {
                        serialize_via_proxy(serializer, field_value, proxy_def)?;
                    } else {
                        shared_serialize(serializer, field_value)?;
                    }
                }
                serializer.end_struct().map_err(SerializeError::Backend)?;

                serializer.end_struct().map_err(SerializeError::Backend)?;
                Ok(())
            }
        };
    }

    Err(SerializeError::Unsupported(Cow::Borrowed(
        "unsupported value kind for serialization",
    )))
}

fn serialize_dynamic_value<'mem, 'facet, S>(
    serializer: &mut S,
    dynamic: facet_reflect::PeekDynamicValue<'mem, 'facet>,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    let tagged = serializer.dynamic_value_encoding() == DynamicValueEncoding::Tagged;

    match dynamic.kind() {
        DynValueKind::Null => {
            if tagged {
                serializer
                    .dynamic_value_tag(DynamicValueTag::Null)
                    .map_err(SerializeError::Backend)?;
            }
            serializer
                .scalar(ScalarValue::Null)
                .map_err(SerializeError::Backend)
        }
        DynValueKind::Bool => {
            let value = dynamic.as_bool().ok_or_else(|| {
                SerializeError::Internal(Cow::Borrowed("dynamic bool missing value"))
            })?;
            if tagged {
                serializer
                    .dynamic_value_tag(DynamicValueTag::Bool)
                    .map_err(SerializeError::Backend)?;
            }
            serializer
                .scalar(ScalarValue::Bool(value))
                .map_err(SerializeError::Backend)
        }
        DynValueKind::Number => {
            if let Some(n) = dynamic.as_i64() {
                if tagged {
                    serializer
                        .dynamic_value_tag(DynamicValueTag::I64)
                        .map_err(SerializeError::Backend)?;
                }
                serializer
                    .scalar(ScalarValue::I64(n))
                    .map_err(SerializeError::Backend)
            } else if let Some(n) = dynamic.as_u64() {
                if tagged {
                    serializer
                        .dynamic_value_tag(DynamicValueTag::U64)
                        .map_err(SerializeError::Backend)?;
                }
                serializer
                    .scalar(ScalarValue::U64(n))
                    .map_err(SerializeError::Backend)
            } else if let Some(n) = dynamic.as_f64() {
                if tagged {
                    serializer
                        .dynamic_value_tag(DynamicValueTag::F64)
                        .map_err(SerializeError::Backend)?;
                }
                serializer
                    .scalar(ScalarValue::F64(n))
                    .map_err(SerializeError::Backend)
            } else {
                Err(SerializeError::Unsupported(Cow::Borrowed(
                    "dynamic number not representable",
                )))
            }
        }
        DynValueKind::String => {
            let value = dynamic.as_str().ok_or_else(|| {
                SerializeError::Internal(Cow::Borrowed("dynamic string missing value"))
            })?;
            if tagged {
                serializer
                    .dynamic_value_tag(DynamicValueTag::String)
                    .map_err(SerializeError::Backend)?;
            }
            serializer
                .scalar(ScalarValue::Str(Cow::Borrowed(value)))
                .map_err(SerializeError::Backend)
        }
        DynValueKind::Bytes => {
            let value = dynamic.as_bytes().ok_or_else(|| {
                SerializeError::Internal(Cow::Borrowed("dynamic bytes missing value"))
            })?;
            if tagged {
                serializer
                    .dynamic_value_tag(DynamicValueTag::Bytes)
                    .map_err(SerializeError::Backend)?;
            }
            serializer
                .scalar(ScalarValue::Bytes(Cow::Borrowed(value)))
                .map_err(SerializeError::Backend)
        }
        DynValueKind::Array => {
            let len = dynamic.array_len().ok_or_else(|| {
                SerializeError::Internal(Cow::Borrowed("dynamic array missing length"))
            })?;
            if tagged {
                serializer
                    .dynamic_value_tag(DynamicValueTag::Array)
                    .map_err(SerializeError::Backend)?;
            }
            serializer
                .begin_seq_with_len(len)
                .map_err(SerializeError::Backend)?;
            if let Some(iter) = dynamic.array_iter() {
                for item in iter {
                    shared_serialize(serializer, item)?;
                }
            }
            serializer.end_seq().map_err(SerializeError::Backend)
        }
        DynValueKind::Object => {
            let len = dynamic.object_len().ok_or_else(|| {
                SerializeError::Internal(Cow::Borrowed("dynamic object missing length"))
            })?;
            if tagged {
                serializer
                    .dynamic_value_tag(DynamicValueTag::Object)
                    .map_err(SerializeError::Backend)?;
            }
            match serializer.map_encoding() {
                MapEncoding::Pairs => {
                    serializer
                        .begin_map_with_len(len)
                        .map_err(SerializeError::Backend)?;
                    if let Some(iter) = dynamic.object_iter() {
                        for (key, value) in iter {
                            serializer
                                .scalar(ScalarValue::Str(Cow::Borrowed(key)))
                                .map_err(SerializeError::Backend)?;
                            shared_serialize(serializer, value)?;
                        }
                    }
                    serializer.end_map().map_err(SerializeError::Backend)
                }
                MapEncoding::Struct => {
                    serializer.begin_struct().map_err(SerializeError::Backend)?;
                    if let Some(iter) = dynamic.object_iter() {
                        for (key, value) in iter {
                            serializer.field_key(key).map_err(SerializeError::Backend)?;
                            shared_serialize(serializer, value)?;
                        }
                    }
                    serializer.end_struct().map_err(SerializeError::Backend)
                }
            }
        }
        DynValueKind::DateTime => {
            let dt = dynamic.as_datetime().ok_or_else(|| {
                SerializeError::Internal(Cow::Borrowed("dynamic datetime missing value"))
            })?;
            if tagged {
                serializer
                    .dynamic_value_tag(DynamicValueTag::DateTime)
                    .map_err(SerializeError::Backend)?;
            }
            let s = format_dyn_datetime(dt);
            serializer
                .scalar(ScalarValue::Str(Cow::Owned(s)))
                .map_err(SerializeError::Backend)
        }
        DynValueKind::QName | DynValueKind::Uuid => Err(SerializeError::Unsupported(
            Cow::Borrowed("dynamic QName/Uuid serialization is not supported"),
        )),
    }
}

fn format_dyn_datetime(
    (year, month, day, hour, minute, second, nanos, kind): (
        i32,
        u8,
        u8,
        u8,
        u8,
        u8,
        u32,
        DynDateTimeKind,
    ),
) -> String {
    let mut out = String::new();
    match kind {
        DynDateTimeKind::Offset { offset_minutes } => {
            let _ = write!(
                out,
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                year, month, day, hour, minute, second
            );
            if nanos > 0 {
                let _ = write!(out, ".{:09}", nanos);
            }
            if offset_minutes == 0 {
                out.push('Z');
            } else {
                let sign = if offset_minutes >= 0 { '+' } else { '-' };
                let abs = offset_minutes.unsigned_abs();
                let _ = write!(out, "{}{:02}:{:02}", sign, abs / 60, abs % 60);
            }
        }
        DynDateTimeKind::LocalDateTime => {
            let _ = write!(
                out,
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                year, month, day, hour, minute, second
            );
            if nanos > 0 {
                let _ = write!(out, ".{:09}", nanos);
            }
        }
        DynDateTimeKind::LocalDate => {
            let _ = write!(out, "{:04}-{:02}-{:02}", year, month, day);
        }
        DynDateTimeKind::LocalTime => {
            let _ = write!(out, "{:02}:{:02}:{:02}", hour, minute, second);
            if nanos > 0 {
                let _ = write!(out, ".{:09}", nanos);
            }
        }
    }
    out
}

fn serialize_numeric_enum<S>(
    serializer: &mut S,
    variant: &'static facet_core::Variant,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    let discriminant = variant
        .discriminant
        .ok_or(SerializeError::Unsupported(Cow::Borrowed(
            "Enum without a discriminant",
        )))?;
    serializer
        .scalar(ScalarValue::I64(discriminant))
        .map_err(SerializeError::Backend)
}

fn serialize_untagged_enum<'mem, 'facet, S>(
    serializer: &mut S,
    enum_: facet_reflect::PeekEnum<'mem, 'facet>,
    variant: &'static facet_core::Variant,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    match variant.data.kind {
        StructKind::Unit => serializer
            .scalar(ScalarValue::Str(Cow::Borrowed(variant.effective_name())))
            .map_err(SerializeError::Backend),
        StructKind::TupleStruct | StructKind::Tuple => {
            let field_count = variant.data.fields.len();
            if field_count == 1 {
                let inner = enum_
                    .field(0)
                    .map_err(|_| {
                        SerializeError::Internal(Cow::Borrowed("variant field lookup failed"))
                    })?
                    .ok_or(SerializeError::Internal(Cow::Borrowed(
                        "variant reported 1 field but field(0) returned None",
                    )))?;
                shared_serialize(serializer, inner)
            } else {
                serializer.begin_seq().map_err(SerializeError::Backend)?;
                for idx in 0..field_count {
                    let inner = enum_
                        .field(idx)
                        .map_err(|_| {
                            SerializeError::Internal(Cow::Borrowed("variant field lookup failed"))
                        })?
                        .ok_or(SerializeError::Internal(Cow::Borrowed(
                            "variant field missing while iterating tuple fields",
                        )))?;
                    shared_serialize(serializer, inner)?;
                }
                serializer.end_seq().map_err(SerializeError::Backend)?;
                Ok(())
            }
        }
        StructKind::Struct => {
            let field_mode = serializer.struct_field_mode();
            serializer.begin_struct().map_err(SerializeError::Backend)?;
            let mut fields: alloc::vec::Vec<_> = if field_mode == StructFieldMode::Unnamed {
                enum_.fields_for_binary_serialize().collect()
            } else {
                enum_.fields_for_serialize().collect()
            };
            sort_fields_if_needed(serializer, &mut fields);
            for (field_item, field_value) in fields {
                serializer
                    .field_metadata(&field_item)
                    .map_err(SerializeError::Backend)?;
                if field_mode == StructFieldMode::Named {
                    serializer
                        .field_key(field_item.effective_name())
                        .map_err(SerializeError::Backend)?;
                }
                // Check for field-level proxy
                if let Some(proxy_def) = field_item
                    .field
                    .and_then(|f| f.effective_proxy(serializer.format_namespace()))
                {
                    serialize_via_proxy(serializer, field_value, proxy_def)?;
                } else {
                    shared_serialize(serializer, field_value)?;
                }
            }
            serializer.end_struct().map_err(SerializeError::Backend)?;
            Ok(())
        }
    }
}

/// Dereference a pointer/reference (Box, Arc, etc.) to get the underlying value
fn deref_if_pointer<'mem, 'facet>(peek: Peek<'mem, 'facet>) -> Peek<'mem, 'facet> {
    if let Ok(ptr) = peek.into_pointer()
        && let Some(target) = ptr.borrow_inner()
    {
        return deref_if_pointer(target);
    }
    peek
}

/// Serialize a value through its proxy type.
///
/// # Safety note
/// This function requires unsafe code to:
/// - Allocate memory for the proxy type
/// - Call the conversion function from target to proxy
/// - Drop the proxy value after serialization
#[allow(unsafe_code)]
fn serialize_via_proxy<'mem, 'facet, S>(
    serializer: &mut S,
    value: Peek<'mem, 'facet>,
    proxy_def: &'static facet_core::ProxyDef,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    use facet_core::PtrUninit;

    let proxy_shape = proxy_def.shape;
    let proxy_layout = proxy_shape.layout.sized_layout().map_err(|_| {
        SerializeError::Unsupported(Cow::Borrowed("proxy type must be sized for serialization"))
    })?;

    // Allocate memory for the proxy value
    let proxy_mem = unsafe { alloc::alloc::alloc(proxy_layout) };
    if proxy_mem.is_null() {
        return Err(SerializeError::Internal(Cow::Borrowed(
            "failed to allocate proxy memory",
        )));
    }

    // Convert target → proxy
    let proxy_uninit = PtrUninit::new(proxy_mem);
    let convert_result = unsafe { (proxy_def.convert_out)(value.data(), proxy_uninit) };

    let proxy_ptr = match convert_result {
        Ok(ptr) => ptr,
        Err(msg) => {
            unsafe { alloc::alloc::dealloc(proxy_mem, proxy_layout) };
            return Err(SerializeError::Unsupported(Cow::Owned(msg)));
        }
    };

    // Create a Peek to the proxy value and serialize it
    let proxy_peek = unsafe { Peek::unchecked_new(proxy_ptr.as_const(), proxy_shape) };
    let result = shared_serialize(serializer, proxy_peek);

    // Clean up: drop the proxy value and deallocate
    unsafe {
        let _ = proxy_shape.call_drop_in_place(proxy_ptr);
        alloc::alloc::dealloc(proxy_mem, proxy_layout);
    }

    result
}

// ─────────────────────────────────────────────────────────────────────────────
// Shape-guided serialization of dynamic values
// ─────────────────────────────────────────────────────────────────────────────

/// Serialize a dynamic value (like `facet_value::Value`) according to a target shape.
///
/// This is the inverse of `FormatDeserializer::deserialize_with_shape`. It allows serializing
/// a `Value` as if it were a typed value matching the shape, without the dynamic value's
/// type discriminants.
///
/// This is useful for non-self-describing formats like postcard where you want to:
/// 1. Parse JSON into a `Value`
/// 2. Serialize it to postcard bytes matching a typed schema
///
/// # Arguments
///
/// * `serializer` - The format serializer to use
/// * `value` - A `Peek` into a dynamic value type (like `facet_value::Value`)
/// * `target_shape` - The shape describing the expected wire format
///
/// # Errors
///
/// Returns an error if:
/// - The value is not a dynamic value type
/// - The value's structure doesn't match the target shape
pub fn serialize_value_with_shape<S>(
    serializer: &mut S,
    value: Peek<'_, '_>,
    target_shape: &'static Shape,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    let dynamic = value.into_dynamic_value().map_err(|_| {
        SerializeError::Unsupported(Cow::Borrowed(
            "serialize_value_with_shape requires a DynamicValue type",
        ))
    })?;

    serialize_dynamic_with_shape(serializer, dynamic, target_shape, value.shape())
}

fn serialize_dynamic_with_shape<S>(
    serializer: &mut S,
    dynamic: facet_reflect::PeekDynamicValue<'_, '_>,
    target_shape: &'static Shape,
    value_shape: &'static Shape,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    use facet_core::{ListDef, OptionDef, ScalarType as CoreScalarType, Type, UserType};

    // Handle smart pointers - unwrap to the inner shape
    if let Def::Pointer(ptr_def) = target_shape.def
        && let Some(pointee) = ptr_def.pointee
    {
        return serialize_dynamic_with_shape(serializer, dynamic, pointee, value_shape);
    }

    // Handle transparent wrappers via .inner
    if let Some(inner_shape) = target_shape.inner {
        // Skip collection types that have .inner for variance but aren't transparent wrappers
        if !matches!(
            target_shape.def,
            Def::List(_) | Def::Map(_) | Def::Set(_) | Def::Array(_)
        ) {
            return serialize_dynamic_with_shape(serializer, dynamic, inner_shape, value_shape);
        }
    }

    // Handle Option<T>
    if let Def::Option(OptionDef { t: inner_shape, .. }) = target_shape.def {
        return serialize_option_from_dynamic(serializer, dynamic, inner_shape, value_shape);
    }

    // Handle List/Vec
    if let Def::List(ListDef { t: item_shape, .. }) = target_shape.def {
        return serialize_list_from_dynamic(serializer, dynamic, item_shape, value_shape);
    }

    // Handle Array [T; N]
    if let Def::Array(array_def) = target_shape.def {
        return serialize_array_from_dynamic(serializer, dynamic, array_def.t, value_shape);
    }

    // Handle Map
    if let Def::Map(map_def) = target_shape.def {
        return serialize_map_from_dynamic(serializer, dynamic, map_def.k, map_def.v, value_shape);
    }

    // Handle scalars
    if let Some(scalar_type) = CoreScalarType::try_from_shape(target_shape) {
        return serialize_scalar_from_dynamic(serializer, dynamic, scalar_type);
    }

    // Handle structs and enums by Type
    match target_shape.ty {
        Type::User(UserType::Struct(struct_def)) => {
            serialize_struct_from_dynamic(serializer, dynamic, struct_def, value_shape)
        }
        Type::User(UserType::Enum(enum_def)) => {
            serialize_enum_from_dynamic(serializer, dynamic, enum_def, target_shape, value_shape)
        }
        _ => Err(SerializeError::Unsupported(Cow::Owned(alloc::format!(
            "unsupported target shape for serialize_value_with_shape: {}",
            target_shape
        )))),
    }
}

fn serialize_option_from_dynamic<S>(
    serializer: &mut S,
    dynamic: facet_reflect::PeekDynamicValue<'_, '_>,
    inner_shape: &'static Shape,
    value_shape: &'static Shape,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    if dynamic.kind() == DynValueKind::Null {
        serializer.serialize_none().map_err(SerializeError::Backend)
    } else {
        serializer
            .begin_option_some()
            .map_err(SerializeError::Backend)?;
        serialize_dynamic_with_shape(serializer, dynamic, inner_shape, value_shape)
    }
}

fn serialize_list_from_dynamic<S>(
    serializer: &mut S,
    dynamic: facet_reflect::PeekDynamicValue<'_, '_>,
    item_shape: &'static Shape,
    value_shape: &'static Shape,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    let len = dynamic.array_len().ok_or_else(|| {
        SerializeError::Unsupported(Cow::Borrowed(
            "expected array value for list/vec target shape",
        ))
    })?;

    serializer
        .begin_seq_with_len(len)
        .map_err(SerializeError::Backend)?;

    if let Some(iter) = dynamic.array_iter() {
        for elem in iter {
            let elem_dyn = elem.into_dynamic_value().map_err(|_| {
                SerializeError::Internal(Cow::Borrowed("array element is not a dynamic value"))
            })?;
            serialize_dynamic_with_shape(serializer, elem_dyn, item_shape, value_shape)?;
        }
    }

    serializer.end_seq().map_err(SerializeError::Backend)
}

fn serialize_array_from_dynamic<S>(
    serializer: &mut S,
    dynamic: facet_reflect::PeekDynamicValue<'_, '_>,
    item_shape: &'static Shape,
    value_shape: &'static Shape,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    // Arrays don't have length prefix in postcard
    serializer.begin_seq().map_err(SerializeError::Backend)?;

    if let Some(iter) = dynamic.array_iter() {
        for elem in iter {
            let elem_dyn = elem.into_dynamic_value().map_err(|_| {
                SerializeError::Internal(Cow::Borrowed("array element is not a dynamic value"))
            })?;
            serialize_dynamic_with_shape(serializer, elem_dyn, item_shape, value_shape)?;
        }
    }

    serializer.end_seq().map_err(SerializeError::Backend)
}

fn serialize_map_from_dynamic<S>(
    serializer: &mut S,
    dynamic: facet_reflect::PeekDynamicValue<'_, '_>,
    key_shape: &'static Shape,
    value_shape_inner: &'static Shape,
    value_shape: &'static Shape,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    let len = dynamic.object_len().ok_or_else(|| {
        SerializeError::Unsupported(Cow::Borrowed("expected object value for map target shape"))
    })?;

    match serializer.map_encoding() {
        MapEncoding::Pairs => {
            serializer
                .begin_map_with_len(len)
                .map_err(SerializeError::Backend)?;

            if let Some(iter) = dynamic.object_iter() {
                for (key, val) in iter {
                    // Serialize key according to key_shape
                    serialize_string_as_scalar(serializer, key, key_shape)?;
                    // Serialize value
                    let val_dyn = val.into_dynamic_value().map_err(|_| {
                        SerializeError::Internal(Cow::Borrowed(
                            "object value is not a dynamic value",
                        ))
                    })?;
                    serialize_dynamic_with_shape(
                        serializer,
                        val_dyn,
                        value_shape_inner,
                        value_shape,
                    )?;
                }
            }

            serializer.end_map().map_err(SerializeError::Backend)
        }
        MapEncoding::Struct => {
            serializer.begin_struct().map_err(SerializeError::Backend)?;

            if let Some(iter) = dynamic.object_iter() {
                for (key, val) in iter {
                    serializer.field_key(key).map_err(SerializeError::Backend)?;
                    let val_dyn = val.into_dynamic_value().map_err(|_| {
                        SerializeError::Internal(Cow::Borrowed(
                            "object value is not a dynamic value",
                        ))
                    })?;
                    serialize_dynamic_with_shape(
                        serializer,
                        val_dyn,
                        value_shape_inner,
                        value_shape,
                    )?;
                }
            }

            serializer.end_struct().map_err(SerializeError::Backend)
        }
    }
}

fn serialize_string_as_scalar<S>(
    serializer: &mut S,
    s: &str,
    _key_shape: &'static Shape,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    // For now, serialize string keys directly
    // TODO: Handle non-string key types if needed
    serializer
        .scalar(ScalarValue::Str(Cow::Borrowed(s)))
        .map_err(SerializeError::Backend)
}

fn serialize_scalar_from_dynamic<S>(
    serializer: &mut S,
    dynamic: facet_reflect::PeekDynamicValue<'_, '_>,
    scalar_type: facet_core::ScalarType,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    use facet_core::ScalarType as ST;

    match scalar_type {
        ST::Unit => serializer
            .scalar(ScalarValue::Null)
            .map_err(SerializeError::Backend),
        ST::Bool => {
            let v = dynamic
                .as_bool()
                .ok_or_else(|| SerializeError::Unsupported(Cow::Borrowed("expected bool value")))?;
            serializer
                .scalar(ScalarValue::Bool(v))
                .map_err(SerializeError::Backend)
        }
        ST::Char => {
            let s = dynamic.as_str().ok_or_else(|| {
                SerializeError::Unsupported(Cow::Borrowed("expected string value for char"))
            })?;
            let c = s.chars().next().ok_or_else(|| {
                SerializeError::Unsupported(Cow::Borrowed("expected non-empty string for char"))
            })?;
            serializer
                .scalar(ScalarValue::Char(c))
                .map_err(SerializeError::Backend)
        }
        ST::Str | ST::String | ST::CowStr => {
            let s = dynamic.as_str().ok_or_else(|| {
                SerializeError::Unsupported(Cow::Borrowed("expected string value"))
            })?;
            serializer
                .scalar(ScalarValue::Str(Cow::Borrowed(s)))
                .map_err(SerializeError::Backend)
        }
        ST::U8 | ST::U16 | ST::U32 | ST::U64 | ST::USize => {
            let n = dynamic.as_u64().ok_or_else(|| {
                SerializeError::Unsupported(Cow::Borrowed("expected unsigned integer value"))
            })?;
            serializer
                .scalar(ScalarValue::U64(n))
                .map_err(SerializeError::Backend)
        }
        ST::U128 => {
            let n = dynamic.as_u64().ok_or_else(|| {
                SerializeError::Unsupported(Cow::Borrowed("expected unsigned integer value"))
            })?;
            serializer
                .scalar(ScalarValue::U128(n as u128))
                .map_err(SerializeError::Backend)
        }
        ST::I8 | ST::I16 | ST::I32 | ST::I64 | ST::ISize => {
            let n = dynamic.as_i64().ok_or_else(|| {
                SerializeError::Unsupported(Cow::Borrowed("expected signed integer value"))
            })?;
            serializer
                .scalar(ScalarValue::I64(n))
                .map_err(SerializeError::Backend)
        }
        ST::I128 => {
            let n = dynamic.as_i64().ok_or_else(|| {
                SerializeError::Unsupported(Cow::Borrowed("expected signed integer value"))
            })?;
            serializer
                .scalar(ScalarValue::I128(n as i128))
                .map_err(SerializeError::Backend)
        }
        ST::F32 | ST::F64 => {
            let n = dynamic.as_f64().ok_or_else(|| {
                SerializeError::Unsupported(Cow::Borrowed("expected float value"))
            })?;
            serializer
                .scalar(ScalarValue::F64(n))
                .map_err(SerializeError::Backend)
        }
        _ => Err(SerializeError::Unsupported(Cow::Owned(alloc::format!(
            "unsupported scalar type: {:?}",
            scalar_type
        )))),
    }
}

fn serialize_struct_from_dynamic<S>(
    serializer: &mut S,
    dynamic: facet_reflect::PeekDynamicValue<'_, '_>,
    struct_def: facet_core::StructType,
    value_shape: &'static Shape,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    let is_tuple = matches!(struct_def.kind, StructKind::Tuple | StructKind::TupleStruct);

    if is_tuple {
        // For tuples, expect an array value
        serializer.begin_seq().map_err(SerializeError::Backend)?;

        let iter = dynamic.array_iter().ok_or_else(|| {
            SerializeError::Unsupported(Cow::Borrowed("expected array value for tuple"))
        })?;

        for (field, elem) in struct_def.fields.iter().zip(iter) {
            let elem_dyn = elem.into_dynamic_value().map_err(|_| {
                SerializeError::Internal(Cow::Borrowed("tuple element is not a dynamic value"))
            })?;
            serialize_dynamic_with_shape(serializer, elem_dyn, field.shape(), value_shape)?;
        }

        serializer.end_seq().map_err(SerializeError::Backend)
    } else {
        // For named structs, expect an object value
        let field_mode = serializer.struct_field_mode();

        serializer.begin_struct().map_err(SerializeError::Backend)?;

        for field in struct_def.fields {
            // Skip metadata fields
            if field.is_metadata() {
                continue;
            }

            let field_name = field.name;
            let field_value = dynamic.object_get(field_name).ok_or_else(|| {
                SerializeError::Unsupported(Cow::Owned(alloc::format!(
                    "missing field '{}' in object",
                    field_name
                )))
            })?;

            if field_mode == StructFieldMode::Named {
                serializer
                    .field_key(field_name)
                    .map_err(SerializeError::Backend)?;
            }

            let field_dyn = field_value.into_dynamic_value().map_err(|_| {
                SerializeError::Internal(Cow::Borrowed("field value is not a dynamic value"))
            })?;
            serialize_dynamic_with_shape(serializer, field_dyn, field.shape(), value_shape)?;
        }

        serializer.end_struct().map_err(SerializeError::Backend)
    }
}

fn serialize_enum_from_dynamic<S>(
    serializer: &mut S,
    dynamic: facet_reflect::PeekDynamicValue<'_, '_>,
    enum_def: facet_core::EnumType,
    target_shape: &'static Shape,
    value_shape: &'static Shape,
) -> Result<(), SerializeError<S::Error>>
where
    S: FormatSerializer,
{
    // For index-based encoding (postcard), we need to:
    // 1. Determine the variant from the Value
    // 2. Emit the variant index
    // 3. Serialize the variant's payload

    let use_index = serializer.enum_variant_encoding() == EnumVariantEncoding::Index;

    match dynamic.kind() {
        // Unit variant represented as a string
        DynValueKind::String => {
            let variant_name = dynamic.as_str().ok_or_else(|| {
                SerializeError::Internal(Cow::Borrowed("expected string for unit variant"))
            })?;

            let (variant_index, variant) = enum_def
                .variants
                .iter()
                .enumerate()
                .find(|(_, v)| v.effective_name() == variant_name)
                .ok_or_else(|| {
                    SerializeError::Unsupported(Cow::Owned(alloc::format!(
                        "unknown variant '{}'",
                        variant_name
                    )))
                })?;

            if use_index {
                serializer
                    .begin_enum_variant(variant_index, variant.effective_name())
                    .map_err(SerializeError::Backend)?;
                // Unit variant has no payload
                Ok(())
            } else {
                serializer
                    .scalar(ScalarValue::Str(Cow::Borrowed(variant.effective_name())))
                    .map_err(SerializeError::Backend)
            }
        }

        // Variant with payload represented as object { "VariantName": payload }
        DynValueKind::Object => {
            // For externally tagged enums, the object has a single key = variant name
            let obj_len = dynamic.object_len().unwrap_or(0);
            if obj_len != 1 {
                return Err(SerializeError::Unsupported(Cow::Owned(alloc::format!(
                    "expected single-key object for enum variant, got {} keys",
                    obj_len
                ))));
            }

            let (variant_name, payload) = dynamic.object_get_entry(0).ok_or_else(|| {
                SerializeError::Internal(Cow::Borrowed("expected object entry for enum variant"))
            })?;

            let (variant_index, variant) = enum_def
                .variants
                .iter()
                .enumerate()
                .find(|(_, v)| v.effective_name() == variant_name)
                .ok_or_else(|| {
                    SerializeError::Unsupported(Cow::Owned(alloc::format!(
                        "unknown variant '{}'",
                        variant_name
                    )))
                })?;

            let payload_dyn = payload.into_dynamic_value().map_err(|_| {
                SerializeError::Internal(Cow::Borrowed("variant payload is not a dynamic value"))
            })?;

            if use_index {
                serializer
                    .begin_enum_variant(variant_index, variant.effective_name())
                    .map_err(SerializeError::Backend)?;

                // Serialize payload based on variant kind
                match variant.data.kind {
                    StructKind::Unit => {
                        // No payload to serialize
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        if variant.data.fields.len() == 1 {
                            // Newtype variant - serialize the single field directly
                            serialize_dynamic_with_shape(
                                serializer,
                                payload_dyn,
                                variant.data.fields[0].shape(),
                                value_shape,
                            )?;
                        } else {
                            // Multi-field tuple variant - expect array
                            let iter = payload_dyn.array_iter().ok_or_else(|| {
                                SerializeError::Unsupported(Cow::Borrowed(
                                    "expected array for tuple variant payload",
                                ))
                            })?;

                            for (field, elem) in variant.data.fields.iter().zip(iter) {
                                let elem_dyn = elem.into_dynamic_value().map_err(|_| {
                                    SerializeError::Internal(Cow::Borrowed(
                                        "tuple element is not a dynamic value",
                                    ))
                                })?;
                                serialize_dynamic_with_shape(
                                    serializer,
                                    elem_dyn,
                                    field.shape(),
                                    value_shape,
                                )?;
                            }
                        }
                    }
                    StructKind::Struct => {
                        // Struct variant - expect object
                        for field in variant.data.fields {
                            let field_value =
                                payload_dyn.object_get(field.name).ok_or_else(|| {
                                    SerializeError::Unsupported(Cow::Owned(alloc::format!(
                                        "missing field '{}' in struct variant",
                                        field.name
                                    )))
                                })?;
                            let field_dyn = field_value.into_dynamic_value().map_err(|_| {
                                SerializeError::Internal(Cow::Borrowed(
                                    "field value is not a dynamic value",
                                ))
                            })?;
                            serialize_dynamic_with_shape(
                                serializer,
                                field_dyn,
                                field.shape(),
                                value_shape,
                            )?;
                        }
                    }
                }

                Ok(())
            } else {
                // Externally tagged representation
                serializer.begin_struct().map_err(SerializeError::Backend)?;
                serializer
                    .field_key(variant.effective_name())
                    .map_err(SerializeError::Backend)?;

                match variant.data.kind {
                    StructKind::Unit => {
                        serializer
                            .scalar(ScalarValue::Null)
                            .map_err(SerializeError::Backend)?;
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        if variant.data.fields.len() == 1 {
                            serialize_dynamic_with_shape(
                                serializer,
                                payload_dyn,
                                variant.data.fields[0].shape(),
                                value_shape,
                            )?;
                        } else {
                            serializer.begin_seq().map_err(SerializeError::Backend)?;
                            let iter = payload_dyn.array_iter().ok_or_else(|| {
                                SerializeError::Unsupported(Cow::Borrowed(
                                    "expected array for tuple variant",
                                ))
                            })?;
                            for (field, elem) in variant.data.fields.iter().zip(iter) {
                                let elem_dyn = elem.into_dynamic_value().map_err(|_| {
                                    SerializeError::Internal(Cow::Borrowed(
                                        "element is not a dynamic value",
                                    ))
                                })?;
                                serialize_dynamic_with_shape(
                                    serializer,
                                    elem_dyn,
                                    field.shape(),
                                    value_shape,
                                )?;
                            }
                            serializer.end_seq().map_err(SerializeError::Backend)?;
                        }
                    }
                    StructKind::Struct => {
                        serializer.begin_struct().map_err(SerializeError::Backend)?;
                        for field in variant.data.fields {
                            let field_value =
                                payload_dyn.object_get(field.name).ok_or_else(|| {
                                    SerializeError::Unsupported(Cow::Owned(alloc::format!(
                                        "missing field '{}'",
                                        field.name
                                    )))
                                })?;
                            serializer
                                .field_key(field.name)
                                .map_err(SerializeError::Backend)?;
                            let field_dyn = field_value.into_dynamic_value().map_err(|_| {
                                SerializeError::Internal(Cow::Borrowed(
                                    "field is not a dynamic value",
                                ))
                            })?;
                            serialize_dynamic_with_shape(
                                serializer,
                                field_dyn,
                                field.shape(),
                                value_shape,
                            )?;
                        }
                        serializer.end_struct().map_err(SerializeError::Backend)?;
                    }
                }

                serializer.end_struct().map_err(SerializeError::Backend)
            }
        }

        // Null could be a unit variant named "Null" (untagged representation)
        DynValueKind::Null => {
            // Check if there's a Null variant or fallback for Option-like enums
            // Note: we match against the Rust name (v.name) since these are well-known Rust identifiers
            if let Some((variant_index, variant)) = enum_def
                .variants
                .iter()
                .enumerate()
                .find(|(_, v)| v.name.eq_ignore_ascii_case("null") || v.name == "None")
            {
                if use_index {
                    serializer
                        .begin_enum_variant(variant_index, variant.effective_name())
                        .map_err(SerializeError::Backend)?;
                    Ok(())
                } else {
                    serializer
                        .scalar(ScalarValue::Str(Cow::Borrowed(variant.effective_name())))
                        .map_err(SerializeError::Backend)
                }
            } else {
                Err(SerializeError::Unsupported(Cow::Borrowed(
                    "null value for enum without null/None variant",
                )))
            }
        }

        _ => {
            // For untagged enums, we might need to try matching variants
            // This is a simplified implementation - could be extended
            let _ = target_shape; // Suppress unused warning
            Err(SerializeError::Unsupported(Cow::Owned(alloc::format!(
                "unsupported dynamic value kind {:?} for enum serialization",
                dynamic.kind()
            ))))
        }
    }
}
