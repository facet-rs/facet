extern crate alloc;

use facet_core::{Type, UserType};
use facet_path::PathStep;
use facet_reflect::Partial;

use crate::{DeserializeError, FormatDeserializer, FormatParser, ParseEvent, ScalarValue};

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    /// Deserialize a struct without flattened fields (simple case).
    pub(crate) fn deserialize_struct_simple(
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

        let struct_type_has_default = wip.shape().is(Characteristic::Default);

        // Peek at the next event first to handle EOF and null gracefully
        let maybe_event = self.parser.peek_event().map_err(DeserializeError::Parser)?;

        // Handle EOF (empty input / comment-only files): use Default if available
        if maybe_event.is_none() {
            if struct_type_has_default {
                wip = wip.set_default().map_err(DeserializeError::reflect)?;
                return Ok(wip);
            }
            return Err(DeserializeError::UnexpectedEof { expected: "value" });
        }

        // Handle Scalar(Null): use Default if available
        if let Some(ParseEvent::Scalar(ScalarValue::Null)) = &maybe_event
            && struct_type_has_default
        {
            let _ = self.expect_event("null")?;
            wip = wip.set_default().map_err(DeserializeError::reflect)?;
            return Ok(wip);
        }

        let event = self.expect_event("value")?;

        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start",
                got: format!("{event:?}"),
                span: self.last_span,
                path: None,
            });
        }
        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();

        // Track which fields have been set
        let num_fields = struct_def.fields.len();
        let mut fields_set = alloc::vec![false; num_fields];
        let mut ordered_field_index = 0usize;

        loop {
            let event = self.expect_event("value")?;
            trace!(
                ?event,
                "deserialize_struct_simple: loop iteration, got event"
            );
            match event {
                ParseEvent::StructEnd => {
                    break;
                }
                ParseEvent::OrderedField => {
                    // Non-self-describing formats emit OrderedField events in order
                    let idx = ordered_field_index;
                    ordered_field_index += 1;
                    if idx < num_fields {
                        // Track path for error reporting
                        self.push_path(PathStep::Field(idx as u32));

                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = match self.deserialize_into(wip) {
                            Ok(wip) => wip,
                            Err(e) => {
                                // Only add path if error doesn't already have one
                                // (inner errors already have more specific paths)
                                let result = if e.path().is_some() {
                                    e
                                } else {
                                    let path = self.path_clone();
                                    e.with_path(path)
                                };
                                self.pop_path();
                                return Err(result);
                            }
                        };
                        wip = wip.end().map_err(DeserializeError::reflect)?;

                        self.pop_path();

                        fields_set[idx] = true;
                    }
                }
                ParseEvent::FieldKey(key) => {
                    trace!(?key, "deserialize_struct_simple: got FieldKey");

                    // Unit keys don't make sense for struct fields
                    let key_name = match &key.name {
                        Some(name) => name.as_ref(),
                        None => {
                            // Skip unit keys in struct context
                            self.parser.skip_value().map_err(DeserializeError::Parser)?;
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

                        // Track path for error reporting
                        self.push_path(PathStep::Field(idx as u32));

                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;

                        wip = match self.deserialize_into(wip) {
                            Ok(wip) => wip,
                            Err(e) => {
                                // Only add path if error doesn't already have one
                                // (inner errors already have more specific paths)
                                let result = if e.path().is_some() {
                                    e
                                } else {
                                    let path = self.path_clone();
                                    e.with_path(path)
                                };
                                self.pop_path();
                                return Err(result);
                            }
                        };

                        // Run validation on the field value before finalizing
                        #[cfg(feature = "validate")]
                        self.run_field_validators(field, &wip)?;

                        #[cfg(not(feature = "validate"))]
                        let _ = field;

                        wip = wip.end().map_err(DeserializeError::reflect)?;

                        self.pop_path();

                        fields_set[idx] = true;
                        continue;
                    }

                    if deny_unknown_fields {
                        return Err(DeserializeError::UnknownField {
                            field: key_name.to_owned(),
                            span: self.last_span,
                            path: None,
                        });
                    } else {
                        // Unknown field - skip it
                        trace!(field_name = ?key_name, "deserialize_struct_simple: skipping unknown field");
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
