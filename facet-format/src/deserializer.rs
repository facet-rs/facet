extern crate alloc;

use alloc::borrow::Cow;
use alloc::format;
use alloc::string::String;
use core::fmt;

use facet_core::{Def, Facet, StructKind, Type, UserType};
use facet_reflect::{HeapValue, Partial, ReflectError};

use crate::{FormatParser, ParseEvent, ScalarValue};

/// Generic deserializer that drives a format-specific parser directly into `Partial`.
///
/// The const generic `BORROW` controls whether string data can be borrowed:
/// - `BORROW=true`: strings without escapes are borrowed from input
/// - `BORROW=false`: all strings are owned
pub struct FormatDeserializer<'input, const BORROW: bool, P> {
    parser: P,
    _marker: core::marker::PhantomData<&'input ()>,
}

impl<'input, P> FormatDeserializer<'input, true, P> {
    /// Create a new deserializer that can borrow strings from input.
    pub const fn new(parser: P) -> Self {
        Self {
            parser,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'input, P> FormatDeserializer<'input, false, P> {
    /// Create a new deserializer that produces owned strings.
    pub const fn new_owned(parser: P) -> Self {
        Self {
            parser,
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
            Partial::alloc::<T>().map_err(DeserializeError::Reflect)?;
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'input, true> =
            partial.build().map_err(DeserializeError::Reflect)?;
        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::Reflect)
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
                Partial::alloc_owned::<T>().map_err(DeserializeError::Reflect)?,
            )
        };
        let partial = self.deserialize_into(wip)?;
        let heap_value: HeapValue<'input, false> =
            partial.build().map_err(DeserializeError::Reflect)?;

        // SAFETY: HeapValue<'input, false> contains no borrowed data because BORROW=false.
        // The transmute only changes the phantom lifetime marker.
        #[allow(unsafe_code)]
        let heap_value: HeapValue<'static, false> = unsafe {
            core::mem::transmute::<HeapValue<'input, false>, HeapValue<'static, false>>(heap_value)
        };

        heap_value
            .materialize::<T>()
            .map_err(DeserializeError::Reflect)
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
    /// Main deserialization entry point - deserialize into a Partial.
    pub fn deserialize_into(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Check for container-level proxy
        let (wip_returned, has_proxy) = wip
            .begin_custom_deserialization_from_shape()
            .map_err(DeserializeError::Reflect)?;
        wip = wip_returned;
        if has_proxy {
            wip = self.deserialize_into(wip)?;
            return wip.end().map_err(DeserializeError::Reflect);
        }

        // Check Def first for Option
        if matches!(&shape.def, Def::Option(_)) {
            return self.deserialize_option(wip);
        }

        // Check for smart pointers (Box, Arc, Rc)
        if matches!(&shape.def, Def::Pointer(_)) {
            return self.deserialize_pointer(wip);
        }

        // Check for transparent/inner wrapper types
        if shape.inner.is_some() {
            wip = wip.begin_inner().map_err(DeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::Reflect)?;
            return Ok(wip);
        }

        // Check the Type for structs and enums
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

        // Check Def for containers and scalars
        match &shape.def {
            Def::Scalar => self.deserialize_scalar(wip),
            Def::List(_) => self.deserialize_list(wip),
            Def::Map(_) => self.deserialize_map(wip),
            Def::Array(_) => self.deserialize_array(wip),
            Def::Set(_) => self.deserialize_set(wip),
            other => Err(DeserializeError::Unsupported(format!(
                "unsupported shape def: {other:?}"
            ))),
        }
    }

    fn deserialize_option(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.parser.peek_event().map_err(DeserializeError::Parser)?;

        if matches!(event, ParseEvent::Scalar(ScalarValue::Null)) {
            // Consume the null
            self.parser.next_event().map_err(DeserializeError::Parser)?;
            // Set to None (default)
            wip = wip.set_default().map_err(DeserializeError::Reflect)?;
        } else {
            // Some(value)
            wip = wip.begin_some().map_err(DeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::Reflect)?;
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
            // Cow<str> - handle specially
            if let Def::Pointer(ptr_def) = shape.def
                && let Some(pointee) = ptr_def.pointee()
                && pointee.type_identifier == "str"
            {
                let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
                if let ParseEvent::Scalar(ScalarValue::Str(s)) = event {
                    let cow: Cow<'static, str> = Cow::Owned(s.into_owned());
                    wip = wip.set(cow).map_err(DeserializeError::Reflect)?;
                    return Ok(wip);
                } else {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "string for Cow<str>",
                        got: format!("{event:?}"),
                    });
                }
            }
            // Other Cow types - use begin_inner
            wip = wip.begin_inner().map_err(DeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::Reflect)?;
            return Ok(wip);
        }

        // Regular smart pointer (Box, Arc, Rc)
        wip = wip.begin_smart_ptr().map_err(DeserializeError::Reflect)?;
        wip = self.deserialize_into(wip)?;
        wip = wip.end().map_err(DeserializeError::Reflect)?;
        Ok(wip)
    }

    fn deserialize_struct(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_core::Characteristic;

        // Expect StructStart
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        if !matches!(event, ParseEvent::StructStart) {
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

        // Track which fields have been set
        let num_fields = struct_def.fields.len();
        let mut fields_set = alloc::vec![false; num_fields];

        loop {
            let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(name, _hint) => {
                    // Look up field in struct definition first
                    let field_info = struct_def.fields.iter().enumerate().find(|(_, f)| {
                        f.name == name.as_ref()
                            || f.alias.iter().any(|alias| *alias == name.as_ref())
                    });

                    if let Some((idx, field)) = field_info {
                        // Use the canonical field name, not the alias
                        wip = wip
                            .begin_field(field.name)
                            .map_err(DeserializeError::Reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::Reflect)?;
                        fields_set[idx] = true;
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
                continue; // Field was already set
            }

            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(Characteristic::Default);
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default || (struct_has_default && field_type_has_default) {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::Reflect)?;
            } else if field_is_option {
                wip = wip
                    .begin_field(field.name)
                    .map_err(DeserializeError::Reflect)?;
                wip = wip.set_default().map_err(DeserializeError::Reflect)?;
                wip = wip.end().map_err(DeserializeError::Reflect)?;
            } else if field.should_skip_deserializing() {
                // Skipped fields should use their default
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::Reflect)?;
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "field to be present or have default",
                    got: format!("missing field '{}'", field.name),
                });
            }
        }

        Ok(wip)
    }

    fn deserialize_tuple(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Expect SequenceStart
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        if !matches!(event, ParseEvent::SequenceStart) {
            return Err(DeserializeError::TypeMismatch {
                expected: "sequence start for tuple",
                got: format!("{event:?}"),
            });
        }

        let mut index = 0usize;
        loop {
            let event = self.parser.peek_event().map_err(DeserializeError::Parser)?;
            if matches!(event, ParseEvent::SequenceEnd) {
                self.parser.next_event().map_err(DeserializeError::Parser)?;
                break;
            }

            // Select field by index
            let field_name = alloc::string::ToString::to_string(&index);
            wip = wip
                .begin_field(&field_name)
                .map_err(DeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::Reflect)?;
            index += 1;
        }

        Ok(wip)
    }

    fn deserialize_enum(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.parser.peek_event().map_err(DeserializeError::Parser)?;

        // Check for unit variant (just a string)
        if let ParseEvent::Scalar(ScalarValue::Str(variant_name)) = &event {
            self.parser.next_event().map_err(DeserializeError::Parser)?;
            wip = wip
                .select_variant_named(variant_name)
                .map_err(DeserializeError::Reflect)?;
            // Unit variant - no content to deserialize
            return Ok(wip);
        }

        // Otherwise expect a struct { VariantName: ... }
        if !matches!(event, ParseEvent::StructStart) {
            return Err(DeserializeError::TypeMismatch {
                expected: "string or struct for enum",
                got: format!("{event:?}"),
            });
        }

        self.parser.next_event().map_err(DeserializeError::Parser)?; // consume StructStart

        // Get the variant name
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        let variant_name = match event {
            ParseEvent::FieldKey(name, _) => name,
            other => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "variant name",
                    got: format!("{other:?}"),
                });
            }
        };

        wip = wip
            .select_variant_named(&variant_name)
            .map_err(DeserializeError::Reflect)?;

        // Deserialize the variant content
        let content_event = self.parser.peek_event().map_err(DeserializeError::Parser)?;
        if matches!(content_event, ParseEvent::StructStart) {
            // Struct variant
            wip = self.deserialize_struct(wip)?;
        } else if matches!(content_event, ParseEvent::SequenceStart) {
            // Tuple variant
            wip = self.deserialize_tuple(wip)?;
        } else {
            // Newtype variant
            wip = self.deserialize_into(wip)?;
        }

        wip = wip.end().map_err(DeserializeError::Reflect)?;

        // Consume StructEnd
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        if !matches!(event, ParseEvent::StructEnd) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct end after enum variant",
                got: format!("{event:?}"),
            });
        }

        Ok(wip)
    }

    fn deserialize_list(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        if !matches!(event, ParseEvent::SequenceStart) {
            return Err(DeserializeError::TypeMismatch {
                expected: "sequence start",
                got: format!("{event:?}"),
            });
        }

        // Initialize the list
        wip = wip.begin_list().map_err(DeserializeError::Reflect)?;

        loop {
            let event = self.parser.peek_event().map_err(DeserializeError::Parser)?;
            if matches!(event, ParseEvent::SequenceEnd) {
                self.parser.next_event().map_err(DeserializeError::Parser)?;
                break;
            }

            wip = wip.begin_list_item().map_err(DeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::Reflect)?;
        }

        Ok(wip)
    }

    fn deserialize_array(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        if !matches!(event, ParseEvent::SequenceStart) {
            return Err(DeserializeError::TypeMismatch {
                expected: "sequence start for array",
                got: format!("{event:?}"),
            });
        }

        let mut index = 0usize;
        loop {
            let event = self.parser.peek_event().map_err(DeserializeError::Parser)?;
            if matches!(event, ParseEvent::SequenceEnd) {
                self.parser.next_event().map_err(DeserializeError::Parser)?;
                break;
            }

            wip = wip
                .begin_nth_field(index)
                .map_err(DeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::Reflect)?;
            index += 1;
        }

        Ok(wip)
    }

    fn deserialize_set(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        if !matches!(event, ParseEvent::SequenceStart) {
            return Err(DeserializeError::TypeMismatch {
                expected: "sequence start for set",
                got: format!("{event:?}"),
            });
        }

        // Initialize the set
        wip = wip.begin_set().map_err(DeserializeError::Reflect)?;

        loop {
            let event = self.parser.peek_event().map_err(DeserializeError::Parser)?;
            if matches!(event, ParseEvent::SequenceEnd) {
                self.parser.next_event().map_err(DeserializeError::Parser)?;
                break;
            }

            wip = wip.begin_set_item().map_err(DeserializeError::Reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::Reflect)?;
        }

        Ok(wip)
    }

    fn deserialize_map(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        if !matches!(event, ParseEvent::StructStart) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start for map",
                got: format!("{event:?}"),
            });
        }

        // Initialize the map
        wip = wip.begin_map().map_err(DeserializeError::Reflect)?;

        loop {
            let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key, _) => {
                    // Begin key
                    wip = wip.begin_key().map_err(DeserializeError::Reflect)?;
                    wip = wip
                        .set(key.into_owned())
                        .map_err(DeserializeError::Reflect)?;
                    wip = wip.end().map_err(DeserializeError::Reflect)?;

                    // Begin value
                    wip = wip.begin_value().map_err(DeserializeError::Reflect)?;
                    wip = self.deserialize_into(wip)?;
                    wip = wip.end().map_err(DeserializeError::Reflect)?;
                }
                other => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "field key or struct end for map",
                        got: format!("{other:?}"),
                    });
                }
            }
        }

        Ok(wip)
    }

    fn deserialize_scalar(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;

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

        match scalar {
            ScalarValue::Null => {
                wip = wip.set_default().map_err(DeserializeError::Reflect)?;
            }
            ScalarValue::Bool(b) => {
                wip = wip.set(b).map_err(DeserializeError::Reflect)?;
            }
            ScalarValue::I64(n) => {
                // Handle signed types
                if shape.type_identifier == "i8" {
                    wip = wip.set(n as i8).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "i16" {
                    wip = wip.set(n as i16).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "i32" {
                    wip = wip.set(n as i32).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "i64" {
                    wip = wip.set(n).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "i128" {
                    wip = wip.set(n as i128).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "isize" {
                    wip = wip.set(n as isize).map_err(DeserializeError::Reflect)?;
                // Handle unsigned types (I64 can fit in unsigned if non-negative)
                } else if shape.type_identifier == "u8" {
                    wip = wip.set(n as u8).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "u16" {
                    wip = wip.set(n as u16).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "u32" {
                    wip = wip.set(n as u32).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "u64" {
                    wip = wip.set(n as u64).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "u128" {
                    wip = wip.set(n as u128).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "usize" {
                    wip = wip.set(n as usize).map_err(DeserializeError::Reflect)?;
                // Handle floats
                } else if shape.type_identifier == "f32" {
                    wip = wip.set(n as f32).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "f64" {
                    wip = wip.set(n as f64).map_err(DeserializeError::Reflect)?;
                } else {
                    wip = wip.set(n).map_err(DeserializeError::Reflect)?;
                }
            }
            ScalarValue::U64(n) => {
                // Handle unsigned types
                if shape.type_identifier == "u8" {
                    wip = wip.set(n as u8).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "u16" {
                    wip = wip.set(n as u16).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "u32" {
                    wip = wip.set(n as u32).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "u64" {
                    wip = wip.set(n).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "u128" {
                    wip = wip.set(n as u128).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "usize" {
                    wip = wip.set(n as usize).map_err(DeserializeError::Reflect)?;
                // Handle signed types (U64 can fit in signed if small enough)
                } else if shape.type_identifier == "i8" {
                    wip = wip.set(n as i8).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "i16" {
                    wip = wip.set(n as i16).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "i32" {
                    wip = wip.set(n as i32).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "i64" {
                    wip = wip.set(n as i64).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "i128" {
                    wip = wip.set(n as i128).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "isize" {
                    wip = wip.set(n as isize).map_err(DeserializeError::Reflect)?;
                // Handle floats
                } else if shape.type_identifier == "f32" {
                    wip = wip.set(n as f32).map_err(DeserializeError::Reflect)?;
                } else if shape.type_identifier == "f64" {
                    wip = wip.set(n as f64).map_err(DeserializeError::Reflect)?;
                } else {
                    wip = wip.set(n).map_err(DeserializeError::Reflect)?;
                }
            }
            ScalarValue::F64(n) => {
                if shape.type_identifier == "f32" {
                    wip = wip.set(n as f32).map_err(DeserializeError::Reflect)?;
                } else {
                    wip = wip.set(n).map_err(DeserializeError::Reflect)?;
                }
            }
            ScalarValue::Str(s) => {
                // Try parse_from_str first if the type supports it
                if shape.vtable.has_parse() {
                    wip = wip
                        .parse_from_str(s.as_ref())
                        .map_err(DeserializeError::Reflect)?;
                } else {
                    wip = wip.set(s.into_owned()).map_err(DeserializeError::Reflect)?;
                }
            }
            ScalarValue::Bytes(b) => {
                wip = wip.set(b.into_owned()).map_err(DeserializeError::Reflect)?;
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
    Reflect(ReflectError),
    /// Type mismatch during deserialization.
    TypeMismatch {
        /// The expected type or token.
        expected: &'static str,
        /// The actual type or token that was encountered.
        got: String,
    },
    /// Unsupported type or operation.
    Unsupported(String),
}

impl<E: fmt::Display> fmt::Display for DeserializeError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeserializeError::Parser(err) => write!(f, "{err}"),
            DeserializeError::Reflect(err) => write!(f, "reflection error: {err}"),
            DeserializeError::TypeMismatch { expected, got } => {
                write!(f, "type mismatch: expected {expected}, got {got}")
            }
            DeserializeError::Unsupported(msg) => write!(f, "unsupported: {msg}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for DeserializeError<E> {}
