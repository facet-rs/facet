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
        use facet_reflect::Resolution;

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
        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();

        // Track which fields have been set
        let num_fields = struct_def.fields.len();
        let mut fields_set = alloc::vec![false; num_fields];

        // Check if we have any flattened fields - if so, we need deferred mode
        let has_flatten = struct_def.fields.iter().any(|f| f.is_flattened());

        // Build flatten info: for each flattened field, get its inner struct fields
        // and track which inner fields have been set
        let mut flatten_info: alloc::vec::Vec<
            Option<(&'static [facet_core::Field], alloc::vec::Vec<bool>)>,
        > = alloc::vec![None; num_fields];
        for (idx, field) in struct_def.fields.iter().enumerate() {
            if field.is_flattened()
                && let Type::User(UserType::Struct(inner_def)) = &field.shape().ty
            {
                let inner_fields = inner_def.fields;
                let inner_set = alloc::vec![false; inner_fields.len()];
                flatten_info[idx] = Some((inner_fields, inner_set));
            }
        }

        // Enter deferred mode if we have flattened fields
        if has_flatten {
            let resolution = Resolution::new();
            wip = wip
                .begin_deferred(resolution)
                .map_err(DeserializeError::Reflect)?;
        }

        loop {
            let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key) => {
                    // First, look up field in direct struct fields (non-flattened)
                    let direct_field_info = struct_def.fields.iter().enumerate().find(|(_, f)| {
                        !f.is_flattened()
                            && (f.name == key.name.as_ref()
                                || f.alias.iter().any(|alias| *alias == key.name.as_ref()))
                    });

                    if let Some((idx, field)) = direct_field_info {
                        // Direct field match
                        wip = wip
                            .begin_field(field.name)
                            .map_err(DeserializeError::Reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::Reflect)?;
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
                            // Look for the field in the inner struct
                            let inner_match = inner_fields.iter().enumerate().find(|(_, f)| {
                                f.name == key.name.as_ref()
                                    || f.alias.iter().any(|alias| *alias == key.name.as_ref())
                            });

                            if let Some((inner_idx, _inner_field)) = inner_match {
                                // Found it! Navigate into the flattened field
                                wip = wip
                                    .begin_nth_field(flatten_idx)
                                    .map_err(DeserializeError::Reflect)?;
                                wip = wip
                                    .begin_nth_field(inner_idx)
                                    .map_err(DeserializeError::Reflect)?;
                                wip = self.deserialize_into(wip)?;
                                wip = wip.end().map_err(DeserializeError::Reflect)?;
                                wip = wip.end().map_err(DeserializeError::Reflect)?;
                                inner_set[inner_idx] = true;
                                fields_set[flatten_idx] = true; // Mark flattened field as touched
                                found_flatten = true;
                                break;
                            }
                        }
                    }

                    if found_flatten {
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
            if field.is_flattened() {
                // For flattened fields, check if all inner fields are set or have defaults
                if let Some((inner_fields, inner_set)) = flatten_info[idx].as_ref() {
                    // Check if any inner field was set
                    let any_inner_set = inner_set.iter().any(|&s| s);
                    if any_inner_set {
                        // Some inner fields were set - need to apply defaults to missing ones
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::Reflect)?;
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
                                    .map_err(DeserializeError::Reflect)?;
                            } else if inner_is_option {
                                wip = wip
                                    .begin_nth_field(inner_idx)
                                    .map_err(DeserializeError::Reflect)?;
                                wip = wip.set_default().map_err(DeserializeError::Reflect)?;
                                wip = wip.end().map_err(DeserializeError::Reflect)?;
                            } else if inner_field.should_skip_deserializing() {
                                wip = wip
                                    .set_nth_field_to_default(inner_idx)
                                    .map_err(DeserializeError::Reflect)?;
                            } else {
                                return Err(DeserializeError::TypeMismatch {
                                    expected: "field to be present or have default",
                                    got: format!("missing field '{}'", inner_field.name),
                                });
                            }
                        }
                        wip = wip.end().map_err(DeserializeError::Reflect)?;
                    } else {
                        // No inner fields set - try to default the whole flattened field
                        let field_has_default = field.has_default();
                        let field_type_has_default = field.shape().is(Characteristic::Default);
                        if field_has_default || (struct_has_default && field_type_has_default) {
                            wip = wip
                                .set_nth_field_to_default(idx)
                                .map_err(DeserializeError::Reflect)?;
                        } else {
                            // Can't default the flattened struct - check if all inner fields can default
                            let all_inner_can_default = inner_fields.iter().all(|f| {
                                f.has_default()
                                    || f.shape().is(Characteristic::Default)
                                    || matches!(f.shape().def, Def::Option(_))
                                    || f.should_skip_deserializing()
                            });
                            if all_inner_can_default {
                                wip = wip
                                    .begin_nth_field(idx)
                                    .map_err(DeserializeError::Reflect)?;
                                for (inner_idx, inner_field) in inner_fields.iter().enumerate() {
                                    let inner_has_default = inner_field.has_default();
                                    let inner_type_has_default =
                                        inner_field.shape().is(Characteristic::Default);
                                    let inner_is_option =
                                        matches!(inner_field.shape().def, Def::Option(_));

                                    if inner_has_default || inner_type_has_default {
                                        wip = wip
                                            .set_nth_field_to_default(inner_idx)
                                            .map_err(DeserializeError::Reflect)?;
                                    } else if inner_is_option {
                                        wip = wip
                                            .begin_nth_field(inner_idx)
                                            .map_err(DeserializeError::Reflect)?;
                                        wip =
                                            wip.set_default().map_err(DeserializeError::Reflect)?;
                                        wip = wip.end().map_err(DeserializeError::Reflect)?;
                                    } else if inner_field.should_skip_deserializing() {
                                        wip = wip
                                            .set_nth_field_to_default(inner_idx)
                                            .map_err(DeserializeError::Reflect)?;
                                    }
                                }
                                wip = wip.end().map_err(DeserializeError::Reflect)?;
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

        // Finish deferred mode if we were in it
        if has_flatten {
            wip = wip.finish_deferred().map_err(DeserializeError::Reflect)?;
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
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let shape = wip.shape();

        // Check for different tagging modes
        let tag_attr = shape.get_tag_attr();
        let content_attr = shape.get_content_attr();
        let is_untagged = shape.is_untagged();

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
        let event = self.parser.peek_event().map_err(DeserializeError::Parser)?;

        // Check for unit variant (just a string)
        if let ParseEvent::Scalar(ScalarValue::Str(variant_name)) = &event {
            self.parser.next_event().map_err(DeserializeError::Parser)?;
            wip = wip
                .select_variant_named(variant_name)
                .map_err(DeserializeError::Reflect)?;
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
            .map_err(DeserializeError::Reflect)?;

        // Deserialize the variant content
        wip = self.deserialize_enum_variant_content(wip)?;

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
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        if !matches!(event, ParseEvent::StructStart) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct for internally tagged enum",
                got: format!("{event:?}"),
            });
        }

        // Step 3: Select the variant
        wip = wip
            .select_variant_named(&variant_name)
            .map_err(DeserializeError::Reflect)?;

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
                let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
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
            let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key) => {
                    // Skip the tag field - already used
                    if key.name.as_ref() == tag_key {
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                        continue;
                    }

                    // Look up field in variant's fields
                    let field_info = variant_fields.iter().enumerate().find(|(_, f)| {
                        f.name == key.name.as_ref()
                            || f.alias.iter().any(|alias| *alias == key.name.as_ref())
                    });

                    if let Some((idx, _field)) = field_info {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::Reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::Reflect)?;
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
                    .map_err(DeserializeError::Reflect)?;
            } else if field_is_option {
                wip = wip
                    .begin_nth_field(idx)
                    .map_err(DeserializeError::Reflect)?;
                wip = wip.set_default().map_err(DeserializeError::Reflect)?;
                wip = wip.end().map_err(DeserializeError::Reflect)?;
            } else if field.should_skip_deserializing() {
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
        let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
        if !matches!(event, ParseEvent::StructStart) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct for adjacently tagged enum",
                got: format!("{event:?}"),
            });
        }

        // Step 3: Select the variant
        wip = wip
            .select_variant_named(&variant_name)
            .map_err(DeserializeError::Reflect)?;

        // Step 4: Process fields in any order
        let mut content_seen = false;
        loop {
            let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
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
                    wip = wip.begin_nth_field(0).map_err(DeserializeError::Reflect)?;
                    wip = self.deserialize_into(wip)?;
                    wip = wip.end().map_err(DeserializeError::Reflect)?;
                } else {
                    // Multi-field tuple variant - expect array
                    let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
                    if !matches!(event, ParseEvent::SequenceStart) {
                        return Err(DeserializeError::TypeMismatch {
                            expected: "sequence for tuple variant",
                            got: format!("{event:?}"),
                        });
                    }

                    for idx in 0..variant_fields.len() {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::Reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::Reflect)?;
                    }

                    let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
                    if !matches!(event, ParseEvent::SequenceEnd) {
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
                let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
                if !matches!(event, ParseEvent::StructStart) {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "struct for struct variant",
                        got: format!("{event:?}"),
                    });
                }

                let num_fields = variant_fields.len();
                let mut fields_set = alloc::vec![false; num_fields];

                loop {
                    let event = self.parser.next_event().map_err(DeserializeError::Parser)?;
                    match event {
                        ParseEvent::StructEnd => break,
                        ParseEvent::FieldKey(key) => {
                            let field_info = variant_fields.iter().enumerate().find(|(_, f)| {
                                f.name == key.name.as_ref()
                                    || f.alias.iter().any(|alias| *alias == key.name.as_ref())
                            });

                            if let Some((idx, _field)) = field_info {
                                wip = wip
                                    .begin_nth_field(idx)
                                    .map_err(DeserializeError::Reflect)?;
                                wip = self.deserialize_into(wip)?;
                                wip = wip.end().map_err(DeserializeError::Reflect)?;
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
                            .map_err(DeserializeError::Reflect)?;
                    } else if field_is_option {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::Reflect)?;
                        wip = wip.set_default().map_err(DeserializeError::Reflect)?;
                        wip = wip.end().map_err(DeserializeError::Reflect)?;
                    } else if field.should_skip_deserializing() {
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
        }
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

        let event = self.parser.peek_event().map_err(DeserializeError::Parser)?;

        match &event {
            ParseEvent::Scalar(scalar) => {
                // Try unit variants for null
                if matches!(scalar, ScalarValue::Null)
                    && let Some(variant) = variants_by_format.unit_variants.first()
                {
                    wip = wip
                        .select_variant_named(variant.name)
                        .map_err(DeserializeError::Reflect)?;
                    // Consume the null
                    self.parser.next_event().map_err(DeserializeError::Parser)?;
                    return Ok(wip);
                }

                // Try scalar variants - match by type
                for (variant, inner_shape) in &variants_by_format.scalar_variants {
                    if self.scalar_matches_shape(scalar, inner_shape) {
                        wip = wip
                            .select_variant_named(variant.name)
                            .map_err(DeserializeError::Reflect)?;
                        wip = self.deserialize_enum_variant_content(wip)?;
                        return Ok(wip);
                    }
                }

                Err(DeserializeError::TypeMismatch {
                    expected: "matching untagged variant for scalar",
                    got: format!("{:?}", scalar),
                })
            }
            ParseEvent::StructStart => {
                // For struct input, use first struct variant
                // TODO: Use solve_variant for proper field-based matching
                if let Some(variant) = variants_by_format.struct_variants.first() {
                    wip = wip
                        .select_variant_named(variant.name)
                        .map_err(DeserializeError::Reflect)?;
                    wip = self.deserialize_enum_variant_content(wip)?;
                    return Ok(wip);
                }

                Err(DeserializeError::Unsupported(
                    "no struct variant found for untagged enum with struct input".into(),
                ))
            }
            ParseEvent::SequenceStart => {
                // For sequence input, use first tuple variant
                if let Some((variant, _arity)) = variants_by_format.tuple_variants.first() {
                    wip = wip
                        .select_variant_named(variant.name)
                        .map_err(DeserializeError::Reflect)?;
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
            ScalarValue::I64(_) => matches!(
                scalar_type,
                ScalarType::I8
                    | ScalarType::I16
                    | ScalarType::I32
                    | ScalarType::I64
                    | ScalarType::I128
                    | ScalarType::ISize
            ),
            ScalarValue::U64(_) => matches!(
                scalar_type,
                ScalarType::U8
                    | ScalarType::U16
                    | ScalarType::U32
                    | ScalarType::U64
                    | ScalarType::U128
                    | ScalarType::USize
            ),
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
                ParseEvent::FieldKey(key) => {
                    // Begin key
                    wip = wip.begin_key().map_err(DeserializeError::Reflect)?;
                    wip = wip
                        .set(key.name.into_owned())
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
                // Handle String - stringify the number
                } else if shape.type_identifier == "String" {
                    wip = wip
                        .set(alloc::string::ToString::to_string(&n))
                        .map_err(DeserializeError::Reflect)?;
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
                // Handle String - stringify the number
                } else if shape.type_identifier == "String" {
                    wip = wip
                        .set(alloc::string::ToString::to_string(&n))
                        .map_err(DeserializeError::Reflect)?;
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
    /// Unknown field encountered when deny_unknown_fields is set.
    UnknownField(String),
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
            DeserializeError::UnknownField(field) => write!(f, "unknown field: {field}"),
        }
    }
}

impl<E: fmt::Debug + fmt::Display> std::error::Error for DeserializeError<E> {}
