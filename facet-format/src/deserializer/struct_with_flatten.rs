use std::borrow::Cow;
use std::collections::BTreeSet;

use facet_core::{Characteristic, Def};
use facet_reflect::{FieldCategory, FieldInfo, Partial, VariantSelection};
use facet_solver::PathSegment;

use super::path_navigator::PathNavigator;
use crate::{
    DeserializeError, DeserializeErrorKind, FormatDeserializer, ParseEventKind, ScalarValue,
    SpanGuard,
};

impl<'parser, 'input, const BORROW: bool> FormatDeserializer<'parser, 'input, BORROW> {
    /// Deserialize a struct with flattened fields using facet-solver.
    ///
    /// This uses the solver's Schema/Resolution to handle arbitrarily nested
    /// flatten structures by looking up the full path for each field.
    /// It also handles flattened enums by using probing to collect keys first,
    /// then using the Solver to disambiguate between resolutions.
    pub(crate) fn deserialize_struct_with_flatten(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        use facet_solver::{Schema, Solver};

        trace!(
            "deserialize_struct_with_flatten: starting shape={}",
            wip.shape().type_identifier
        );

        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();
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

        // Build the schema for this type - this recursively expands all flatten fields
        let schema = Schema::build_auto(wip.shape()).map_err(|e| {
            self.mk_err(
                &wip,
                DeserializeErrorKind::Solver {
                    message: format!("failed to build schema: {e}").into(),
                },
            )
        })?;

        // Check if we have multiple resolutions (i.e., flattened enums)
        let resolutions = schema.resolutions();
        if resolutions.is_empty() {
            return Err(self.mk_err(
                &wip,
                DeserializeErrorKind::Solver {
                    message: "schema has no resolutions".into(),
                },
            ));
        }

        // ========== PASS 1: Probe to collect all field keys ==========
        let evidence = self.collect_evidence()?;

        let mut solver = Solver::new(&schema);

        // First pass: process tag hints BEFORE field-based narrowing.
        // For internally-tagged enums we must apply it first
        for ev in &evidence {
            if let Some(ScalarValue::Str(variant_name)) = &ev.scalar_value {
                solver.hint_variant_for_tag(&ev.name, variant_name);
            }
        }

        // Second pass: feed keys to solver to narrow down resolutions.
        for ev in &evidence {
            solver.see_key(ev.name.clone());
        }

        // Get the resolved configuration
        let config_handle = solver.finish().map_err(|e| {
            self.mk_err(
                &wip,
                DeserializeErrorKind::Solver {
                    message: format!("solver failed: {e}").into(),
                },
            )
        })?;
        let resolution = config_handle.resolution();

        // ========== PASS 2: Parse the struct with resolved paths ==========
        // Expect StructStart
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

        // Enter deferred mode for flatten handling (if not already in deferred mode)
        let already_deferred = wip.is_deferred();
        if !already_deferred {
            let _guard = SpanGuard::new(self.last_span);
            wip = wip.begin_deferred()?;
        }

        // Track which fields have been set (by serialized name)
        let mut fields_set: BTreeSet<&'static str> = BTreeSet::new();

        // Create navigator for path management
        let mut nav = PathNavigator::new(wip, self.last_span);

        let variant_selections = resolution.variant_selections();

        loop {
            let event = self.expect_event("value")?;
            nav.set_span(self.last_span);

            match event.kind {
                ParseEventKind::StructEnd => break,
                ParseEventKind::FieldKey(key) => {
                    // Unit keys don't make sense for struct fields
                    let key_name = match key.name() {
                        Some(name) => name.as_ref(),
                        None => {
                            self.skip_value()?;
                            continue;
                        }
                    };

                    // Look up field in the resolution
                    if let Some(field_info) = resolution.field_by_name(key_name) {
                        let nav_result = nav.navigate_to(&field_info.path, variant_selections)?;

                        // Handle trailing variant (externally-tagged or internally-tagged enum)
                        if let Some(variant_name) = nav_result.trailing_variant {
                            // Check if this is an internally-tagged enum tag field
                            let is_internally_tagged_tag = field_info
                                .value_shape
                                .get_tag_attr()
                                .is_some_and(|tag| tag == field_info.serialized_name);

                            if is_internally_tagged_tag {
                                // Read and validate the tag value
                                let tag_event =
                                    self.expect_event("internally-tagged enum tag value")?;
                                nav.set_span(self.last_span);

                                let actual_tag = match &tag_event.kind {
                                    ParseEventKind::Scalar(ScalarValue::Str(s)) => s.as_ref(),
                                    _ => {
                                        return Err(self.mk_err(
                                            nav.wip(),
                                            DeserializeErrorKind::UnexpectedToken {
                                                expected: "string tag value",
                                                got: tag_event.kind_name().into(),
                                            },
                                        ));
                                    }
                                };

                                if actual_tag != variant_name {
                                    return Err(self.mk_err(
                                        nav.wip(),
                                        DeserializeErrorKind::InvalidValue {
                                            message: format!(
                                                "expected tag value '{}', got '{}'",
                                                variant_name, actual_tag
                                            )
                                            .into(),
                                        },
                                    ));
                                }

                                let _guard = SpanGuard::new(self.last_span);
                                let wip = nav.take_wip();
                                nav.return_wip(wip.select_variant_named(variant_name)?);

                                // Only close final if we opened one (may be None if field was already open)
                                if nav_result.final_segment.is_some() {
                                    nav.close_final(nav_result.final_is_option)?;
                                }
                                fields_set.insert(field_info.serialized_name);
                                continue;
                            }

                            // For externally-tagged enums: select variant and deserialize content
                            let _guard = SpanGuard::new(self.last_span);
                            let wip = nav.take_wip();
                            let wip = wip.select_variant_named(variant_name)?;
                            let wip = self.deserialize_variant_struct_fields(wip)?;
                            nav.return_wip(wip);
                        } else {
                            // Regular field: deserialize into it
                            let wip = nav.take_wip();
                            let wip = self.deserialize_into(wip)?;
                            nav.return_wip(wip);
                        }

                        nav.set_span(self.last_span);
                        nav.close_final(nav_result.final_is_option)?;
                        fields_set.insert(field_info.serialized_name);
                        continue;
                    }

                    // Check if we have a catch-all map for unknown fields
                    if let Some(catch_all_info) = resolution.catch_all_map(FieldCategory::Flat) {
                        self.insert_into_catch_all_map(
                            &mut nav,
                            catch_all_info,
                            key_name,
                            &mut fields_set,
                            variant_selections,
                        )?;
                        continue;
                    }

                    if deny_unknown_fields {
                        return Err(self.mk_err(
                            nav.wip(),
                            DeserializeErrorKind::UnknownField {
                                field: key_name.to_owned().into(),
                                suggestion: None,
                            },
                        ));
                    } else {
                        self.skip_value()?;
                    }
                }
                other => {
                    return Err(self.mk_err(
                        nav.wip(),
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "field key or struct end",
                            got: other.kind_name().into(),
                        },
                    ));
                }
            }
        }

        // Close any remaining open segments
        nav.set_span(self.last_span);
        nav.close_all()?;

        let mut wip = nav.into_wip();

        // Initialize catch-all map/value if it was never touched (no unknown fields)
        if let Some(catch_all_info) = resolution.catch_all_map(FieldCategory::Flat)
            && !fields_set.contains(catch_all_info.serialized_name)
        {
            wip = self.initialize_empty_catch_all(wip, catch_all_info)?;
        }

        // Finish deferred mode (only if we started it)
        if !already_deferred {
            let _guard = SpanGuard::new(self.last_span);
            wip = wip.finish_deferred()?;
        }

        Ok(wip)
    }

    /// Helper for inserting a key-value pair into a catch-all map field.
    fn insert_into_catch_all_map(
        &mut self,
        nav: &mut PathNavigator<'input, BORROW>,
        catch_all_info: &FieldInfo,
        key: &str,
        fields_set: &mut BTreeSet<&'static str>,
        variant_selections: &[VariantSelection],
    ) -> Result<(), DeserializeError> {
        // Navigate to the catch-all map field
        let nav_result = nav.navigate_to(&catch_all_info.path, variant_selections)?;

        // Initialize the map if this is our first time
        let map_field_name = catch_all_info.serialized_name;
        let is_dynamic_value = matches!(nav.wip().shape().def, Def::DynamicValue(_));

        if !fields_set.contains(map_field_name) {
            let _guard = SpanGuard::new(self.last_span);
            let wip = nav.take_wip();
            nav.return_wip(wip.init_map()?);
            fields_set.insert(map_field_name);
        }

        // Insert the key-value pair
        let _guard = SpanGuard::new(self.last_span);
        if is_dynamic_value {
            let key_owned = key.to_owned();
            let wip = nav.take_wip();
            let wip = wip
                .begin_object_entry(&key_owned)?
                .with(|w| self.deserialize_into(w))?
                .end()?;
            nav.return_wip(wip);
        } else {
            // Map uses begin_key() + set value + end() + begin_value() + deserialize + end()
            let wip = nav.take_wip();
            let wip = wip.begin_key()?;
            let wip = self.set_string_value(wip, Cow::Owned(key.to_owned()))?;
            let wip = wip.end()?;
            let wip = wip
                .begin_value()?
                .with(|w| self.deserialize_into(w))?
                .end()?;
            nav.return_wip(wip);
        }

        // Keep the map field open for potential future entries
        nav.keep_final_open(&nav_result);

        Ok(())
    }

    /// Helper for initializing an empty catch-all field (no parser calls).
    fn initialize_empty_catch_all(
        &self,
        mut wip: Partial<'input, BORROW>,
        catch_all_info: &FieldInfo,
    ) -> Result<Partial<'input, BORROW>, DeserializeError> {
        let _guard = SpanGuard::new(self.last_span);
        let segments = catch_all_info.path.segments();

        // Extract field names from the path
        let field_segments: Vec<&str> = segments
            .iter()
            .filter_map(|s| match s {
                PathSegment::Field(name) => Some(*name),
                PathSegment::Variant(_, _) => None,
            })
            .collect();

        // Track opened segments so we can close them
        let mut opened_options: Vec<bool> = Vec::new();

        // Navigate to the catch-all field
        for &segment in &field_segments {
            wip = wip.begin_field(segment)?;
            let is_option = matches!(wip.shape().def, Def::Option(_));
            if is_option {
                wip = wip.begin_some()?;
            }
            opened_options.push(is_option);
        }

        // Initialize as empty based on the field's type
        match &wip.shape().def {
            Def::Map(_) | Def::DynamicValue(_) => {
                wip = wip.init_map()?;
            }
            _ => {
                if wip.shape().is(Characteristic::Default) {
                    wip = wip.set_default()?;
                }
            }
        }

        // Close segments in reverse order
        for is_option in opened_options.into_iter().rev() {
            if is_option {
                wip = wip.end()?;
            }
            wip = wip.end()?;
        }

        Ok(wip)
    }
}
