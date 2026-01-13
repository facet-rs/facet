extern crate alloc;

use std::borrow::Cow;

use facet_core::{StructKind, Type, UserType};
use facet_reflect::Partial;

use crate::{DeserializeError, FormatDeserializer, FormatParser, ParseEvent, ScalarValue};

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
        tracing::trace!("deserialize_enum_externally_tagged called");
        let event = self.expect_peek("value")?;
        tracing::trace!(?event, "peeked event");
        // Check for unit variant (just a string)
        if let ParseEvent::Scalar(
            ScalarValue::Str(variant_name) | ScalarValue::StringlyTyped(variant_name),
        ) = &event
        {
            self.expect_event("value")?;
            wip = wip
                .select_variant_named(variant_name)
                .map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        // Check for VariantTag (self-describing formats like STYX)
        if let ParseEvent::VariantTag(tag_name) = &event {
            let tag_name = *tag_name;
            self.expect_event("value")?; // consume VariantTag

            // Look up the real variant name respecting rename attributes
            let enum_def = match &wip.shape().ty {
                Type::User(UserType::Enum(e)) => e,
                _ => return Err(DeserializeError::Unsupported("expected enum".into())),
            };
            // First try exact match, then fall back to #[facet(other)] variant
            let by_display = Self::find_variant_by_display_name(enum_def, tag_name);
            let variant_name = by_display
                .or_else(|| {
                    enum_def
                        .variants
                        .iter()
                        .find(|v| v.is_other())
                        .map(|v| v.name)
                })
                .ok_or_else(|| DeserializeError::TypeMismatch {
                    expected: "known enum variant",
                    got: format!("@{}", tag_name),
                    span: self.last_span,
                    path: None,
                })?;

            wip = wip
                .select_variant_named(variant_name)
                .map_err(DeserializeError::reflect)?;
            // Deserialize the variant content
            wip = self.deserialize_enum_variant_content(wip)?;
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
                    .map(|v| v.name)
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
}
