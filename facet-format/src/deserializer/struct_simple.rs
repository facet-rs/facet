use facet_core::{StructType, Type, UserType};
use facet_reflect::Partial;

use crate::{
    DeserializeError, DeserializeErrorKind, FormatDeserializer, ParseEventKind, ScalarValue,
    SpanGuard, ValueMeta,
};

/// Look up a field by name using precomputed TypePlan if available, otherwise linear scan.
///
/// The TypePlan's FieldLookup is precomputed at `Partial::alloc()` time and provides
/// O(1) or O(log n) lookup. If no TypePlan is available (custom deserialization frames),
/// this falls back to a linear scan through struct fields.
#[inline]
fn lookup_field<const BORROW: bool>(
    wip: &Partial<'_, BORROW>,
    struct_def: &'static StructType,
    name: &str,
) -> Option<usize> {
    // Try precomputed lookup from TypePlan first
    if let Some(plan) = wip.struct_plan() {
        return plan.field_lookup.find(name, wip.type_plan_core());
    }

    // Fallback: linear scan through fields (for frames without TypePlan)
    struct_def
        .fields
        .iter()
        .enumerate()
        .find(|(_, f)| f.effective_name() == name || f.alias == Some(name))
        .map(|(i, _)| i)
}

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
        if self.is_non_self_describing() {
            self.parser.hint_struct_fields(struct_def.fields.len());
        }

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
        let deny_unknown_fields = wip.struct_plan().unwrap().deny_unknown_fields;

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
                    if idx < struct_def.fields.len() {
                        wip = wip
                            .begin_nth_field(idx)?
                            .with(|w| self.deserialize_into(w, None))?
                            .end()?;
                    }
                }
                ParseEventKind::FieldKey(key) => {
                    trace!(?key, "deserialize_struct_simple: got FieldKey");

                    // Unit keys don't make sense for struct fields
                    let key_name = match key.name() {
                        Some(name) => name.as_ref(),
                        None => {
                            // Skip unit keys in struct context
                            self.skip_value()?;
                            continue;
                        }
                    };

                    // Look up field by name/alias using precomputed TypePlan lookup
                    if let Some(idx) = lookup_field(&wip, struct_def, key_name) {
                        trace!(
                            idx,
                            field_name = struct_def.fields[idx].name,
                            "deserialize_struct_simple: matched field"
                        );

                        // Extract metadata from key for metadata containers
                        let mut meta_builder = ValueMeta::builder();
                        if let Some(lines) = key.doc() {
                            meta_builder = meta_builder.doc(
                                lines
                                    .iter()
                                    .map(|s| std::borrow::Cow::Owned(s.to_string()))
                                    .collect(),
                            );
                        }
                        if let Some(t) = key.tag() {
                            meta_builder = meta_builder.tag(std::borrow::Cow::Owned(t.to_string()));
                        }
                        let meta = meta_builder.build();

                        wip = wip.begin_nth_field(idx)?;
                        wip = self.deserialize_into(wip, Some(&meta))?;

                        let _guard = SpanGuard::new(self.last_span);
                        wip = wip.end()?;
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
