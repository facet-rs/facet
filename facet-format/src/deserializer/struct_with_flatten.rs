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

        trace!(
            "deserialize_struct_with_flatten: starting shape={}",
            wip.shape().type_identifier
        );

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
                    // Unit keys don't make sense for struct fields
                    let key_name = match &key.name {
                        Some(name) => name.as_ref(),
                        None => {
                            // Skip unit keys in struct context
                            self.parser.skip_value().map_err(DeserializeError::Parser)?;
                            continue;
                        }
                    };

                    // Look up field in the resolution
                    if let Some(field_info) = resolution.field_by_name(key_name) {
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

                        // Open new segments (all except last)
                        let segments_to_open = &field_segments[common_len..];
                        let (intermediate_segments, last_segment) = if segments_to_open.is_empty() {
                            (&[][..], None)
                        } else {
                            (
                                &segments_to_open[..segments_to_open.len() - 1],
                                Some(segments_to_open[segments_to_open.len() - 1]),
                            )
                        };

                        // Open intermediate segments (these are flatten containers, always enter Some)
                        for &segment in intermediate_segments {
                            wip = wip
                                .begin_field(segment)
                                .map_err(DeserializeError::reflect)?;
                            let is_option = matches!(wip.shape().def, Def::Option(_));
                            if is_option {
                                wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                            }
                            open_segments.push((segment, is_option, false));
                        }

                        // Open the last segment (the actual field being deserialized)
                        if let Some(segment) = last_segment {
                            wip = wip
                                .begin_field(segment)
                                .map_err(DeserializeError::reflect)?;
                            let is_option = matches!(wip.shape().def, Def::Option(_));

                            if is_option {
                                // Check if the value is null before deciding to enter Some
                                let peeked = self
                                    .parser
                                    .peek_event()
                                    .map_err(DeserializeError::Parser)?;
                                if matches!(
                                    peeked,
                                    Some(ParseEvent::Scalar(
                                        ScalarValue::Null | ScalarValue::Unit
                                    ))
                                ) {
                                    // Value is null - consume it and set Option to None
                                    let _ = self.expect_event("null or unit")?;
                                    // Set default (None) for the Option field
                                    wip = wip.set_default().map_err(DeserializeError::reflect)?;
                                    open_segments.push((segment, false, false));
                                    // Skip the deserialization below since we handled it
                                    // Close segments we just opened (we're done with this field)
                                    while open_segments.len() > common_len {
                                        let (_, is_opt, _) = open_segments.pop().unwrap();
                                        if is_opt {
                                            wip = wip.end().map_err(DeserializeError::reflect)?;
                                        }
                                        wip = wip.end().map_err(DeserializeError::reflect)?;
                                    }
                                    fields_set.insert(field_info.serialized_name);
                                    continue;
                                }
                                // Value is not null - enter Some and deserialize
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
                            Cow::Borrowed(key_name),
                            &mut fields_set,
                            &mut open_segments,
                        )?;
                        continue;
                    }

                    if deny_unknown_fields {
                        return Err(DeserializeError::UnknownField {
                            field: key_name.to_owned(),
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

        // Initialize catch-all map/value if it was never touched (no unknown fields)
        // This ensures the field is initialized even when empty
        if let Some(catch_all_info) = resolution.catch_all_map(FieldCategory::Flat)
            && !fields_set.contains(catch_all_info.serialized_name)
        {
            wip = self.initialize_empty_catch_all(wip, catch_all_info)?;
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
    ///
    /// Uses the shared `open_segments` state to avoid reopening/closing the path
    /// for consecutive unknown fields going to the same catch-all map.
    fn insert_into_catch_all_map<'a>(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        catch_all_info: &facet_reflect::FieldInfo,
        key: alloc::borrow::Cow<'_, str>,
        fields_set: &mut alloc::collections::BTreeSet<&'static str>,
        open_segments: &mut alloc::vec::Vec<(&'a str, bool, bool)>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>>
    where
        'input: 'a,
    {
        use facet_solver::PathSegment;

        let segments = catch_all_info.path.segments();

        // Extract field names from the path (these are 'static str from the schema)
        let field_segments: alloc::vec::Vec<&'static str> = segments
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

        // Open new segments needed for the catch-all map path
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

        // Initialize the map if this is our first time
        let map_field_name = catch_all_info.serialized_name;
        let is_dynamic_value = matches!(wip.shape().def, Def::DynamicValue(_));
        if !fields_set.contains(map_field_name) {
            wip = wip.init_map().map_err(DeserializeError::reflect)?;
            fields_set.insert(map_field_name);
        }

        // Insert the key-value pair - use different API for DynamicValue vs Map
        if is_dynamic_value {
            // DynamicValue uses begin_object_entry(key) which combines key setting
            let key_owned = key.into_owned();
            wip = wip
                .begin_object_entry(&key_owned)
                .map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        } else {
            // Map uses begin_key() + set value + end() + begin_value() + deserialize + end()
            wip = wip.begin_key().map_err(DeserializeError::reflect)?;
            wip = self.set_string_value(wip, alloc::borrow::Cow::Owned(key.into_owned()))?;
            wip = wip.end().map_err(DeserializeError::reflect)?;

            wip = wip.begin_value().map_err(DeserializeError::reflect)?;
            wip = self.deserialize_into(wip)?;
            wip = wip.end().map_err(DeserializeError::reflect)?;
        }

        // Don't close segments here - leave them open for consecutive catch-all insertions.
        // The main loop will close them when switching to a different path or at the end.

        Ok(wip)
    }

    /// Initialize an empty catch-all field when no unknown fields were encountered.
    ///
    /// This handles both HashMap catch-alls (init as empty map) and DynamicValue
    /// catch-alls (init as empty object).
    fn initialize_empty_catch_all(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        catch_all_info: &facet_reflect::FieldInfo,
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

        // Track opened segments so we can close them
        let mut opened_segments: alloc::vec::Vec<bool> = alloc::vec::Vec::new();

        // Navigate to the catch-all field
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

        // Initialize as empty based on the field's type
        match &wip.shape().def {
            Def::Map(_) => {
                // HashMap catch-all: init as empty map
                wip = wip.init_map().map_err(DeserializeError::reflect)?;
            }
            Def::DynamicValue(_) => {
                // DynamicValue catch-all: init as empty object
                wip = wip.init_map().map_err(DeserializeError::reflect)?;
            }
            _ => {
                // Other types: try to set default if available
                use facet_core::Characteristic;
                if wip.shape().is(Characteristic::Default) {
                    wip = wip.set_default().map_err(DeserializeError::reflect)?;
                }
                // If no default, let the deferred mode handle the error
            }
        }

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
