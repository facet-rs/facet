extern crate alloc;

use facet_core::{Def, Type, UserType};
use facet_path::PathStep;
use facet_reflect::Partial;

use crate::{
    DeserializeError, FieldLocationHint, FormatDeserializer, FormatParser, ParseEvent, ScalarValue,
    deserializer::VariantMatch,
};

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

        // Expect StructStart, but for XML/HTML, a scalar means text-only element
        let event = self.expect_event("value")?;
        if let ParseEvent::Scalar(scalar) = &event {
            // For XML/HTML, a text-only element is emitted as a scalar.
            // If the struct has a text field, set it from the scalar.
            if let Some((idx, _field)) = struct_def
                .fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.is_text())
            {
                wip = wip
                    .begin_nth_field(idx)
                    .map_err(DeserializeError::reflect)?;

                // Handle Option<T>
                let is_option = matches!(&wip.shape().def, Def::Option(_));
                if is_option {
                    wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                }

                wip = self.set_scalar(wip, scalar.clone())?;

                if is_option {
                    wip = wip.end().map_err(DeserializeError::reflect)?;
                }
                wip = wip.end().map_err(DeserializeError::reflect)?;

                // Defaults for other fields are applied automatically by facet-reflect's
                // fill_defaults() when build() or end() is called.
                return Ok(wip);
            }

            // No xml::text field - this is an error
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start",
                got: format!("{event:?}"),
                span: self.last_span,
                path: None,
            });
        }

        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start",
                got: format!("{event:?}"),
                span: self.last_span,
                path: None,
            });
        }
        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();

        // Extract container-level default namespace (xml::ns_all) for namespace-aware matching
        let ns_all = wip
            .shape()
            .attributes
            .iter()
            .find(|attr| attr.ns == Some("xml") && attr.key == "ns_all")
            .and_then(|attr| attr.get_as::<&str>().copied());

        // Track which fields have been set
        let num_fields = struct_def.fields.len();
        let mut fields_set = alloc::vec![false; num_fields];
        let mut ordered_field_index = 0usize;

        // Track xml::elements field state for collecting child elements into lists
        // When Some((idx, in_list)), we're collecting items into field at idx
        let mut elements_field_state: Option<(usize, bool)> = None;

        loop {
            let event = self.expect_event("value")?;
            match event {
                ParseEvent::StructEnd => {
                    // End any open xml::elements field
                    // Note: begin_list() doesn't push a frame, so we only need to end the field
                    if let Some((_, true)) = elements_field_state {
                        wip = wip.end().map_err(DeserializeError::reflect)?; // end field only
                    }
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
                    // Look up field in struct fields (direct match)
                    // Exclude xml::elements fields - they accumulate repeated child elements
                    // and must be handled via find_elements_field_for_element below
                    let field_info = struct_def.fields.iter().enumerate().find(|(_, f)| {
                        !f.is_elements()
                            && Self::field_matches_with_namespace(
                                f,
                                key.name.as_ref(),
                                key.namespace.as_deref(),
                                key.location,
                                ns_all,
                            )
                    });

                    if let Some((idx, field)) = field_info {
                        // End any open xml::elements field before switching to a different field
                        // Note: begin_list() doesn't push a frame, so we only end the field
                        if let Some((elem_idx, true)) = elements_field_state
                            && elem_idx != idx
                        {
                            wip = wip.end().map_err(DeserializeError::reflect)?; // end field only
                            elements_field_state = None;
                        }

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

                    // Check if this child element should go into an elements field
                    if key.location == FieldLocationHint::Child
                        && let Some((idx, field)) = self.find_elements_field_for_element(
                            struct_def.fields,
                            key.name.as_ref(),
                            key.namespace.as_deref(),
                            ns_all,
                        )
                    {
                        // Start or continue the list for this elements field
                        match elements_field_state {
                            None => {
                                // Start new list
                                wip = wip
                                    .begin_nth_field(idx)
                                    .map_err(DeserializeError::reflect)?;
                                wip = wip.begin_list().map_err(DeserializeError::reflect)?;
                                elements_field_state = Some((idx, true));
                                fields_set[idx] = true;
                            }
                            Some((current_idx, true)) if current_idx != idx => {
                                // Switching to a different xml::elements field
                                // Note: begin_list() doesn't push a frame, so we only end the field
                                wip = wip.end().map_err(DeserializeError::reflect)?; // end field only
                                wip = wip
                                    .begin_nth_field(idx)
                                    .map_err(DeserializeError::reflect)?;
                                wip = wip.begin_list().map_err(DeserializeError::reflect)?;
                                elements_field_state = Some((idx, true));
                                fields_set[idx] = true;
                            }
                            Some((current_idx, true)) if current_idx == idx => {
                                // Continue adding to same list
                            }
                            _ => {}
                        }

                        // Add item to list
                        wip = wip.begin_list_item().map_err(DeserializeError::reflect)?;

                        // For enum item types, we need to select the variant based on element name
                        let item_shape = Self::get_list_item_shape(field.shape());
                        if let Some(item_shape) = item_shape {
                            if let Type::User(UserType::Enum(enum_def)) = &item_shape.ty {
                                // Find matching variant (direct or custom_element fallback)
                                match Self::find_variant_for_element(enum_def, key.name.as_ref()) {
                                    Some(VariantMatch::Direct(variant_idx))
                                    | Some(VariantMatch::CustomElement(variant_idx)) => {
                                        wip = wip
                                            .select_nth_variant(variant_idx)
                                            .map_err(DeserializeError::reflect)?;
                                        // After selecting variant, deserialize the variant content
                                        // For custom elements, the _tag field will be matched
                                        // by FieldLocationHint::Tag
                                        wip = self.deserialize_enum_variant_content(wip)?;
                                    }
                                    None => {
                                        // No matching variant - deserialize directly
                                        wip = self.deserialize_into(wip)?;
                                    }
                                }
                            } else {
                                // Not an enum - deserialize directly
                                wip = self.deserialize_into(wip)?;
                            }
                        } else {
                            wip = self.deserialize_into(wip)?;
                        }

                        wip = wip.end().map_err(DeserializeError::reflect)?; // end list item
                        continue;
                    }

                    // Skip whitespace-only text fields when the struct has no text field
                    // (HTML parsers preserve inter-element whitespace, but structs without
                    // a text field don't want it - e.g., <head>\n<meta>\n</head>)
                    if key.location == FieldLocationHint::Text
                        && !struct_def.fields.iter().any(|f| f.is_text())
                        && let Some(ParseEvent::Scalar(ScalarValue::Str(s))) =
                            self.parser.peek_event().ok().flatten()
                        && s.chars().all(|c| c.is_whitespace())
                    {
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
                        continue;
                    }

                    if deny_unknown_fields {
                        return Err(DeserializeError::UnknownField {
                            field: key.name.into_owned(),
                            span: self.last_span,
                            path: None,
                        });
                    } else {
                        // Unknown field - skip it
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

        // Initialize empty lists for xml::elements fields that weren't populated
        for (idx, field) in struct_def.fields.iter().enumerate() {
            if fields_set[idx] {
                continue;
            }

            // elements fields with no items should get an empty list
            // begin_list() doesn't push a frame, so we just begin the field, begin the list,
            // then end the field (no end() for the list itself).
            if field.is_elements() {
                wip = wip
                    .begin_nth_field(idx)
                    .map_err(DeserializeError::reflect)?;
                wip = wip.begin_list().map_err(DeserializeError::reflect)?;
                wip = wip.end().map_err(DeserializeError::reflect)?; // end field only
            }
        }

        // Defaults for missing fields are applied automatically by facet-reflect's
        // fill_defaults() when build() or end() is called.

        Ok(wip)
    }
}
