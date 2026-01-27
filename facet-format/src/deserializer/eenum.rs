extern crate alloc;

use std::borrow::Cow;

use facet_core::{Def, StructKind, Type, UserType};
use facet_reflect::Partial;

use crate::{
    ContainerKind, DeserializeError, DeserializeErrorKind, EnumVariantHint, FieldEvidence,
    FormatDeserializer, FormatParser, ParseEvent, ScalarValue,
    deserializer::scalar_matches::scalar_matches_shape,
};

/// Inner implementation of `deserialize_enum_variant_content` using dyn dispatch.
///
/// This function is non-generic over the parser type, reducing monomorphization.
fn deserialize_enum_variant_content_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn FormatParser<'input>>,
    mut wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    use alloc::vec;
    use facet_core::Characteristic;

    // Get the selected variant's info
    let variant = wip.selected_variant().ok_or_else(|| DeserializeError {
        span: deser.last_span,
        path: None,
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
            let event = deser.expect_peek("value")?;
            if matches!(event, ParseEvent::Scalar(ScalarValue::Unit)) {
                deser.expect_event("value")?; // consume Unit
            } else if matches!(event, ParseEvent::StructStart(_)) {
                deser.expect_event("value")?; // consume StructStart
                // Expect immediate StructEnd for empty struct
                let end_event = deser.expect_event("value")?;
                if !matches!(end_event, ParseEvent::StructEnd) {
                    return Err(DeserializeError {
                        span: deser.last_span,
                        path: None,
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
                wip = wip.begin_nth_field(0)?;
                wip = deser.deserialize_into(wip)?;
                wip = wip.end()?;
            } else {
                // Multi-field tuple variant - expect array or struct
                let event = deser.expect_event("value")?;

                let struct_mode = match event {
                    ParseEvent::SequenceStart(_) => false,
                    ParseEvent::StructStart(ContainerKind::Object) => true,
                    ParseEvent::StructStart(kind) => {
                        return Err(DeserializeError {
                            span: deser.last_span,
                            path: None,
                            kind: DeserializeErrorKind::UnexpectedToken {
                                expected: "array",
                                got: kind.name().into(),
                            },
                        });
                    }
                    _ => {
                        return Err(DeserializeError {
                            span: deser.last_span,
                            path: None,
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
                        let event = deser.expect_peek("value")?;
                        if matches!(event, ParseEvent::FieldKey(_)) {
                            deser.expect_event("value")?;
                            continue;
                        }
                    }

                    wip = wip.begin_nth_field(idx)?;
                    wip = deser.deserialize_into(wip)?;
                    wip = wip.end()?;
                    idx += 1;
                }

                let event = deser.expect_event("value")?;
                if !matches!(event, ParseEvent::SequenceEnd | ParseEvent::StructEnd) {
                    return Err(DeserializeError {
                        span: deser.last_span,
                        path: None,
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
            let event = deser.expect_event("value")?;
            if !matches!(event, ParseEvent::StructStart(_)) {
                return Err(DeserializeError {
                    span: deser.last_span,
                    path: None,
                    kind: DeserializeErrorKind::UnexpectedToken {
                        expected: "struct for struct variant",
                        got: event.kind_name().into(),
                    },
                });
            }

            // Check if variant has any flattened fields
            let has_flatten = variant_fields.iter().any(|f| f.is_flattened());

            // Enter deferred mode for flatten handling
            let already_deferred = wip.is_deferred();
            if has_flatten && !already_deferred {
                wip = wip.begin_deferred()?;
            }

            let num_fields = variant_fields.len();
            let mut fields_set = vec![false; num_fields];
            let mut ordered_field_index = 0usize;

            // Track currently open path segments for flatten handling
            let mut open_segments: alloc::vec::Vec<(&str, bool)> = alloc::vec::Vec::new();

            // Track which top-level fields have been touched
            let mut touched_fields: alloc::collections::BTreeSet<&str> =
                alloc::collections::BTreeSet::new();

            loop {
                let event = deser.expect_event("value")?;
                match event {
                    ParseEvent::StructEnd => break,
                    ParseEvent::OrderedField => {
                        let idx = ordered_field_index;
                        ordered_field_index += 1;
                        if idx < num_fields {
                            wip = wip.begin_nth_field(idx)?;
                            wip = deser.deserialize_into(wip)?;
                            wip = wip.end()?;
                            fields_set[idx] = true;
                        }
                    }
                    ParseEvent::FieldKey(key) => {
                        let key_name = match &key.name {
                            Some(name) => name.as_ref(),
                            None => {
                                deser.parser.skip_value()?;
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

                                wip = deser.deserialize_into(wip)?;

                                if let Some((_, is_option)) = open_segments.pop() {
                                    if is_option {
                                        wip = wip.end()?;
                                    }
                                    wip = wip.end()?;
                                }
                            } else {
                                deser.parser.skip_value()?;
                            }
                        } else {
                            let field_info = variant_fields
                                .iter()
                                .enumerate()
                                .find(|(_, f)| field_matches(f, key_name));

                            if let Some((idx, _field)) = field_info {
                                wip = wip.begin_nth_field(idx)?;
                                wip = deser.deserialize_into(wip)?;
                                wip = wip.end()?;
                                fields_set[idx] = true;
                            } else {
                                deser.parser.skip_value()?;
                            }
                        }
                    }
                    other => {
                        return Err(DeserializeError {
                            span: deser.last_span,
                            path: None,
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
                        wip = wip.begin_field(field.name)?;
                        wip = wip.end()?;
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
                    if fields_set[idx] {
                        continue;
                    }

                    let field_has_default = field.has_default();
                    let field_type_has_default = field.shape().is(Characteristic::Default);
                    let field_is_option = matches!(field.shape().def, Def::Option(_));

                    if field_has_default || field_type_has_default {
                        wip = wip.set_nth_field_to_default(idx)?;
                    } else if field_is_option {
                        wip = wip.begin_nth_field(idx)?;
                        wip = wip.set_default()?;
                        wip = wip.end()?;
                    } else if field.should_skip_deserializing() {
                        wip = wip.set_nth_field_to_default(idx)?;
                    } else {
                        return Err(DeserializeError {
                            span: deser.last_span,
                            path: None,
                            kind: DeserializeErrorKind::MissingField {
                                field: field.name,
                                type_name: "variant struct",
                            },
                        });
                    }
                }
            }

            Ok(wip)
        }
    }
}

/// Check if a field matches a given name (by name or alias).
fn field_matches(field: &facet_core::Field, name: &str) -> bool {
    field.effective_name() == name || field.alias.iter().any(|alias| *alias == name)
}

/// Find a variant by its display name (checking rename attributes).
/// Returns the effective name to use with `select_variant_named`.
fn find_variant_by_display_name<'a>(
    enum_def: &'a facet_core::EnumType,
    display_name: &str,
) -> Option<&'a str> {
    enum_def.variants.iter().find_map(|v| {
        if v.effective_name() == display_name {
            Some(v.effective_name())
        } else {
            None
        }
    })
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

/// Inner implementation of `deserialize_enum_externally_tagged` using dyn dispatch.
fn deserialize_enum_externally_tagged_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn FormatParser<'input>>,
    mut wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    use super::dyn_helpers::*;
    use alloc::format;

    trace!("deserialize_enum_externally_tagged called");
    let event = expect_peek(deser, "value")?;
    trace!(?event, "peeked event");

    // Check for any bare scalar (string, bool, int, etc.)
    if let ParseEvent::Scalar(scalar) = &event {
        let shape = wip.shape();
        let enum_def = match &shape.ty {
            Type::User(UserType::Enum(e)) => e,
            _ => {
                return Err(DeserializeErrorKind::TypeMismatch {
                    expected: shape,
                    got: "non-enum type".into(),
                }
                .without_source_span_yes_i_feel_bad());
            }
        };

        // For string scalars, first try to match as a unit variant name
        if let ScalarValue::Str(variant_name) = scalar {
            let matched_variant = find_variant_by_display_name(enum_def, variant_name);

            if let Some(matched_name) = matched_variant {
                // Found a matching unit variant
                let actual_variant = cow_redirect_variant_name::<BORROW>(enum_def, matched_name);
                expect_event(deser, "value")?;
                wip = wip.select_variant_named(actual_variant)?;
                return Ok(wip);
            }
        }

        // No matching variant - check for #[facet(other)] fallback
        if let Some(other_variant) = enum_def.variants.iter().find(|v| v.is_other()) {
            let has_tag_field = other_variant.data.fields.iter().any(|f| f.is_variant_tag());
            let has_content_field = other_variant
                .data
                .fields
                .iter()
                .any(|f| f.is_variant_content());

            if has_tag_field || has_content_field {
                wip = wip.select_variant_named(other_variant.effective_name())?;
                wip = deserialize_other_variant_with_captured_tag(deser, wip, None)?;
            } else {
                expect_event(deser, "value")?;
                wip = wip.select_variant_named(other_variant.effective_name())?;

                let scalar_as_string =
                    scalar.to_string_value().ok_or_else(|| DeserializeError {
                        span: deser.last_span,
                        path: None,
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "string or struct for enum",
                            got: "bytes".into(),
                        },
                    })?;

                wip = wip.begin_nth_field(0)?;
                wip = set_string_value(deser, wip, Cow::Owned(scalar_as_string))?;
                wip = wip.end()?;
            }
            return Ok(wip);
        }

        // No fallback available - error
        return Err(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "known enum variant",
                got: scalar.to_display_string().into(),
            },
        });
    }

    // Check for VariantTag (self-describing formats like Styx)
    if let ParseEvent::VariantTag(tag_name) = &event {
        let tag_name = *tag_name;
        expect_event(deser, "value")?; // consume VariantTag

        let shape = wip.shape();
        let enum_def = match &shape.ty {
            Type::User(UserType::Enum(e)) => e,
            _ => {
                return Err(DeserializeErrorKind::TypeMismatch {
                    expected: shape,
                    got: "non-enum type".into(),
                }
                .without_source_span_yes_i_feel_bad());
            }
        };

        let (variant_name, is_using_other_fallback) = match tag_name {
            Some(name) => {
                let by_display = find_variant_by_display_name(enum_def, name);
                let is_fallback = by_display.is_none();
                let variant = by_display
                    .or_else(|| {
                        enum_def
                            .variants
                            .iter()
                            .find(|v| v.is_other())
                            .map(|v| v.effective_name())
                    })
                    .ok_or_else(|| DeserializeError {
                        span: deser.last_span,
                        path: None,
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "known enum variant",
                            got: format!("@{}", name).into(),
                        },
                    })?;
                (variant, is_fallback)
            }
            None => {
                let variant = enum_def
                    .variants
                    .iter()
                    .find(|v| v.is_other())
                    .map(|v| v.effective_name())
                    .ok_or_else(|| DeserializeError {
                        span: deser.last_span,
                        path: None,
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "#[facet(other)] fallback variant for unit tag",
                            got: "@".into(),
                        },
                    })?;
                (variant, true)
            }
        };

        let actual_variant = cow_redirect_variant_name::<BORROW>(enum_def, variant_name);
        wip = wip.select_variant_named(actual_variant)?;

        if is_using_other_fallback {
            wip = deserialize_other_variant_with_captured_tag(deser, wip, tag_name)?;
        } else {
            wip = deserialize_enum_variant_content(deser, wip)?;
        }
        return Ok(wip);
    }

    // Otherwise expect a struct { VariantName: ... }
    if !matches!(event, ParseEvent::StructStart(_)) {
        return Err(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "string or struct for enum",
                got: event.kind_name().into(),
            },
        });
    }

    expect_event(deser, "value")?; // consume StructStart

    // Get the variant name from the field key
    let event = expect_event(deser, "value")?;
    let field_key_name = match event {
        ParseEvent::FieldKey(key) => key.name.ok_or_else(|| DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "variant name",
                got: "unit key".into(),
            },
        })?,
        other => {
            return Err(DeserializeError {
                span: deser.last_span,
                path: None,
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "variant name",
                    got: other.kind_name().into(),
                },
            });
        }
    };

    let shape = wip.shape();
    let enum_def = match &shape.ty {
        Type::User(UserType::Enum(e)) => e,
        _ => {
            return Err(DeserializeErrorKind::TypeMismatch {
                expected: shape,
                got: "non-enum type".into(),
            }
            .without_source_span_yes_i_feel_bad());
        }
    };
    let is_using_other_fallback = find_variant_by_display_name(enum_def, &field_key_name).is_none();
    let variant_name = find_variant_by_display_name(enum_def, &field_key_name)
        .or_else(|| {
            enum_def
                .variants
                .iter()
                .find(|v| v.is_other())
                .map(|v| v.effective_name())
        })
        .ok_or_else(|| DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "known enum variant",
                got: field_key_name.to_string().into(),
            },
        })?;

    let actual_variant = cow_redirect_variant_name::<BORROW>(enum_def, variant_name);
    wip = wip.select_variant_named(actual_variant)?;

    // For #[facet(other)] fallback variants, if the content is Unit, use the field key name as the value
    if is_using_other_fallback {
        let event = expect_peek(deser, "value")?;
        if matches!(event, ParseEvent::Scalar(ScalarValue::Unit)) {
            expect_event(deser, "value")?; // consume Unit
            wip = wip.begin_nth_field(0)?;
            wip = set_string_value(deser, wip, Cow::Owned(field_key_name.into_owned()))?;
            wip = wip.end()?;
        } else {
            wip = deserialize_enum_variant_content(deser, wip)?;
        }
    } else {
        wip = deserialize_enum_variant_content(deser, wip)?;
    }

    // Consume StructEnd
    let event = expect_event(deser, "value")?;
    if !matches!(event, ParseEvent::StructEnd) {
        return Err(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "struct end after enum variant",
                got: event.kind_name().into(),
            },
        });
    }

    Ok(wip)
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

/// Inner implementation of `deserialize_enum_internally_tagged` using dyn dispatch.
fn deserialize_enum_internally_tagged_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn FormatParser<'input>>,
    mut wip: Partial<'input, BORROW>,
    tag_key: &'static str,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    use super::dyn_helpers::*;
    use alloc::string::ToString;
    use alloc::vec::Vec;

    // Step 1: Probe to find the tag value (handles out-of-order fields)
    let evidence = collect_evidence(deser)?;

    let variant_name = find_tag_value(&evidence, tag_key)
        .ok_or(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::MissingField {
                field: tag_key,
                type_name: "internally tagged enum",
            },
        })?
        .to_string();

    // Step 2: Consume StructStart
    let event = expect_event(deser, "value")?;
    if !matches!(event, ParseEvent::StructStart(_)) {
        return Err(DeserializeError {
            span: deser.last_span,
            path: None,
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
        _ => return Err(DeserializeError::unsupported("expected enum")),
    };
    let actual_variant = cow_redirect_variant_name::<BORROW>(enum_def, &variant_name);
    wip = wip.select_variant_named(actual_variant)?;

    // Get the selected variant info
    let variant = wip.selected_variant().ok_or_else(|| DeserializeError {
        span: deser.last_span,
        path: None,
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
            let event = expect_event(deser, "value")?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(_) => {
                    skip(deser)?;
                }
                other => {
                    return Err(DeserializeError {
                        span: deser.last_span,
                        path: None,
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

    // Check if variant has any flattened fields
    let has_flatten = variant_fields.iter().any(|f| f.is_flattened());

    // Track currently open path segments for flatten handling: (field_name, is_option)
    let mut open_segments: Vec<(&str, bool)> = Vec::new();

    // Process all fields (they can come in any order now)
    loop {
        let event = expect_event(deser, "value")?;
        match event {
            ParseEvent::StructEnd => break,
            ParseEvent::FieldKey(key) => {
                // Unit keys don't make sense for struct fields
                let key_name = match &key.name {
                    Some(name) => name.as_ref(),
                    None => {
                        // Skip unit keys in struct context
                        skip(deser)?;
                        continue;
                    }
                };

                // Skip the tag field - already used
                if key_name == tag_key {
                    skip(deser)?;
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
                        wip = deserialize_into(deser, wip)?;

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
                        skip(deser)?;
                    }
                } else {
                    // Simple case: direct field lookup by name/alias
                    let field_info = variant_fields
                        .iter()
                        .enumerate()
                        .find(|(_, f)| field_matches(f, key_name));

                    if let Some((idx, _field)) = field_info {
                        wip = wip.begin_nth_field(idx)?;
                        wip = deserialize_into(deser, wip)?;
                        wip = wip.end()?;
                    } else {
                        // Unknown field - skip
                        skip(deser)?;
                    }
                }
            }
            other => {
                return Err(DeserializeError {
                    span: deser.last_span,
                    path: None,
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

/// Inner implementation of `deserialize_enum_adjacently_tagged` using dyn dispatch.
fn deserialize_enum_adjacently_tagged_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn FormatParser<'input>>,
    mut wip: Partial<'input, BORROW>,
    tag_key: &'static str,
    content_key: &'static str,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    use super::dyn_helpers::*;
    use alloc::string::ToString;

    // Step 1: Probe to find the tag value (handles out-of-order fields)
    let evidence = collect_evidence(deser)?;

    let variant_name = find_tag_value(&evidence, tag_key)
        .ok_or(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::MissingField {
                field: tag_key,
                type_name: "adjacently tagged enum",
            },
        })?
        .to_string();

    // Step 2: Consume StructStart
    let event = expect_event(deser, "value")?;
    if !matches!(event, ParseEvent::StructStart(_)) {
        return Err(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "struct for adjacently tagged enum",
                got: event.kind_name().into(),
            },
        });
    }

    // Step 3: Select the variant
    // For cow-like enums, redirect Borrowed -> Owned when borrowing is disabled
    let enum_def = match &wip.shape().ty {
        Type::User(UserType::Enum(e)) => e,
        _ => return Err(DeserializeError::unsupported("expected enum")),
    };
    let actual_variant = cow_redirect_variant_name::<BORROW>(enum_def, &variant_name);
    wip = wip.select_variant_named(actual_variant)?;

    // Step 4: Process fields in any order
    let mut content_seen = false;
    loop {
        let event = expect_event(deser, "value")?;
        match event {
            ParseEvent::StructEnd => break,
            ParseEvent::FieldKey(key) => {
                // Unit keys don't make sense for adjacently tagged enums
                let key_name = match &key.name {
                    Some(name) => name.as_ref(),
                    None => {
                        // Skip unit keys
                        skip(deser)?;
                        continue;
                    }
                };

                if key_name == tag_key {
                    // Skip the tag field - already used
                    skip(deser)?;
                } else if key_name == content_key {
                    // Deserialize the content
                    wip = deserialize_enum_variant_content(deser, wip)?;
                    content_seen = true;
                } else {
                    // Unknown field - skip
                    skip(deser)?;
                }
            }
            other => {
                return Err(DeserializeError {
                    span: deser.last_span,
                    path: None,
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
            return Err(DeserializeError {
                span: deser.last_span,
                path: None,
                kind: DeserializeErrorKind::MissingField {
                    field: content_key,
                    type_name: "non-unit variant",
                },
            });
        }
    }

    Ok(wip)
}

/// Inner implementation of `deserialize_variant_struct_fields` using dyn dispatch.
fn deserialize_variant_struct_fields_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn FormatParser<'input>>,
    mut wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    use super::dyn_helpers::*;
    use alloc::vec;
    use facet_core::Characteristic;

    let variant = wip.selected_variant().ok_or_else(|| DeserializeError {
        span: deser.last_span,
        path: None,
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
            wip = wip.begin_nth_field(0)?;
            wip = deserialize_into(deser, wip)?;
            wip = wip.end()?;
            return Ok(wip);
        }
        StructKind::TupleStruct | StructKind::Tuple => {
            // Multi-element tuple variant - not yet supported in this context
            return Err(DeserializeError::unsupported(
                "multi-element tuple variants in flatten not yet supported",
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
    let event = expect_event(deser, "value")?;
    if !matches!(event, ParseEvent::StructStart(_)) {
        return Err(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "struct start for variant content",
                got: event.kind_name().into(),
            },
        });
    }

    // Track which fields have been set
    let num_fields = variant_fields.len();
    let mut fields_set = vec![false; num_fields];

    // Process all fields
    loop {
        let event = expect_event(deser, "value")?;
        match event {
            ParseEvent::StructEnd => break,
            ParseEvent::FieldKey(key) => {
                // Unit keys don't make sense for struct fields
                let key_name = match &key.name {
                    Some(name) => name.as_ref(),
                    None => {
                        // Skip unit keys in struct context
                        skip(deser)?;
                        continue;
                    }
                };

                // Look up field in variant's fields by name/alias
                let field_info = variant_fields
                    .iter()
                    .enumerate()
                    .find(|(_, f)| field_matches(f, key_name));

                if let Some((idx, _field)) = field_info {
                    wip = wip.begin_nth_field(idx)?;
                    wip = deserialize_into(deser, wip)?;
                    wip = wip.end()?;
                    fields_set[idx] = true;
                } else {
                    // Unknown field - skip
                    skip(deser)?;
                }
            }
            other => {
                return Err(DeserializeError {
                    span: deser.last_span,
                    path: None,
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
        if fields_set[idx] {
            continue;
        }

        let field_has_default = field.has_default();
        let field_type_has_default = field.shape().is(Characteristic::Default);
        let field_is_option = matches!(field.shape().def, Def::Option(_));

        if field_has_default || field_type_has_default {
            wip = wip.set_nth_field_to_default(idx)?;
        } else if field_is_option {
            wip = wip.begin_nth_field(idx)?;
            wip = wip.set_default()?;
            wip = wip.end()?;
        } else if field.should_skip_deserializing() {
            wip = wip.set_nth_field_to_default(idx)?;
        } else {
            return Err(DeserializeError {
                span: deser.last_span,
                path: None,
                kind: DeserializeErrorKind::MissingField {
                    field: field.name,
                    type_name: "variant struct",
                },
            });
        }
    }

    Ok(wip)
}

/// Inner implementation of `deserialize_enum_as_struct` using dyn dispatch.
fn deserialize_enum_as_struct_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn FormatParser<'input>>,
    mut wip: Partial<'input, BORROW>,
    enum_def: &'static facet_core::EnumType,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    use super::dyn_helpers::*;
    use alloc::format;

    // Get the variant name from FieldKey
    let field_event = expect_event(deser, "enum field key")?;
    let variant_name = match field_event {
        ParseEvent::FieldKey(key) => key.name.ok_or_else(|| DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "variant name",
                got: "unit key".into(),
            },
        })?,
        ParseEvent::StructEnd => {
            // Empty struct - this shouldn't happen for valid enums
            return Err(DeserializeError::unsupported(
                "unexpected empty struct for enum",
            ));
        }
        _ => {
            return Err(DeserializeError {
                span: deser.last_span,
                path: None,
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "field key for enum variant",
                    got: field_event.kind_name().into(),
                },
            });
        }
    };

    // Find the variant definition
    let variant = enum_def
        .variants
        .iter()
        .find(|v| v.name == variant_name.as_ref())
        .ok_or_else(|| DeserializeError::unsupported(format!("unknown variant: {variant_name}")))?;

    match variant.data.kind {
        StructKind::Unit => {
            // Unit variant - the parser will emit StructEnd next
            wip = set_string_value(deser, wip, variant_name)?;
        }
        StructKind::TupleStruct | StructKind::Tuple => {
            wip = wip.init_map()?;
            wip = wip.begin_object_entry(variant.name)?;
            if variant.data.fields.len() == 1 {
                // Newtype variant - single field content, no wrapper
                wip = deserialize_value_recursive(deser, wip, variant.data.fields[0].shape.get())?;
            } else {
                // Multi-field tuple variant - parser emits SequenceStart
                let seq_event = expect_event(deser, "tuple variant start")?;
                if !matches!(seq_event, ParseEvent::SequenceStart(_)) {
                    return Err(DeserializeError {
                        span: deser.last_span,
                        path: None,
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "SequenceStart for tuple variant",
                            got: seq_event.kind_name().into(),
                        },
                    });
                }

                wip = wip.init_list()?;
                for field in variant.data.fields {
                    // The parser's InSequence state will emit OrderedField for each element
                    let _elem_event = expect_event(deser, "tuple element")?;
                    wip = wip.begin_list_item()?;
                    wip = deserialize_value_recursive(deser, wip, field.shape.get())?;
                    wip = wip.end()?;
                }

                let seq_end = expect_event(deser, "tuple variant end")?;
                if !matches!(seq_end, ParseEvent::SequenceEnd) {
                    return Err(DeserializeError {
                        span: deser.last_span,
                        path: None,
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
            let struct_event = expect_event(deser, "struct variant start")?;
            if !matches!(struct_event, ParseEvent::StructStart(_)) {
                return Err(DeserializeError {
                    span: deser.last_span,
                    path: None,
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
                let field_event = expect_event(deser, "struct field")?;
                match field_event {
                    ParseEvent::OrderedField | ParseEvent::FieldKey(_) => {
                        let key = field.rename.unwrap_or(field.name);
                        wip = wip.begin_object_entry(key)?;
                        wip = deserialize_value_recursive(deser, wip, field.shape.get())?;
                        wip = wip.end()?;
                    }
                    ParseEvent::StructEnd => {
                        return Err(DeserializeError {
                            span: deser.last_span,
                            path: None,
                            kind: DeserializeErrorKind::UnexpectedToken {
                                expected: "field",
                                got: "StructEnd (struct ended too early)".into(),
                            },
                        });
                    }
                    _ => {
                        return Err(DeserializeError {
                            span: deser.last_span,
                            path: None,
                            kind: DeserializeErrorKind::UnexpectedToken {
                                expected: "field",
                                got: field_event.kind_name().into(),
                            },
                        });
                    }
                }
            }

            // Consume inner StructEnd
            let inner_end = expect_event(deser, "struct variant inner end")?;
            if !matches!(inner_end, ParseEvent::StructEnd) {
                return Err(DeserializeError {
                    span: deser.last_span,
                    path: None,
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
    let end_event = expect_event(deser, "enum struct end")?;
    if !matches!(end_event, ParseEvent::StructEnd) {
        return Err(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "StructEnd for enum wrapper",
                got: end_event.kind_name().into(),
            },
        });
    }

    Ok(wip)
}

/// Inner implementation of `deserialize_other_variant_with_captured_tag` using dyn dispatch.
fn deserialize_other_variant_with_captured_tag_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn FormatParser<'input>>,
    mut wip: Partial<'input, BORROW>,
    captured_tag: Option<&'input str>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    use super::dyn_helpers::*;

    let variant = wip.selected_variant().ok_or_else(|| DeserializeError {
        span: deser.last_span,
        path: None,
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
        return deserialize_enum_variant_content(deser, wip);
    }

    // Set the tag field to the captured tag name (or None for unit tags)
    if let Some(idx) = tag_field_idx {
        wip = wip.begin_nth_field(idx)?;
        match captured_tag {
            Some(tag) => {
                wip = set_string_value(deser, wip, Cow::Borrowed(tag))?;
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
        wip = wip.begin_nth_field(idx)?;
        wip = deserialize_into(deser, wip)?;
        wip = wip.end()?;
    } else {
        // No content field - the payload must be Unit
        let event = expect_peek(deser, "value")?;
        if matches!(event, ParseEvent::Scalar(ScalarValue::Unit)) {
            expect_event(deser, "value")?; // consume Unit
        } else {
            return Err(DeserializeError {
                span: deser.last_span,
                path: None,
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "unit payload for #[facet(other)] variant without #[facet(content)]",
                    got: event.kind_name().into(),
                },
            });
        }
    }

    Ok(wip)
}

/// Inner implementation of `deserialize_result_as_enum` using dyn dispatch.
fn deserialize_result_as_enum_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn FormatParser<'input>>,
    mut wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    use super::dyn_helpers::*;
    use alloc::format;
    use alloc::vec;

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
    hint_enum(deser, &variant_hints);

    // Read the StructStart emitted by the parser after hint_enum
    let event = expect_event(deser, "struct start for Result")?;
    if !matches!(event, ParseEvent::StructStart(_)) {
        return Err(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "struct start for Result variant",
                got: event.kind_name().into(),
            },
        });
    }

    // Read the FieldKey with the variant name ("Ok" or "Err")
    let key_event = expect_event(deser, "variant key for Result")?;
    let variant_name = match key_event {
        ParseEvent::FieldKey(key) => key.name.ok_or_else(|| DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "variant name",
                got: "unit key".into(),
            },
        })?,
        other => {
            return Err(DeserializeError {
                span: deser.last_span,
                path: None,
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "field key with variant name",
                    got: other.kind_name().into(),
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
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "Ok or Err variant",
                got: format!("variant '{}'", variant_name).into(),
            },
        });
    }

    // Deserialize the variant's value (newtype pattern - single field)
    wip = deserialize_into(deser, wip)?;
    wip = wip.end()?;

    // Consume StructEnd
    let end_event = expect_event(deser, "struct end for Result")?;
    if !matches!(end_event, ParseEvent::StructEnd) {
        return Err(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "struct end for Result variant",
                got: end_event.kind_name().into(),
            },
        });
    }

    Ok(wip)
}

/// Inner implementation of `deserialize_enum_untagged` using dyn dispatch.
fn deserialize_enum_untagged_inner<'input, const BORROW: bool>(
    deser: &mut FormatDeserializer<'input, BORROW, &mut dyn FormatParser<'input>>,
    mut wip: Partial<'input, BORROW>,
) -> Result<Partial<'input, BORROW>, DeserializeError> {
    use super::dyn_helpers::*;
    use facet_solver::VariantsByFormat;

    let shape = wip.shape();
    let variants_by_format = VariantsByFormat::from_shape(shape)
        .ok_or_else(|| DeserializeError::unsupported("expected enum type for untagged"))?;

    let event = expect_peek(deser, "value")?;

    match &event {
        ParseEvent::Scalar(scalar) => {
            // Try unit variants for null
            if matches!(scalar, ScalarValue::Null)
                && let Some(variant) = variants_by_format.unit_variants.first()
            {
                wip = wip.select_variant_named(variant.effective_name())?;
                // Consume the null
                expect_event(deser, "value")?;
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
                        expect_event(deser, "value")?;
                        return Ok(wip);
                    }
                }
            }

            // Try scalar variants that match the scalar type
            for (variant, inner_shape) in &variants_by_format.scalar_variants {
                if scalar_matches_shape(scalar, inner_shape) {
                    wip = wip.select_variant_named(variant.effective_name())?;
                    wip = deserialize_enum_variant_content(deser, wip)?;
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
                    wip = deserialize_enum_variant_content(deser, wip)?;
                    return Ok(wip);
                }
            }

            Err(DeserializeError {
                span: deser.last_span,
                path: None,
                kind: DeserializeErrorKind::UnexpectedToken {
                    expected: "matching untagged variant for scalar",
                    got: scalar.kind_name().into(),
                },
            })
        }
        ParseEvent::StructStart(_) => {
            // For struct input, use solve_variant for proper field-based matching
            match solve_variant(deser, shape)? {
                Some(variant_name) => {
                    // Successfully identified which variant matches based on fields
                    wip = wip.select_variant_named(variant_name)?;
                    wip = deserialize_enum_variant_content(deser, wip)?;
                    Ok(wip)
                }
                None => {
                    // No variant matched - fall back to trying the first struct variant
                    // (we can't backtrack parser state to try multiple variants)
                    if let Some(variant) = variants_by_format.struct_variants.first() {
                        wip = wip.select_variant_named(variant.effective_name())?;
                        wip = deserialize_enum_variant_content(deser, wip)?;
                        Ok(wip)
                    } else {
                        Err(DeserializeError::unsupported(
                            "no struct variant found for untagged enum with struct input",
                        ))
                    }
                }
            }
        }
        ParseEvent::SequenceStart(_) => {
            // For sequence input, use first tuple variant
            if let Some((variant, _arity)) = variants_by_format.tuple_variants.first() {
                wip = wip.select_variant_named(variant.effective_name())?;
                wip = deserialize_enum_variant_content(deser, wip)?;
                return Ok(wip);
            }

            Err(DeserializeError::unsupported(
                "no tuple variant found for untagged enum with sequence input",
            ))
        }
        _ => Err(DeserializeError {
            span: deser.last_span,
            path: None,
            kind: DeserializeErrorKind::UnexpectedToken {
                expected: "scalar, struct, or sequence for untagged enum",
                got: event.kind_name().into(),
            },
        }),
    }
}

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    pub(crate) fn deserialize_enum(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
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
            let variant_hints: Vec<crate::EnumVariantHint> = enum_def
                .variants
                .iter()
                .map(|v| crate::EnumVariantHint {
                    name: v.effective_name(),
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
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use crate::FormatParser;
        let dyn_parser: &mut dyn FormatParser<'input> = &mut self.parser;
        let mut dyn_deser = crate::FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };
        let result = deserialize_enum_externally_tagged_inner(&mut dyn_deser, wip);
        self.last_span = dyn_deser.last_span;
        result
    }

    fn deserialize_enum_internally_tagged(
        &mut self,
        wip: Partial<'input, BORROW>,
        tag_key: &'static str,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use crate::FormatParser;
        let dyn_parser: &mut dyn FormatParser<'input> = &mut self.parser;
        let mut dyn_deser = crate::FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };
        let result = deserialize_enum_internally_tagged_inner(&mut dyn_deser, wip, tag_key);
        self.last_span = dyn_deser.last_span;
        result
    }

    /// Deserialize enum represented as struct (used by postcard and similar formats).
    ///
    /// The parser emits the enum as `{variant_name: content}` where content depends
    /// on the variant kind. The parser auto-handles struct/tuple variants by pushing
    /// appropriate state, so we just consume the events it produces.
    pub(crate) fn deserialize_enum_as_struct(
        &mut self,
        wip: Partial<'input, BORROW>,
        enum_def: &'static facet_core::EnumType,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use crate::FormatParser;
        let dyn_parser: &mut dyn FormatParser<'input> = &mut self.parser;
        let mut dyn_deser = crate::FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };
        let result = deserialize_enum_as_struct_inner(&mut dyn_deser, wip, enum_def);
        self.last_span = dyn_deser.last_span;
        result
    }

    pub(crate) fn deserialize_result_as_enum(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use crate::FormatParser;
        let dyn_parser: &mut dyn FormatParser<'input> = &mut self.parser;
        let mut dyn_deser = crate::FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };
        let result = deserialize_result_as_enum_inner(&mut dyn_deser, wip);
        self.last_span = dyn_deser.last_span;
        result
    }

    /// Deserialize the struct fields of a variant.
    /// Expects the variant to already be selected.
    pub(crate) fn deserialize_variant_struct_fields(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use crate::FormatParser;
        let dyn_parser: &mut dyn FormatParser<'input> = &mut self.parser;
        let mut dyn_deser = crate::FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };
        let result = deserialize_variant_struct_fields_inner(&mut dyn_deser, wip);
        self.last_span = dyn_deser.last_span;
        result
    }

    fn deserialize_enum_adjacently_tagged(
        &mut self,
        wip: Partial<'input, BORROW>,
        tag_key: &'static str,
        content_key: &'static str,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use crate::FormatParser;
        let dyn_parser: &mut dyn FormatParser<'input> = &mut self.parser;
        let mut dyn_deser = crate::FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };
        let result =
            deserialize_enum_adjacently_tagged_inner(&mut dyn_deser, wip, tag_key, content_key);
        self.last_span = dyn_deser.last_span;
        result
    }

    /// Deserialize the content of an already-selected enum variant.
    ///
    /// This uses dynamic dispatch to reduce monomorphization - the inner logic
    /// is compiled once (not per parser type).
    pub(crate) fn deserialize_enum_variant_content(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use crate::FormatParser;
        let dyn_parser: &mut dyn FormatParser<'input> = &mut self.parser;
        let mut dyn_deser = crate::FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };
        let result = deserialize_enum_variant_content_inner(&mut dyn_deser, wip);
        self.last_span = dyn_deser.last_span;
        result
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
        // Always use Owned variant - we need to own the deserialized data
        wip = wip.select_variant_named("Owned")?;

        // Deserialize directly into the variant's single field
        wip = wip.begin_nth_field(0)?;
        wip = self.deserialize_into(wip)?;
        wip = wip.end()?;

        Ok(wip)
    }

    fn deserialize_numeric_enum(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let event = self.parser.peek_event()?;

        if let Some(ParseEvent::Scalar(scalar)) = event {
            let span = self.last_span;
            wip = match scalar {
                ScalarValue::I64(discriminant) => {
                    wip.select_variant(discriminant)
                        .map_err(|error| DeserializeError {
                            span,
                            path: None,
                            kind: DeserializeErrorKind::Reflect(error),
                        })?
                }
                ScalarValue::U64(discriminant) => {
                    wip.select_variant(discriminant as i64)
                        .map_err(|error| DeserializeError {
                            span,
                            path: None,
                            kind: DeserializeErrorKind::Reflect(error),
                        })?
                }
                ScalarValue::Str(str_discriminant) => {
                    let discriminant = str_discriminant.parse().map_err(|_| DeserializeError {
                        span: self.last_span,
                        path: None,
                        kind: DeserializeErrorKind::UnexpectedToken {
                            expected: "string representing an integer (i64)",
                            got: str_discriminant.to_string().into(),
                        },
                    })?;
                    wip.select_variant(discriminant)
                        .map_err(|error| DeserializeError {
                            span,
                            path: None,
                            kind: DeserializeErrorKind::Reflect(error),
                        })?
                }
                _ => {
                    return Err(DeserializeError::unsupported(
                        "Unexpected ScalarValue".to_string(),
                    ));
                }
            };
            self.parser.next_event()?;
            Ok(wip)
        } else {
            Err(DeserializeError::unsupported(
                "Expected integer value".to_string(),
            ))
        }
    }

    fn deserialize_enum_untagged(
        &mut self,
        wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use crate::FormatParser;
        let dyn_parser: &mut dyn FormatParser<'input> = &mut self.parser;
        let mut dyn_deser = crate::FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };
        let result = deserialize_enum_untagged_inner(&mut dyn_deser, wip);
        self.last_span = dyn_deser.last_span;
        result
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
        wip: Partial<'input, BORROW>,
        captured_tag: Option<&'input str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use crate::FormatParser;
        let dyn_parser: &mut dyn FormatParser<'input> = &mut self.parser;
        let mut dyn_deser = crate::FormatDeserializer {
            parser: dyn_parser,
            last_span: self.last_span,
            current_path: self.current_path.clone(),
            _marker: core::marker::PhantomData,
        };
        let result =
            deserialize_other_variant_with_captured_tag_inner(&mut dyn_deser, wip, captured_tag);
        self.last_span = dyn_deser.last_span;
        result
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
fn find_field_path(
    fields: &'static [facet_core::Field],
    key: &str,
) -> Option<alloc::vec::Vec<&'static str>> {
    for field in fields {
        // Check if this field matches directly (by effective name or alias)
        if field.effective_name() == key {
            return Some(alloc::vec![field.name]);
        }

        // Check alias
        if field.alias == Some(key) {
            return Some(alloc::vec![field.name]);
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
