extern crate alloc;

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::String;
use core::fmt;

use facet_core::{
    Def, Facet, KnownPointer, NumericType, PrimitiveType, StructKind, Type, UserType,
};
use facet_reflect::{HeapValue, Partial, ReflectError, is_spanned_shape};

use crate::{FieldLocationHint, FormatParser, ParseEvent, ScalarTypeHint, ScalarValue};

/// Generic deserializer that drives a format-specific parser directly into `Partial`.
///
/// The const generic `BORROW` controls whether string data can be borrowed:
/// - `BORROW=true`: strings without escapes are borrowed from input
/// - `BORROW=false`: all strings are owned
pub struct FormatDeserializer<'input, const BORROW: bool, P> {
    parser: P,
    /// The span of the most recently consumed event (for error reporting).
    last_span: Option<facet_reflect::Span>,
    _marker: core::marker::PhantomData<&'input ()>,
}

impl<'input, P> FormatDeserializer<'input, true, P> {
    /// Create a new deserializer that can borrow strings from input.
    pub const fn new(parser: P) -> Self {
        Self {
            parser,
            last_span: None,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'input, P> FormatDeserializer<'input, false, P> {
    /// Create a new deserializer that produces owned strings.
    pub const fn new_owned(parser: P) -> Self {
        Self {
            parser,
            last_span: None,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P> {
    /// Consume the facade and return the underlying parser.
    pub fn into_inner(self) -> P {
        self.parser
    }

    /// Borrow the inner parser mutably.
    pub fn parser_mut(&mut self) -> &mut P {
        &mut self.parser
    }
}

impl<'input, P> FormatDeserializer<'input, true, P>
where
    P: FormatParser<'input>,
{
    /// Deserialize the next value in the stream into `T`, allowing borrowed strings.
    pub fn deserialize<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'input>,
    {
        let wip: Partial<'input, true> =
            Partial::alloc::<T>().map_err(DeserializeError::reflect)?;
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'input, true> =
            partial.build().map_err(DeserializeError::reflect)?;
        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::reflect)
    }

    /// Deserialize the next value in the stream into `T` (for backward compatibility).
    pub fn deserialize_root<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'input>,
    {
        self.deserialize()
    }
}

impl<'input, P> FormatDeserializer<'input, false, P>
where
    P: FormatParser<'input>,
{
    /// Deserialize the next value in the stream into `T`, using owned strings.
    pub fn deserialize<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'static>,
    {
        // SAFETY: alloc_owned produces Partial<'static, false>, but our deserializer
        // expects 'input. Since BORROW=false means we never borrow from input anyway,
        // this is safe. We also transmute the HeapValue back to 'static before materializing.
        #[allow(unsafe_code)]
        let wip: Partial<'input, false> = unsafe {
            core::mem::transmute::<Partial<'static, false>, Partial<'input, false>>(
                Partial::alloc_owned::<T>().map_err(DeserializeError::reflect)?,
            )
        };
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'input, false> =
            partial.build().map_err(DeserializeError::reflect)?;

        // SAFETY: HeapValue<'input, false> contains no borrowed data because BORROW=false.
        // The transmute only changes the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::reflect)
    }

    /// Deserialize the next value in the stream into `T` (for backward compatibility).
    pub fn deserialize_root<T>(&mut self) -> Result<T, DeserializeError<P::Error>>
    where
        T: Facet<'static>,
    {
        self.deserialize()
    }
}

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    /// Read the next event, returning an error if EOF is reached.
    #[inline]
    fn expect_event(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError<P::Error>> {
        let event = self
            .parser
            .next_event()
            .map_err(DeserializeError::Parser)?
            .ok_or(DeserializeError::UnexpectedEof { expected })?;
        // Capture the span of the consumed event for error reporting
        self.last_span = self.parser.current_span();
        Ok(event)
    }

    /// Peek at the next event, returning an error if EOF is reached.
    #[inline]
    fn expect_peek(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError<P::Error>> {
        self.parser
            .peek_event()
            .map_err(DeserializeError::Parser)?
            .ok_or(DeserializeError::UnexpectedEof { expected })
    }

    /// Main deserialization entry point - deserialize into a Partial.
    pub fn deserialize_into(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Check for raw capture type (e.g., RawJson)
        // Raw capture types are tuple structs with a single Cow<str> field
        // If capture_raw returns None (e.g., streaming mode), fall through
        // and try normal deserialization (which will likely fail with a helpful error)
        if self.parser.raw_capture_shape() == Some(shape)
            && let Some(raw) = self
                .parser
                .capture_raw()
                .map_err(DeserializeError::Parser)?
        {
            // The raw type is a tuple struct like RawJson(Cow<str>)
            // Access field 0 (the Cow<str>) and set it
            wip = wip.begin_nth_field(0).map_err(DeserializeError::reflect)?;
            wip = self.set_string_value(wip, Cow::Borrowed(raw))?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Check for container-level proxy
        let (wip_returned, has_proxy) = wip
            .begin_custom_deserialization_from_shape()
            .map_err(DeserializeError::reflect)?;
        wip = wip_returned;
        if has_proxy {
            wip = self.deserialize_into(wip)?;
            return wip.end().map_err(DeserializeError::reflect);
        }

        // Check for field-level proxy (opaque types with proxy attribute)
        if wip
            .parent_field()
            .and_then(|field| field.proxy_convert_in_fn())
            .is_some()
        {
            wip = wip
                .begin_custom_deserialization()
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Check Def first for Option
        if matches!(&shape.def, Def::Option(_)) {
            return self.deserialize_option(wip);
        }

        // Check Def for Result - treat it as a 2-variant enum
        if matches!(&shape.def, Def::Result(_)) {
            return self.deserialize_result_as_enum(wip);
        }

        // Priority 1: Check for builder_shape (immutable collections like Bytes -> BytesMut)
        if shape.builder_shape.is_some() {
            wip = wip.begin_inner().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Priority 2: Check for smart pointers (Box, Arc, Rc)
        if matches!(&shape.def, Def::Pointer(_)) {
            return self.deserialize_pointer(wip);
        }

        // Priority 3: Check for .inner (transparent wrappers like NonZero)
        // Collections (List/Map/Set/Array) have .inner for variance but shouldn't use this path
        // Opaque scalars (like ULID) may have .inner for documentation but should NOT be
        // deserialized as transparent wrappers - they use hint_opaque_scalar instead
        let is_opaque_scalar =
            matches!(shape.def, Def::Scalar) && matches!(shape.ty, Type::User(UserType::Opaque));
        if shape.inner.is_some()
            && !is_opaque_scalar
            && !matches!(
                &shape.def,
                Def::List(_) | Def::Map(_) | Def::Set(_) | Def::Array(_)
            )
        {
            wip = wip.begin_inner().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Priority 4: Check for metadata-annotated types (like Spanned<T>)
        if is_spanned_shape(shape) {
            return self.deserialize_spanned(wip);
        }

        // Priority 5: Check the Type for structs and enums
        match &shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                if matches!(struct_def.kind, StructKind::Tuple | StructKind::TupleStruct) {
                    return self.deserialize_tuple(wip);
                }
                return self.deserialize_struct(wip);
            }
            Type::User(UserType::Enum(_)) => return self.deserialize_enum(wip),
            _ => {}
        }

        // Priority 6: Check Def for containers and scalars
        match &shape.def {
            Def::Scalar => self.deserialize_scalar(wip),
            Def::List(_) => self.deserialize_list(wip),
            Def::Map(_) => self.deserialize_map(wip),
            Def::Array(_) => self.deserialize_array(wip),
            Def::Set(_) => self.deserialize_set(wip),
            Def::DynamicValue(_) => self.deserialize_dynamic_value(wip),
            _ => Err(DeserializeError::Unsupported(format!(
                "unsupported shape def: {:?}",
                shape.def
            ))),
        }
    }

    fn deserialize_option(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Hint to non-self-describing parsers that an Option is expected
        self.parser.hint_option();

        let event = self.expect_peek("value for option")?;

        if matches!(event, ParseEvent::Scalar(ScalarValue::Null)) {
            // Consume the null
            let _ = self.expect_event("null")?;
            // Set to None (default)
            wip = wip.set_default().map_err(DeserializeError::reflect)?;
        } else {
            // Some(value)
            wip = wip.begin_some().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }
        Ok(wip)
    }

    fn deserialize_result_as_enum(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_core::StructKind;

        // Hint to non-self-describing parsers that a Result enum is expected
        // Result is encoded as a 2-variant enum: Ok (index 0) and Err (index 1)
        let variant_hints: Vec<crate::EnumVariantHint> = vec![
            crate::EnumVariantHint {
                name: "Ok",
                kind: StructKind::TupleStruct,
                field_count: 1,
            },
            crate::EnumVariantHint {
                name: "Err",
                kind: StructKind::TupleStruct,
                field_count: 1,
            },
        ];
        self.parser.hint_enum(&variant_hints);

        // Read the StructStart emitted by the parser after hint_enum
        let event = self.expect_event("struct start for Result")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start for Result variant",
                got: format!("{event:?}"),
            });
        }

        // Read the FieldKey with the variant name ("Ok" or "Err")
        let key_event = self.expect_event("variant key for Result")?;
        let variant_name = match key_event {
            ParseEvent::FieldKey(key) => key.name,
            other => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "field key with variant name",
                    got: format!("{other:?}"),
                });
            }
        };

        // Select the appropriate variant and deserialize its content
        if variant_name == "Ok" {
            wip = wip.begin_ok().map_err(DeserializeError::reflect)?;
        } else if variant_name == "Err" {
            wip = wip.begin_err().map_err(DeserializeError::reflect)?;
        } else {
            return Err(DeserializeError::TypeMismatch {
                expected: "Ok or Err variant",
                got: alloc::format!("variant '{}'", variant_name),
            });
        }

        // Deserialize the variant's value (newtype pattern - single field)
        wip = self.deserialize_into(wip)?;
        wip = wip.end().map_err(DeserializeError::reflect)?;

        // Consume StructEnd
        let end_event = self.expect_event("struct end for Result")?;
        if !matches!(end_event, ParseEvent::StructEnd) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct end for Result variant",
                got: format!("{end_event:?}"),
            });
        }

        Ok(wip)
    }

    fn deserialize_pointer(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_core::KnownPointer;

        let shape = wip.shape();
        let is_cow = if let Def::Pointer(ptr_def) = shape.def {
            matches!(ptr_def.known, Some(KnownPointer::Cow))
        } else {
            false
        };

        if is_cow {
            // Cow<str> - handle specially to preserve borrowing
            if let Def::Pointer(ptr_def) = shape.def
                && let Some(pointee) = ptr_def.pointee()
                && pointee.type_identifier == "str"
            {
                // Hint to non-self-describing parsers that a string is expected
                self.parser.hint_scalar_type(ScalarTypeHint::String);
                let event = self.expect_event("string for Cow<str>")?;
                if let ParseEvent::Scalar(ScalarValue::Str(s)) = event {
                    // Pass through the Cow as-is to preserve borrowing
                    wip = wip.set(s).map_err(DeserializeError::reflect)?;
                    return Ok(wip);
                } else {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "string for Cow<str>",
                        got: format!("{event:?}"),
                    });
                }
            }
            // Cow<[u8]> - handle specially to preserve borrowing
            if let Def::Pointer(ptr_def) = shape.def
                && let Some(pointee) = ptr_def.pointee()
                && let Def::Slice(slice_def) = pointee.def
                && slice_def.t.type_identifier == "u8"
            {
                // Hint to non-self-describing parsers that bytes are expected
                self.parser.hint_scalar_type(ScalarTypeHint::Bytes);
                let event = self.expect_event("bytes for Cow<[u8]>")?;
                if let ParseEvent::Scalar(ScalarValue::Bytes(b)) = event {
                    // Pass through the Cow as-is to preserve borrowing
                    wip = wip.set(b).map_err(DeserializeError::reflect)?;
                    return Ok(wip);
                } else {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "bytes for Cow<[u8]>",
                        got: format!("{event:?}"),
                    });
                }
            }
            // Other Cow types - use begin_inner
            wip = wip.begin_inner().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // &str - handle specially for zero-copy borrowing
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && ptr_def
                .pointee()
                .is_some_and(|p| p.type_identifier == "str")
        {
            // Hint to non-self-describing parsers that a string is expected
            self.parser.hint_scalar_type(ScalarTypeHint::String);
            let event = self.expect_event("string for &str")?;
            if let ParseEvent::Scalar(ScalarValue::Str(s)) = event {
                return self.set_string_value(wip, s);
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "string for &str",
                    got: format!("{event:?}"),
                });
            }
        }

        // &[u8] - handle specially for zero-copy borrowing
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && let Some(pointee) = ptr_def.pointee()
            && let Def::Slice(slice_def) = pointee.def
            && slice_def.t.type_identifier == "u8"
        {
            // Hint to non-self-describing parsers that bytes are expected
            self.parser.hint_scalar_type(ScalarTypeHint::Bytes);
            let event = self.expect_event("bytes for &[u8]")?;
            if let ParseEvent::Scalar(ScalarValue::Bytes(b)) = event {
                return self.set_bytes_value(wip, b);
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "bytes for &[u8]",
                    got: format!("{event:?}"),
                });
            }
        }

        // Regular smart pointer (Box, Arc, Rc)
        wip = wip.begin_smart_ptr().map_err(DeserializeError::reflect)?;

        // Check if begin_smart_ptr set up a slice builder (for Arc<[T]>, Rc<[T]>, Box<[T]>)
        // In this case, we need to deserialize as a list manually
        let is_slice_builder = wip.is_building_smart_ptr_slice();

        if is_slice_builder {
            // Deserialize the list elements into the slice builder
            // We can't use deserialize_list() because it calls begin_list() which interferes
            // Hint to non-self-describing parsers that a sequence is expected
            self.parser.hint_sequence();
            let event = self.expect_event("value")?;

            // Accept either SequenceStart (JSON arrays) or StructStart (XML elements)
            // Only accept StructStart if the container kind is ambiguous (e.g., XML Element)
            let struct_mode = match event {
                ParseEvent::SequenceStart(_) => false,
                ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
                ParseEvent::StructStart(kind) => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "array",
                        got: kind.name().into(),
                    });
                }
                _ => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "sequence start for Arc<[T]>/Rc<[T]>/Box<[T]>",
                        got: format!("{event:?}"),
                    });
                }
            };

            loop {
                let event = self.expect_peek("value")?;

                // Check for end of container
                if matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                    self.expect_event("value")?;
                    break;
                }

                // In struct mode, skip FieldKey events
                if struct_mode && matches!(event, ParseEvent::FieldKey(_)) {
                    self.expect_event("value")?;
                    continue;
                }

                wip = wip.begin_list_item().map_err(DeserializeError::reflect)?;
                wip = self.deserialize_into(wip)?;
                wip = wip.end().map_err(DeserializeError::reflect)?;
            }

            // Convert the slice builder to Arc/Rc/Box and mark as initialized
            wip = wip.end().map_err(DeserializeError::reflect)?;
            // DON'T call end() again - the caller (deserialize_struct) will do that
        } else {
            // Regular smart pointer with sized pointee
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        Ok(wip)
    }

    /// Check if a field matches the given name and namespace constraints.
    ///
    /// This implements namespace-aware field matching for XML:
    /// - Text: Match fields with xml::text attribute (name is ignored - text content goes to the field)
    /// - Attributes: Only match if explicit xml::ns matches (no ns_all inheritance per XML spec)
    /// - Elements: Match if explicit xml::ns OR ns_all matches
    /// - No constraint: Backwards compatible - match any namespace by name only
    fn field_matches_with_namespace(
        field: &facet_core::Field,
        name: &str,
        namespace: Option<&str>,
        location: FieldLocationHint,
        ns_all: Option<&str>,
    ) -> bool {
        // Special case: Text location matches fields with xml::text attribute
        // The name "_text" from the parser is ignored - we match by attribute presence
        if matches!(location, FieldLocationHint::Text) {
            return field.get_attr(Some("xml"), "text").is_some();
        }

        // Check name/alias
        let name_matches = field.name == name || field.alias.iter().any(|alias| *alias == name);

        if !name_matches {
            return false;
        }

        // Get the expected namespace for this field
        let field_xml_ns = field
            .get_attr(Some("xml"), "ns")
            .and_then(|attr| attr.get_as::<&str>().copied());

        // CRITICAL: Attributes don't inherit ns_all (per XML spec)
        let expected_ns = if matches!(location, FieldLocationHint::Attribute) {
            field_xml_ns // Attributes: only explicit xml::ns
        } else {
            field_xml_ns.or(ns_all) // Elements: xml::ns OR ns_all
        };

        // Check if namespaces match
        match (namespace, expected_ns) {
            (Some(input_ns), Some(expected)) => input_ns == expected,
            (Some(_input_ns), None) => true, // Input has namespace, field doesn't require one - match
            (None, Some(_expected)) => false, // Input has no namespace, field requires one - NO match
            (None, None) => true,             // Neither has namespace - match
        }
    }

    fn deserialize_struct(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Get struct fields for lookup
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DeserializeError::Unsupported(format!(
                    "expected struct type but got {:?}",
                    wip.shape().ty
                )));
            }
        };

        // Check if we have any flattened fields
        let has_flatten = struct_def.fields.iter().any(|f| f.is_flattened());

        if has_flatten {
            // Check if any flatten field is an enum (requires solver)
            // or if there's nested flatten (flatten inside flatten)
            let needs_solver = struct_def.fields.iter().any(|f| {
                if !f.is_flattened() {
                    return false;
                }
                // Get inner type, unwrapping Option if present
                let inner_shape = match f.shape().def {
                    Def::Option(opt) => opt.t,
                    _ => f.shape(),
                };
                match inner_shape.ty {
                    // Enum flatten needs solver
                    Type::User(UserType::Enum(_)) => true,
                    // Check for nested flatten (flatten field has its own flatten fields)
                    Type::User(UserType::Struct(inner_struct)) => inner_struct
                        .fields
                        .iter()
                        .any(|inner_f| inner_f.is_flattened()),
                    _ => false,
                }
            });

            if needs_solver {
                self.deserialize_struct_with_flatten(wip)
            } else {
                // Simple single-level flatten - use the original approach
                self.deserialize_struct_single_flatten(wip)
            }
        } else {
            self.deserialize_struct_simple(wip)
        }
    }

    /// Deserialize a struct without flattened fields (simple case).
    fn deserialize_struct_simple(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_core::Characteristic;

        // Get struct fields for lookup (needed before hint)
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DeserializeError::Unsupported(format!(
                    "expected struct type but got {:?}",
                    wip.shape().ty
                )));
            }
        };

        // Hint to non-self-describing parsers how many fields to expect
        self.parser.hint_struct_fields(struct_def.fields.len());

        // Expect StructStart
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start",
                got: format!("{event:?}"),
            });
        }

        let struct_has_default = wip.shape().has_default_attr();
        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();

        // Extract container-level default namespace (xml::ns_all) for namespace-aware matching
        let ns_all = wip
            .shape()
            .attributes
            .iter()
            .find(|attr| attr.ns == Some("xml") && attr.key == "ns_all")
            .and_then(|attr| attr.get_as::<&str>().copied());

        // Track which fields have been set
        let num_fields = struct_def.fields.len();
        let mut fields_set = alloc::vec![false; num_fields];
        let mut ordered_field_index = 0usize;

        loop {
            let event = self.expect_event("value")?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::OrderedField => {
                    // Non-self-describing formats emit OrderedField events in order
                    let idx = ordered_field_index;
                    ordered_field_index += 1;
                    if idx < num_fields {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                        fields_set[idx] = true;
                    }
                }
                ParseEvent::FieldKey(key) => {
                    // Look up field in struct fields
                    let field_info = struct_def.fields.iter().enumerate().find(|(_, f)| {
                        Self::field_matches_with_namespace(
                            f,
                            key.name.as_ref(),
                            key.namespace.as_deref(),
                            key.location,
                            ns_all,
                        )
                    });

                    if let Some((idx, _field)) = field_info {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                        fields_set[idx] = true;
                        continue;
                    }

                    if deny_unknown_fields {
                        return Err(DeserializeError::UnknownField(key.name.into_owned()));
                    } else {
                        // Unknown field - skip it
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                    }
                }
                other => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "field key or struct end",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // Apply defaults for missing fields
        for (idx, field) in struct_def.fields.iter().enumerate() {
            if fields_set[idx] {
                continue;
            }

            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(Characteristic::Default);
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default || (struct_has_default && field_type_has_default) {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else if field_is_option {
                wip = wip
                    .begin_field(field.name)
                    .map_err(DeserializeError::reflect)?;
                wip = wip.set_default().map_err(DeserializeError::reflect)?;
                wip = wip.end().map_err(DeserializeError::reflect)?;
            } else if field.should_skip_deserializing() {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "field to be present or have default",
                    got: format!("missing field '{}'", field.name),
                });
            }
        }

        Ok(wip)
    }

    /// Deserialize a struct with single-level flattened fields (original approach).
    /// This handles simple flatten cases where there's no nested flatten or enum flatten.
    fn deserialize_struct_single_flatten(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use alloc::collections::BTreeMap;
        use facet_core::Characteristic;
        use facet_reflect::Resolution;

        // Expect StructStart
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start",
                got: format!("{event:?}"),
            });
        }

        // Get struct fields for lookup
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DeserializeError::Unsupported(format!(
                    "expected struct type but got {:?}",
                    wip.shape().ty
                )));
            }
        };

        let struct_has_default = wip.shape().has_default_attr();
        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();

        // Extract container-level default namespace (xml::ns_all) for namespace-aware matching
        let ns_all = wip
            .shape()
            .attributes
            .iter()
            .find(|attr| attr.ns == Some("xml") && attr.key == "ns_all")
            .and_then(|attr| attr.get_as::<&str>().copied());

        // Track which fields have been set
        let num_fields = struct_def.fields.len();
        let mut fields_set = alloc::vec![false; num_fields];

        // Build flatten info: for each flattened field, get its inner struct fields
        // and track which inner fields have been set
        let mut flatten_info: alloc::vec::Vec<
            Option<(&'static [facet_core::Field], alloc::vec::Vec<bool>)>,
        > = alloc::vec![None; num_fields];

        // Track which fields are DynamicValue flattens (like facet_value::Value)
        let mut dynamic_value_flattens: alloc::vec::Vec<bool> = alloc::vec![false; num_fields];

        // Track field names across flattened structs to detect duplicates
        let mut flatten_field_names: BTreeMap<&str, usize> = BTreeMap::new();

        for (idx, field) in struct_def.fields.iter().enumerate() {
            if field.is_flattened() {
                // Handle Option<T> flatten by unwrapping to inner type
                let inner_shape = match field.shape().def {
                    Def::Option(opt) => opt.t,
                    _ => field.shape(),
                };

                // Check if this is a DynamicValue flatten (like facet_value::Value)
                if matches!(inner_shape.def, Def::DynamicValue(_)) {
                    dynamic_value_flattens[idx] = true;
                } else if let Type::User(UserType::Struct(inner_def)) = &inner_shape.ty {
                    let inner_fields = inner_def.fields;
                    let inner_set = alloc::vec![false; inner_fields.len()];
                    flatten_info[idx] = Some((inner_fields, inner_set));

                    // Check for duplicate field names across flattened structs
                    for inner_field in inner_fields.iter() {
                        let field_name = inner_field.rename.unwrap_or(inner_field.name);
                        if let Some(_prev_idx) = flatten_field_names.insert(field_name, idx) {
                            return Err(DeserializeError::Unsupported(format!(
                                "duplicate field `{}` in flattened structs",
                                field_name
                            )));
                        }
                    }
                }
            }
        }

        // Enter deferred mode for flatten handling
        let resolution = Resolution::new();
        wip = wip
            .begin_deferred(resolution)
            .map_err(DeserializeError::reflect)?;

        loop {
            let event = self.expect_event("value")?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key) => {
                    // First, look up field in direct struct fields (non-flattened)
                    let direct_field_info = struct_def.fields.iter().enumerate().find(|(_, f)| {
                        !f.is_flattened()
                            && Self::field_matches_with_namespace(
                                f,
                                key.name.as_ref(),
                                key.namespace.as_deref(),
                                key.location,
                                ns_all,
                            )
                    });

                    if let Some((idx, _field)) = direct_field_info {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                        fields_set[idx] = true;
                        continue;
                    }

                    // Check flattened fields for a match
                    let mut found_flatten = false;
                    for (flatten_idx, field) in struct_def.fields.iter().enumerate() {
                        if !field.is_flattened() {
                            continue;
                        }
                        if let Some((inner_fields, inner_set)) = flatten_info[flatten_idx].as_mut()
                        {
                            let inner_match = inner_fields.iter().enumerate().find(|(_, f)| {
                                Self::field_matches_with_namespace(
                                    f,
                                    key.name.as_ref(),
                                    key.namespace.as_deref(),
                                    key.location,
                                    ns_all,
                                )
                            });

                            if let Some((inner_idx, _inner_field)) = inner_match {
                                // Check if flatten field is Option - if so, wrap in Some
                                let is_option = matches!(field.shape().def, Def::Option(_));
                                wip = wip
                                    .begin_nth_field(flatten_idx)
                                    .map_err(DeserializeError::reflect)?;
                                if is_option {
                                    wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                                }
                                wip = wip
                                    .begin_nth_field(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                                wip = self.deserialize_into(wip)?;
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                                if is_option {
                                    wip = wip.end().map_err(DeserializeError::reflect)?;
                                }
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                                inner_set[inner_idx] = true;
                                fields_set[flatten_idx] = true;
                                found_flatten = true;
                                break;
                            }
                        }
                    }

                    if found_flatten {
                        continue;
                    }

                    // Check if this unknown field should go to a DynamicValue flatten
                    let mut found_dynamic = false;
                    for (flatten_idx, _field) in struct_def.fields.iter().enumerate() {
                        if !dynamic_value_flattens[flatten_idx] {
                            continue;
                        }

                        // This is a DynamicValue flatten - insert the field into it
                        // First, ensure the DynamicValue is initialized as an object
                        let is_option =
                            matches!(struct_def.fields[flatten_idx].shape().def, Def::Option(_));

                        // Navigate to the DynamicValue field
                        if !fields_set[flatten_idx] {
                            // First time - need to initialize
                            wip = wip
                                .begin_nth_field(flatten_idx)
                                .map_err(DeserializeError::reflect)?;
                            if is_option {
                                wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                            }
                            // Initialize the DynamicValue as an object
                            wip = wip.begin_map().map_err(DeserializeError::reflect)?;
                            fields_set[flatten_idx] = true;
                        } else {
                            // Already initialized - just navigate to it
                            wip = wip
                                .begin_nth_field(flatten_idx)
                                .map_err(DeserializeError::reflect)?;
                            if is_option {
                                wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                            }
                        }

                        // Insert the key-value pair into the object
                        wip = wip
                            .begin_object_entry(key.name.as_ref())
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;

                        // Navigate back out (Note: we close the map when we're done with ALL fields, not per-field)
                        if is_option {
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        wip = wip.end().map_err(DeserializeError::reflect)?;

                        found_dynamic = true;
                        break;
                    }

                    if found_dynamic {
                        continue;
                    }

                    if deny_unknown_fields {
                        return Err(DeserializeError::UnknownField(key.name.into_owned()));
                    } else {
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                    }
                }
                other => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "field key or struct end",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // Apply defaults for missing fields
        for (idx, field) in struct_def.fields.iter().enumerate() {
            if field.is_flattened() {
                // Handle DynamicValue flattens that received no fields
                if dynamic_value_flattens[idx] && !fields_set[idx] {
                    let is_option = matches!(field.shape().def, Def::Option(_));

                    if is_option {
                        // Option<DynamicValue> with no fields -> set to None
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = wip.set_default().map_err(DeserializeError::reflect)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else {
                        // DynamicValue with no fields -> initialize as empty object
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        // Initialize as object (for DynamicValue, begin_map creates an object)
                        wip = wip.begin_map().map_err(DeserializeError::reflect)?;
                        // The map is now initialized and empty, just end the field
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    }
                    continue;
                }

                if let Some((inner_fields, inner_set)) = flatten_info[idx].as_ref() {
                    let any_inner_set = inner_set.iter().any(|&s| s);
                    let is_option = matches!(field.shape().def, Def::Option(_));

                    if any_inner_set {
                        // Some inner fields were set - apply defaults to missing ones
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        if is_option {
                            wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                        }
                        for (inner_idx, inner_field) in inner_fields.iter().enumerate() {
                            if inner_set[inner_idx] {
                                continue;
                            }
                            let inner_has_default = inner_field.has_default();
                            let inner_type_has_default =
                                inner_field.shape().is(Characteristic::Default);
                            let inner_is_option = matches!(inner_field.shape().def, Def::Option(_));

                            if inner_has_default || inner_type_has_default {
                                wip = wip
                                    .set_nth_field_to_default(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                            } else if inner_is_option {
                                wip = wip
                                    .begin_nth_field(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                                wip = wip.set_default().map_err(DeserializeError::reflect)?;
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                            } else if inner_field.should_skip_deserializing() {
                                wip = wip
                                    .set_nth_field_to_default(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                            } else {
                                return Err(DeserializeError::TypeMismatch {
                                    expected: "field to be present or have default",
                                    got: format!("missing field '{}'", inner_field.name),
                                });
                            }
                        }
                        if is_option {
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else if is_option {
                        // No inner fields set and field is Option - set to None
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = wip.set_default().map_err(DeserializeError::reflect)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else {
                        // No inner fields set - try to default the whole flattened field
                        let field_has_default = field.has_default();
                        let field_type_has_default = field.shape().is(Characteristic::Default);
                        if field_has_default || (struct_has_default && field_type_has_default) {
                            wip = wip
                                .set_nth_field_to_default(idx)
                                .map_err(DeserializeError::reflect)?;
                        } else {
                            let all_inner_can_default = inner_fields.iter().all(|f| {
                                f.has_default()
                                    || f.shape().is(Characteristic::Default)
                                    || matches!(f.shape().def, Def::Option(_))
                                    || f.should_skip_deserializing()
                            });
                            if all_inner_can_default {
                                wip = wip
                                    .begin_nth_field(idx)
                                    .map_err(DeserializeError::reflect)?;
                                for (inner_idx, inner_field) in inner_fields.iter().enumerate() {
                                    let inner_has_default = inner_field.has_default();
                                    let inner_type_has_default =
                                        inner_field.shape().is(Characteristic::Default);
                                    let inner_is_option =
                                        matches!(inner_field.shape().def, Def::Option(_));

                                    if inner_has_default || inner_type_has_default {
                                        wip = wip
                                            .set_nth_field_to_default(inner_idx)
                                            .map_err(DeserializeError::reflect)?;
                                    } else if inner_is_option {
                                        wip = wip
                                            .begin_nth_field(inner_idx)
                                            .map_err(DeserializeError::reflect)?;
                                        wip =
                                            wip.set_default().map_err(DeserializeError::reflect)?;
                                        wip = wip.end().map_err(DeserializeError::reflect)?;
                                    } else if inner_field.should_skip_deserializing() {
                                        wip = wip
                                            .set_nth_field_to_default(inner_idx)
                                            .map_err(DeserializeError::reflect)?;
                                    }
                                }
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                            } else {
                                return Err(DeserializeError::TypeMismatch {
                                    expected: "field to be present or have default",
                                    got: format!("missing flattened field '{}'", field.name),
                                });
                            }
                        }
                    }
                }
                continue;
            }

            if fields_set[idx] {
                continue;
            }

            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(Characteristic::Default);
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default || (struct_has_default && field_type_has_default) {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else if field_is_option {
                wip = wip
                    .begin_field(field.name)
                    .map_err(DeserializeError::reflect)?;
                wip = wip.set_default().map_err(DeserializeError::reflect)?;
                wip = wip.end().map_err(DeserializeError::reflect)?;
            } else if field.should_skip_deserializing() {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "field to be present or have default",
                    got: format!("missing field '{}'", field.name),
                });
            }
        }

        // Finish deferred mode
        wip = wip.finish_deferred().map_err(DeserializeError::reflect)?;

        Ok(wip)
    }

    /// Deserialize a struct with flattened fields using facet-solver.
    ///
    /// This uses the solver's Schema/Resolution to handle arbitrarily nested
    /// flatten structures by looking up the full path for each field.
    /// It also handles flattened enums by using probing to collect keys first,
    /// then using the Solver to disambiguate between resolutions.
    fn deserialize_struct_with_flatten(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use alloc::collections::BTreeSet;
        use facet_core::Characteristic;
        use facet_reflect::Resolution;
        use facet_solver::{PathSegment, Schema, Solver};

        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();

        // Build the schema for this type - this recursively expands all flatten fields
        let schema = Schema::build_auto(wip.shape())
            .map_err(|e| DeserializeError::Unsupported(format!("failed to build schema: {e}")))?;

        // Check if we have multiple resolutions (i.e., flattened enums)
        let resolutions = schema.resolutions();
        if resolutions.is_empty() {
            return Err(DeserializeError::Unsupported(
                "schema has no resolutions".into(),
            ));
        }

        // ========== PASS 1: Probe to collect all field keys ==========
        let probe = self
            .parser
            .begin_probe()
            .map_err(DeserializeError::Parser)?;
        let evidence = Self::collect_evidence(probe).map_err(DeserializeError::Parser)?;

        // Feed keys to solver to narrow down resolutions
        let mut solver = Solver::new(&schema);
        for ev in &evidence {
            solver.see_key(ev.name.clone());
        }

        // Get the resolved configuration
        let config_handle = solver
            .finish()
            .map_err(|e| DeserializeError::Unsupported(format!("solver failed: {e}")))?;
        let resolution = config_handle.resolution();

        // ========== PASS 2: Parse the struct with resolved paths ==========
        // Expect StructStart
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start",
                got: format!("{event:?}"),
            });
        }

        // Enter deferred mode for flatten handling
        let reflect_resolution = Resolution::new();
        wip = wip
            .begin_deferred(reflect_resolution)
            .map_err(DeserializeError::reflect)?;

        // Track which fields have been set (by serialized name - uses 'static str from resolution)
        let mut fields_set: BTreeSet<&'static str> = BTreeSet::new();

        // Track currently open path segments: (field_name, is_option, is_variant)
        // The is_variant flag indicates if we've selected a variant at this level
        let mut open_segments: alloc::vec::Vec<(&str, bool, bool)> = alloc::vec::Vec::new();

        loop {
            let event = self.expect_event("value")?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key) => {
                    // Look up field in the resolution
                    if let Some(field_info) = resolution.field(key.name.as_ref()) {
                        let segments = field_info.path.segments();

                        // Check if this path ends with a Variant segment (externally-tagged enum)
                        let ends_with_variant = segments
                            .last()
                            .is_some_and(|s| matches!(s, PathSegment::Variant(_, _)));

                        // Extract field names from the path (excluding trailing Variant)
                        let field_segments: alloc::vec::Vec<&str> = segments
                            .iter()
                            .filter_map(|s| match s {
                                PathSegment::Field(name) => Some(*name),
                                PathSegment::Variant(_, _) => None,
                            })
                            .collect();

                        // Find common prefix with currently open segments
                        let common_len = open_segments
                            .iter()
                            .zip(field_segments.iter())
                            .take_while(|((name, _, _), b)| *name == **b)
                            .count();

                        // Close segments that are no longer needed (in reverse order)
                        while open_segments.len() > common_len {
                            let (_, is_option, _) = open_segments.pop().unwrap();
                            if is_option {
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                            }
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }

                        // Open new segments
                        for &segment in &field_segments[common_len..] {
                            wip = wip
                                .begin_field(segment)
                                .map_err(DeserializeError::reflect)?;
                            let is_option = matches!(wip.shape().def, Def::Option(_));
                            if is_option {
                                wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                            }
                            open_segments.push((segment, is_option, false));
                        }

                        if ends_with_variant {
                            // For externally-tagged enums: select variant and deserialize content
                            if let Some(PathSegment::Variant(_, variant_name)) = segments.last() {
                                wip = wip
                                    .select_variant_named(variant_name)
                                    .map_err(DeserializeError::reflect)?;
                                // Deserialize the variant's struct content (the nested object)
                                wip = self.deserialize_variant_struct_fields(wip)?;
                            }
                        } else {
                            // Regular field: deserialize into it
                            wip = self.deserialize_into(wip)?;
                        }

                        // Close segments we just opened (we're done with this field)
                        while open_segments.len() > common_len {
                            let (_, is_option, _) = open_segments.pop().unwrap();
                            if is_option {
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                            }
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }

                        // Store the static serialized_name from the resolution
                        fields_set.insert(field_info.serialized_name);
                        continue;
                    }

                    if deny_unknown_fields {
                        return Err(DeserializeError::UnknownField(key.name.into_owned()));
                    } else {
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                    }
                }
                other => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "field key or struct end",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // Close any remaining open segments
        while let Some((_, is_option, _)) = open_segments.pop() {
            if is_option {
                wip = wip.end().map_err(DeserializeError::reflect)?;
            }
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        // Handle missing fields - apply defaults
        // Get all fields sorted by path depth (deepest first for proper default handling)
        let all_fields = resolution.deserialization_order();

        // Track which top-level flatten fields have had any sub-fields set
        let mut touched_top_fields: BTreeSet<&str> = BTreeSet::new();
        for field_name in &fields_set {
            if let Some(info) = resolution.field(field_name)
                && let Some(PathSegment::Field(top)) = info.path.segments().first()
            {
                touched_top_fields.insert(*top);
            }
        }

        for field_info in all_fields {
            if fields_set.contains(field_info.serialized_name) {
                continue;
            }

            // Skip fields that end with Variant - these are handled by enum deserialization
            let ends_with_variant = field_info
                .path
                .segments()
                .last()
                .is_some_and(|s| matches!(s, PathSegment::Variant(_, _)));
            if ends_with_variant {
                continue;
            }

            let path_segments: alloc::vec::Vec<&str> = field_info
                .path
                .segments()
                .iter()
                .filter_map(|s| match s {
                    PathSegment::Field(name) => Some(*name),
                    PathSegment::Variant(_, _) => None,
                })
                .collect();

            // Check if this field's parent was touched
            let first_segment = path_segments.first().copied();
            let parent_touched = first_segment
                .map(|s| touched_top_fields.contains(s))
                .unwrap_or(false);

            // If parent wasn't touched at all, we might default the whole parent
            // For now, handle individual field defaults
            let field_has_default = field_info.field.has_default();
            let field_type_has_default = field_info.value_shape.is(Characteristic::Default);
            let field_is_option = matches!(field_info.value_shape.def, Def::Option(_));

            if field_has_default
                || field_type_has_default
                || field_is_option
                || field_info.field.should_skip_deserializing()
            {
                // Navigate to the field and set default
                for &segment in &path_segments[..path_segments.len().saturating_sub(1)] {
                    wip = wip
                        .begin_field(segment)
                        .map_err(DeserializeError::reflect)?;
                    if matches!(wip.shape().def, Def::Option(_)) {
                        wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                    }
                }

                if let Some(&last) = path_segments.last() {
                    wip = wip.begin_field(last).map_err(DeserializeError::reflect)?;
                    wip = wip.set_default().map_err(DeserializeError::reflect)?;
                    wip = wip.end().map_err(DeserializeError::reflect)?;
                }

                // Close the path we opened
                for _ in 0..path_segments.len().saturating_sub(1) {
                    // Need to check if we're in an option
                    wip = wip.end().map_err(DeserializeError::reflect)?;
                }
            } else if !parent_touched && path_segments.len() > 1 {
                // Parent wasn't touched and field has no default - this is OK if the whole
                // parent can be defaulted (handled by deferred mode)
                continue;
            } else if field_info.required {
                return Err(DeserializeError::TypeMismatch {
                    expected: "field to be present or have default",
                    got: format!("missing field '{}'", field_info.serialized_name),
                });
            }
        }

        // Finish deferred mode
        wip = wip.finish_deferred().map_err(DeserializeError::reflect)?;

        Ok(wip)
    }

    /// Deserialize the struct fields of a variant.
    /// Expects the variant to already be selected.
    fn deserialize_variant_struct_fields(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_core::StructKind;

        let variant = wip
            .selected_variant()
            .ok_or_else(|| DeserializeError::TypeMismatch {
                expected: "selected variant",
                got: "no variant selected".into(),
            })?;

        let variant_fields = variant.data.fields;
        let kind = variant.data.kind;

        // Handle based on variant kind
        match kind {
            StructKind::TupleStruct if variant_fields.len() == 1 => {
                // Single-element tuple variant (newtype): deserialize the inner value directly
                wip = wip.begin_nth_field(0).map_err(DeserializeError::reflect)?;
                wip = self.deserialize_into(wip)?;
                wip = wip.end().map_err(DeserializeError::reflect)?;
                return Ok(wip);
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                // Multi-element tuple variant - not yet supported in this context
                return Err(DeserializeError::Unsupported(
                    "multi-element tuple variants in flatten not yet supported".into(),
                ));
            }
            StructKind::Unit => {
                // Unit variant - nothing to deserialize
                return Ok(wip);
            }
            StructKind::Struct => {
                // Struct variant - fall through to struct deserialization below
            }
        }

        // Struct variant: deserialize as a struct with named fields
        // Expect StructStart for the variant content
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start for variant content",
                got: format!("{event:?}"),
            });
        }

        // Track which fields have been set
        let num_fields = variant_fields.len();
        let mut fields_set = alloc::vec![false; num_fields];

        // Process all fields
        loop {
            let event = self.expect_event("value")?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key) => {
                    // Look up field in variant's fields
                    let field_info = variant_fields.iter().enumerate().find(|(_, f)| {
                        Self::field_matches_with_namespace(
                            f,
                            key.name.as_ref(),
                            key.namespace.as_deref(),
                            key.location,
                            None,
                        )
                    });

                    if let Some((idx, _field)) = field_info {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                        fields_set[idx] = true;
                    } else {
                        // Unknown field - skip
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                    }
                }
                other => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "field key or struct end",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // Apply defaults for missing fields
        for (idx, field) in variant_fields.iter().enumerate() {
            if fields_set[idx] {
                continue;
            }

            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(facet_core::Characteristic::Default);
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default || field_type_has_default {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else if field_is_option {
                wip = wip
                    .begin_nth_field(idx)
                    .map_err(DeserializeError::reflect)?;
                wip = wip.set_default().map_err(DeserializeError::reflect)?;
                wip = wip.end().map_err(DeserializeError::reflect)?;
            } else if field.should_skip_deserializing() {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "field to be present or have default",
                    got: format!("missing field '{}'", field.name),
                });
            }
        }

        Ok(wip)
    }

    /// Deserialize into a type with span metadata (like `Spanned<T>`).
    ///
    /// This handles structs that have:
    /// - One or more non-metadata fields (the actual values to deserialize)
    /// - A field with `#[facet(metadata = span)]` to store source location
    ///
    /// The metadata field is populated with a default span since most format parsers
    /// don't track source locations.
    fn deserialize_spanned(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Find the span metadata field and non-metadata fields
        let Type::User(UserType::Struct(struct_def)) = &shape.ty else {
            return Err(DeserializeError::Unsupported(format!(
                "expected struct with span metadata, found {}",
                shape.type_identifier
            )));
        };

        let span_field = struct_def
            .fields
            .iter()
            .find(|f| f.metadata_kind() == Some("span"))
            .ok_or_else(|| {
                DeserializeError::Unsupported(format!(
                    "expected struct with span metadata field, found {}",
                    shape.type_identifier
                ))
            })?;

        let value_fields: alloc::vec::Vec<_> = struct_def
            .fields
            .iter()
            .filter(|f| !f.is_metadata())
            .collect();

        // Deserialize all non-metadata fields transparently
        // For the common case (Spanned<T> with a single "value" field), this is just one field
        for field in value_fields {
            wip = wip
                .begin_field(field.name)
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        // Set the span metadata field to default
        // Most format parsers don't track source spans, so we use a default (unknown) span
        wip = wip
            .begin_field(span_field.name)
            .map_err(DeserializeError::reflect)?;
        wip = wip.set_default().map_err(DeserializeError::reflect)?;
        wip = wip.end().map_err(DeserializeError::reflect)?;

        Ok(wip)
    }

    fn deserialize_tuple(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Get field count for tuple hints (needed for non-self-describing formats like postcard)
        let field_count = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def.fields.len(),
            _ => 0, // Unit type or unknown - will be handled below
        };

        // Hint to non-self-describing parsers how many fields to expect
        // Tuples are like positional structs, so we use hint_struct_fields
        self.parser.hint_struct_fields(field_count);

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (for XML elements or
        // non-self-describing formats like postcard where tuples are positional structs)
        let struct_mode = match event {
            ParseEvent::SequenceStart(_) => false,
            // Ambiguous containers (XML elements) always use struct mode
            ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
            // For non-self-describing formats, StructStart(Object) is valid for tuples
            // because hint_struct_fields was called and tuples are positional structs
            ParseEvent::StructStart(_) if !self.parser.is_self_describing() => true,
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "array",
                    got: kind.name().into(),
                });
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "sequence start for tuple",
                    got: format!("{event:?}"),
                });
            }
        };

        let mut index = 0usize;
        loop {
            let event = self.expect_peek("value")?;

            // Check for end of container
            if matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                self.expect_event("value")?;
                break;
            }

            // In struct mode, skip FieldKey events
            if struct_mode && matches!(event, ParseEvent::FieldKey(_)) {
                self.expect_event("value")?;
                continue;
            }

            // Select field by index
            let field_name = alloc::string::ToString::to_string(&index);
            wip = wip
                .begin_field(&field_name)
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            index += 1;
        }

        Ok(wip)
    }

    fn deserialize_enum(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Hint to non-self-describing parsers what variant metadata to expect
        if let Type::User(UserType::Enum(enum_def)) = &shape.ty {
            let variant_hints: Vec<crate::EnumVariantHint> = enum_def
                .variants
                .iter()
                .map(|v| crate::EnumVariantHint {
                    name: v.name,
                    kind: v.data.kind,
                    field_count: v.data.fields.len(),
                })
                .collect();
            self.parser.hint_enum(&variant_hints);
        }

        // Check for different tagging modes
        let tag_attr = shape.get_tag_attr();
        let content_attr = shape.get_content_attr();
        let is_numeric = shape.is_numeric();
        let is_untagged = shape.is_untagged();

        if is_numeric {
            return self.deserialize_numeric_enum(wip);
        }

        // Determine tagging mode
        if is_untagged {
            return self.deserialize_enum_untagged(wip);
        }

        if let (Some(tag_key), Some(content_key)) = (tag_attr, content_attr) {
            // Adjacently tagged: {"t": "VariantName", "c": {...}}
            return self.deserialize_enum_adjacently_tagged(wip, tag_key, content_key);
        }

        if let Some(tag_key) = tag_attr {
            // Internally tagged: {"type": "VariantName", ...fields...}
            return self.deserialize_enum_internally_tagged(wip, tag_key);
        }

        // Externally tagged (default): {"VariantName": {...}} or just "VariantName"
        self.deserialize_enum_externally_tagged(wip)
    }

    fn deserialize_enum_externally_tagged(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.expect_peek("value")?;

        // Check for unit variant (just a string)
        if let ParseEvent::Scalar(ScalarValue::Str(variant_name)) = &event {
            self.expect_event("value")?;
            wip = wip
                .select_variant_named(variant_name)
                .map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Otherwise expect a struct { VariantName: ... }
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "string or struct for enum",
                got: format!("{event:?}"),
            });
        }

        self.expect_event("value")?; // consume StructStart

        // Get the variant name
        let event = self.expect_event("value")?;
        let variant_name = match event {
            ParseEvent::FieldKey(key) => key.name,
            other => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "variant name",
                    got: format!("{other:?}"),
                });
            }
        };

        wip = wip
            .select_variant_named(&variant_name)
            .map_err(DeserializeError::reflect)?;

        // Deserialize the variant content
        wip = self.deserialize_enum_variant_content(wip)?;

        // Consume StructEnd
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructEnd) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct end after enum variant",
                got: format!("{event:?}"),
            });
        }

        Ok(wip)
    }

    fn deserialize_enum_internally_tagged(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        tag_key: &str,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_core::Characteristic;

        // Step 1: Probe to find the tag value (handles out-of-order fields)
        let probe = self
            .parser
            .begin_probe()
            .map_err(DeserializeError::Parser)?;
        let evidence = Self::collect_evidence(probe).map_err(DeserializeError::Parser)?;

        let variant_name = Self::find_tag_value(&evidence, tag_key)
            .ok_or_else(|| DeserializeError::TypeMismatch {
                expected: "tag field in internally tagged enum",
                got: format!("missing '{tag_key}' field"),
            })?
            .to_string();

        // Step 2: Consume StructStart
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct for internally tagged enum",
                got: format!("{event:?}"),
            });
        }

        // Step 3: Select the variant
        wip = wip
            .select_variant_named(&variant_name)
            .map_err(DeserializeError::reflect)?;

        // Get the selected variant info
        let variant = wip
            .selected_variant()
            .ok_or_else(|| DeserializeError::TypeMismatch {
                expected: "selected variant",
                got: "no variant selected".into(),
            })?;

        let variant_fields = variant.data.fields;

        // Check if this is a unit variant (no fields)
        if variant_fields.is_empty() || variant.data.kind == StructKind::Unit {
            // Consume remaining fields in the object
            loop {
                let event = self.expect_event("value")?;
                match event {
                    ParseEvent::StructEnd => break,
                    ParseEvent::FieldKey(_) => {
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                    }
                    other => {
                        return Err(DeserializeError::TypeMismatch {
                            expected: "field key or struct end",
                            got: format!("{other:?}"),
                        });
                    }
                }
            }
            return Ok(wip);
        }

        // Track which fields have been set
        let num_fields = variant_fields.len();
        let mut fields_set = alloc::vec![false; num_fields];

        // Step 4: Process all fields (they can come in any order now)
        loop {
            let event = self.expect_event("value")?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key) => {
                    // Skip the tag field - already used
                    if key.name.as_ref() == tag_key {
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                        continue;
                    }

                    // Look up field in variant's fields
                    // Uses namespace-aware matching when namespace is present
                    let field_info = variant_fields.iter().enumerate().find(|(_, f)| {
                        Self::field_matches_with_namespace(
                            f,
                            key.name.as_ref(),
                            key.namespace.as_deref(),
                            key.location,
                            None, // Enums don't have ns_all
                        )
                    });

                    if let Some((idx, _field)) = field_info {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                        fields_set[idx] = true;
                    } else {
                        // Unknown field - skip
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                    }
                }
                other => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "field key or struct end",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // Apply defaults for missing fields
        for (idx, field) in variant_fields.iter().enumerate() {
            if fields_set[idx] {
                continue;
            }

            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(Characteristic::Default);
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default || field_type_has_default {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else if field_is_option {
                wip = wip
                    .begin_nth_field(idx)
                    .map_err(DeserializeError::reflect)?;
                wip = wip.set_default().map_err(DeserializeError::reflect)?;
                wip = wip.end().map_err(DeserializeError::reflect)?;
            } else if field.should_skip_deserializing() {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "field to be present or have default",
                    got: format!("missing field '{}'", field.name),
                });
            }
        }

        Ok(wip)
    }

    /// Helper to find a tag value from field evidence.
    fn find_tag_value<'a>(
        evidence: &'a [crate::FieldEvidence<'input>],
        tag_key: &str,
    ) -> Option<&'a str> {
        evidence
            .iter()
            .find(|e| e.name == tag_key)
            .and_then(|e| match &e.scalar_value {
                Some(ScalarValue::Str(s)) => Some(s.as_ref()),
                _ => None,
            })
    }

    /// Helper to collect all evidence from a probe stream.
    fn collect_evidence<S: crate::ProbeStream<'input, Error = P::Error>>(
        mut probe: S,
    ) -> Result<alloc::vec::Vec<crate::FieldEvidence<'input>>, P::Error> {
        let mut evidence = alloc::vec::Vec::new();
        while let Some(ev) = probe.next()? {
            evidence.push(ev);
        }
        Ok(evidence)
    }

    fn deserialize_enum_adjacently_tagged(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        tag_key: &str,
        content_key: &str,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Step 1: Probe to find the tag value (handles out-of-order fields)
        let probe = self
            .parser
            .begin_probe()
            .map_err(DeserializeError::Parser)?;
        let evidence = Self::collect_evidence(probe).map_err(DeserializeError::Parser)?;

        let variant_name = Self::find_tag_value(&evidence, tag_key)
            .ok_or_else(|| DeserializeError::TypeMismatch {
                expected: "tag field in adjacently tagged enum",
                got: format!("missing '{tag_key}' field"),
            })?
            .to_string();

        // Step 2: Consume StructStart
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct for adjacently tagged enum",
                got: format!("{event:?}"),
            });
        }

        // Step 3: Select the variant
        wip = wip
            .select_variant_named(&variant_name)
            .map_err(DeserializeError::reflect)?;

        // Step 4: Process fields in any order
        let mut content_seen = false;
        loop {
            let event = self.expect_event("value")?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key) => {
                    if key.name.as_ref() == tag_key {
                        // Skip the tag field - already used
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                    } else if key.name.as_ref() == content_key {
                        // Deserialize the content
                        wip = self.deserialize_enum_variant_content(wip)?;
                        content_seen = true;
                    } else {
                        // Unknown field - skip
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                    }
                }
                other => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "field key or struct end",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        // If no content field was present, it's a unit variant (already selected above)
        if !content_seen {
            // Check if the variant expects content
            let variant = wip.selected_variant();
            if let Some(v) = variant
                && v.data.kind != StructKind::Unit
                && !v.data.fields.is_empty()
            {
                return Err(DeserializeError::TypeMismatch {
                    expected: "content field for non-unit variant",
                    got: format!("missing '{content_key}' field"),
                });
            }
        }

        Ok(wip)
    }

    fn deserialize_enum_variant_content(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_core::Characteristic;

        // Get the selected variant's info
        let variant = wip
            .selected_variant()
            .ok_or_else(|| DeserializeError::TypeMismatch {
                expected: "selected variant",
                got: "no variant selected".into(),
            })?;

        let variant_kind = variant.data.kind;
        let variant_fields = variant.data.fields;

        match variant_kind {
            StructKind::Unit => {
                // Unit variant - nothing to deserialize
                // But we might have gotten here with content that should be consumed
                Ok(wip)
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                if variant_fields.len() == 1 {
                    // Newtype variant - content is the single field's value
                    wip = wip.begin_nth_field(0).map_err(DeserializeError::reflect)?;
                    wip = self.deserialize_into(wip)?;
                    wip = wip.end().map_err(DeserializeError::reflect)?;
                } else {
                    // Multi-field tuple variant - expect array or struct (for XML)
                    let event = self.expect_event("value")?;

                    // Only accept StructStart if the container kind is ambiguous (e.g., XML Element)
                    let struct_mode = match event {
                        ParseEvent::SequenceStart(_) => false,
                        ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
                        ParseEvent::StructStart(kind) => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "array",
                                got: kind.name().into(),
                            });
                        }
                        _ => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "sequence for tuple variant",
                                got: format!("{event:?}"),
                            });
                        }
                    };

                    let mut idx = 0;
                    while idx < variant_fields.len() {
                        // In struct mode, skip FieldKey events
                        if struct_mode {
                            let event = self.expect_peek("value")?;
                            if matches!(event, ParseEvent::FieldKey(_)) {
                                self.expect_event("value")?;
                                continue;
                            }
                        }

                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                        idx += 1;
                    }

                    let event = self.expect_event("value")?;
                    if !matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                        return Err(DeserializeError::TypeMismatch {
                            expected: "sequence end for tuple variant",
                            got: format!("{event:?}"),
                        });
                    }
                }
                Ok(wip)
            }
            StructKind::Struct => {
                // Struct variant - expect object with fields
                let event = self.expect_event("value")?;
                if !matches!(event, ParseEvent::StructStart(_)) {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "struct for struct variant",
                        got: format!("{event:?}"),
                    });
                }

                let num_fields = variant_fields.len();
                let mut fields_set = alloc::vec![false; num_fields];
                let mut ordered_field_index = 0usize;

                loop {
                    let event = self.expect_event("value")?;
                    match event {
                        ParseEvent::StructEnd => break,
                        ParseEvent::OrderedField => {
                            // Non-self-describing formats emit OrderedField events in order
                            let idx = ordered_field_index;
                            ordered_field_index += 1;
                            if idx < num_fields {
                                wip = wip
                                    .begin_nth_field(idx)
                                    .map_err(DeserializeError::reflect)?;
                                wip = self.deserialize_into(wip)?;
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                                fields_set[idx] = true;
                            }
                        }
                        ParseEvent::FieldKey(key) => {
                            // Uses namespace-aware matching when namespace is present
                            let field_info = variant_fields.iter().enumerate().find(|(_, f)| {
                                Self::field_matches_with_namespace(
                                    f,
                                    key.name.as_ref(),
                                    key.namespace.as_deref(),
                                    key.location,
                                    None, // Enums don't have ns_all
                                )
                            });

                            if let Some((idx, _field)) = field_info {
                                wip = wip
                                    .begin_nth_field(idx)
                                    .map_err(DeserializeError::reflect)?;
                                wip = self.deserialize_into(wip)?;
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                                fields_set[idx] = true;
                            } else {
                                // Unknown field - skip
                                self.parser.skip_value().map_err(DeserializeError::Parser)?;
                            }
                        }
                        other => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "field key, ordered field, or struct end",
                                got: format!("{other:?}"),
                            });
                        }
                    }
                }

                // Apply defaults for missing fields
                for (idx, field) in variant_fields.iter().enumerate() {
                    if fields_set[idx] {
                        continue;
                    }

                    let field_has_default = field.has_default();
                    let field_type_has_default = field.shape().is(Characteristic::Default);
                    let field_is_option = matches!(field.shape().def, Def::Option(_));

                    if field_has_default || field_type_has_default {
                        wip = wip
                            .set_nth_field_to_default(idx)
                            .map_err(DeserializeError::reflect)?;
                    } else if field_is_option {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = wip.set_default().map_err(DeserializeError::reflect)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else if field.should_skip_deserializing() {
                        wip = wip
                            .set_nth_field_to_default(idx)
                            .map_err(DeserializeError::reflect)?;
                    } else {
                        return Err(DeserializeError::TypeMismatch {
                            expected: "field to be present or have default",
                            got: format!("missing field '{}'", field.name),
                        });
                    }
                }

                Ok(wip)
            }
        }
    }

    fn deserialize_numeric_enum(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.parser.peek_event().map_err(DeserializeError::Parser)?;

        if let ParseEvent::Scalar(scalar) = event {
            wip = match scalar {
                ScalarValue::I64(scalar) => wip
                    .select_variant(scalar)
                    .map_err(DeserializeError::Reflect)?,
                ScalarValue::U64(scalar) => wip
                    .select_variant(scalar as i64)
                    .map_err(DeserializeError::Reflect)?,
                ScalarValue::Str(str_scalar) => {
                    let discrimenent =
                        str_scalar
                            .parse()
                            .map_err(|_| DeserializeError::TypeMismatch {
                                expected: "String representing an integer (i64)",
                                got: str_scalar.to_string(),
                            })?;
                    wip.select_variant(discrimenent)
                        .map_err(DeserializeError::Reflect)?
                }
                _ => {
                    return Err(DeserializeError::Unsupported(
                        "No clue how to treat this".to_string(),
                    ));
                }
            };
            self.parser.next_event().map_err(DeserializeError::Parser)?;
            Ok(wip)
        } else {
            Err(DeserializeError::Unsupported(
                "Expected integer value".to_string(),
            ))
        }

        // if let ParseEvent::Scalar(scalar) = event
        //     // Not sure if this will always be U64
        //     && let ScalarValue::U64(scalar) = scalar
        // {
        //     wip = wip
        //         .select_variant(scalar as i64)
        //         .map_err(DeserializeError::Reflect)?;

        //     self.parser.next_event().map_err(DeserializeError::Parser)?;
        //     Ok(wip)
        // } else {
        //     Err(DeserializeError::Unsupported(
        //         "Expected integer value".to_string(),
        //     ))
        // }
    }

    fn deserialize_enum_untagged(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_solver::VariantsByFormat;

        let shape = wip.shape();
        let variants_by_format = VariantsByFormat::from_shape(shape).ok_or_else(|| {
            DeserializeError::Unsupported("expected enum type for untagged".into())
        })?;

        let event = self.expect_peek("value")?;

        match &event {
            ParseEvent::Scalar(scalar) => {
                // Try unit variants for null
                if matches!(scalar, ScalarValue::Null)
                    && let Some(variant) = variants_by_format.unit_variants.first()
                {
                    wip = wip
                        .select_variant_named(variant.name)
                        .map_err(DeserializeError::reflect)?;
                    // Consume the null
                    self.expect_event("value")?;
                    return Ok(wip);
                }

                // Try unit variants for string values (match variant name)
                // This handles untagged enums with only unit variants like:
                // #[facet(untagged)] enum Color { Red, Green, Blue }
                // which deserialize from "Red", "Green", "Blue"
                if let ScalarValue::Str(s) = scalar {
                    for variant in &variants_by_format.unit_variants {
                        // Match against variant name or rename attribute
                        let variant_display_name = variant
                            .get_builtin_attr("rename")
                            .and_then(|attr| attr.get_as::<&str>().copied())
                            .unwrap_or(variant.name);
                        if s.as_ref() == variant_display_name {
                            wip = wip
                                .select_variant_named(variant.name)
                                .map_err(DeserializeError::reflect)?;
                            // Consume the string
                            self.expect_event("value")?;
                            return Ok(wip);
                        }
                    }
                }

                // Try scalar variants
                // For untagged enums, we should try to deserialize each scalar variant in order.
                // This handles both primitive scalars (String, i32, etc.) and complex types that
                // can be deserialized from scalars (e.g., enums with #[facet(rename)]).
                //
                // Note: We can't easily back track parser state, so we only try the first variant
                // that matches. For proper untagged behavior with multiple possibilities, we'd need
                // to either:
                // 1. Implement parser checkpointing/backtracking
                // 2. Use a probe to determine which variant will succeed before attempting deserialization
                //
                // For now, we prioritize variants that match primitive scalars (fast path),
                // then try other scalar variants.

                // First try variants that match primitive scalar types (fast path for String, i32, etc.)
                for (variant, inner_shape) in &variants_by_format.scalar_variants {
                    if self.scalar_matches_shape(scalar, inner_shape) {
                        wip = wip
                            .select_variant_named(variant.name)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_enum_variant_content(wip)?;
                        return Ok(wip);
                    }
                }

                // Then try other scalar variants that don't match primitive types.
                // This handles cases like newtype variants wrapping enums with #[facet(rename)]:
                //   #[facet(untagged)]
                //   enum EditionOrWorkspace {
                //       Edition(Edition),  // Edition is an enum with #[facet(rename = "2024")]
                //       Workspace(WorkspaceRef),
                //   }
                // When deserializing "2024", Edition doesn't match as a primitive scalar,
                // but it CAN be deserialized from the string via its renamed unit variants.
                for (variant, inner_shape) in &variants_by_format.scalar_variants {
                    if !self.scalar_matches_shape(scalar, inner_shape) {
                        wip = wip
                            .select_variant_named(variant.name)
                            .map_err(DeserializeError::reflect)?;
                        // Try to deserialize - if this fails, it will bubble up as an error.
                        // TODO: Implement proper variant trying with backtracking for better error messages
                        wip = self.deserialize_enum_variant_content(wip)?;
                        return Ok(wip);
                    }
                }

                Err(DeserializeError::TypeMismatch {
                    expected: "matching untagged variant for scalar",
                    got: format!("{:?}", scalar),
                })
            }
            ParseEvent::StructStart(_) => {
                // For struct input, use solve_variant for proper field-based matching
                match crate::solve_variant(shape, &mut self.parser) {
                    Ok(Some(outcome)) => {
                        // Successfully identified which variant matches based on fields
                        let resolution = outcome.resolution();
                        // For top-level untagged enum, there should be exactly one variant selection
                        let variant_name = resolution
                            .variant_selections()
                            .first()
                            .map(|vs| vs.variant_name)
                            .ok_or_else(|| {
                                DeserializeError::Unsupported(
                                    "solved resolution has no variant selection".into(),
                                )
                            })?;
                        wip = wip
                            .select_variant_named(variant_name)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_enum_variant_content(wip)?;
                        Ok(wip)
                    }
                    Ok(None) => {
                        // No variant matched - fall back to trying the first struct variant
                        // (we can't backtrack parser state to try multiple variants)
                        if let Some(variant) = variants_by_format.struct_variants.first() {
                            wip = wip
                                .select_variant_named(variant.name)
                                .map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_enum_variant_content(wip)?;
                            Ok(wip)
                        } else {
                            Err(DeserializeError::Unsupported(
                                "no struct variant found for untagged enum with struct input"
                                    .into(),
                            ))
                        }
                    }
                    Err(_) => Err(DeserializeError::Unsupported(
                        "failed to solve variant for untagged enum".into(),
                    )),
                }
            }
            ParseEvent::SequenceStart(_) => {
                // For sequence input, use first tuple variant
                if let Some((variant, _arity)) = variants_by_format.tuple_variants.first() {
                    wip = wip
                        .select_variant_named(variant.name)
                        .map_err(DeserializeError::reflect)?;
                    wip = self.deserialize_enum_variant_content(wip)?;
                    return Ok(wip);
                }

                Err(DeserializeError::Unsupported(
                    "no tuple variant found for untagged enum with sequence input".into(),
                ))
            }
            _ => Err(DeserializeError::TypeMismatch {
                expected: "scalar, struct, or sequence for untagged enum",
                got: format!("{:?}", event),
            }),
        }
    }

    fn scalar_matches_shape(
        &self,
        scalar: &ScalarValue<'input>,
        shape: &'static facet_core::Shape,
    ) -> bool {
        use facet_core::ScalarType;

        let Some(scalar_type) = shape.scalar_type() else {
            // Not a scalar type - check for Option wrapping null
            if matches!(scalar, ScalarValue::Null) {
                return matches!(shape.def, Def::Option(_));
            }
            return false;
        };

        match scalar {
            ScalarValue::Bool(_) => matches!(scalar_type, ScalarType::Bool),
            ScalarValue::I64(val) => {
                // I64 matches signed types directly
                if matches!(
                    scalar_type,
                    ScalarType::I8
                        | ScalarType::I16
                        | ScalarType::I32
                        | ScalarType::I64
                        | ScalarType::I128
                        | ScalarType::ISize
                ) {
                    return true;
                }

                // I64 can also match unsigned types if the value is non-negative and in range
                // This handles TOML's requirement to represent all integers as i64
                if *val >= 0 {
                    let uval = *val as u64;
                    match scalar_type {
                        ScalarType::U8 => uval <= u8::MAX as u64,
                        ScalarType::U16 => uval <= u16::MAX as u64,
                        ScalarType::U32 => uval <= u32::MAX as u64,
                        ScalarType::U64 | ScalarType::U128 | ScalarType::USize => true,
                        _ => false,
                    }
                } else {
                    false
                }
            }
            ScalarValue::U64(_) => matches!(
                scalar_type,
                ScalarType::U8
                    | ScalarType::U16
                    | ScalarType::U32
                    | ScalarType::U64
                    | ScalarType::U128
                    | ScalarType::USize
            ),
            ScalarValue::U128(_) => matches!(scalar_type, ScalarType::U128 | ScalarType::I128),
            ScalarValue::I128(_) => matches!(scalar_type, ScalarType::I128 | ScalarType::U128),
            ScalarValue::F64(_) => matches!(scalar_type, ScalarType::F32 | ScalarType::F64),
            ScalarValue::Str(_) => matches!(
                scalar_type,
                ScalarType::String | ScalarType::Str | ScalarType::CowStr | ScalarType::Char
            ),
            ScalarValue::Bytes(_) => {
                // Bytes don't have a ScalarType - would need to check for Vec<u8> or [u8]
                false
            }
            ScalarValue::Null => {
                // Null matches Unit type
                matches!(scalar_type, ScalarType::Unit)
            }
        }
    }

    fn deserialize_list(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Hint to non-self-describing parsers that a sequence is expected
        self.parser.hint_sequence();

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (XML elements)
        // In struct mode, we skip FieldKey events and treat values as sequence items
        // Only accept StructStart if the container kind is ambiguous (e.g., XML Element)
        let struct_mode = match event {
            ParseEvent::SequenceStart(_) => false,
            ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "array",
                    got: kind.name().into(),
                });
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "sequence start",
                    got: format!("{event:?}"),
                });
            }
        };

        // Initialize the list
        wip = wip.begin_list().map_err(DeserializeError::reflect)?;

        loop {
            let event = self.expect_peek("value")?;

            // Check for end of container
            if matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                self.expect_event("value")?;
                break;
            }

            // In struct mode, skip FieldKey events (they're just labels for items)
            if struct_mode && matches!(event, ParseEvent::FieldKey(_)) {
                self.expect_event("value")?;
                continue;
            }

            wip = wip.begin_list_item().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        Ok(wip)
    }

    fn deserialize_array(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Get the fixed array length from the type definition
        let array_len = match &wip.shape().def {
            Def::Array(array_def) => array_def.n,
            _ => {
                return Err(DeserializeError::Unsupported(
                    "deserialize_array called on non-array type".into(),
                ));
            }
        };

        // Hint to non-self-describing parsers that a fixed-size array is expected
        // (unlike hint_sequence, this doesn't read a length prefix)
        self.parser.hint_array(array_len);

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (XML elements)
        // Only accept StructStart if the container kind is ambiguous (e.g., XML Element)
        let struct_mode = match event {
            ParseEvent::SequenceStart(_) => false,
            ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "array",
                    got: kind.name().into(),
                });
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "sequence start for array",
                    got: format!("{event:?}"),
                });
            }
        };

        // Transition to Array tracker state. This is important for empty arrays
        // like [u8; 0] which have no elements to initialize but still need
        // their tracker state set correctly for require_full_initialization to pass.
        wip = wip.begin_array().map_err(DeserializeError::reflect)?;

        let mut index = 0usize;
        loop {
            let event = self.expect_peek("value")?;

            // Check for end of container
            if matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                self.expect_event("value")?;
                break;
            }

            // In struct mode, skip FieldKey events
            if struct_mode && matches!(event, ParseEvent::FieldKey(_)) {
                self.expect_event("value")?;
                continue;
            }

            wip = wip
                .begin_nth_field(index)
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            index += 1;
        }

        Ok(wip)
    }

    fn deserialize_set(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Hint to non-self-describing parsers that a sequence is expected
        self.parser.hint_sequence();

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (XML elements)
        // Only accept StructStart if the container kind is ambiguous (e.g., XML Element)
        let struct_mode = match event {
            ParseEvent::SequenceStart(_) => false,
            ParseEvent::StructStart(kind) if kind.is_ambiguous() => true,
            ParseEvent::StructStart(kind) => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "array",
                    got: kind.name().into(),
                });
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "sequence start for set",
                    got: format!("{event:?}"),
                });
            }
        };

        // Initialize the set
        wip = wip.begin_set().map_err(DeserializeError::reflect)?;

        loop {
            let event = self.expect_peek("value")?;

            // Check for end of container
            if matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                self.expect_event("value")?;
                break;
            }

            // In struct mode, skip FieldKey events
            if struct_mode && matches!(event, ParseEvent::FieldKey(_)) {
                self.expect_event("value")?;
                continue;
            }

            wip = wip.begin_set_item().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        Ok(wip)
    }

    fn deserialize_map(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // For non-self-describing formats, hint that a map is expected
        self.parser.hint_map();

        let event = self.expect_event("value")?;

        // Initialize the map
        wip = wip.begin_map().map_err(DeserializeError::reflect)?;

        // Handle both self-describing (StructStart) and non-self-describing (SequenceStart) formats
        match event {
            ParseEvent::StructStart(_) => {
                // Self-describing format (e.g., JSON): maps are represented as objects
                loop {
                    let event = self.expect_event("value")?;
                    match event {
                        ParseEvent::StructEnd => break,
                        ParseEvent::FieldKey(key) => {
                            // Begin key
                            wip = wip.begin_key().map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_map_key(wip, key.name)?;
                            wip = wip.end().map_err(DeserializeError::reflect)?;

                            // Begin value
                            wip = wip.begin_value().map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        other => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "field key or struct end for map",
                                got: format!("{other:?}"),
                            });
                        }
                    }
                }
            }
            ParseEvent::SequenceStart(_) => {
                // Non-self-describing format (e.g., postcard): maps are sequences of key-value pairs
                loop {
                    let event = self.expect_peek("value")?;
                    match event {
                        ParseEvent::SequenceEnd => {
                            self.expect_event("value")?;
                            break;
                        }
                        ParseEvent::OrderedField => {
                            self.expect_event("value")?;

                            // Deserialize key
                            wip = wip.begin_key().map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end().map_err(DeserializeError::reflect)?;

                            // Deserialize value
                            wip = wip.begin_value().map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_into(wip)?;
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        other => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "ordered field or sequence end for map",
                                got: format!("{other:?}"),
                            });
                        }
                    }
                }
            }
            other => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "struct start or sequence start for map",
                    got: format!("{other:?}"),
                });
            }
        }

        Ok(wip)
    }

    fn deserialize_scalar(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Hint to non-self-describing parsers what scalar type is expected
        let shape = wip.shape();

        // First, try hint_opaque_scalar for types that may have format-specific
        // binary representations (e.g., UUID as 16 raw bytes in postcard)
        let opaque_handled = match shape.type_identifier {
            // Standard primitives are never opaque
            "bool" | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32"
            | "i64" | "i128" | "isize" | "f32" | "f64" | "String" | "&str" | "char" => false,
            // For all other scalar types, ask the parser if it handles them specially
            _ => self.parser.hint_opaque_scalar(shape.type_identifier, shape),
        };

        // If the parser didn't handle the opaque type, fall back to standard hints
        if !opaque_handled {
            let hint = match shape.type_identifier {
                "bool" => Some(ScalarTypeHint::Bool),
                "u8" => Some(ScalarTypeHint::U8),
                "u16" => Some(ScalarTypeHint::U16),
                "u32" => Some(ScalarTypeHint::U32),
                "u64" => Some(ScalarTypeHint::U64),
                "u128" => Some(ScalarTypeHint::U128),
                "usize" => Some(ScalarTypeHint::Usize),
                "i8" => Some(ScalarTypeHint::I8),
                "i16" => Some(ScalarTypeHint::I16),
                "i32" => Some(ScalarTypeHint::I32),
                "i64" => Some(ScalarTypeHint::I64),
                "i128" => Some(ScalarTypeHint::I128),
                "isize" => Some(ScalarTypeHint::Isize),
                "f32" => Some(ScalarTypeHint::F32),
                "f64" => Some(ScalarTypeHint::F64),
                "String" | "&str" => Some(ScalarTypeHint::String),
                "char" => Some(ScalarTypeHint::Char),
                // For unknown scalar types, check if they implement FromStr
                // (e.g., camino::Utf8PathBuf, types not handled by hint_opaque_scalar)
                _ if shape.is_from_str() => Some(ScalarTypeHint::String),
                _ => None,
            };
            if let Some(hint) = hint {
                self.parser.hint_scalar_type(hint);
            }
        }

        let event = self.expect_event("value")?;

        match event {
            ParseEvent::Scalar(scalar) => {
                wip = self.set_scalar(wip, scalar)?;
                Ok(wip)
            }
            other => Err(DeserializeError::TypeMismatch {
                expected: "scalar value",
                got: format!("{other:?}"),
            }),
        }
    }

    fn set_scalar(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        scalar: ScalarValue<'input>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();
        // Capture the span for error reporting - this is where the scalar value was parsed
        let span = self.last_span;
        let reflect_err = |e: ReflectError| DeserializeError::Reflect { error: e, span };

        match scalar {
            ScalarValue::Null => {
                wip = wip.set_default().map_err(&reflect_err)?;
            }
            ScalarValue::Bool(b) => {
                wip = wip.set(b).map_err(&reflect_err)?;
            }
            ScalarValue::I64(n) => {
                // Handle signed types
                if shape.type_identifier == "i8" {
                    wip = wip.set(n as i8).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i16" {
                    wip = wip.set(n as i16).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i32" {
                    wip = wip.set(n as i32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i64" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i128" {
                    wip = wip.set(n as i128).map_err(&reflect_err)?;
                } else if shape.type_identifier == "isize" {
                    wip = wip.set(n as isize).map_err(&reflect_err)?;
                // Handle unsigned types (I64 can fit in unsigned if non-negative)
                } else if shape.type_identifier == "u8" {
                    wip = wip.set(n as u8).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u16" {
                    wip = wip.set(n as u16).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u32" {
                    wip = wip.set(n as u32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u64" {
                    wip = wip.set(n as u64).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u128" {
                    wip = wip.set(n as u128).map_err(&reflect_err)?;
                } else if shape.type_identifier == "usize" {
                    wip = wip.set(n as usize).map_err(&reflect_err)?;
                // Handle floats
                } else if shape.type_identifier == "f32" {
                    wip = wip.set(n as f32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "f64" {
                    wip = wip.set(n as f64).map_err(&reflect_err)?;
                // Handle String - stringify the number
                } else if shape.type_identifier == "String" {
                    wip = wip
                        .set(alloc::string::ToString::to_string(&n))
                        .map_err(&reflect_err)?;
                } else {
                    wip = wip.set(n).map_err(&reflect_err)?;
                }
            }
            ScalarValue::U64(n) => {
                // Handle unsigned types
                if shape.type_identifier == "u8" {
                    wip = wip.set(n as u8).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u16" {
                    wip = wip.set(n as u16).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u32" {
                    wip = wip.set(n as u32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u64" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u128" {
                    wip = wip.set(n as u128).map_err(&reflect_err)?;
                } else if shape.type_identifier == "usize" {
                    wip = wip.set(n as usize).map_err(&reflect_err)?;
                // Handle signed types (U64 can fit in signed if small enough)
                } else if shape.type_identifier == "i8" {
                    wip = wip.set(n as i8).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i16" {
                    wip = wip.set(n as i16).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i32" {
                    wip = wip.set(n as i32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i64" {
                    wip = wip.set(n as i64).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i128" {
                    wip = wip.set(n as i128).map_err(&reflect_err)?;
                } else if shape.type_identifier == "isize" {
                    wip = wip.set(n as isize).map_err(&reflect_err)?;
                // Handle floats
                } else if shape.type_identifier == "f32" {
                    wip = wip.set(n as f32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "f64" {
                    wip = wip.set(n as f64).map_err(&reflect_err)?;
                // Handle String - stringify the number
                } else if shape.type_identifier == "String" {
                    wip = wip
                        .set(alloc::string::ToString::to_string(&n))
                        .map_err(&reflect_err)?;
                } else {
                    wip = wip.set(n).map_err(&reflect_err)?;
                }
            }
            ScalarValue::U128(n) => {
                // Handle u128 scalar
                if shape.type_identifier == "u128" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.type_identifier == "i128" {
                    wip = wip.set(n as i128).map_err(&reflect_err)?;
                } else {
                    // For smaller types, truncate (caller should have used correct hint)
                    wip = wip.set(n as u64).map_err(&reflect_err)?;
                }
            }
            ScalarValue::I128(n) => {
                // Handle i128 scalar
                if shape.type_identifier == "i128" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.type_identifier == "u128" {
                    wip = wip.set(n as u128).map_err(&reflect_err)?;
                } else {
                    // For smaller types, truncate (caller should have used correct hint)
                    wip = wip.set(n as i64).map_err(&reflect_err)?;
                }
            }
            ScalarValue::F64(n) => {
                if shape.type_identifier == "f32" {
                    wip = wip.set(n as f32).map_err(&reflect_err)?;
                } else if shape.type_identifier == "f64" {
                    wip = wip.set(n).map_err(&reflect_err)?;
                } else if shape.vtable.has_try_from() && shape.inner.is_some() {
                    // For opaque types with try_from (like NotNan, OrderedFloat), use
                    // begin_inner() + set + end() to trigger conversion
                    let inner_shape = shape.inner.unwrap();
                    wip = wip.begin_inner().map_err(&reflect_err)?;
                    if inner_shape.is_type::<f32>() {
                        wip = wip.set(n as f32).map_err(&reflect_err)?;
                    } else {
                        wip = wip.set(n).map_err(&reflect_err)?;
                    }
                    wip = wip.end().map_err(&reflect_err)?;
                } else {
                    wip = wip.set(n).map_err(&reflect_err)?;
                }
            }
            ScalarValue::Str(s) => {
                // Try parse_from_str first if the type supports it
                if shape.vtable.has_parse() {
                    wip = wip.parse_from_str(s.as_ref()).map_err(&reflect_err)?;
                } else {
                    wip = self.set_string_value(wip, s)?;
                }
            }
            ScalarValue::Bytes(b) => {
                // First try parse_from_bytes if the type supports it (e.g., UUID from 16 bytes)
                if shape.vtable.has_parse_bytes() {
                    wip = wip.parse_from_bytes(b.as_ref()).map_err(&reflect_err)?;
                } else {
                    // Fall back to setting as Vec<u8>
                    wip = wip.set(b.into_owned()).map_err(&reflect_err)?;
                }
            }
        }

        Ok(wip)
    }

    /// Set a string value, handling `&str`, `Cow<str>`, and `String` appropriately.
    fn set_string_value(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        s: Cow<'input, str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Check if target is &str (shared reference to str)
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && ptr_def
                .pointee()
                .is_some_and(|p| p.type_identifier == "str")
        {
            // In owned mode, we cannot borrow from input at all
            if !BORROW {
                return Err(DeserializeError::CannotBorrow {
                    message: "cannot deserialize into &str when borrowing is disabled - use String or Cow<str> instead".into(),
                });
            }
            match s {
                Cow::Borrowed(borrowed) => {
                    wip = wip.set(borrowed).map_err(DeserializeError::reflect)?;
                    return Ok(wip);
                }
                Cow::Owned(_) => {
                    return Err(DeserializeError::CannotBorrow {
                        message: "cannot borrow &str from string containing escape sequences - use String or Cow<str> instead".into(),
                    });
                }
            }
        }

        // Check if target is Cow<str>
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::Cow))
            && ptr_def
                .pointee()
                .is_some_and(|p| p.type_identifier == "str")
        {
            wip = wip.set(s).map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Default: convert to owned String
        wip = wip.set(s.into_owned()).map_err(DeserializeError::reflect)?;
        Ok(wip)
    }

    /// Set a bytes value with proper handling for borrowed vs owned data.
    ///
    /// This handles `&[u8]`, `Cow<[u8]>`, and `Vec<u8>` appropriately based on
    /// whether borrowing is enabled and whether the data is borrowed or owned.
    fn set_bytes_value(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        b: Cow<'input, [u8]>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Helper to check if a shape is a byte slice ([u8])
        let is_byte_slice = |pointee: &facet_core::Shape| matches!(pointee.def, Def::Slice(slice_def) if slice_def.t.type_identifier == "u8");

        // Check if target is &[u8] (shared reference to byte slice)
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::SharedReference))
            && ptr_def.pointee().is_some_and(is_byte_slice)
        {
            // In owned mode, we cannot borrow from input at all
            if !BORROW {
                return Err(DeserializeError::CannotBorrow {
                    message: "cannot deserialize into &[u8] when borrowing is disabled - use Vec<u8> or Cow<[u8]> instead".into(),
                });
            }
            match b {
                Cow::Borrowed(borrowed) => {
                    wip = wip.set(borrowed).map_err(DeserializeError::reflect)?;
                    return Ok(wip);
                }
                Cow::Owned(_) => {
                    return Err(DeserializeError::CannotBorrow {
                        message: "cannot borrow &[u8] from owned bytes - use Vec<u8> or Cow<[u8]> instead".into(),
                    });
                }
            }
        }

        // Check if target is Cow<[u8]>
        if let Def::Pointer(ptr_def) = shape.def
            && matches!(ptr_def.known, Some(KnownPointer::Cow))
            && ptr_def.pointee().is_some_and(is_byte_slice)
        {
            wip = wip.set(b).map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Default: convert to owned Vec<u8>
        wip = wip.set(b.into_owned()).map_err(DeserializeError::reflect)?;
        Ok(wip)
    }

    /// Deserialize a map key from a string.
    ///
    /// Format parsers typically emit string keys, but the target map might have non-string key types
    /// (e.g., integers, enums). This function parses the string key into the appropriate type:
    /// - String types: set directly
    /// - Enum unit variants: use select_variant_named
    /// - Integer types: parse the string as a number
    /// - Transparent newtypes: descend into the inner type
    fn deserialize_map_key(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        key: Cow<'input, str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // For transparent types (like UserId(String)), we need to use begin_inner
        // to set the inner value. But NOT for pointer types like &str or Cow<str>
        // which are handled directly.
        let is_pointer = matches!(shape.def, Def::Pointer(_));
        if shape.inner.is_some() && !is_pointer {
            wip = wip.begin_inner().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_map_key(wip, key)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Check if target is an enum - use select_variant_named for unit variants
        if let Type::User(UserType::Enum(_)) = &shape.ty {
            wip = wip
                .select_variant_named(&key)
                .map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Check if target is a numeric type - parse the string key as a number
        if let Type::Primitive(PrimitiveType::Numeric(num_ty)) = &shape.ty {
            match num_ty {
                NumericType::Integer { signed } => {
                    if *signed {
                        let n: i64 = key.parse().map_err(|_| DeserializeError::TypeMismatch {
                            expected: "valid integer for map key",
                            got: format!("string '{}'", key),
                        })?;
                        // Use set for each size - the Partial handles type conversion
                        wip = wip.set(n).map_err(DeserializeError::reflect)?;
                    } else {
                        let n: u64 = key.parse().map_err(|_| DeserializeError::TypeMismatch {
                            expected: "valid unsigned integer for map key",
                            got: format!("string '{}'", key),
                        })?;
                        wip = wip.set(n).map_err(DeserializeError::reflect)?;
                    }
                    return Ok(wip);
                }
                NumericType::Float => {
                    let n: f64 = key.parse().map_err(|_| DeserializeError::TypeMismatch {
                        expected: "valid float for map key",
                        got: format!("string '{}'", key),
                    })?;
                    wip = wip.set(n).map_err(DeserializeError::reflect)?;
                    return Ok(wip);
                }
            }
        }

        // Default: treat as string
        wip = self.set_string_value(wip, key)?;
        Ok(wip)
    }

    /// Deserialize any value into a DynamicValue type (e.g., facet_value::Value).
    ///
    /// This handles all value types by inspecting the parse events and calling
    /// the appropriate methods on the Partial, which delegates to the DynamicValue vtable.
    fn deserialize_dynamic_value(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.expect_peek("value for dynamic value")?;

        match event {
            ParseEvent::Scalar(_) => {
                // Consume the scalar
                let event = self.expect_event("scalar")?;
                if let ParseEvent::Scalar(scalar) = event {
                    // Use set_scalar which already handles all scalar types
                    wip = self.set_scalar(wip, scalar)?;
                }
            }
            ParseEvent::SequenceStart(_) => {
                // Array/list
                self.expect_event("sequence start")?; // consume '['
                wip = wip.begin_list().map_err(DeserializeError::reflect)?;

                loop {
                    let event = self.expect_peek("value or end")?;
                    if matches!(event, ParseEvent::SequenceEnd) {
                        self.expect_event("sequence end")?;
                        break;
                    }

                    wip = wip.begin_list_item().map_err(DeserializeError::reflect)?;
                    wip = self.deserialize_dynamic_value(wip)?;
                    wip = wip.end().map_err(DeserializeError::reflect)?;
                }
            }
            ParseEvent::StructStart(_) => {
                // Object/map/table
                self.expect_event("struct start")?; // consume '{'
                wip = wip.begin_map().map_err(DeserializeError::reflect)?;

                loop {
                    let event = self.expect_peek("field key or end")?;
                    if matches!(event, ParseEvent::StructEnd) {
                        self.expect_event("struct end")?;
                        break;
                    }

                    // Parse the key
                    let key_event = self.expect_event("field key")?;
                    let key = match key_event {
                        ParseEvent::FieldKey(field_key) => field_key.name.into_owned(),
                        _ => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "field key",
                                got: format!("{:?}", key_event),
                            });
                        }
                    };

                    // Begin the object entry and deserialize the value
                    wip = wip
                        .begin_object_entry(&key)
                        .map_err(DeserializeError::reflect)?;
                    wip = self.deserialize_dynamic_value(wip)?;
                    wip = wip.end().map_err(DeserializeError::reflect)?;
                }
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "scalar, sequence, or struct",
                    got: format!("{:?}", event),
                });
            }
        }

        Ok(wip)
    }
}

/// Error produced by [`FormatDeserializer`].
#[derive(Debug)]
pub enum DeserializeError<E> {
    /// Error emitted by the format-specific parser.
    Parser(E),
    /// Reflection error from Partial operations.
    Reflect {
        /// The underlying reflection error.
        error: ReflectError,
        /// Source span where the error occurred (if available).
        span: Option<facet_reflect::Span>,
    },
    /// Type mismatch during deserialization.
    TypeMismatch {
        /// The expected type or token.
        expected: &'static str,
        /// The actual type or token that was encountered.
        got: String,
    },
    /// Unsupported type or operation.
    Unsupported(String),
    /// Unknown field encountered when deny_unknown_fields is set.
    UnknownField(String),
    /// Cannot borrow string from input (e.g., escaped string into &str).
    CannotBorrow {
        /// Description of why borrowing failed.
        message: String,
    },
    /// Required field missing from input.
    MissingField {
        /// The field that is missing.
        field: &'static str,
        /// The type that contains the field.
        type_name: &'static str,
    },
    /// Unexpected end of input.
    UnexpectedEof {
        /// What was expected before EOF.
        expected: &'static str,
    },
}

impl<E: fmt::Display> fmt::Display for DeserializeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::Parser(err) => write!(f, "{err}"),
            DeserializeError::Reflect { error, .. } => write!(f, "reflection error: {error}"),
            DeserializeError::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            DeserializeError::Unsupported(msg) => write!(f, "unsupported: {msg}"),
            DeserializeError::UnknownField(field) => write!(f, "unknown field: {field}"),
            DeserializeError::CannotBorrow { message } => write!(f, "{message}"),
            DeserializeError::MissingField { field, type_name } => {
                write!(f, "missing field `{field}` in type `{type_name}`")
            }
            DeserializeError::UnexpectedEof { expected } => {
                write!(f, "unexpected end of input, expected {expected}")
            }
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for DeserializeError<E> {}

impl<E> DeserializeError<E> {
    /// Create a Reflect error without span information.
    #[inline]
    pub fn reflect(error: ReflectError) -> Self {
        DeserializeError::Reflect { error, span: None }
    }

    /// Create a Reflect error with span information.
    #[inline]
    pub fn reflect_with_span(error: ReflectError, span: facet_reflect::Span) -> Self {
        DeserializeError::Reflect {
            error,
            span: Some(span),
        }
    }
}
