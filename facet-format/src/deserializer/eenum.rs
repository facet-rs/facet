use std::borrow::Cow;

use facet_core::{Characteristic, Def, Field, StructKind, Type, UserType};
use facet_reflect::Partial;
use facet_solver::VariantsByFormat;

use crate::{
    ContainerKind, DeserializeError, DeserializeErrorKind, EnumVariantHint, FieldEvidence,
    FormatDeserializer, ParseEventKind, ScalarValue, SpanGuard, deserializer::entry::MetaSource,
    deserializer::scalar_matches::scalar_matches_shape,
};

impl<'parser, 'input, const BORROW: bool> FormatDeserializer<'parser, 'input, BORROW> {
    pub(crate) fn deserialize_enum(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        let shape = wip.shape();

        // Cow-like enums serialize/deserialize transparently as their inner value,
        // without any variant wrapper or discriminant. Check this BEFORE hint_enum
        // and is_numeric because cow enums may have #[repr(u8)] but should still
        // be transparent.
        if shape.is_cow() {
            return self.deserialize_cow_enum(wip);
        }

        // Hint to non-self-describing parsers what variant metadata to expect
        if let Type::User(UserType::Enum(enum_def)) = &shape.ty {
            let variant_hints: Vec<EnumVariantHint> = enum_def
                .variants
                .iter()
                .map(|v| EnumVariantHint {
                    name: v.effective_name(),
                    kind: v.data.kind,
                    field_count: v.data.fields.len(),
                })
                .collect();
            if self.is_non_self_describing() {
                self.parser.hint_enum(&variant_hints);
            }
        }

        // Check for different tagging modes
        let tag_attr = shape.get_tag_attr();
        let content_attr = shape.get_content_attr();
        let is_numeric = shape.is_numeric();
        let is_untagged = shape.is_untagged();

        if is_numeric & tag_attr.is_none() {
            return self.deserialize_numeric_enum(wip);
        }

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

    fn deserialize_numeric_enum(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        let event = self.peek_event_opt()?;

        if let Some(ref event) = event
            && let ParseEventKind::Scalar(scalar) = &event.kind
        {
            wip = match scalar {
                ScalarValue::I64(discriminant) => wip.select_variant(*discriminant)?,
                ScalarValue::U64(discriminant) => wip.select_variant(*discriminant as i64)?,
                ScalarValue::Str(str_discriminant) => {
                    let discriminant = str_discriminant.parse().map_err(|_| DeserializeError {
                        span: Some(self.last_span),
                        path: Some(wip.path()),
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "string representing an integer (i64)",
                            got: str_discriminant.to_string().into(),
                        },
                    })?;
                    wip.select_variant(discriminant)?
                }
                _ => {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::Unsupported {
                            message: "unexpected scalar for numeric enum".into(),
                        },
                    ));
                }
            };
            self.expect_event("numeric enum value")?;
            Ok(wip)
        } else {
            Err(self.mk_err(
                &wip,
                DeserializeErrorKind::Unsupported {
                    message: "expected integer value for numeric enum".into(),
                },
            ))
        }
    }

    fn deserialize_enum_externally_tagged(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        trace!("deserialize_enum_externally_tagged called");
        let event = self.expect_peek("value")?;
        trace!(?event, "peeked event");

        // Check for any bare scalar (string, bool, int, etc.)
        if let ParseEventKind::Scalar(scalar) = &event.kind {
            let shape = wip.shape();
            let enum_def = match &shape.ty {
                Type::User(UserType::Enum(e)) => e,
                _ => {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::TypeMismatch {
                            expected: shape,
                            got: "non-enum type".into(),
                        },
                    ));
                }
            };

            // Use precomputed lookups from EnumPlan
            let enum_plan = wip.enum_plan().unwrap();

            // For string scalars, first try to match as a unit variant name
            if let ScalarValue::Str(variant_name) = scalar {
                // Use VariantLookup for fast lookup
                if let Some(matched_idx) = enum_plan.variant_lookup.find(variant_name) {
                    // Found a matching unit variant
                    let matched_name = enum_def.variants[matched_idx].effective_name();
                    let actual_variant =
                        cow_redirect_variant_name::<BORROW>(enum_def, matched_name);
                    self.expect_event("value")?;
                    wip = wip.select_variant_named(actual_variant)?;
                    return Ok(wip);
                }
            }

            // No matching variant - check for #[facet(other)] fallback using precomputed index
            if let Some(other_idx) = enum_plan.other_variant_idx {
                let other_variant = &enum_def.variants[other_idx];
                let has_tag_field = other_variant.data.fields.iter().any(|f| f.is_variant_tag());
                let has_content_field = other_variant
                    .data
                    .fields
                    .iter()
                    .any(|f| f.is_variant_content());

                // Use select_nth_variant since #[facet(other)] variants are excluded from variant_lookup
                if has_tag_field || has_content_field {
                    wip = wip.select_nth_variant(other_idx)?;
                    wip = self.deserialize_other_variant_with_captured_tag(wip, None)?;
                } else {
                    // Don't consume the event - let deserialize_into handle it properly
                    // This ensures metadata containers like Meta<String> work correctly
                    wip = wip.select_nth_variant(other_idx)?;
                    // Don't consume the event - let deserialize_into handle it properly.
                    // This ensures metadata containers like Meta<String> work correctly
                    // by reading metadata fresh from the unconsumed event.
                    wip = wip
                        .begin_nth_field(0)?
                        .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                        .end()?;
                }
                return Ok(wip);
            }

            // No fallback available - error
            return Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "known enum variant",
                    got: scalar.to_display_string().into(),
                },
            });
        }

        // Check for VariantTag (self-describing formats like Styx)
        if let ParseEventKind::VariantTag(tag_name) = &event.kind {
            let tag_name = *tag_name;
            self.expect_event("value")?; // consume VariantTag

            let shape = wip.shape();
            // Verify this is an enum type
            if !matches!(&shape.ty, Type::User(UserType::Enum(_))) {
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::TypeMismatch {
                        expected: shape,
                        got: "non-enum type".into(),
                    },
                ));
            }

            // Use precomputed lookups from EnumPlan
            let enum_plan = wip.enum_plan().unwrap();

            let (variant_idx, is_using_other_fallback) = match tag_name {
                Some(name) => {
                    // Use VariantLookup for fast lookup
                    let found_idx = enum_plan.variant_lookup.find(name);
                    let is_fallback = found_idx.is_none();
                    let variant_idx =
                        found_idx.or(enum_plan.other_variant_idx).ok_or_else(|| {
                            DeserializeError {
                                span: Some(self.last_span),
                                path: Some(wip.path()),
                                kind: DeserializeErrorKind::UnexpectedToken {
                                    expected: "known enum variant",
                                    got: format!("@{}", name).into(),
                                },
                            }
                        })?;
                    (variant_idx, is_fallback)
                }
                None => {
                    // Use precomputed other_variant_idx
                    let variant_idx =
                        enum_plan
                            .other_variant_idx
                            .ok_or_else(|| DeserializeError {
                                span: Some(self.last_span),
                                path: Some(wip.path()),
                                kind: DeserializeErrorKind::UnexpectedToken {
                                    expected: "#[facet(other)] fallback variant for unit tag",
                                    got: "@".into(),
                                },
                            })?;
                    (variant_idx, true)
                }
            };

            // Use select_nth_variant to handle #[facet(other)] variants which are
            // excluded from variant_lookup (they should only be used as fallbacks)
            wip = wip.select_nth_variant(variant_idx)?;

            if is_using_other_fallback {
                wip = self.deserialize_other_variant_with_captured_tag(wip, tag_name)?;
            } else {
                wip = self.deserialize_enum_variant_content(wip)?;
            }
            return Ok(wip);
        }

        // Otherwise expect a struct { VariantName: ... }
        if !matches!(event.kind, ParseEventKind::StructStart(_)) {
            return Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "string or struct for enum",
                    got: event.kind_name().into(),
                },
            });
        }

        self.expect_event("value")?; // consume StructStart

        // Get the variant name from the field key
        let event = self.expect_event("value")?;
        let field_key_name = match event.kind {
            ParseEventKind::FieldKey(key) => {
                key.name().cloned().ok_or_else(|| DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "variant name",
                        got: "unit key".into(),
                    },
                })?
            }
            other => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "variant name",
                        got: other.kind_name().into(),
                    },
                });
            }
        };

        let shape = wip.shape();
        // Verify this is an enum type
        if !matches!(&shape.ty, Type::User(UserType::Enum(_))) {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::TypeMismatch {
                    expected: shape,
                    got: "non-enum type".into(),
                },
            ));
        }
        // Use precomputed lookups from EnumPlan
        let enum_plan = wip.enum_plan().unwrap();
        let found_idx = enum_plan.variant_lookup.find(&field_key_name);
        let is_using_other_fallback = found_idx.is_none();
        let variant_idx =
            found_idx
                .or(enum_plan.other_variant_idx)
                .ok_or_else(|| DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "known enum variant",
                        got: field_key_name.to_string().into(),
                    },
                })?;

        // Use select_nth_variant to handle #[facet(other)] variants which are
        // excluded from variant_lookup (they should only be used as fallbacks)
        wip = wip.select_nth_variant(variant_idx)?;

        // For #[facet(other)] fallback variants, if the content is Unit, use the field key name as the value
        if is_using_other_fallback {
            let event = self.expect_peek("value")?;
            if matches!(event.kind, ParseEventKind::Scalar(ScalarValue::Unit)) {
                self.expect_event("value")?; // consume Unit
                wip = wip
                    .begin_nth_field(0)?
                    .with(|w| self.set_string_value(w, Cow::Owned(field_key_name.into_owned())))?
                    .end()?;
            } else {
                wip = self.deserialize_enum_variant_content(wip)?;
            }
        } else {
            wip = self.deserialize_enum_variant_content(wip)?;
        }

        // Consume StructEnd
        let event = self.expect_event("value")?;
        if !matches!(event.kind, ParseEventKind::StructEnd) {
            return Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "struct end after enum variant",
                    got: event.kind_name().into(),
                },
            });
        }

        Ok(wip)
    }

    fn deserialize_enum_internally_tagged(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        tag_key: &'static str,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Step 1: Probe to find the tag value (handles out-of-order fields)
        let evidence = self.collect_evidence()?;

        // Step 2: Consume StructStart
        let event = self.expect_event("value")?;
        if !matches!(event.kind, ParseEventKind::StructStart(_)) {
            return Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "struct for internally tagged enum",
                    got: event.kind_name().into(),
                },
            });
        }

        // Step 3: Select the variant
        // For cow-like enums, redirect Borrowed -> Owned when borrowing is disabled
        let enum_def = match &wip.shape().ty {
            Type::User(UserType::Enum(e)) => e,
            _ => {
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::Unsupported {
                        message: "expected enum for internally tagged".into(),
                    },
                ));
            }
        };

        if wip.shape().is_numeric() {
            let discriminant = find_tag_discriminant(&evidence, tag_key).ok_or_else(|| {
                self.mk_err(
                    &wip,
                    DeserializeErrorKind::MissingField {
                        field: tag_key,
                        container_shape: wip.shape(),
                    },
                )
            })?;
            wip = wip.select_variant(discriminant)?;
        } else {
            let variant_name = find_tag_value(&evidence, tag_key)
                .ok_or_else(|| {
                    self.mk_err(
                        &wip,
                        DeserializeErrorKind::MissingField {
                            field: tag_key,
                            container_shape: wip.shape(),
                        },
                    )
                })?
                .to_string();
            let actual_variant = cow_redirect_variant_name::<BORROW>(enum_def, &variant_name);
            wip = wip.select_variant_named(actual_variant)?;
        }

        // Get the selected variant info
        let variant = wip.selected_variant().ok_or_else(|| DeserializeError {
            span: Some(self.last_span),
            path: Some(wip.path()),
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "selected variant",
                got: "no variant selected".into(),
            },
        })?;

        let variant_fields = variant.data.fields;

        // Check if this is a unit variant (no fields)
        if variant_fields.is_empty() || variant.data.kind == StructKind::Unit {
            // Consume remaining fields in the object
            loop {
                let event = self.expect_event("value")?;
                match event.kind {
                    ParseEventKind::StructEnd => break,
                    ParseEventKind::FieldKey(_) => {
                        self.skip_value()?;
                    }
                    other => {
                        return Err(DeserializeError {
                            span: Some(self.last_span),
                            path: Some(wip.path()),
                            kind: DeserializeErrorKind::UnexpectedToken {
                                expected: "field key or struct end",
                                got: other.kind_name().into(),
                            },
                        });
                    }
                }
            }
            return Ok(wip);
        }

        // Check if this is a single-field tuple variant containing an internally-tagged enum
        // In this case, the inner enum's fields are flattened into the outer object
        if matches!(
            variant.data.kind,
            StructKind::TupleStruct | StructKind::Tuple
        ) && variant_fields.len() == 1
        {
            let inner_shape = variant_fields[0].shape();
            if let Some(inner_tag_key) = inner_shape.get_tag_attr()
                && inner_shape.get_content_attr().is_none()
            {
                // Find the inner tag value in evidence
                let inner_variant_name = find_tag_value(&evidence, inner_tag_key)
                    .ok_or_else(|| {
                        self.mk_err(
                            &wip,
                            DeserializeErrorKind::MissingField {
                                field: inner_tag_key,
                                container_shape: inner_shape,
                            },
                        )
                    })?
                    .to_string();

                // Begin the tuple field (field 0)
                wip = wip.begin_nth_field(0)?;

                // Select the inner variant
                wip = wip.select_variant_named(&inner_variant_name)?;

                // Get the inner variant info
                let inner_variant = wip.selected_variant().ok_or_else(|| DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "selected inner variant",
                        got: "no variant selected".into(),
                    },
                })?;

                let inner_variant_fields = inner_variant.data.fields;

                // Process all fields for the inner enum
                loop {
                    let event = self.expect_event("value")?;
                    match event.kind {
                        ParseEventKind::StructEnd => break,
                        ParseEventKind::FieldKey(key) => {
                            let key_name = match key.name() {
                                Some(name) => name.as_ref(),
                                None => {
                                    self.skip_value()?;
                                    continue;
                                }
                            };

                            // Skip outer and inner tag fields
                            if key_name == tag_key || key_name == inner_tag_key {
                                self.skip_value()?;
                                continue;
                            }

                            // Look up field in the inner variant
                            let inner_variant_plan = wip.selected_variant_plan().unwrap();
                            if let Some(idx) = inner_variant_plan
                                .field_lookup
                                .find(key_name, wip.type_plan_core())
                            {
                                wip = wip
                                    .begin_nth_field(idx)?
                                    .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                                    .end()?;
                            } else {
                                // Unknown field - skip
                                self.skip_value()?;
                            }
                        }
                        other => {
                            return Err(DeserializeError {
                                span: Some(self.last_span),
                                path: Some(wip.path()),
                                kind: DeserializeErrorKind::UnexpectedToken {
                                    expected: "field key or struct end",
                                    got: other.kind_name().into(),
                                },
                            });
                        }
                    }
                }

                // Apply defaults for missing inner fields
                for (idx, field) in inner_variant_fields.iter().enumerate() {
                    if wip.is_field_set(idx)? {
                        continue;
                    }

                    let field_has_default = field.has_default();
                    let field_type_has_default = field.shape().is(Characteristic::Default);
                    let field_is_option = matches!(field.shape().def, Def::Option(_));

                    if field_has_default || field_type_has_default {
                        wip = wip.set_nth_field_to_default(idx)?;
                    } else if field_is_option {
                        wip = wip.begin_nth_field(idx)?.set_default()?.end()?;
                    } else if field.should_skip_deserializing() {
                        wip = wip.set_nth_field_to_default(idx)?;
                    }
                    // Note: don't error on missing required fields here - let the outer
                    // build() call handle that with proper error messages
                }

                // End the tuple field
                wip = wip.end()?;

                return Ok(wip);
            }
        }

        // Use precomputed has_flatten from VariantPlanMeta
        let has_flatten = wip.selected_variant_plan().unwrap().has_flatten;

        // Track currently open path segments for flatten handling: (field_name, is_option)
        let mut open_segments: Vec<(&str, bool)> = Vec::new();

        // Process all fields (they can come in any order now)
        loop {
            let event = self.expect_event("value")?;
            match event.kind {
                ParseEventKind::StructEnd => break,
                ParseEventKind::FieldKey(key) => {
                    // Unit keys don't make sense for struct fields
                    let key_name = match key.name() {
                        Some(name) => name.as_ref(),
                        None => {
                            // Skip unit keys in struct context
                            self.skip_value()?;
                            continue;
                        }
                    };

                    // Skip the tag field - already used
                    if key_name == tag_key {
                        self.skip_value()?;
                        continue;
                    }

                    if has_flatten {
                        // Use path-based lookup for variants with flattened fields
                        if let Some(path) = find_field_path(variant_fields, key_name) {
                            // Find common prefix with currently open segments
                            let common_len = open_segments
                                .iter()
                                .zip(path.iter())
                                .take_while(|((name, _), b)| *name == **b)
                                .count();

                            // Close segments that are no longer needed (in reverse order)
                            while open_segments.len() > common_len {
                                let (_, is_option) = open_segments.pop().unwrap();
                                if is_option {
                                    wip = wip.end()?;
                                }
                                wip = wip.end()?;
                            }

                            // Open new segments
                            for &field_name in &path[common_len..] {
                                wip = wip.begin_field(field_name)?;
                                let is_option = matches!(wip.shape().def, Def::Option(_));
                                if is_option {
                                    wip = wip.begin_some()?;
                                }
                                open_segments.push((field_name, is_option));
                            }

                            // Deserialize the value
                            wip = self.deserialize_into(wip, MetaSource::FromEvents)?;

                            // Close the leaf field we just deserialized into
                            // (but keep parent segments open for potential sibling fields)
                            if let Some((_, is_option)) = open_segments.pop() {
                                if is_option {
                                    wip = wip.end()?;
                                }
                                wip = wip.end()?;
                            }
                        } else {
                            // Unknown field - skip
                            self.skip_value()?;
                        }
                    } else {
                        // Simple case: direct field lookup using precomputed FieldLookup
                        let variant_plan = wip.selected_variant_plan().unwrap();

                        if let Some(idx) = variant_plan
                            .field_lookup
                            .find(key_name, wip.type_plan_core())
                        {
                            wip = wip
                                .begin_nth_field(idx)?
                                .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                                .end()?;
                        } else {
                            // Unknown field - skip
                            self.skip_value()?;
                        }
                    }
                }
                other => {
                    return Err(DeserializeError {
                        span: Some(self.last_span),
                        path: Some(wip.path()),
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "field key or struct end",
                            got: other.kind_name().into(),
                        },
                    });
                }
            }
        }

        // Close any remaining open segments
        while let Some((_, is_option)) = open_segments.pop() {
            if is_option {
                wip = wip.end()?;
            }
            wip = wip.end()?;
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
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Get the variant name from FieldKey
        let field_event = self.expect_event("enum field key")?;
        let variant_name = match field_event.kind {
            ParseEventKind::FieldKey(key) => key.name().cloned().ok_or_else(|| {
                self.mk_err(
                    &wip,
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "variant name",
                        got: "unit key".into(),
                    },
                )
            })?,
            ParseEventKind::StructEnd => {
                // Empty struct - this shouldn't happen for valid enums
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::Unsupported {
                        message: "unexpected empty struct for enum".into(),
                    },
                ));
            }
            _ => {
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "field key for enum variant",
                        got: field_event.kind_name().into(),
                    },
                ));
            }
        };

        // Find the variant definition
        let variant = enum_def
            .variants
            .iter()
            .find(|v| v.name == variant_name.as_ref())
            .ok_or_else(|| {
                self.mk_err(
                    &wip,
                    DeserializeErrorKind::UnknownVariant {
                        variant: Cow::Owned(variant_name.to_string()),
                        enum_shape: wip.shape(),
                    },
                )
            })?;

        match variant.data.kind {
            StructKind::Unit => {
                // Unit variant - the parser will emit StructEnd next
                wip = self.set_string_value(wip, variant_name)?;
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                wip = wip.init_map()?;
                wip = wip.begin_object_entry(variant.name)?;
                if variant.data.fields.len() == 1 {
                    // Newtype variant - single field content, no wrapper
                    wip =
                        self.deserialize_value_recursive(wip, variant.data.fields[0].shape.get())?;
                } else {
                    // Multi-field tuple variant - parser emits SequenceStart
                    let seq_event = self.expect_event("tuple variant start")?;
                    if !matches!(seq_event.kind, ParseEventKind::SequenceStart(_)) {
                        return Err(DeserializeError {
                            span: Some(self.last_span),
                            path: Some(wip.path()),
                            kind: DeserializeErrorKind::UnexpectedToken {
                                expected: "SequenceStart for tuple variant",
                                got: seq_event.kind_name().into(),
                            },
                        });
                    }

                    wip = wip.init_list()?;
                    for field in variant.data.fields {
                        // The parser's InSequence state will emit OrderedField for each element
                        let _elem_event = self.expect_event("tuple element")?;
                        wip = wip
                            .begin_list_item()?
                            .with(|w| self.deserialize_value_recursive(w, field.shape.get()))?
                            .end()?;
                    }

                    let seq_end = self.expect_event("tuple variant end")?;
                    if !matches!(seq_end.kind, ParseEventKind::SequenceEnd) {
                        return Err(DeserializeError {
                            span: Some(self.last_span),
                            path: Some(wip.path()),
                            kind: DeserializeErrorKind::UnexpectedToken {
                                expected: "SequenceEnd for tuple variant",
                                got: seq_end.kind_name().into(),
                            },
                        });
                    }
                    wip = wip.end()?;
                }
                wip = wip.end()?;
            }
            StructKind::Struct => {
                // The parser auto-emits StructStart and pushes InStruct state
                let struct_event = self.expect_event("struct variant start")?;
                if !matches!(struct_event.kind, ParseEventKind::StructStart(_)) {
                    return Err(DeserializeError {
                        span: Some(self.last_span),
                        path: Some(wip.path()),
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "StructStart for struct variant",
                            got: struct_event.kind_name().into(),
                        },
                    });
                }

                wip = wip.init_map()?;
                wip = wip.begin_object_entry(variant.name)?;
                // begin_map() initializes the entry's value as an Object (doesn't push a frame)
                wip = wip.init_map()?;

                // Deserialize each field - parser will emit OrderedField for each
                for field in variant.data.fields {
                    let field_event = self.expect_event("struct field")?;
                    match field_event.kind {
                        ParseEventKind::OrderedField | ParseEventKind::FieldKey(_) => {
                            let key = field.rename.unwrap_or(field.name);
                            wip = wip
                                .begin_object_entry(key)?
                                .with(|w| self.deserialize_value_recursive(w, field.shape.get()))?
                                .end()?;
                        }
                        ParseEventKind::StructEnd => {
                            return Err(DeserializeError {
                                span: Some(self.last_span),
                                path: Some(wip.path()),
                                kind: DeserializeErrorKind::UnexpectedToken {
                                    expected: "field",
                                    got: "StructEnd (struct ended too early)".into(),
                                },
                            });
                        }
                        _ => {
                            return Err(DeserializeError {
                                span: Some(self.last_span),
                                path: Some(wip.path()),
                                kind: DeserializeErrorKind::UnexpectedToken {
                                    expected: "field",
                                    got: field_event.kind_name().into(),
                                },
                            });
                        }
                    }
                }

                // Consume inner StructEnd
                let inner_end = self.expect_event("struct variant inner end")?;
                if !matches!(inner_end.kind, ParseEventKind::StructEnd) {
                    return Err(DeserializeError {
                        span: Some(self.last_span),
                        path: Some(wip.path()),
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "StructEnd for struct variant inner",
                            got: inner_end.kind_name().into(),
                        },
                    });
                }
                // Only end the object entry (begin_map doesn't push a frame)
                wip = wip.end()?;
            }
        }

        // Consume the outer StructEnd
        let end_event = self.expect_event("enum struct end")?;
        if !matches!(end_event.kind, ParseEventKind::StructEnd) {
            return Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "StructEnd for enum wrapper",
                    got: end_event.kind_name().into(),
                },
            });
        }

        Ok(wip)
    }

    pub(crate) fn deserialize_result_as_enum(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Hint to non-self-describing parsers that a Result enum is expected
        // Result is encoded as a 2-variant enum: Ok (index 0) and Err (index 1)
        let variant_hints = vec![
            EnumVariantHint {
                name: "Ok",
                kind: StructKind::TupleStruct,
                field_count: 1,
            },
            EnumVariantHint {
                name: "Err",
                kind: StructKind::TupleStruct,
                field_count: 1,
            },
        ];
        if self.is_non_self_describing() {
            self.parser.hint_enum(&variant_hints);
        }

        // Read the StructStart emitted by the parser after hint_enum
        let event = self.expect_event("struct start for Result")?;
        if !matches!(event.kind, ParseEventKind::StructStart(_)) {
            return Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "struct start for Result variant",
                    got: event.kind_name().into(),
                },
            });
        }

        // Read the FieldKey with the variant name ("Ok" or "Err")
        let key_event = self.expect_event("variant key for Result")?;
        let variant_name = match key_event.kind {
            ParseEventKind::FieldKey(key) => {
                key.name().cloned().ok_or_else(|| DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "variant name",
                        got: "unit key".into(),
                    },
                })?
            }
            _ => {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "field key with variant name",
                        got: key_event.kind_name().into(),
                    },
                });
            }
        };

        // Select the appropriate variant and deserialize its content
        if variant_name.as_ref() == "Ok" {
            wip = wip.begin_ok()?;
        } else if variant_name.as_ref() == "Err" {
            wip = wip.begin_err()?;
        } else {
            return Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "Ok or Err variant",
                    got: format!("variant '{}'", variant_name).into(),
                },
            });
        }

        // Deserialize the variant's value (newtype pattern - single field)
        wip = self.deserialize_into(wip, MetaSource::FromEvents)?;
        wip = wip.end()?;

        // Consume StructEnd
        let end_event = self.expect_event("struct end for Result")?;
        if !matches!(end_event.kind, ParseEventKind::StructEnd) {
            return Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "struct end for Result variant",
                    got: end_event.kind_name().into(),
                },
            });
        }

        Ok(wip)
    }

    /// Deserialize the struct fields of a variant.
    /// Expects the variant to already be selected.
    pub(crate) fn deserialize_variant_struct_fields(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        let variant = wip.selected_variant().ok_or_else(|| DeserializeError {
            span: Some(self.last_span),
            path: Some(wip.path()),
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "selected variant",
                got: "no variant selected".into(),
            },
        })?;

        let variant_fields = variant.data.fields;
        let kind = variant.data.kind;

        // Handle based on variant kind
        match kind {
            StructKind::TupleStruct if variant_fields.len() == 1 => {
                // Single-element tuple variant (newtype): deserialize the inner value directly
                wip = wip
                    .begin_nth_field(0)?
                    .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                    .end()?;
                return Ok(wip);
            }
            StructKind::TupleStruct | StructKind::Tuple => {
                // Multi-element tuple variant - not yet supported in this context
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::Unsupported {
                        message: "multi-element tuple variants in flatten not yet supported".into(),
                    },
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
        if !matches!(event.kind, ParseEventKind::StructStart(_)) {
            return Err(DeserializeError {
                span: Some(self.last_span),
                path: Some(wip.path()),
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "struct start for variant content",
                    got: event.kind_name().into(),
                },
            });
        }

        // Process all fields
        loop {
            let event = self.expect_event("value")?;
            match event.kind {
                ParseEventKind::StructEnd => break,
                ParseEventKind::FieldKey(key) => {
                    // Unit keys don't make sense for struct fields
                    let key_name = match key.name() {
                        Some(name) => name.as_ref(),
                        None => {
                            // Skip unit keys in struct context
                            self.skip_value()?;
                            continue;
                        }
                    };

                    // Look up field using precomputed FieldLookup
                    let variant_plan = wip.selected_variant_plan().unwrap();

                    if let Some(idx) = variant_plan
                        .field_lookup
                        .find(key_name, wip.type_plan_core())
                    {
                        wip = wip
                            .begin_nth_field(idx)?
                            .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                            .end()?;
                    } else {
                        // Unknown field - skip
                        self.skip_value()?;
                    }
                }
                other => {
                    return Err(DeserializeError {
                        span: Some(self.last_span),
                        path: Some(wip.path()),
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "field key or struct end",
                            got: other.kind_name().into(),
                        },
                    });
                }
            }
        }

        // Apply defaults for missing fields
        for (idx, field) in variant_fields.iter().enumerate() {
            if wip.is_field_set(idx)? {
                continue;
            }

            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(Characteristic::Default);
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default || field_type_has_default {
                wip = wip.set_nth_field_to_default(idx)?;
            } else if field_is_option {
                wip = wip.begin_nth_field(idx)?.set_default()?.end()?;
            } else if field.should_skip_deserializing() {
                wip = wip.set_nth_field_to_default(idx)?;
            } else {
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::MissingField {
                        field: field.name,
                        container_shape: wip.shape(),
                    },
                ));
            }
        }

        Ok(wip)
    }

    fn deserialize_enum_adjacently_tagged(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        tag_key: &'static str,
        content_key: &'static str,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Step 1: Probe to find the tag value (handles out-of-order fields)
        let evidence = self.collect_evidence()?;

        // Step 2: Consume StructStart
        let event = self.expect_event("value")?;
        if !matches!(event.kind, ParseEventKind::StructStart(_)) {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "struct for adjacently tagged enum",
                    got: event.kind_name().into(),
                },
            ));
        }

        // Step 3: Select the variant
        // For cow-like enums, redirect Borrowed -> Owned when borrowing is disabled
        let enum_def = match &wip.shape().ty {
            Type::User(UserType::Enum(e)) => e,
            _ => {
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::Unsupported {
                        message: "expected enum for adjacently tagged".into(),
                    },
                ));
            }
        };

        if wip.shape().is_numeric() {
            let discriminant = find_tag_discriminant(&evidence, tag_key).ok_or_else(|| {
                self.mk_err(
                    &wip,
                    DeserializeErrorKind::MissingField {
                        field: tag_key,
                        container_shape: wip.shape(),
                    },
                )
            })?;
            wip = wip.select_variant(discriminant)?;
        } else {
            let variant_name = find_tag_value(&evidence, tag_key)
                .ok_or_else(|| {
                    self.mk_err(
                        &wip,
                        DeserializeErrorKind::MissingField {
                            field: tag_key,
                            container_shape: wip.shape(),
                        },
                    )
                })?
                .to_string();
            let actual_variant = cow_redirect_variant_name::<BORROW>(enum_def, &variant_name);
            wip = wip.select_variant_named(actual_variant)?;
        }

        // Step 4: Process fields in any order
        let mut content_seen = false;
        loop {
            let event = self.expect_event("value")?;
            match event.kind {
                ParseEventKind::StructEnd => break,
                ParseEventKind::FieldKey(key) => {
                    // Unit keys don't make sense for adjacently tagged enums
                    let key_name = match key.name() {
                        Some(name) => name.as_ref(),
                        None => {
                            // Skip unit keys
                            self.skip_value()?;
                            continue;
                        }
                    };

                    if key_name == tag_key {
                        // Skip the tag field - already used
                        self.skip_value()?;
                    } else if key_name == content_key {
                        // Deserialize the content
                        wip = self.deserialize_enum_variant_content(wip)?;
                        content_seen = true;
                    } else {
                        // Unknown field - skip
                        self.skip_value()?;
                    }
                }
                other => {
                    return Err(DeserializeError {
                        span: Some(self.last_span),
                        path: Some(wip.path()),
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "field key or struct end",
                            got: other.kind_name().into(),
                        },
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
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::MissingField {
                        field: content_key,
                        container_shape: wip.shape(),
                    },
                ));
            }
        }

        Ok(wip)
    }

    /// Deserialize the content of an already-selected enum variant.
    pub(crate) fn deserialize_enum_variant_content(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Get the selected variant's info
        let variant = wip.selected_variant().ok_or_else(|| DeserializeError {
            span: Some(self.last_span),
            path: Some(wip.path()),
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "selected variant",
                got: "no variant selected".into(),
            },
        })?;

        let variant_kind = variant.data.kind;
        let variant_fields = variant.data.fields;

        match variant_kind {
            StructKind::Unit => {
                // Unit variant - normally nothing to deserialize
                // But some formats may emit extra tokens
                let event = self.expect_peek("value")?;
                if matches!(event.kind, ParseEventKind::Scalar(ScalarValue::Unit)) {
                    self.expect_event("value")?; // consume Unit
                } else if matches!(event.kind, ParseEventKind::StructStart(_)) {
                    self.expect_event("value")?; // consume StructStart
                    // Expect immediate StructEnd for empty struct
                    let end_event = self.expect_event("value")?;
                    if !matches!(end_event.kind, ParseEventKind::StructEnd) {
                        return Err(DeserializeError {
                            span: Some(self.last_span),
                            path: Some(wip.path()),
                            kind: DeserializeErrorKind::UnexpectedToken {
                                expected: "empty struct for unit variant",
                                got: end_event.kind_name().into(),
                            },
                        });
                    }
                }
                Ok(wip)
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                if variant_fields.len() == 1 {
                    // Newtype variant - content is the single field's value
                    wip = wip
                        .begin_nth_field(0)?
                        .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                        .end()?;
                } else {
                    // Multi-field tuple variant - expect array or struct
                    let event = self.expect_event("value")?;

                    let struct_mode = match event.kind {
                        ParseEventKind::SequenceStart(_) => false,
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
                                    expected: "sequence for tuple variant",
                                    got: event.kind_name().into(),
                                },
                            });
                        }
                    };

                    let mut idx = 0;
                    while idx < variant_fields.len() {
                        // In struct mode, skip FieldKey events
                        if struct_mode {
                            let event = self.expect_peek("value")?;
                            if matches!(event.kind, ParseEventKind::FieldKey(_)) {
                                self.expect_event("value")?;
                                continue;
                            }
                        }

                        wip = wip
                            .begin_nth_field(idx)?
                            .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                            .end()?;
                        idx += 1;
                    }

                    let event = self.expect_event("value")?;
                    if !matches!(
                        event.kind,
                        ParseEventKind::SequenceEnd | ParseEventKind::StructEnd
                    ) {
                        return Err(DeserializeError {
                            span: Some(self.last_span),
                            path: Some(wip.path()),
                            kind: DeserializeErrorKind::UnexpectedToken {
                                expected: "sequence end for tuple variant",
                                got: event.kind_name().into(),
                            },
                        });
                    }
                }
                Ok(wip)
            }
            StructKind::Struct => {
                // Struct variant - expect object with fields
                let event = self.expect_event("value")?;
                if !matches!(event.kind, ParseEventKind::StructStart(_)) {
                    return Err(DeserializeError {
                        span: Some(self.last_span),
                        path: Some(wip.path()),
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "struct for struct variant",
                            got: event.kind_name().into(),
                        },
                    });
                }

                // Use precomputed has_flatten from VariantPlanMeta
                let has_flatten = wip.selected_variant_plan().unwrap().has_flatten;

                // Enter deferred mode for flatten handling
                let already_deferred = wip.is_deferred();
                if has_flatten && !already_deferred {
                    wip = wip.begin_deferred()?;
                }

                let num_fields = variant_fields.len();
                let mut ordered_field_index = 0usize;

                // Track currently open path segments for flatten handling
                let mut open_segments: Vec<(&str, bool)> = Vec::new();

                // Track which top-level fields have been touched
                let mut touched_fields: std::collections::BTreeSet<&str> =
                    std::collections::BTreeSet::new();

                loop {
                    let event = self.expect_event("value")?;
                    match event.kind {
                        ParseEventKind::StructEnd => break,
                        ParseEventKind::OrderedField => {
                            let idx = ordered_field_index;
                            ordered_field_index += 1;
                            if idx < num_fields {
                                wip = wip
                                    .begin_nth_field(idx)?
                                    .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                                    .end()?;
                            }
                        }
                        ParseEventKind::FieldKey(key) => {
                            let key_name = match key.name() {
                                Some(name) => name.as_ref(),
                                None => {
                                    self.skip_value()?;
                                    continue;
                                }
                            };

                            if has_flatten {
                                if let Some(path) = find_field_path(variant_fields, key_name) {
                                    if let Some(&first) = path.first() {
                                        touched_fields.insert(first);
                                    }

                                    let common_len = open_segments
                                        .iter()
                                        .zip(path.iter())
                                        .take_while(|((name, _), b)| *name == **b)
                                        .count();

                                    while open_segments.len() > common_len {
                                        let (_, is_option) = open_segments.pop().unwrap();
                                        if is_option {
                                            wip = wip.end()?;
                                        }
                                        wip = wip.end()?;
                                    }

                                    for &field_name in &path[common_len..] {
                                        wip = wip.begin_field(field_name)?;
                                        let is_option = matches!(wip.shape().def, Def::Option(_));
                                        if is_option {
                                            wip = wip.begin_some()?;
                                        }
                                        open_segments.push((field_name, is_option));
                                    }

                                    wip = self.deserialize_into(wip, MetaSource::FromEvents)?;

                                    if let Some((_, is_option)) = open_segments.pop() {
                                        if is_option {
                                            wip = wip.end()?;
                                        }
                                        wip = wip.end()?;
                                    }
                                } else {
                                    self.skip_value()?;
                                }
                            } else {
                                // Use precomputed FieldLookup for direct field matching
                                let variant_plan = wip.selected_variant_plan().unwrap();

                                if let Some(idx) = variant_plan
                                    .field_lookup
                                    .find(key_name, wip.type_plan_core())
                                {
                                    wip = wip
                                        .begin_nth_field(idx)?
                                        .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                                        .end()?;
                                } else {
                                    self.skip_value()?;
                                }
                            }
                        }
                        other => {
                            return Err(DeserializeError {
                                span: Some(self.last_span),
                                path: Some(wip.path()),
                                kind: DeserializeErrorKind::UnexpectedToken {
                                    expected: "field key, ordered field, or struct end",
                                    got: other.kind_name().into(),
                                },
                            });
                        }
                    }
                }

                // Close any remaining open segments
                while let Some((_, is_option)) = open_segments.pop() {
                    if is_option {
                        wip = wip.end()?;
                    }
                    wip = wip.end()?;
                }

                // Touch any flattened fields that weren't visited
                if has_flatten {
                    for field in variant_fields.iter() {
                        if field.is_flattened() && !touched_fields.contains(field.name) {
                            wip = wip.begin_field(field.name)?.end()?;
                        }
                    }
                }

                // Finish deferred mode
                if has_flatten && !already_deferred {
                    wip = wip.finish_deferred()?;
                }

                // Apply defaults for missing fields (when not using flatten/deferred mode)
                if !has_flatten {
                    for (idx, field) in variant_fields.iter().enumerate() {
                        if wip.is_field_set(idx)? {
                            continue;
                        }

                        let field_has_default = field.has_default();
                        let field_type_has_default = field.shape().is(Characteristic::Default);
                        let field_is_option = matches!(field.shape().def, Def::Option(_));

                        if field_has_default || field_type_has_default {
                            wip = wip.set_nth_field_to_default(idx)?;
                        } else if field_is_option {
                            wip = wip.begin_nth_field(idx)?.set_default()?.end()?;
                        } else if field.should_skip_deserializing() {
                            wip = wip.set_nth_field_to_default(idx)?;
                        } else {
                            return Err(self.mk_err(
                                &wip,
                                DeserializeErrorKind::MissingField {
                                    field: field.name,
                                    container_shape: wip.shape(),
                                },
                            ));
                        }
                    }
                }

                Ok(wip)
            }
        }
    }

    /// Deserialize a cow-like enum transparently from its inner value.
    ///
    /// Cow-like enums (`#[facet(cow)]`) serialize/deserialize transparently as their
    /// inner value, without any variant wrapper. The Borrowed/Owned distinction is
    /// purely an implementation detail for memory management.
    ///
    /// This always selects the "Owned" variant since we need to own the deserialized data.
    fn deserialize_cow_enum(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        // Always use Owned variant - we need to own the deserialized data
        wip = wip.select_variant_named("Owned")?;

        // Deserialize directly into the variant's single field
        wip = wip
            .begin_nth_field(0)?
            .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
            .end()?;

        Ok(wip)
    }

    fn deserialize_enum_untagged(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        let shape = wip.shape();
        let Some(variants_by_format) = VariantsByFormat::from_shape(shape) else {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::Unsupported {
                    message: "expected enum type for untagged".into(),
                },
            ));
        };

        let event = self.expect_peek("value")?;

        match &event.kind {
            ParseEventKind::Scalar(scalar) => {
                // Try unit variants for null
                if matches!(scalar, ScalarValue::Null)
                    && let Some(variant) = variants_by_format.unit_variants.first()
                {
                    wip = wip.select_variant_named(variant.effective_name())?;
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
                            wip = wip.select_variant_named(variant.effective_name())?;
                            // Consume the string
                            self.expect_event("value")?;
                            return Ok(wip);
                        }
                    }
                }

                // Try scalar variants that match the scalar type
                for (variant, inner_shape) in &variants_by_format.scalar_variants {
                    if scalar_matches_shape(scalar, inner_shape) {
                        wip = wip.select_variant_named(variant.effective_name())?;
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
                    if !scalar_matches_shape(scalar, inner_shape) {
                        wip = wip.select_variant_named(variant.effective_name())?;
                        // Try to deserialize - if this fails, it will bubble up as an error.
                        // TODO: Implement proper variant trying with backtracking for better error messages
                        wip = self.deserialize_enum_variant_content(wip)?;
                        return Ok(wip);
                    }
                }

                Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "matching untagged variant for scalar",
                        got: scalar.kind_name().into(),
                    },
                })
            }
            ParseEventKind::StructStart(_) => {
                // For struct input, use solve_variant for proper field-based matching
                let solve_result = self.solve_variant(shape).map_err(|e| match e {
                    crate::SolveVariantError::Parser(e) => {
                        // Convert ParseError to DeserializeError, adding path
                        DeserializeError::from(e).set_path(wip.path())
                    }
                    crate::SolveVariantError::NoMatch => self.mk_err(
                        &wip,
                        DeserializeErrorKind::NoMatchingVariant {
                            enum_shape: shape,
                            input_kind: "struct",
                        },
                    ),
                    crate::SolveVariantError::SchemaError(e) => self.mk_err(
                        &wip,
                        DeserializeErrorKind::Solver {
                            message: e.to_string().into(),
                        },
                    ),
                })?;

                if let Some(outcome) = solve_result {
                    // Successfully identified which variant matches based on fields
                    // Extract the variant name from the first variant selection
                    let variant_name = outcome
                        .resolution()
                        .variant_selections()
                        .first()
                        .map(|vs| vs.variant_name)
                        .ok_or_else(|| {
                            self.mk_err(
                                &wip,
                                DeserializeErrorKind::Unsupported {
                                    message:
                                        "solve_variant returned outcome with no variant selection"
                                            .into(),
                                },
                            )
                        })?;
                    wip = wip.select_variant_named(variant_name)?;
                    wip = self.deserialize_enum_variant_content(wip)?;
                    Ok(wip)
                } else {
                    // No variant matched - fall back to trying the first struct variant
                    // (we can't backtrack parser state to try multiple variants)
                    if let Some(variant) = variants_by_format.struct_variants.first() {
                        wip = wip.select_variant_named(variant.effective_name())?;
                        wip = self.deserialize_enum_variant_content(wip)?;
                        Ok(wip)
                    } else {
                        Err(self.mk_err(
                            &wip,
                            DeserializeErrorKind::Unsupported {
                                message:
                                    "no struct variant found for untagged enum with struct input"
                                        .into(),
                            },
                        ))
                    }
                }
            }
            ParseEventKind::SequenceStart(_) => {
                // For sequence input, use first tuple variant
                if let Some((variant, _arity)) = variants_by_format.tuple_variants.first() {
                    wip = wip.select_variant_named(variant.effective_name())?;
                    wip = self.deserialize_enum_variant_content(wip)?;
                    return Ok(wip);
                }

                Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::Unsupported {
                        message: "no tuple variant found for untagged enum with sequence input"
                            .into(),
                    },
                ))
            }
            _ => Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "scalar, struct, or sequence for untagged enum",
                    got: event.kind_name().into(),
                },
            )),
        }
    }

    /// Deserialize an `#[facet(other)]` variant that may have `#[facet(tag)]` and `#[facet(content)]` fields.
    ///
    /// This is called when a VariantTag event didn't match any known variant and we're falling
    /// back to an `#[facet(other)]` variant. The tag name is captured and stored in the
    /// `#[facet(tag)]` field, while the payload is deserialized into the `#[facet(content)]` field.
    ///
    /// `captured_tag` is `None` for unit tags (bare `@` in Styx).
    pub(crate) fn deserialize_other_variant_with_captured_tag(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        captured_tag: Option<&'input str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);

        let variant = wip.selected_variant().ok_or_else(|| DeserializeError {
            span: Some(self.last_span),
            path: Some(wip.path()),
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "selected variant",
                got: "no variant selected".into(),
            },
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
            wip = wip.begin_nth_field(idx)?;
            match captured_tag {
                Some(tag) => {
                    wip = self.set_string_value(wip, Cow::Borrowed(tag))?;
                }
                None => {
                    // Unit tag - set the field to its default (None for Option<String>)
                    wip = wip.set_default()?;
                }
            }
            wip = wip.end()?;
        }

        // Deserialize the content into the content field (if present)
        if let Some(idx) = content_field_idx {
            wip = wip
                .begin_nth_field(idx)?
                .with(|w| self.deserialize_into(w, MetaSource::FromEvents))?
                .end()?;
        } else {
            // No content field - the payload must be Unit
            let event = self.expect_peek("value")?;
            if matches!(event.kind, ParseEventKind::Scalar(ScalarValue::Unit)) {
                self.expect_event("value")?; // consume Unit
            } else {
                return Err(DeserializeError {
                    span: Some(self.last_span),
                    path: Some(wip.path()),
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "unit payload for #[facet(other)] variant without #[facet(content)]",
                        got: event.kind_name().into(),
                    },
                });
            }
        }

        Ok(wip)
    }
}

/// Find a field path through flattened fields.
///
/// Given a list of fields and a serialized key name, finds the path of field names
/// to navigate to reach that key. For flattened fields, this recursively searches
/// through the flattened struct's fields.
///
/// Returns `Some(path)` where path is a Vec of field names (e.g., `["base", "name"]`),
/// or `None` if the key doesn't match any field.
fn find_field_path(fields: &'static [Field], key: &str) -> Option<Vec<&'static str>> {
    for field in fields {
        // Check if this field matches directly (by effective name or alias)
        if field.effective_name() == key {
            return Some(vec![field.name]);
        }

        // Check alias
        if field.alias == Some(key) {
            return Some(vec![field.name]);
        }

        // If this is a flattened field, search recursively
        if field.is_flattened() {
            let shape = field.shape();
            // Unwrap Option if present
            let inner_shape = match shape.def {
                Def::Option(opt) => opt.t,
                _ => shape,
            };

            if let Type::User(UserType::Struct(inner_struct)) = inner_shape.ty
                && let Some(mut inner_path) = find_field_path(inner_struct.fields, key)
            {
                inner_path.insert(0, field.name);
                return Some(inner_path);
            }
        }
    }
    None
}

/// For cow-like enums, redirect from "Borrowed" to "Owned" variant when borrowing is disabled.
fn cow_redirect_variant_name<'a, const BORROW: bool>(
    enum_def: &facet_core::EnumType,
    variant_name: &'a str,
) -> &'a str {
    if !BORROW && enum_def.is_cow && variant_name == "Borrowed" {
        "Owned"
    } else {
        variant_name
    }
}

/// Helper to find a tag value from field evidence.
fn find_tag_value<'a, 'input>(
    evidence: &'a [FieldEvidence<'input>],
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

/// Helper to find a tag discriminant from field evidence.
fn find_tag_discriminant<'a, 'input>(
    evidence: &'a [FieldEvidence<'input>],
    tag_key: &str,
) -> Option<i64> {
    evidence
        .iter()
        .find(|e| e.name == tag_key)
        .and_then(|e| match &e.scalar_value {
            Some(ScalarValue::Str(s)) => s.parse().ok(),
            Some(ScalarValue::U64(d)) => Some(*d as i64),
            Some(ScalarValue::I64(d)) => Some(*d),
            _ => None,
        })
}
