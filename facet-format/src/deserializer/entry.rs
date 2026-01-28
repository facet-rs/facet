use std::borrow::Cow;

use facet_core::{Def, Shape, StructKind, Type, UserType};
use facet_reflect::{Partial, typeplan::DeserStrategy};

use crate::{
    ContainerKind, DeserializeError, DeserializeErrorKind, FieldEvidence, FieldLocationHint,
    FormatDeserializer, ParseEventKind, ScalarTypeHint, ScalarValue, SpanGuard,
};

impl<'parser, 'input, const BORROW: bool> FormatDeserializer<'parser, 'input, BORROW> {
    /// Main deserialization entry point - deserialize into a Partial.
    ///
    /// Uses the precomputed `DeserStrategy` from TypePlan for fast dispatch.
    /// The strategy is computed once at Partial allocation time, eliminating
    /// repeated runtime inspection of Shape/Def/vtable during deserialization.
    pub fn deserialize_into(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        let shape = wip.shape();
        trace!(
            shape_name = shape.type_identifier,
            "deserialize_into: starting"
        );

        // === SPECIAL CASES (cannot be precomputed) ===

        // Check for raw capture type (e.g., RawJson) - parser-specific
        if self.parser.raw_capture_shape() == Some(shape) {
            let Some(raw) = self.capture_raw()? else {
                return Err(DeserializeErrorKind::RawCaptureNotSupported { shape }
                    .with_span(self.last_span));
            };
            return Ok(wip
                .begin_nth_field(0)?
                .with(|w| self.set_string_value(w, Cow::Borrowed(raw)))?
                .end()?);
        }

        // Check for builder_shape (immutable collections like Bytes -> BytesMut)
        // This is a special case that can't easily be represented in DeserStrategy
        if shape.builder_shape.is_some() {
            return Ok(wip
                .begin_inner()?
                .with(|w| self.deserialize_into(w))?
                .end()?);
        }

        // Check for metadata containers (like Spanned<T>, Documented<T>)
        // These require field-by-field handling that can't be precomputed
        if shape.is_metadata_container() {
            return self.deserialize_metadata_container(wip);
        }

        // === STRATEGY-BASED DISPATCH ===
        // Use the precomputed DeserStrategy for O(1) dispatch

        let strategy = wip.deser_strategy().unwrap_or(DeserStrategy::Unknown);
        trace!(?strategy, "deserialize_into: using precomputed strategy");

        match strategy {
            DeserStrategy::Proxy => {
                // Container-level or field-level proxy - deserialize into proxy type then convert
                let format_ns = self.parser.format_namespace();
                let (wip_returned, has_proxy) =
                    wip.begin_custom_deserialization_from_shape_with_format(format_ns)?;
                wip = wip_returned;
                if has_proxy {
                    return Ok(wip.with(|w| self.deserialize_into(w))?.end()?);
                }
                // Fallback: check field-level proxy
                if wip
                    .parent_field()
                    .and_then(|field| field.effective_proxy(format_ns))
                    .is_some()
                {
                    return Ok(wip
                        .begin_custom_deserialization_with_format(format_ns)?
                        .with(|w| self.deserialize_into(w))?
                        .end()?);
                }
                // If no proxy found (shouldn't happen if TypePlan is correct), fall through to Unknown
                self.deserialize_fallback(wip)
            }

            DeserStrategy::Pointer => {
                trace!("deserialize_into: dispatching to deserialize_pointer");
                self.deserialize_pointer(wip)
            }

            DeserStrategy::TransparentConvert => {
                // Transparent wrapper with try_from (like NonZero)
                trace!("deserialize_into: dispatching via begin_inner (transparent convert)");
                Ok(wip
                    .begin_inner()?
                    .with(|w| self.deserialize_into(w))?
                    .end()?)
            }

            DeserStrategy::Scalar => {
                trace!("deserialize_into: dispatching to deserialize_scalar");
                self.deserialize_scalar(wip)
            }

            DeserStrategy::Struct => {
                trace!("deserialize_into: dispatching to deserialize_struct");
                self.deserialize_struct(wip)
            }

            DeserStrategy::Tuple => {
                trace!("deserialize_into: dispatching to deserialize_tuple");
                self.deserialize_tuple(wip)
            }

            DeserStrategy::Enum => {
                trace!("deserialize_into: dispatching to deserialize_enum");
                self.deserialize_enum(wip)
            }

            DeserStrategy::Option => {
                trace!("deserialize_into: dispatching to deserialize_option");
                self.deserialize_option(wip)
            }

            DeserStrategy::Result => {
                trace!("deserialize_into: dispatching to deserialize_result_as_enum");
                self.deserialize_result_as_enum(wip)
            }

            DeserStrategy::List => {
                trace!("deserialize_into: dispatching to deserialize_list");
                self.deserialize_list(wip)
            }

            DeserStrategy::Map => {
                trace!("deserialize_into: dispatching to deserialize_map");
                self.deserialize_map(wip)
            }

            DeserStrategy::Set => {
                trace!("deserialize_into: dispatching to deserialize_set");
                self.deserialize_set(wip)
            }

            DeserStrategy::Array => {
                trace!("deserialize_into: dispatching to deserialize_array");
                self.deserialize_array(wip)
            }

            DeserStrategy::BackRef => {
                // BackRef is automatically resolved by deser_strategy() - this branch
                // should never be reached. If it is, something is wrong with TypePlan.
                unreachable!("deser_strategy() should resolve BackRef to target strategy")
            }

            DeserStrategy::Unknown => {
                trace!("deserialize_into: Unknown strategy - using runtime fallback");
                self.deserialize_fallback(wip)
            }
        }
    }

    /// Fallback deserialization using runtime shape inspection.
    ///
    /// This is used when the precomputed strategy is Unknown or BackRef,
    /// or when special cases need runtime dispatch.
    fn deserialize_fallback(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let shape = wip.shape();

        // Check Def for containers and special types
        match &shape.def {
            Def::Option(_) => self.deserialize_option(wip),
            Def::Result(_) => self.deserialize_result_as_enum(wip),
            Def::Pointer(_) => self.deserialize_pointer(wip),
            Def::List(_) => self.deserialize_list(wip),
            Def::Map(_) => self.deserialize_map(wip),
            Def::Array(_) => self.deserialize_array(wip),
            Def::Set(_) => self.deserialize_set(wip),
            Def::DynamicValue(_) => self.deserialize_dynamic_value(wip),
            Def::Scalar if shape.vtable.has_parse() => self.deserialize_scalar(wip),
            _ => {
                // Check Type for structs and enums
                match &shape.ty {
                    Type::User(UserType::Struct(struct_def)) => {
                        if matches!(struct_def.kind, StructKind::Tuple | StructKind::TupleStruct) {
                            self.deserialize_tuple(wip)
                        } else {
                            self.deserialize_struct(wip)
                        }
                    }
                    Type::User(UserType::Enum(_)) => self.deserialize_enum(wip),
                    _ => Err(DeserializeErrorKind::Unsupported {
                        message: format!("unsupported shape def: {:?}", shape.def).into(),
                    }
                    .with_span(self.last_span)),
                }
            }
        }
    }

    /// Deserialize a metadata container (like Spanned<T>, Documented<T>).
    ///
    /// These require special handling - the value field gets the data,
    /// metadata fields are populated from parser state.
    fn deserialize_metadata_container(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        trace!("deserialize_into: metadata container detected");
        let shape = wip.shape();

        if let Type::User(UserType::Struct(st)) = &shape.ty {
            for field in st.fields {
                match field.metadata_kind() {
                    Some("span") => {
                        // Populate span from parser's current position
                        let span = self.last_span;
                        wip = wip
                            .begin_field(field.effective_name())?
                            .begin_some()?
                            .begin_field("offset")?
                            .set(span.offset)?
                            .end()?
                            .begin_field("len")?
                            .set(span.len)?
                            .end()?
                            .end()?
                            .end()?;
                    }
                    Some(_other) => {
                        // Other metadata types (doc, tag) - set to default for now
                        wip = wip
                            .begin_field(field.effective_name())?
                            .set_default()?
                            .end()?;
                    }
                    None => {
                        // This is the value field - recurse into it
                        wip = wip
                            .begin_field(field.effective_name())?
                            .with(|w| self.deserialize_into(w))?
                            .end()?;
                    }
                }
            }
        }
        Ok(wip)
    }

    /// Deserialize using an explicit source shape for parser hints.
    ///
    /// This walks `hint_shape` for control flow and parser hints, but builds
    /// into the `wip` Partial (which should be a DynamicValue like `Value`).
    pub fn deserialize_into_with_shape(
        &mut self,
        wip: Partial<'input, BORROW>,
        hint_shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        self.deserialize_value_recursive(wip, hint_shape)
    }

    /// Internal recursive deserialization using hint_shape for dispatch.
    pub(crate) fn deserialize_value_recursive(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        hint_shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // Handle Option
        if let Def::Option(opt_def) = &hint_shape.def {
            self.parser.hint_option();
            let event = self.expect_peek("value for option")?;
            // Treat both Null and Unit as None
            // Unit is used by Styx for tags without payload (e.g., @string vs @string{...})
            if matches!(
                event.kind,
                ParseEventKind::Scalar(ScalarValue::Null | ScalarValue::Unit)
            ) {
                let _ = self.expect_event("null or unit")?;
                wip = wip.set_default()?;
            } else {
                wip = self.deserialize_value_recursive(wip, opt_def.t)?;
            }
            return Ok(wip);
        }

        // Handle smart pointers - unwrap to inner type
        if let Def::Pointer(ptr_def) = &hint_shape.def
            && let Some(pointee) = ptr_def.pointee()
        {
            return self.deserialize_value_recursive(wip, pointee);
        }

        // Handle transparent wrappers (but not collections)
        if let Some(inner) = hint_shape.inner
            && !matches!(
                &hint_shape.def,
                Def::List(_) | Def::Map(_) | Def::Set(_) | Def::Array(_)
            )
        {
            return self.deserialize_value_recursive(wip, inner);
        }

        // Dispatch based on hint shape type
        match &hint_shape.ty {
            Type::User(UserType::Struct(struct_def)) => {
                if matches!(struct_def.kind, StructKind::Tuple | StructKind::TupleStruct) {
                    self.deserialize_tuple_dynamic(wip, struct_def.fields)
                } else {
                    self.deserialize_struct_dynamic(wip, struct_def.fields)
                }
            }
            Type::User(UserType::Enum(enum_def)) => self.deserialize_enum_dynamic(wip, enum_def),
            _ => match &hint_shape.def {
                Def::Scalar => self.deserialize_scalar_dynamic(wip, hint_shape),
                Def::List(list_def) => self.deserialize_list_dynamic(wip, list_def.t),
                Def::Array(array_def) => {
                    self.deserialize_array_dynamic(wip, array_def.t, array_def.n)
                }
                Def::Map(map_def) => self.deserialize_map_dynamic(wip, map_def.k, map_def.v),
                Def::Set(set_def) => self.deserialize_list_dynamic(wip, set_def.t),
                _ => Err(DeserializeErrorKind::Unsupported {
                    message: format!(
                        "unsupported hint shape for dynamic deserialization: {:?}",
                        hint_shape.def
                    )
                    .into(),
                }
                .with_span(self.last_span)),
            },
        }
    }

    pub(crate) fn deserialize_option(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Hint to non-self-describing parsers that an Option is expected
        self.parser.hint_option();

        let event = self.expect_peek("value for option")?;

        // Treat both Null and Unit as None
        // Unit is used by Styx for tags without payload (e.g., @string vs @string{...})
        if matches!(
            event.kind,
            ParseEventKind::Scalar(ScalarValue::Null | ScalarValue::Unit)
        ) {
            // Consume the null/unit
            let _ = self.expect_event("null or unit")?;
            // Set to None (default)
            wip = wip.set_default()?;
        } else {
            // Some(value)
            wip = wip
                .begin_some()?
                .with(|w| self.deserialize_into(w))?
                .end()?;
        }
        Ok(wip)
    }

    pub(crate) fn deserialize_struct(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        // Get struct fields for lookup
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DeserializeErrorKind::UnexpectedToken {
                    expected: "struct",
                    got: format!("{:?}", wip.shape().ty).into(),
                }
                .with_span(self.last_span));
            }
        };

        // Check if we have any flattened fields
        let has_flatten = struct_def.fields.iter().any(|f| f.is_flattened());

        if has_flatten {
            self.deserialize_struct_with_flatten(wip)
        } else {
            self.deserialize_struct_simple(wip)
        }
    }

    pub(crate) fn deserialize_tuple(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Get field count for tuple hints
        let field_count = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def.fields.len(),
            _ => 0, // Unit type or unknown - will be handled below
        };

        // Hint to non-self-describing parsers how many fields to expect
        // Tuples are like positional structs, so we use hint_struct_fields
        self.parser.hint_struct_fields(field_count);

        // Special case: transparent newtypes (marked with #[facet(transparent)] or
        // #[repr(transparent)]) can accept values directly without a sequence wrapper.
        // This enables patterns like:
        //   #[facet(transparent)]
        //   struct Wrapper(i32);
        //   toml: "value = 42"  ->  Wrapper(42)
        // Plain tuple structs without the transparent attribute use array syntax.
        if field_count == 1 && wip.shape().is_transparent() {
            // Unwrap into field 0 and deserialize directly
            return Ok(wip
                .begin_nth_field(0)?
                .with(|w| self.deserialize_into(w))?
                .end()?);
        }

        // Special case: unit type () can accept Scalar(Unit) or Scalar(Null) directly
        // This enables patterns like styx bare identifiers: { id, name } -> IndexMap<String, ()>
        // and JSON null values for unit types (e.g., ConfigValue::Null(Spanned<()>))
        if field_count == 0 {
            let peeked = self.expect_peek("value")?;
            if matches!(
                peeked.kind,
                ParseEventKind::Scalar(ScalarValue::Unit | ScalarValue::Null)
            ) {
                self.expect_event("value")?; // consume the unit/null scalar
                return Ok(wip);
            }
        }

        let event = self.expect_event("value")?;

        // Accept either SequenceStart (JSON arrays) or StructStart (for
        // non-self-describing formats like postcard where tuples are positional structs)
        let struct_mode = match event.kind {
            ParseEventKind::SequenceStart(_) => false,
            // For non-self-describing formats, StructStart(Object) is valid for tuples
            // because hint_struct_fields was called and tuples are positional structs
            ParseEventKind::StructStart(_) if !self.parser.is_self_describing() => true,
            // For self-describing formats like TOML/JSON, objects with numeric keys
            // (e.g., { "0" = true, "1" = 1 }) are valid tuple representations
            ParseEventKind::StructStart(ContainerKind::Object) => true,
            ParseEventKind::StructStart(kind) => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "array",
                        got: kind.name().into(),
                    },
                });
            }
            _ => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "sequence start for tuple",
                        got: event.kind_name().into(),
                    },
                });
            }
        };

        let mut index = 0usize;
        loop {
            let event = self.expect_peek("value")?;

            // Check for end of container
            if matches!(
                event.kind,
                ParseEventKind::SequenceEnd | ParseEventKind::StructEnd
            ) {
                self.expect_event("value")?;
                break;
            }

            // In struct mode, skip FieldKey events
            if struct_mode && matches!(event.kind, ParseEventKind::FieldKey(_)) {
                self.expect_event("value")?;
                continue;
            }

            // Select field by index
            wip = wip
                .begin_nth_field(index)?
                .with(|w| self.deserialize_into(w))?
                .end()?;
            index += 1;
        }

        Ok(wip)
    }

    /// Helper to collect field evidence using save/restore.
    ///
    /// This saves the parser position, reads through the current struct to
    /// collect field names and their scalar values, then restores the position.
    pub(crate) fn collect_evidence(
        &mut self,
    ) -> Result<Vec<FieldEvidence<'input>>, DeserializeError> {
        self.save();

        let mut evidence = Vec::new();
        let mut depth = 0i32;
        let mut pending_field_name: Option<Cow<'input, str>> = None;

        // Read through the structure
        loop {
            let Ok(event) = self.expect_event("evidence") else {
                break;
            };

            match event.kind {
                ParseEventKind::StructStart(_) => {
                    depth += 1;
                    // If we were expecting a value, record field with no scalar
                    if depth > 1
                        && let Some(name) = pending_field_name.take()
                    {
                        evidence.push(FieldEvidence {
                            name,
                            location: FieldLocationHint::KeyValue,
                            value_type: None,
                            scalar_value: None,
                        });
                    }
                }
                ParseEventKind::StructEnd => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ParseEventKind::SequenceStart(_) => {
                    depth += 1;
                    // If we were expecting a value, record field with no scalar
                    if let Some(name) = pending_field_name.take() {
                        evidence.push(FieldEvidence {
                            name,
                            location: FieldLocationHint::KeyValue,
                            value_type: None,
                            scalar_value: None,
                        });
                    }
                }
                ParseEventKind::SequenceEnd => {
                    depth -= 1;
                }
                ParseEventKind::FieldKey(key) => {
                    // If there's a pending field, record it without a value
                    if let Some(name) = pending_field_name.take() {
                        evidence.push(FieldEvidence {
                            name,
                            location: FieldLocationHint::KeyValue,
                            value_type: None,
                            scalar_value: None,
                        });
                    }
                    if depth == 1 {
                        // Top-level field - save name, wait for value
                        pending_field_name = key.name().cloned();
                    }
                }
                ParseEventKind::Scalar(scalar) => {
                    if let Some(name) = pending_field_name.take() {
                        // Record field with its scalar value
                        evidence.push(FieldEvidence {
                            name,
                            location: FieldLocationHint::KeyValue,
                            value_type: None,
                            scalar_value: Some(scalar),
                        });
                    }
                }
                ParseEventKind::OrderedField | ParseEventKind::VariantTag(_) => {}
            }
        }

        // Handle any remaining pending field
        if let Some(name) = pending_field_name.take() {
            evidence.push(FieldEvidence {
                name,
                location: FieldLocationHint::KeyValue,
                value_type: None,
                scalar_value: None,
            });
        }

        self.restore();
        Ok(evidence)
    }

    pub(crate) fn deserialize_list(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        trace!("deserialize_list: starting");

        // Check if this is a Vec<u8> - if so, try the optimized byte sequence path
        // We specifically check for Vec (not Bytes, BytesMut, or other list-like types)
        // because those types may have different builder patterns
        let is_byte_vec = wip.shape().type_identifier == "Vec"
            && matches!(
                &wip.shape().def,
                Def::List(list_def) if list_def.t.type_identifier == "u8"
            );

        if is_byte_vec && self.parser.hint_byte_sequence() {
            // Parser supports bulk byte reading - expect Scalar(Bytes(...))
            let event = self.expect_event("bytes")?;
            trace!(?event, "deserialize_list: got bytes event");

            return match event.kind {
                ParseEventKind::Scalar(ScalarValue::Bytes(bytes)) => {
                    self.set_bytes_value(wip, bytes)
                }
                _ => Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "bytes",
                        got: event.kind_name().into(),
                    },
                }),
            };
        }

        // Fallback: element-by-element deserialization
        // Hint to non-self-describing parsers that a sequence is expected
        self.parser.hint_sequence();

        let event = self.expect_event("value")?;
        trace!(?event, "deserialize_list: got container start event");

        // Expect SequenceStart for lists
        match event.kind {
            ParseEventKind::SequenceStart(_) => {
                trace!("deserialize_list: got sequence start");
            }
            ParseEventKind::StructStart(kind) => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "array",
                        got: kind.name().into(),
                    },
                });
            }
            _ => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "sequence start",
                        got: event.kind_name().into(),
                    },
                });
            }
        };

        // Count buffered items to pre-reserve capacity
        let capacity_hint = self.count_buffered_sequence_items();
        trace!("deserialize_list: capacity hint = {capacity_hint}");

        // Initialize the list with capacity hint
        wip = wip.init_list_with_capacity(capacity_hint)?;
        trace!("deserialize_list: initialized list, starting loop");

        loop {
            let event = self.expect_peek("value")?;
            trace!(?event, "deserialize_list: loop iteration");

            // Check for end of sequence
            if matches!(event.kind, ParseEventKind::SequenceEnd) {
                self.expect_event("value")?;
                trace!("deserialize_list: reached end of sequence");
                break;
            }

            trace!("deserialize_list: deserializing list item");
            wip = wip
                .begin_list_item()?
                .with(|w| self.deserialize_into(w))?
                .end()?;
        }

        trace!("deserialize_list: completed");
        Ok(wip)
    }

    pub(crate) fn deserialize_array(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        // Get the fixed array length from the type definition
        let array_len = match &wip.shape().def {
            Def::Array(array_def) => array_def.n,
            _ => {
                return Err(DeserializeErrorKind::UnexpectedToken {
                    expected: "array",
                    got: format!("{:?}", wip.shape().def).into(),
                }
                .with_span(self.last_span));
            }
        };

        // Hint to non-self-describing parsers that a fixed-size array is expected
        // (unlike hint_sequence, this doesn't read a length prefix)
        self.parser.hint_array(array_len);

        let event = self.expect_event("value")?;

        // Expect SequenceStart for arrays
        match event.kind {
            ParseEventKind::SequenceStart(_) => {}
            ParseEventKind::StructStart(kind) => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "array",
                        got: kind.name().into(),
                    },
                });
            }
            _ => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "sequence start for array",
                        got: event.kind_name().into(),
                    },
                });
            }
        };

        // Transition to Array tracker state. This is important for empty arrays
        // like [u8; 0] which have no elements to initialize but still need
        // their tracker state set correctly for require_full_initialization to pass.
        wip = wip.init_array()?;

        let mut index = 0usize;
        loop {
            let event = self.expect_peek("value")?;

            // Check for end of sequence
            if matches!(event.kind, ParseEventKind::SequenceEnd) {
                self.expect_event("value")?;
                break;
            }

            wip = wip
                .begin_nth_field(index)?
                .with(|w| self.deserialize_into(w))?
                .end()?;
            index += 1;
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_set(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Hint to non-self-describing parsers that a sequence is expected
        self.parser.hint_sequence();

        let event = self.expect_event("value")?;

        // Expect SequenceStart for sets
        match event.kind {
            ParseEventKind::SequenceStart(_) => {}
            ParseEventKind::StructStart(kind) => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "set",
                        got: kind.name().into(),
                    },
                });
            }
            _ => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "sequence start for set",
                        got: event.kind_name().into(),
                    },
                });
            }
        };

        // Initialize the set
        wip = wip.init_set()?;

        loop {
            let event = self.expect_peek("value")?;

            // Check for end of sequence
            if matches!(event.kind, ParseEventKind::SequenceEnd) {
                self.expect_event("value")?;
                break;
            }

            wip = wip
                .begin_set_item()?
                .with(|w| self.deserialize_into(w))?
                .end()?;
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_map(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // For non-self-describing formats, hint that a map is expected
        self.parser.hint_map();

        let event = self.expect_event("value")?;

        // Initialize the map
        wip = wip.init_map()?;

        // Handle both self-describing (StructStart) and non-self-describing (SequenceStart) formats
        match event.kind {
            ParseEventKind::StructStart(_) => {
                // Self-describing format (e.g., JSON): maps are represented as objects
                loop {
                    let event = self.expect_event("value")?;
                    match event.kind {
                        ParseEventKind::StructEnd => break,
                        ParseEventKind::FieldKey(key) => {
                            // Begin key
                            wip = wip
                                .begin_key()?
                                .with(|w| {
                                    self.deserialize_map_key(
                                        w,
                                        key.name().cloned(),
                                        key.doc().map(|d| d.to_vec()),
                                        key.tag().cloned(),
                                    )
                                })?
                                .end()?;

                            // Begin value
                            wip = wip
                                .begin_value()?
                                .with(|w| self.deserialize_into(w))?
                                .end()?;
                        }
                        _ => {
                            return Err(DeserializeError {
                                span: Some(self.last_span),
                                path: Some(wip.path()),
                                kind: DeserializeErrorKind::UnexpectedToken {
                                    expected: "field key or struct end for map",
                                    got: event.kind_name().into(),
                                },
                            });
                        }
                    }
                }
            }
            ParseEventKind::SequenceStart(_) => {
                // Non-self-describing format (e.g., postcard): maps are sequences of key-value pairs
                loop {
                    let event = self.expect_peek("value")?;
                    match event.kind {
                        ParseEventKind::SequenceEnd => {
                            self.expect_event("value")?;
                            break;
                        }
                        ParseEventKind::OrderedField => {
                            self.expect_event("value")?;

                            // Deserialize key
                            wip = wip.begin_key()?.with(|w| self.deserialize_into(w))?.end()?;

                            // Deserialize value
                            wip = wip
                                .begin_value()?
                                .with(|w| self.deserialize_into(w))?
                                .end()?;
                        }
                        _ => {
                            return Err(DeserializeError {
                                span: Some(self.last_span),
                                path: Some(wip.path()),
                                kind: DeserializeErrorKind::UnexpectedToken {
                                    expected: "ordered field or sequence end for map",
                                    got: event.kind_name().into(),
                                },
                            });
                        }
                    }
                }
            }
            _ => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "struct start or sequence start for map",
                        got: event.kind_name().into(),
                    },
                });
            }
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_scalar(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
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

        match event.kind {
            ParseEventKind::Scalar(scalar) => {
                wip = self.set_scalar(wip, scalar)?;
                Ok(wip)
            }
            ParseEventKind::StructStart(_container_kind) => {
                // When deserializing into a scalar, extract the _arg value.
                let mut found_scalar: Option<ScalarValue<'input>> = None;

                loop {
                    let inner_event = self.expect_event("field or struct end")?;
                    match inner_event.kind {
                        ParseEventKind::StructEnd => break,
                        ParseEventKind::FieldKey(key) => {
                            // Look for _arg field (single argument)
                            if key.name().map(|c| c.as_ref()) == Some("_arg") {
                                let value_event = self.expect_event("argument value")?;
                                if let ParseEventKind::Scalar(scalar) = value_event.kind {
                                    found_scalar = Some(scalar);
                                } else {
                                    // Skip non-scalar argument
                                    self.skip_value()?;
                                }
                            } else {
                                // Skip other fields (_node_name, _arguments, properties, etc.)
                                self.skip_value()?;
                            }
                        }
                        _ => {
                            // Skip unexpected events
                        }
                    }
                }

                if let Some(scalar) = found_scalar {
                    wip = self.set_scalar(wip, scalar)?;
                    Ok(wip)
                } else {
                    Err(DeserializeError {
                        span: Some(self.last_span),
                        path: Some(wip.path()),
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "scalar value or node with argument",
                            got: "node without argument".into(),
                        },
                    })
                }
            }
            _ => Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "scalar value",
                    got: event.kind_name().into(),
                },
            }),
        }
    }

    /// Deserialize a map key from a string or tag.
    ///
    /// Format parsers typically emit string keys, but the target map might have non-string key types
    /// (e.g., integers, enums). This function parses the string key into the appropriate type:
    /// - String types: set directly
    /// - Enum unit variants: use select_variant_named
    /// - Integer types: parse the string as a number
    /// - Transparent newtypes: descend into the inner type
    /// - Option types: None key becomes None, Some(key) recurses into inner type
    /// - Metadata containers (like `Documented<T>`): populate doc/tag metadata and recurse into value
    ///
    /// The `tag` parameter is for formats like Styx where keys can be type patterns (e.g., `@string`).
    /// When present, it indicates the key was a tag rather than a bare identifier.
    pub(crate) fn deserialize_map_key(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        key: Option<Cow<'input, str>>,
        doc: Option<Vec<Cow<'input, str>>>,
        tag: Option<Cow<'input, str>>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        let shape = wip.shape();

        trace!(shape_name = %shape, shape_def = ?shape.def, ?key, ?doc, ?tag, "deserialize_map_key");

        // Handle metadata containers (like Documented<T> or ObjectKey): populate metadata and recurse into value
        if shape.is_metadata_container() {
            trace!("deserialize_map_key: metadata container detected");

            // Find field info from the shape's struct type
            if let Type::User(UserType::Struct(st)) = &shape.ty {
                for field in st.fields {
                    if field.metadata_kind() == Some("doc") {
                        // This is the doc field - set it from the doc parameter
                        wip = wip.begin_field(field.effective_name())?;
                        if let Some(ref doc_lines) = doc {
                            // Set as Some(Vec<String>)
                            wip = wip.begin_some()?.init_list()?;
                            for line in doc_lines {
                                wip = wip
                                    .begin_list_item()?
                                    .with(|w| self.set_string_value(w, line.clone()))?
                                    .end()?;
                            }
                            wip = wip.end()?;
                        } else {
                            // Set as None
                            wip = wip.set_default()?;
                        }
                        wip = wip.end()?;
                    } else if field.metadata_kind() == Some("tag") {
                        // This is the tag field - set it from the tag parameter
                        wip = wip.begin_field(field.effective_name())?;
                        if let Some(ref tag_name) = tag {
                            // Set as Some(String)
                            wip = wip
                                .begin_some()?
                                .with(|w| self.set_string_value(w, tag_name.clone()))?
                                .end()?;
                        } else {
                            // Set as None (not a tagged key)
                            wip = wip.set_default()?;
                        }
                        wip = wip.end()?;
                    } else if field.metadata_kind().is_none() {
                        // This is the value field - recurse with the key and tag.
                        // Doc is already consumed by this container, but tag may be needed
                        // by a nested metadata container (e.g., Documented<ObjectKey>).
                        wip = wip
                            .begin_field(field.effective_name())?
                            .with(|w| self.deserialize_map_key(w, key.clone(), None, tag.clone()))?
                            .end()?;
                    }
                }
            }

            return Ok(wip);
        }

        // Handle Option<T> key types: None key -> None variant, Some(key) -> Some(inner)
        if let Def::Option(_) = &shape.def {
            match key {
                None => {
                    // Unit key -> None variant (use set_default to mark as initialized)
                    wip = wip.set_default()?;
                    return Ok(wip);
                }
                Some(inner_key) => {
                    // Named key -> Some(inner)
                    return Ok(wip
                        .begin_some()?
                        .with(|w| self.deserialize_map_key(w, Some(inner_key), None, None))?
                        .end()?);
                }
            }
        }

        // From here on, we need an actual key name.
        // For tagged keys (e.g., @schema in Styx), use the tag (with @ prefix) as the key.
        let key = key
            .or_else(|| {
                tag.as_ref()
                    .filter(|t| !t.is_empty())
                    .map(|t| Cow::Owned(format!("@{}", t)))
            })
            .ok_or_else(|| DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "named key",
                    got: "unit key".into(),
                },
            })?;

        // For transparent types (like UserId(String)), we need to use begin_inner
        // to set the inner value. But NOT for pointer types like &str or Cow<str>
        // which are handled directly.
        let is_pointer = matches!(shape.def, Def::Pointer(_));
        if shape.inner.is_some() && !is_pointer {
            return Ok(wip
                .begin_inner()?
                .with(|w| self.deserialize_map_key(w, Some(key), None, None))?
                .end()?);
        }

        // Handle terminal cases (enum, numeric, string) via non-generic inner function
        use crate::deserializer::setters::{
            MapKeyTerminalResult, deserialize_map_key_terminal_inner,
        };
        match deserialize_map_key_terminal_inner(wip, key, self.last_span) {
            Ok(wip) => Ok(wip),
            Err(MapKeyTerminalResult::NeedsSetString { wip, s }) => self.set_string_value(wip, s),
            Err(MapKeyTerminalResult::Error(e)) => Err(e),
        }
    }
}
