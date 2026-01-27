use facet_core::{Type, UserType};
use facet_reflect::Partial;

use crate::{
    DeserializeError, DeserializeErrorKind, FormatDeserializer, ParseEventKind, ScalarValue,
    SpanGuard,
};

impl<'parser, 'input, const BORROW: bool> FormatDeserializer<'parser, 'input, BORROW> {
    /// Deserialize a struct without flattened fields (simple case).
    pub(crate) fn deserialize_struct_simple(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use facet_core::Characteristic;

        // Get struct fields for lookup (needed before hint)
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(self.mk_err(
                    &wip,
                    DeserializeErrorKind::Unsupported {
                        message: format!("expected struct type but got {:?}", wip.shape().ty)
                            .into(),
                    },
                ));
            }
        };

        // Hint to non-self-describing parsers how many fields to expect
        self.parser.hint_struct_fields(struct_def.fields.len());

        let struct_type_has_default = wip.shape().is(Characteristic::Default);

        // Peek at the next event first to handle EOF and null gracefully
        let maybe_event = self.peek_event_opt()?;

        // Handle EOF (empty input / comment-only files): use Default if available
        if maybe_event.is_none() {
            if struct_type_has_default {
                let _guard = SpanGuard::new(self.last_span);
                wip = wip.set_default()?;
                return Ok(wip);
            }
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedEof { expected: "value" },
            ));
        }

        // Handle Scalar(Null): use Default if available
        if let Some(ref event) = maybe_event
            && matches!(event.kind, ParseEventKind::Scalar(ScalarValue::Null))
            && struct_type_has_default
        {
            let _ = self.expect_event("null")?;
            let _guard = SpanGuard::new(self.last_span);
            wip = wip.set_default()?;
            return Ok(wip);
        }

        let event = self.expect_event("value")?;

        if !matches!(event.kind, ParseEventKind::StructStart(_)) {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "struct start",
                    got: event.kind_name().into(),
                },
            ));
        }
        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();

        // Track which fields have been set
        let num_fields = struct_def.fields.len();
        let mut fields_set = vec![false; num_fields];
        let mut ordered_field_index = 0usize;

        loop {
            let event = self.expect_event("value")?;
            let _guard = SpanGuard::new(self.last_span);
            trace!(
                ?event,
                "deserialize_struct_simple: loop iteration, got event"
            );
            match event.kind {
                ParseEventKind::StructEnd => {
                    break;
                }
                ParseEventKind::OrderedField => {
                    // Non-self-describing formats emit OrderedField events in order
                    let idx = ordered_field_index;
                    ordered_field_index += 1;
                    if idx < num_fields {
                        wip = wip
                            .begin_nth_field(idx)?
                            .with(|w| self.deserialize_into(w))?
                            .end()?;

                        fields_set[idx] = true;
                    }
                }
                ParseEventKind::FieldKey(key) => {
                    trace!(?key, "deserialize_struct_simple: got FieldKey");

                    // Unit keys don't make sense for struct fields
                    let key_name = match &key.name {
                        Some(name) => name.as_ref(),
                        None => {
                            // Skip unit keys in struct context
                            self.skip_value()?;
                            continue;
                        }
                    };

                    // Look up field in struct fields by name/alias
                    let field_info = struct_def
                        .fields
                        .iter()
                        .enumerate()
                        .find(|(_, f)| Self::field_matches(f, key_name));

                    if let Some((idx, field)) = field_info {
                        trace!(
                            idx,
                            field_name = field.name,
                            "deserialize_struct_simple: matched field"
                        );

                        wip = wip.begin_nth_field(idx)?;
                        wip = self.deserialize_into(wip)?;

                        // Run validation on the field value before finalizing
                        self.run_field_validators(field, &wip)?;

                        let _guard = SpanGuard::new(self.last_span);
                        wip = wip.end()?;

                        fields_set[idx] = true;
                        continue;
                    }

                    if deny_unknown_fields {
                        return Err(self.mk_err(
                            &wip,
                            DeserializeErrorKind::UnknownField {
                                field: key_name.to_owned().into(),
                                suggestion: None,
                            },
                        ));
                    } else {
                        // Unknown field - skip it
                        trace!(field_name = ?key_name, "deserialize_struct_simple: skipping unknown field");
                        self.skip_value()?;
                    }
                }
                other => {
                    return Err(self.mk_err(
                        &wip,
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "field key or struct end",
                            got: other.kind_name().into(),
                        },
                    ));
                }
            }
        }

        // In deferred mode, skip validation - finish_deferred() will handle it.
        // This allows formats like TOML to reopen tables and set more fields later.
        if wip.is_deferred() {
            return Ok(wip);
        }

        // Defaults for missing fields are applied automatically by facet-reflect's
        // fill_defaults() when build() or end() is called.

        Ok(wip)
    }
}
