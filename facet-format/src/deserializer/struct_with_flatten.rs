extern crate alloc;

use facet_core::Def;
use facet_reflect::{FieldCategory, Partial};

use crate::{DeserializeError, FormatDeserializer, FormatParser, ParseEvent, ScalarValue};

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    /// Deserialize a struct with flattened fields using facet-solver.
    ///
    /// This uses the solver's Schema/Resolution to handle arbitrarily nested
    /// flatten structures by looking up the full path for each field.
    /// It also handles flattened enums by using probing to collect keys first,
    /// then using the Solver to disambiguate between resolutions.
    pub(crate) fn deserialize_struct_with_flatten(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use alloc::borrow::Cow;
        use alloc::collections::BTreeSet;
        use facet_core::Characteristic;
        use facet_solver::{PathSegment, Schema, Solver};

        let deny_unknown_fields = wip.shape().has_deny_unknown_fields_attr();
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

        // Build the schema for this type - this recursively expands all flatten fields
        let schema = Schema::build_auto(wip.shape())
            .map_err(|e| DeserializeError::Unsupported(format!("failed to build schema: {e}")))?;

        // Check if we have multiple resolutions (i.e., flattened enums)
        let resolutions = schema.resolutions();
        if resolutions.is_empty() {
            return Err(DeserializeError::Unsupported(
                "schema has no resolutions".into(),
            ));
        }

        // ========== PASS 1: Probe to collect all field keys ==========
        let probe = self
            .parser
            .begin_probe()
            .map_err(DeserializeError::Parser)?;
        let evidence = Self::collect_evidence(probe).map_err(DeserializeError::Parser)?;

        // Feed keys to solver to narrow down resolutions
        let mut solver = Solver::new(&schema);
        for ev in &evidence {
            solver.see_key(ev.name.clone());
        }

        // Get the resolved configuration
        let config_handle = solver
            .finish()
            .map_err(|e| DeserializeError::Unsupported(format!("solver failed: {e}")))?;
        let resolution = config_handle.resolution();

        // ========== PASS 2: Parse the struct with resolved paths ==========
        // Expect StructStart
        let event = self.expect_event("value")?;
        if !matches!(event, ParseEvent::StructStart(_)) {
            return Err(DeserializeError::TypeMismatch {
                expected: "struct start",
                got: format!("{event:?}"),
                span: self.last_span,
                path: None,
            });
        }

        // Enter deferred mode for flatten handling (if not already in deferred mode)
        let already_deferred = wip.is_deferred();
        if !already_deferred {
            wip = wip.begin_deferred().map_err(DeserializeError::reflect)?;
        }

        // Track which fields have been set (by serialized name - uses 'static str from resolution)

        let mut fields_set: BTreeSet<&'static str> = BTreeSet::new();

        // Track currently open path segments: (field_name, is_option, is_variant)
        // The is_variant flag indicates if we've selected a variant at this level
        let mut open_segments: alloc::vec::Vec<(&str, bool, bool)> = alloc::vec::Vec::new();

        loop {
            let event = self.expect_event("value")?;
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key) => {
                    // Look up field in the resolution
                    if let Some(field_info) = resolution.field_by_name(key.name.as_ref()) {
                        let segments = field_info.path.segments();

                        // Check if this path ends with a Variant segment (externally-tagged enum)
                        let ends_with_variant = segments
                            .last()
                            .is_some_and(|s| matches!(s, PathSegment::Variant(_, _)));

                        // Extract field names from the path (excluding trailing Variant)
                        let field_segments: alloc::vec::Vec<&str> = segments
                            .iter()
                            .filter_map(|s| match s {
                                PathSegment::Field(name) => Some(*name),
                                PathSegment::Variant(_, _) => None,
                            })
                            .collect();

                        // Find common prefix with currently open segments
                        let common_len = open_segments
                            .iter()
                            .zip(field_segments.iter())
                            .take_while(|((name, _, _), b)| *name == **b)
                            .count();

                        // Close segments that are no longer needed (in reverse order)
                        while open_segments.len() > common_len {
                            let (_, is_option, _) = open_segments.pop().unwrap();
                            if is_option {
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                            }
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }

                        // Open new segments
                        for &segment in &field_segments[common_len..] {
                            wip = wip
                                .begin_field(segment)
                                .map_err(DeserializeError::reflect)?;
                            let is_option = matches!(wip.shape().def, Def::Option(_));
                            if is_option {
                                wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                            }
                            open_segments.push((segment, is_option, false));
                        }

                        if ends_with_variant {
                            // For externally-tagged enums: select variant and deserialize content
                            if let Some(PathSegment::Variant(_, variant_name)) = segments.last() {
                                wip = wip
                                    .select_variant_named(variant_name)
                                    .map_err(DeserializeError::reflect)?;
                                // Deserialize the variant's struct content (the nested object)
                                wip = self.deserialize_variant_struct_fields(wip)?;
                            }
                        } else {
                            // Regular field: deserialize into it
                            wip = self.deserialize_into(wip)?;
                        }

                        // Close segments we just opened (we're done with this field)
                        while open_segments.len() > common_len {
                            let (_, is_option, _) = open_segments.pop().unwrap();
                            if is_option {
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                            }
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }

                        // Store the static serialized_name from the resolution
                        fields_set.insert(field_info.serialized_name);
                        continue;
                    }

                    // Check if we have a catch-all map for unknown fields
                    // For flat formats, use FieldCategory::Flat
                    if let Some(catch_all_info) = resolution.catch_all_map(FieldCategory::Flat) {
                        // Route unknown field to catch-all map
                        wip = self.insert_into_catch_all_map(
                            wip,
                            catch_all_info,
                            Cow::Borrowed(key.name.as_ref()),
                            &mut fields_set,
                        )?;
                        continue;
                    }

                    if deny_unknown_fields {
                        return Err(DeserializeError::UnknownField {
                            field: key.name.into_owned(),
                            span: self.last_span,
                            path: None,
                        });
                    } else {
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

        // Close any remaining open segments
        while let Some((_, is_option, _)) = open_segments.pop() {
            if is_option {
                wip = wip.end().map_err(DeserializeError::reflect)?;
            }
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        // Defaults for missing fields are applied automatically by facet-reflect's
        // fill_defaults() when finish_deferred() or build()/end() is called.

        // Finish deferred mode (only if we started it)
        if !already_deferred {
            wip = wip.finish_deferred().map_err(DeserializeError::reflect)?;
        }

        Ok(wip)
    }

    /// Insert a key-value pair into a catch-all map field.
    ///
    /// This navigates to the catch-all map field (handling Option wrappers),
    /// initializes it if needed, and inserts the key-value pair.
    fn insert_into_catch_all_map(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        catch_all_info: &facet_reflect::FieldInfo,
        key: alloc::borrow::Cow<'_, str>,
        fields_set: &mut alloc::collections::BTreeSet<&'static str>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use facet_solver::PathSegment;

        let segments = catch_all_info.path.segments();

        // Extract field names from the path
        let field_segments: alloc::vec::Vec<&str> = segments
            .iter()
            .filter_map(|s| match s {
                PathSegment::Field(name) => Some(*name),
                PathSegment::Variant(_, _) => None,
            })
            .collect();

        // Track how many segments we opened so we can close them
        let mut opened_segments: alloc::vec::Vec<bool> = alloc::vec::Vec::new(); // true = is_option

        // Navigate to the map field
        for &segment in &field_segments {
            wip = wip
                .begin_field(segment)
                .map_err(DeserializeError::reflect)?;
            let is_option = matches!(wip.shape().def, Def::Option(_));
            if is_option {
                wip = wip.begin_some().map_err(DeserializeError::reflect)?;
            }
            opened_segments.push(is_option);
        }

        // Initialize the map if this is our first time
        let map_field_name = catch_all_info.serialized_name;
        if !fields_set.contains(map_field_name) {
            wip = wip.init_map().map_err(DeserializeError::reflect)?;
            fields_set.insert(map_field_name);
        }

        // Insert the key-value pair
        wip = wip.begin_key().map_err(DeserializeError::reflect)?;
        wip = self.set_string_value(wip, alloc::borrow::Cow::Owned(key.into_owned()))?;
        wip = wip.end().map_err(DeserializeError::reflect)?;

        wip = wip.begin_value().map_err(DeserializeError::reflect)?;
        wip = self.deserialize_into(wip)?;
        wip = wip.end().map_err(DeserializeError::reflect)?;

        // Close segments in reverse order
        for is_option in opened_segments.into_iter().rev() {
            if is_option {
                wip = wip.end().map_err(DeserializeError::reflect)?;
            }
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        Ok(wip)
    }
}
