extern crate alloc;

use std::borrow::Cow;

use facet_core::{ScalarType, Shape, StructKind};
use facet_reflect::Partial;

use crate::{
    DeserializeError, DeserializeErrorKind, EnumVariantHint, FormatDeserializer, ParseEvent,
    ScalarTypeHint, ScalarValue, SpanGuard,
};

/// Helper to get variant display name (used by deserialize_enum_dynamic_inner)
fn get_variant_display_name(variant: &'static facet_core::Variant) -> &'static str {
    variant.effective_name()
}

impl<'parser, 'input, const BORROW: bool> FormatDeserializer<'parser, 'input, BORROW> {
    /// Deserialize any value into a DynamicValue type (e.g., facet_value::Value).
    ///
    /// This handles all value types by inspecting the parse events and calling
    /// the appropriate methods on the Partial, which delegates to the DynamicValue vtable.
    pub(crate) fn deserialize_dynamic_value(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        self.parser.hint_dynamic_value();
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
                wip = wip.init_list()?;

                loop {
                    let event = self.expect_peek("value or end")?;
                    if matches!(event, ParseEvent::SequenceEnd) {
                        self.expect_event("sequence end")?;
                        break;
                    }

                    wip = wip
                        .begin_list_item()?
                        .with(|w| self.deserialize_dynamic_value(w))?
                        .end()?;
                }
            }
            ParseEvent::StructStart(_) => {
                // Object/map/table
                self.expect_event("struct start")?; // consume '{'
                wip = wip.init_map()?;

                loop {
                    let event = self.expect_peek("field key or end")?;
                    if matches!(event, ParseEvent::StructEnd) {
                        self.expect_event("struct end")?;
                        break;
                    }

                    // Parse the key
                    let key_event = self.expect_event("field key")?;
                    let key = match key_event {
                        ParseEvent::FieldKey(field_key) => {
                            // For dynamic values, unit keys become "@"
                            field_key
                                .name
                                .map(|n| n.into_owned())
                                .unwrap_or_else(|| "@".to_owned())
                        }
                        _ => {
                            return Err(DeserializeError {
                                span: Some(self.last_span),
                                path: Some(wip.path()),
                                kind: DeserializeErrorKind::UnexpectedToken {
                                    expected: "field key",
                                    got: key_event.kind_name().into(),
                                },
                            });
                        }
                    };

                    // Begin the object entry and deserialize the value
                    wip = wip
                        .begin_object_entry(&key)?
                        .with(|w| self.deserialize_dynamic_value(w))?
                        .end()?;
                }
            }
            _ => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "scalar, sequence, or struct",
                        got: event.kind_name().into(),
                    },
                });
            }
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_struct_dynamic(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        fields: &'static [facet_core::Field],
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        self.parser.hint_struct_fields(fields.len());

        let event = self.expect_event("struct start")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "struct",
                    got: event.kind_name().into(),
                },
            ));
        }

        wip = wip.init_map()?;

        for field in fields {
            let field_shape = field.shape.get();
            let event = self.expect_event("field")?;
            match event {
                ParseEvent::OrderedField | ParseEvent::FieldKey(_) => {
                    let key = field.rename.unwrap_or(field.name);
                    wip = wip
                        .begin_object_entry(key)?
                        .with(|w| self.deserialize_value_recursive(w, field_shape))?
                        .end()?;
                }
                ParseEvent::StructEnd => break,
                _ => {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "field or struct end",
                            got: event.kind_name().into(),
                        },
                    ));
                }
            }
        }

        // Consume remaining StructEnd if needed
        if let Ok(event) = self.expect_peek("struct end")
            && matches!(event, ParseEvent::StructEnd)
        {
            let _ = self.expect_event("struct end")?;
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_tuple_dynamic(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        fields: &'static [facet_core::Field],
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        self.parser.hint_struct_fields(fields.len());

        let event = self.expect_event("tuple start")?;
        if !matches!(
            event,
            ParseEvent::StructStart(_) | ParseEvent::SequenceStart(_)
        ) {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "tuple",
                    got: event.kind_name().into(),
                },
            ));
        }

        wip = wip.init_list()?;

        for field in fields {
            let field_shape = field.shape.get();
            let event = self.expect_event("tuple element")?;
            match event {
                ParseEvent::OrderedField | ParseEvent::FieldKey(_) => {
                    wip = wip
                        .begin_list_item()?
                        .with(|w| self.deserialize_value_recursive(w, field_shape))?
                        .end()?;
                }
                ParseEvent::StructEnd | ParseEvent::SequenceEnd => break,
                _ => {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "tuple element or end",
                            got: event.kind_name().into(),
                        },
                    ));
                }
            }
        }

        if let Ok(event) = self.expect_peek("tuple end")
            && matches!(event, ParseEvent::StructEnd | ParseEvent::SequenceEnd)
        {
            let _ = self.expect_event("tuple end")?;
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_enum_dynamic(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        enum_def: &'static facet_core::EnumType,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Build and send the hint
        let variants: alloc::vec::Vec<EnumVariantHint> = enum_def
            .variants
            .iter()
            .map(|v| EnumVariantHint {
                name: v.effective_name(),
                kind: v.data.kind,
                field_count: v.data.fields.len(),
            })
            .collect();
        self.parser.hint_enum(&variants);

        let event = self.expect_event("enum")?;

        match event {
            ParseEvent::Scalar(ScalarValue::Str(s)) => {
                // Unit variant as string (self-describing formats)
                wip = self.set_string_value(wip, s)?;
            }
            ParseEvent::Scalar(ScalarValue::I64(i)) => {
                wip = wip.set(i)?;
            }
            ParseEvent::Scalar(ScalarValue::U64(u)) => {
                wip = wip.set(u)?;
            }
            ParseEvent::VariantTag(input_tag) => {
                // `input_tag`: the variant name as it appeared in the input (e.g. Some("SomethingUnknown"))
                //              or None for unit tags (bare `@` in Styx)
                // `variant.name`: the Rust identifier of the matched variant (e.g. "Other")
                //
                // These differ when using #[facet(other)] to catch unknown variants.

                // Find variant by display name (respecting rename) or fall back to #[facet(other)]
                let (variant, is_using_other_fallback) = match input_tag {
                    Some(tag) => {
                        let is_fallback = !enum_def
                            .variants
                            .iter()
                            .any(|v| get_variant_display_name(v) == tag);
                        let variant = enum_def
                            .variants
                            .iter()
                            .find(|v| get_variant_display_name(v) == tag)
                            .or_else(|| enum_def.variants.iter().find(|v| v.is_other()))
                            .ok_or_else(|| {
                                self.mk_err(
                                    &wip,
                                    DeserializeErrorKind::UnknownVariant {
                                        variant: Cow::Owned(tag.to_owned()),
                                        enum_shape: wip.shape(),
                                    },
                                )
                            })?;
                        (variant, is_fallback)
                    }
                    None => {
                        // Unit tag - must use #[facet(other)] fallback
                        let variant =
                            enum_def
                                .variants
                                .iter()
                                .find(|v| v.is_other())
                                .ok_or_else(|| {
                                    self.mk_err(
                                        &wip,
                                        DeserializeErrorKind::Unsupported {
                                            message: "unit tag requires #[facet(other)] fallback"
                                                .into(),
                                        },
                                    )
                                })?;
                        (variant, true)
                    }
                };

                match variant.data.kind {
                    StructKind::Unit => {
                        if is_using_other_fallback {
                            // #[facet(other)] fallback: preserve the original input tag
                            // so that "SomethingUnknown" round-trips correctly
                            if let Some(tag) = input_tag {
                                wip = self.set_string_value(wip, Cow::Borrowed(tag))?;
                            } else {
                                // Unit tag - set to default (None for Option<String>)
                                wip = wip.set_default()?;
                            }
                        } else {
                            // Direct match: use effective_name (wire format name)
                            wip = self
                                .set_string_value(wip, Cow::Borrowed(variant.effective_name()))?;
                        }
                    }
                    StructKind::TupleStruct | StructKind::Tuple => {
                        if variant.data.fields.len() == 1 {
                            wip = wip.init_map()?;
                            wip = wip
                                .begin_object_entry(variant.effective_name())?
                                .with(|w| {
                                    self.deserialize_value_recursive(
                                        w,
                                        variant.data.fields[0].shape.get(),
                                    )
                                })?
                                .end()?;
                        } else {
                            wip = wip.init_map()?;
                            wip = wip
                                .begin_object_entry(variant.effective_name())?
                                .with(|w| self.deserialize_tuple_dynamic(w, variant.data.fields))?
                                .end()?;
                        }
                    }
                    StructKind::Struct => {
                        wip = wip.init_map()?;
                        wip = wip
                            .begin_object_entry(variant.effective_name())?
                            .with(|w| self.deserialize_struct_dynamic(w, variant.data.fields))?
                            .end()?;
                    }
                }
            }
            ParseEvent::StructStart(_) => {
                // Non-self-describing formats emit enum as {variant_name: value}
                // The parser has already parsed the discriminant and will emit
                // FieldKey events for the variant name
                wip = self.deserialize_enum_as_struct(wip, enum_def)?;
            }
            _ => {
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "enum variant",
                        got: event.kind_name().into(),
                    },
                ));
            }
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_scalar_dynamic(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        hint_shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        let hint = match hint_shape.scalar_type() {
            Some(ScalarType::Bool) => Some(ScalarTypeHint::Bool),
            Some(ScalarType::U8) => Some(ScalarTypeHint::U8),
            Some(ScalarType::U16) => Some(ScalarTypeHint::U16),
            Some(ScalarType::U32) => Some(ScalarTypeHint::U32),
            Some(ScalarType::U64) => Some(ScalarTypeHint::U64),
            Some(ScalarType::U128) => Some(ScalarTypeHint::U128),
            Some(ScalarType::USize) => Some(ScalarTypeHint::Usize),
            Some(ScalarType::I8) => Some(ScalarTypeHint::I8),
            Some(ScalarType::I16) => Some(ScalarTypeHint::I16),
            Some(ScalarType::I32) => Some(ScalarTypeHint::I32),
            Some(ScalarType::I64) => Some(ScalarTypeHint::I64),
            Some(ScalarType::I128) => Some(ScalarTypeHint::I128),
            Some(ScalarType::ISize) => Some(ScalarTypeHint::Isize),
            Some(ScalarType::F32) => Some(ScalarTypeHint::F32),
            Some(ScalarType::F64) => Some(ScalarTypeHint::F64),
            Some(ScalarType::Char) => Some(ScalarTypeHint::Char),
            Some(ScalarType::String | ScalarType::CowStr) => Some(ScalarTypeHint::String),
            Some(ScalarType::Str) => Some(ScalarTypeHint::String),
            _ if hint_shape.is_from_str() => Some(ScalarTypeHint::String),
            _ => None,
        };
        if let Some(h) = hint {
            self.parser.hint_scalar_type(h);
        }

        let event = self.expect_event("scalar")?;

        match event {
            ParseEvent::Scalar(scalar) => match scalar {
                ScalarValue::Null => {
                    wip = wip.set_default()?;
                }
                ScalarValue::Bool(b) => {
                    wip = wip.set(b)?;
                }
                ScalarValue::Char(c) => {
                    wip = self.set_string_value(wip, Cow::Owned(c.to_string()))?;
                }
                ScalarValue::I64(i) => {
                    wip = wip.set(i)?;
                }
                ScalarValue::U64(u) => {
                    wip = wip.set(u)?;
                }
                ScalarValue::I128(i) => {
                    wip = self.set_string_value(wip, Cow::Owned(i.to_string()))?;
                }
                ScalarValue::U128(u) => {
                    wip = self.set_string_value(wip, Cow::Owned(u.to_string()))?;
                }
                ScalarValue::F64(f) => {
                    wip = wip.set(f)?;
                }
                ScalarValue::Str(s) => {
                    wip = self.set_string_value(wip, s)?;
                }
                ScalarValue::Bytes(b) => {
                    wip = self.set_bytes_value(wip, b)?;
                }
                ScalarValue::Unit => {
                    // Unit value - set to default/unit value
                    wip = wip.set_default()?;
                }
            },
            _ => {
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "scalar",
                        got: event.kind_name().into(),
                    },
                ));
            }
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_list_dynamic(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        element_shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        self.parser.hint_sequence();

        let event = self.expect_event("sequence start")?;
        if !matches!(event, ParseEvent::SequenceStart(_)) {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "sequence",
                    got: event.kind_name().into(),
                },
            ));
        }

        wip = wip.init_list()?;

        loop {
            let event = self.expect_peek("element or sequence end")?;
            if matches!(event, ParseEvent::SequenceEnd) {
                let _ = self.expect_event("sequence end")?;
                break;
            }

            wip = wip
                .begin_list_item()?
                .with(|w| self.deserialize_value_recursive(w, element_shape))?
                .end()?;
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_array_dynamic(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        element_shape: &'static Shape,
        len: usize,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        self.parser.hint_array(len);

        let event = self.expect_event("array start")?;
        if !matches!(event, ParseEvent::SequenceStart(_)) {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "array",
                    got: event.kind_name().into(),
                },
            ));
        }

        wip = wip.init_list()?;

        for _ in 0..len {
            wip = wip
                .begin_list_item()?
                .with(|w| self.deserialize_value_recursive(w, element_shape))?
                .end()?;
        }

        let event = self.expect_event("array end")?;
        if !matches!(event, ParseEvent::SequenceEnd) {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "array end",
                    got: event.kind_name().into(),
                },
            ));
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_map_dynamic(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        key_shape: &'static Shape,
        value_shape: &'static Shape,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        self.parser.hint_map();

        let event = self.expect_event("map start")?;
        if !matches!(
            event,
            ParseEvent::SequenceStart(_) | ParseEvent::StructStart(_)
        ) {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "map",
                    got: event.kind_name().into(),
                },
            ));
        }

        wip = wip.init_map()?;

        let key_hint = match key_shape.scalar_type() {
            Some(ScalarType::String | ScalarType::CowStr) => Some(ScalarTypeHint::String),
            Some(ScalarType::Str) => Some(ScalarTypeHint::String),
            Some(
                ScalarType::I64
                | ScalarType::I32
                | ScalarType::I16
                | ScalarType::I8
                | ScalarType::ISize,
            ) => Some(ScalarTypeHint::I64),
            Some(
                ScalarType::U64
                | ScalarType::U32
                | ScalarType::U16
                | ScalarType::U8
                | ScalarType::USize,
            ) => Some(ScalarTypeHint::U64),
            _ => None,
        };

        loop {
            let event = self.expect_peek("map entry or end")?;
            if matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                let _ = self.expect_event("map end")?;
                break;
            }

            if let Some(h) = key_hint {
                self.parser.hint_scalar_type(h);
            }
            let key_event = self.expect_event("map key")?;
            let key_str: Cow<'_, str> = match key_event {
                ParseEvent::Scalar(ScalarValue::Str(s)) => s,
                ParseEvent::Scalar(ScalarValue::I64(i)) => Cow::Owned(i.to_string()),
                ParseEvent::Scalar(ScalarValue::U64(u)) => Cow::Owned(u.to_string()),
                ParseEvent::FieldKey(k) => k.name.unwrap_or(Cow::Borrowed("@")),
                _ => {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "map key",
                            got: key_event.kind_name().into(),
                        },
                    ));
                }
            };

            wip = wip
                .begin_object_entry(&key_str)?
                .with(|w| self.deserialize_value_recursive(w, value_shape))?
                .end()?;
        }

        Ok(wip)
    }
}
