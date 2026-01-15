extern crate alloc;

use std::borrow::Cow;

use facet_core::{Def, StructKind, Type, UserType};
use facet_reflect::Partial;

use crate::{
    ContainerKind, DeserializeError, FormatDeserializer, FormatParser, ParseEvent, ScalarValue,
    trace,
};

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    pub(crate) fn deserialize_enum(
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
        trace!("deserialize_enum_externally_tagged called");
        let event = self.expect_peek("value")?;
        trace!(?event, "peeked event");
        // Check for unit variant (just a string)
        if let ParseEvent::Scalar(ScalarValue::Str(variant_name)) = &event {
            self.expect_event("value")?;
            wip = wip
                .select_variant_named(variant_name)
                .map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Check for VariantTag (self-describing formats like Styx)
        if let ParseEvent::VariantTag(tag_name) = &event {
            let tag_name = *tag_name;
            self.expect_event("value")?; // consume VariantTag

            // Look up the real variant name respecting rename attributes
            let enum_def = match &wip.shape().ty {
                Type::User(UserType::Enum(e)) => e,
                _ => return Err(DeserializeError::Unsupported("expected enum".into())),
            };

            // For unit tags (None), go straight to #[facet(other)] fallback
            // For named tags, try exact match first, then fall back to #[facet(other)]
            let (variant_name, is_using_other_fallback) = match tag_name {
                Some(name) => {
                    let by_display = Self::find_variant_by_display_name(enum_def, name);
                    let is_fallback = by_display.is_none();
                    let variant = by_display
                        .or_else(|| {
                            enum_def
                                .variants
                                .iter()
                                .find(|v| v.is_other())
                                .map(|v| v.effective_name())
                        })
                        .ok_or_else(|| DeserializeError::TypeMismatch {
                            expected: "known enum variant",
                            got: format!("@{}", name),
                            span: self.last_span,
                            path: None,
                        })?;
                    (variant, is_fallback)
                }
                None => {
                    // Unit tag - must use #[facet(other)] fallback
                    let variant = enum_def
                        .variants
                        .iter()
                        .find(|v| v.is_other())
                        .map(|v| v.effective_name())
                        .ok_or_else(|| DeserializeError::TypeMismatch {
                            expected: "#[facet(other)] fallback variant for unit tag",
                            got: "@".to_string(),
                            span: self.last_span,
                            path: None,
                        })?;
                    (variant, true)
                }
            };

            wip = wip
                .select_variant_named(variant_name)
                .map_err(DeserializeError::reflect)?;

            // For #[facet(other)] variants, check for #[facet(tag)] and #[facet(content)] fields
            if is_using_other_fallback {
                wip = self.deserialize_other_variant_with_captured_tag(wip, tag_name)?;
            } else {
                // Deserialize the variant content normally
                wip = self.deserialize_enum_variant_content(wip)?;
            }
            return Ok(wip);
        }

        // Otherwise expect a struct { VariantName: ... }
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "string or struct for enum",
                got: format!("{event:?}"),
                span: self.last_span,
                path: None,
            });
        }

        self.expect_event("value")?; // consume StructStart

        // Get the variant name from the field key
        let event = self.expect_event("value")?;
        let field_key_name = match event {
            ParseEvent::FieldKey(key) => key.name,
            other => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "variant name",
                    got: format!("{other:?}"),
                    span: self.last_span,
                    path: None,
                });
            }
        };

        // Look up the real variant name respecting rename attributes, with fallback to #[facet(other)]
        let enum_def = match &wip.shape().ty {
            Type::User(UserType::Enum(e)) => e,
            _ => return Err(DeserializeError::Unsupported("expected enum".into())),
        };
        let is_using_other_fallback =
            Self::find_variant_by_display_name(enum_def, &field_key_name).is_none();
        let variant_name = Self::find_variant_by_display_name(enum_def, &field_key_name)
            .or_else(|| {
                enum_def
                    .variants
                    .iter()
                    .find(|v| v.is_other())
                    .map(|v| v.effective_name())
            })
            .ok_or_else(|| DeserializeError::TypeMismatch {
                expected: "known enum variant",
                got: format!("{}", field_key_name),
                span: self.last_span,
                path: None,
            })?;

        wip = wip
            .select_variant_named(variant_name)
            .map_err(DeserializeError::reflect)?;

        // For #[facet(other)] fallback variants, if the content is Unit, use the field key name as the value
        if is_using_other_fallback {
            let event = self.expect_peek("value")?;
            if matches!(event, ParseEvent::Scalar(ScalarValue::Unit)) {
                self.expect_event("value")?; // consume Unit
                // Enter field 0 of the newtype variant (e.g., Type(String))
                wip = wip.begin_nth_field(0).map_err(DeserializeError::reflect)?;
                wip = self.set_string_value(wip, Cow::Owned(field_key_name.into_owned()))?;
                wip = wip.end().map_err(DeserializeError::reflect)?;
            } else {
                wip = self.deserialize_enum_variant_content(wip)?;
            }
        } else {
            // Deserialize the variant content normally
            wip = self.deserialize_enum_variant_content(wip)?;
        }

        // Consume StructEnd
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructEnd) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct end after enum variant",
                got: format!("{event:?}"),
                span: self.last_span,
                path: None,
            });
        }

        Ok(wip)
    }

    fn deserialize_enum_internally_tagged(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        tag_key: &str,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
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
                span: self.last_span,
                path: None,
            })?
            .to_string();

        // Step 2: Consume StructStart
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct for internally tagged enum",
                got: format!("{event:?}"),
                span: self.last_span,
                path: None,
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
                span: self.last_span,
                path: None,
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
                            span: self.last_span,
                            path: None,
                        });
                    }
                }
            }
            return Ok(wip);
        }

        // Process all fields (they can come in any order now)
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

                    // Look up field in variant's fields by name/alias
                    let field_info = variant_fields
                        .iter()
                        .enumerate()
                        .find(|(_, f)| Self::field_matches(f, key.name.as_ref()));

                    if let Some((idx, _field)) = field_info {
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else {
                        // Unknown field - skip
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                    }
                }
                other => {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "field key or struct end",
                        got: format!("{other:?}"),
                        span: self.last_span,
                        path: None,
                    });
                }
            }
        }

        // Defaults for missing fields are applied automatically by facet-reflect's
        // fill_defaults() when build() or end() is called.

        Ok(wip)
    }

    /// Deserialize enum represented as struct (used by postcard and similar formats).
    ///
    /// The parser emits the enum as `{variant_name: content}` where content depends
    /// on the variant kind. The parser auto-handles struct/tuple variants by pushing
    /// appropriate state, so we just consume the events it produces.
    pub(crate) fn deserialize_enum_as_struct(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        enum_def: &'static facet_core::EnumType,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        // Get the variant name from FieldKey
        let field_event = self.expect_event("enum field key")?;
        let variant_name = match field_event {
            ParseEvent::FieldKey(key) => key.name,
            ParseEvent::StructEnd => {
                // Empty struct - this shouldn't happen for valid enums
                return Err(DeserializeError::Unsupported(
                    "unexpected empty struct for enum".into(),
                ));
            }
            _ => {
                return Err(DeserializeError::TypeMismatch {
                    expected: "field key for enum variant",
                    got: format!("{field_event:?}"),
                    span: self.last_span,
                    path: Some(self.path_clone()),
                });
            }
        };

        // Find the variant definition
        let variant = enum_def
            .variants
            .iter()
            .find(|v| v.name == variant_name.as_ref())
            .ok_or_else(|| {
                DeserializeError::Unsupported(format!("unknown variant: {variant_name}"))
            })?;

        match variant.data.kind {
            StructKind::Unit => {
                // Unit variant - the parser will emit StructEnd next
                wip = self.set_string_value(wip, variant_name)?;
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                wip = wip.init_map().map_err(DeserializeError::reflect)?;
                wip = wip
                    .begin_object_entry(variant.name)
                    .map_err(DeserializeError::reflect)?;
                if variant.data.fields.len() == 1 {
                    // Newtype variant - single field content, no wrapper
                    wip =
                        self.deserialize_value_recursive(wip, variant.data.fields[0].shape.get())?;
                } else {
                    // Multi-field tuple variant - parser emits SequenceStart
                    let seq_event = self.expect_event("tuple variant start")?;
                    if !matches!(seq_event, ParseEvent::SequenceStart(_)) {
                        return Err(DeserializeError::TypeMismatch {
                            expected: "SequenceStart for tuple variant",
                            got: format!("{seq_event:?}"),
                            span: self.last_span,
                            path: Some(self.path_clone()),
                        });
                    }

                    wip = wip.init_list().map_err(DeserializeError::reflect)?;
                    for field in variant.data.fields {
                        // The parser's InSequence state will emit OrderedField for each element
                        let _elem_event = self.expect_event("tuple element")?;
                        wip = wip.begin_list_item().map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_value_recursive(wip, field.shape.get())?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    }

                    let seq_end = self.expect_event("tuple variant end")?;
                    if !matches!(seq_end, ParseEvent::SequenceEnd) {
                        return Err(DeserializeError::TypeMismatch {
                            expected: "SequenceEnd for tuple variant",
                            got: format!("{seq_end:?}"),
                            span: self.last_span,
                            path: Some(self.path_clone()),
                        });
                    }
                    wip = wip.end().map_err(DeserializeError::reflect)?;
                }
                wip = wip.end().map_err(DeserializeError::reflect)?;
            }
            StructKind::Struct => {
                // The parser auto-emits StructStart and pushes InStruct state
                let struct_event = self.expect_event("struct variant start")?;
                if !matches!(struct_event, ParseEvent::StructStart(_)) {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "StructStart for struct variant",
                        got: format!("{struct_event:?}"),
                        span: self.last_span,
                        path: Some(self.path_clone()),
                    });
                }

                wip = wip.init_map().map_err(DeserializeError::reflect)?;
                wip = wip
                    .begin_object_entry(variant.name)
                    .map_err(DeserializeError::reflect)?;
                // begin_map() initializes the entry's value as an Object (doesn't push a frame)
                wip = wip.init_map().map_err(DeserializeError::reflect)?;

                // Deserialize each field - parser will emit OrderedField for each
                for field in variant.data.fields {
                    let field_event = self.expect_event("struct field")?;
                    match field_event {
                        ParseEvent::OrderedField | ParseEvent::FieldKey(_) => {
                            let key = field.rename.unwrap_or(field.name);
                            wip = wip
                                .begin_object_entry(key)
                                .map_err(DeserializeError::reflect)?;
                            wip = self.deserialize_value_recursive(wip, field.shape.get())?;
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        ParseEvent::StructEnd => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "field",
                                got: "StructEnd (struct ended too early)".into(),
                                span: self.last_span,
                                path: Some(self.path_clone()),
                            });
                        }
                        _ => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "field",
                                got: format!("{field_event:?}"),
                                span: self.last_span,
                                path: Some(self.path_clone()),
                            });
                        }
                    }
                }

                // Consume inner StructEnd
                let inner_end = self.expect_event("struct variant inner end")?;
                if !matches!(inner_end, ParseEvent::StructEnd) {
                    return Err(DeserializeError::TypeMismatch {
                        expected: "StructEnd for struct variant inner",
                        got: format!("{inner_end:?}"),
                        span: self.last_span,
                        path: Some(self.path_clone()),
                    });
                }
                // Only end the object entry (begin_map doesn't push a frame)
                wip = wip.end().map_err(DeserializeError::reflect)?;
            }
        }

        // Consume the outer StructEnd
        let end_event = self.expect_event("enum struct end")?;
        if !matches!(end_event, ParseEvent::StructEnd) {
            return Err(DeserializeError::TypeMismatch {
                expected: "StructEnd for enum wrapper",
                got: format!("{end_event:?}"),
                span: self.last_span,
                path: Some(self.path_clone()),
            });
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_result_as_enum(
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
                span: self.last_span,
                path: None,
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
                    span: self.last_span,
                    path: None,
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
                span: self.last_span,
                path: None,
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
                span: self.last_span,
                path: None,
            });
        }

        Ok(wip)
    }

    /// Deserialize the struct fields of a variant.
    /// Expects the variant to already be selected.
    pub(crate) fn deserialize_variant_struct_fields(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_core::StructKind;

        let variant = wip
            .selected_variant()
            .ok_or_else(|| DeserializeError::TypeMismatch {
                expected: "selected variant",
                got: "no variant selected".into(),
                span: self.last_span,
                path: None,
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
                span: self.last_span,
                path: None,
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
                    // Look up field in variant's fields by name/alias
                    let field_info = variant_fields
                        .iter()
                        .enumerate()
                        .find(|(_, f)| Self::field_matches(f, key.name.as_ref()));

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
                        span: self.last_span,
                        path: None,
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
                    span: self.last_span,
                    path: None,
                });
            }
        }

        Ok(wip)
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
                span: self.last_span,
                path: None,
            })?
            .to_string();

        // Step 2: Consume StructStart
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct for adjacently tagged enum",
                got: format!("{event:?}"),
                span: self.last_span,
                path: None,
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
                        span: self.last_span,
                        path: None,
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
                    span: self.last_span,
                    path: None,
                });
            }
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_enum_variant_content(
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
                span: self.last_span,
                path: None,
            })?;

        let variant_kind = variant.data.kind;
        let variant_fields = variant.data.fields;

        match variant_kind {
            StructKind::Unit => {
                // Unit variant - normally nothing to deserialize
                // But some formats may emit extra tokens:
                // - TOML with [VariantName]: emits StructStart/StructEnd
                // - STYX with @Foo: emits Scalar(Unit)
                let event = self.expect_peek("value")?;
                if matches!(event, ParseEvent::Scalar(ScalarValue::Unit)) {
                    self.expect_event("value")?; // consume Unit
                } else if matches!(event, ParseEvent::StructStart(_)) {
                    self.expect_event("value")?; // consume StructStart
                    // Expect immediate StructEnd for empty struct
                    let end_event = self.expect_event("value")?;
                    if !matches!(end_event, ParseEvent::StructEnd) {
                        return Err(DeserializeError::TypeMismatch {
                            expected: "empty struct for unit variant",
                            got: format!("{end_event:?}"),
                            span: self.last_span,
                            path: None,
                        });
                    }
                }
                Ok(wip)
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                if variant_fields.len() == 1 {
                    // Newtype variant - content is the single field's value
                    wip = wip.begin_nth_field(0).map_err(DeserializeError::reflect)?;
                    wip = self.deserialize_into(wip)?;
                    wip = wip.end().map_err(DeserializeError::reflect)?;
                } else {
                    // Multi-field tuple variant - expect array or struct (for XML/TOML with numeric keys)
                    let event = self.expect_event("value")?;

                    // Accept SequenceStart (JSON arrays) or Object StructStart (TOML/JSON with numeric keys like "0", "1")
                    let struct_mode = match event {
                        ParseEvent::SequenceStart(_) => false,
                        // Accept objects with numeric keys as valid tuple representations
                        ParseEvent::StructStart(ContainerKind::Object) => true,
                        ParseEvent::StructStart(kind) => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "array",
                                got: kind.name().into(),
                                span: self.last_span,
                                path: None,
                            });
                        }
                        _ => {
                            return Err(DeserializeError::TypeMismatch {
                                expected: "sequence for tuple variant",
                                got: format!("{event:?}"),
                                span: self.last_span,
                                path: None,
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
                            span: self.last_span,
                            path: None,
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
                        span: self.last_span,
                        path: None,
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
                            // Look up field in variant's fields by name/alias
                            let field_info = variant_fields
                                .iter()
                                .enumerate()
                                .find(|(_, f)| Self::field_matches(f, key.name.as_ref()));

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
                                span: self.last_span,
                                path: None,
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
                            span: self.last_span,
                            path: None,
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

        if let Some(ParseEvent::Scalar(scalar)) = event {
            let span = self.last_span;
            wip = match scalar {
                ScalarValue::I64(discriminant) => {
                    wip.select_variant(discriminant)
                        .map_err(|error| DeserializeError::Reflect {
                            error,
                            span,
                            path: None,
                        })?
                }
                ScalarValue::U64(discriminant) => {
                    wip.select_variant(discriminant as i64).map_err(|error| {
                        DeserializeError::Reflect {
                            error,
                            span,
                            path: None,
                        }
                    })?
                }
                ScalarValue::Str(str_discriminant) => {
                    let discriminant =
                        str_discriminant
                            .parse()
                            .map_err(|_| DeserializeError::TypeMismatch {
                                expected: "String representing an integer (i64)",
                                got: str_discriminant.to_string(),
                                span: self.last_span,
                                path: None,
                            })?;
                    wip.select_variant(discriminant)
                        .map_err(|error| DeserializeError::Reflect {
                            error,
                            span,
                            path: None,
                        })?
                }
                _ => {
                    return Err(DeserializeError::Unsupported(
                        "Unexpected ScalarValue".to_string(),
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
                        .select_variant_named(variant.effective_name())
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
                        let variant_display_name = variant.effective_name();

                        if s.as_ref() == variant_display_name {
                            wip = wip
                                .select_variant_named(variant.effective_name())
                                .map_err(DeserializeError::reflect)?;
                            // Consume the string
                            self.expect_event("value")?;
                            return Ok(wip);
                        }
                    }
                }

                // Try scalar variants that match the scalar type
                for (variant, inner_shape) in &variants_by_format.scalar_variants {
                    if self.scalar_matches_shape(scalar, inner_shape) {
                        wip = wip
                            .select_variant_named(variant.effective_name())
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_enum_variant_content(wip)?;
                        return Ok(wip);
                    }
                }

                // Try other scalar variants that don't match primitive types.
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
                            .select_variant_named(variant.effective_name())
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
                    span: self.last_span,
                    path: None,
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
                                .select_variant_named(variant.effective_name())
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
                        .select_variant_named(variant.effective_name())
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
                span: self.last_span,
                path: None,
            }),
        }
    }

    /// Deserialize an `#[facet(other)]` variant that may have `#[facet(tag)]` and `#[facet(content)]` fields.
    ///
    /// This is called when a VariantTag event didn't match any known variant and we're falling
    /// back to an `#[facet(other)]` variant. The tag name is captured and stored in the
    /// `#[facet(tag)]` field, while the payload is deserialized into the `#[facet(content)]` field.
    ///
    /// `captured_tag` is `None` for unit tags (bare `@` in Styx).
    fn deserialize_other_variant_with_captured_tag(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        captured_tag: Option<&'input str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        let variant = wip
            .selected_variant()
            .ok_or_else(|| DeserializeError::TypeMismatch {
                expected: "selected variant",
                got: "no variant selected".into(),
                span: self.last_span,
                path: None,
            })?;

        let variant_fields = variant.data.fields;

        // Find tag and content field indices
        let tag_field_idx = variant_fields.iter().position(|f| f.is_variant_tag());
        let content_field_idx = variant_fields.iter().position(|f| f.is_variant_content());

        // If no tag field and no content field, fall back to regular deserialization
        if tag_field_idx.is_none() && content_field_idx.is_none() {
            return self.deserialize_enum_variant_content(wip);
        }

        // Set the tag field to the captured tag name (or None for unit tags)
        if let Some(idx) = tag_field_idx {
            wip = wip
                .begin_nth_field(idx)
                .map_err(DeserializeError::reflect)?;
            match captured_tag {
                Some(tag) => {
                    wip = self.set_string_value(wip, Cow::Borrowed(tag))?;
                }
                None => {
                    // Unit tag - set the field to its default (None for Option<String>)
                    wip = wip.set_default().map_err(DeserializeError::reflect)?;
                }
            }
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        // Deserialize the content into the content field (if present)
        if let Some(idx) = content_field_idx {
            wip = wip
                .begin_nth_field(idx)
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        } else {
            // No content field - the payload must be Unit
            let event = self.expect_peek("value")?;
            if matches!(event, ParseEvent::Scalar(ScalarValue::Unit)) {
                self.expect_event("value")?; // consume Unit
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "unit payload for #[facet(other)] variant without #[facet(content)]",
                    got: format!("{event:?}"),
                    span: self.last_span,
                    path: None,
                });
            }
        }

        Ok(wip)
    }
}
