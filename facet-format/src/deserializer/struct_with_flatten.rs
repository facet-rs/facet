use std::borrow::Cow;
use std::collections::BTreeSet;

use facet_core::{Characteristic, Def};
use facet_reflect::{FieldCategory, FieldInfo, Partial};
use facet_solver::PathSegment;

use crate::{
    DeserializeError, DeserializeErrorKind, FormatDeserializer, ParseEvent, ScalarValue, SpanGuard,
};

/// Tracks an open path segment during flatten deserialization.
#[derive(Debug, Clone)]
struct OpenSegment<'a> {
    /// The field name of this segment.
    name: &'a str,
    /// Whether this segment is wrapped in an Option (and we entered Some).
    is_option: bool,
    /// Whether a variant was selected at this segment.
    has_variant: bool,
}

impl<'input, const BORROW: bool> FormatDeserializer<'input, BORROW> {
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
        let maybe_event = self.parser.peek_event()?;

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
        if let Some(ParseEvent::Scalar(ScalarValue::Null)) = &maybe_event
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
        if !matches!(event, ParseEvent::StructStart(_)) {
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

        // Track which fields have been set (by serialized name - uses 'static str from resolution)
        let mut fields_set: BTreeSet<&'static str> = BTreeSet::new();

        // Track currently open path segments
        let mut open_segments: Vec<OpenSegment<'_>> = Vec::new();

        // Build a lookup for variant selections by path depth
        let variant_selections = resolution.variant_selections();

        loop {
            let event = self.expect_event("value")?;
            let _guard = SpanGuard::new(self.last_span);
            match event {
                ParseEvent::StructEnd => break,
                ParseEvent::FieldKey(key) => {
                    // Unit keys don't make sense for struct fields
                    let key_name = match &key.name {
                        Some(name) => name.as_ref(),
                        None => {
                            // Skip unit keys in struct context
                            self.parser.skip_value()?;
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
                        let field_segments: Vec<&str> = segments
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
                            .take_while(|(seg, field_name)| seg.name == **field_name)
                            .count();

                        // Close segments that are no longer needed (in reverse order)
                        while open_segments.len() > common_len {
                            let seg = open_segments.pop().unwrap();
                            if seg.is_option {
                                wip = wip.end()?;
                            }
                            wip = wip.end()?;
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
                            wip = wip.begin_field(segment)?;
                            let is_option = matches!(wip.shape().def, Def::Option(_));
                            if is_option {
                                wip = wip.begin_some()?;
                            }

                            // Check if we need to select a variant at this point
                            let mut has_variant = false;

                            // Build current path from open_segments plus the segment we just opened
                            let current_path: Vec<&str> = open_segments
                                .iter()
                                .map(|seg| seg.name)
                                .chain(core::iter::once(segment))
                                .collect();

                            for vs in variant_selections {
                                let vs_fields: Vec<&str> = vs
                                    .path
                                    .segments()
                                    .iter()
                                    .filter_map(|s| match s {
                                        PathSegment::Field(name) => Some(*name),
                                        PathSegment::Variant(_, _) => None,
                                    })
                                    .collect();

                                // Check if current path matches the variant selection path
                                if current_path == vs_fields {
                                    wip = wip.select_variant_named(vs.variant_name)?;
                                    has_variant = true;
                                    break;
                                }
                            }

                            open_segments.push(OpenSegment {
                                name: segment,
                                is_option,
                                has_variant,
                            });
                        }

                        // Open the last segment (the actual field being deserialized)
                        if let Some(segment) = last_segment {
                            wip = wip.begin_field(segment)?;

                            let is_option = matches!(wip.shape().def, Def::Option(_));

                            if is_option {
                                // Check if the value is null before deciding to enter Some
                                let peeked = self.parser.peek_event()?;
                                if matches!(
                                    peeked,
                                    Some(ParseEvent::Scalar(ScalarValue::Null | ScalarValue::Unit))
                                ) {
                                    // Value is null - consume it and set Option to None
                                    let _ = self.expect_event("null or unit")?;
                                    let _guard = SpanGuard::new(self.last_span);
                                    // Set default (None) for the Option field
                                    wip = wip.set_default()?;
                                    open_segments.push(OpenSegment {
                                        name: segment,
                                        is_option: false,
                                        has_variant: false,
                                    });
                                    // Close segments we just opened
                                    while open_segments.len() > common_len {
                                        let seg = open_segments.pop().unwrap();
                                        if seg.is_option {
                                            wip = wip.end()?;
                                        }
                                        wip = wip.end()?;
                                    }
                                    fields_set.insert(field_info.serialized_name);
                                    continue;
                                }
                                // Value is not null - enter Some and deserialize
                                wip = wip.begin_some()?;
                            }
                            open_segments.push(OpenSegment {
                                name: segment,
                                is_option,
                                has_variant: false,
                            });
                        }

                        if ends_with_variant {
                            if let Some(PathSegment::Variant(_, variant_name)) = segments.last() {
                                // Check if this is an internally-tagged enum tag field.
                                let is_internally_tagged_tag = field_info
                                    .value_shape
                                    .get_tag_attr()
                                    .is_some_and(|tag| tag == field_info.serialized_name);

                                if is_internally_tagged_tag {
                                    // Read and validate the tag value
                                    let tag_event =
                                        self.expect_event("internally-tagged enum tag value")?;
                                    let actual_tag = match &tag_event {
                                        ParseEvent::Scalar(ScalarValue::Str(s)) => s.as_ref(),
                                        _ => {
                                            return Err(self.mk_err(
                                                &wip,
                                                DeserializeErrorKind::UnexpectedToken {
                                                    expected: "string tag value",
                                                    got: tag_event.kind_name().into(),
                                                },
                                            ));
                                        }
                                    };

                                    if actual_tag != *variant_name {
                                        return Err(self.mk_err(
                                            &wip,
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
                                    wip = wip.select_variant_named(variant_name)?;

                                    // Mark this segment as having a variant selected
                                    if let Some(last) = open_segments.last_mut() {
                                        last.has_variant = true;
                                    }

                                    fields_set.insert(field_info.serialized_name);
                                    continue;
                                }

                                // For externally-tagged enums: select variant and deserialize content
                                let _guard = SpanGuard::new(self.last_span);
                                wip = wip.select_variant_named(variant_name)?;
                                wip = self.deserialize_variant_struct_fields(wip)?;
                            }
                        } else {
                            // Regular field: deserialize into it
                            wip = self.deserialize_into(wip)?;
                        }

                        // Close segments we just opened (we're done with this field)
                        let _guard = SpanGuard::new(self.last_span);
                        while open_segments.len() > common_len {
                            let seg = open_segments.pop().unwrap();
                            if seg.is_option {
                                wip = wip.end()?;
                            }
                            wip = wip.end()?;
                        }

                        fields_set.insert(field_info.serialized_name);
                        continue;
                    }

                    // Check if we have a catch-all map for unknown fields
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
                        return Err(self.mk_err(
                            &wip,
                            DeserializeErrorKind::UnknownField {
                                field: key_name.to_owned().into(),
                                suggestion: None,
                            },
                        ));
                    } else {
                        self.parser.skip_value()?;
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

        // Close any remaining open segments
        let _guard = SpanGuard::new(self.last_span);
        while let Some(seg) = open_segments.pop() {
            if seg.is_option {
                wip = wip.end()?;
            }
            wip = wip.end()?;
        }

        // Initialize catch-all map/value if it was never touched (no unknown fields)
        if let Some(catch_all_info) = resolution.catch_all_map(FieldCategory::Flat)
            && !fields_set.contains(catch_all_info.serialized_name)
        {
            wip = self.initialize_empty_catch_all(wip, catch_all_info)?;
        }

        // Finish deferred mode (only if we started it)
        if !already_deferred {
            wip = wip.finish_deferred()?;
        }

        Ok(wip)
    }

    /// Helper for inserting a key-value pair into a catch-all map field.
    fn insert_into_catch_all_map<'a>(
        &mut self,
        mut wip: Partial<'input, BORROW>,
        catch_all_info: &FieldInfo,
        key: Cow<'_, str>,
        fields_set: &mut BTreeSet<&'static str>,
        open_segments: &mut Vec<OpenSegment<'a>>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError>
    where
        'input: 'a,
    {
        let _guard = SpanGuard::new(self.last_span);
        let segments = catch_all_info.path.segments();

        // Extract field names from the path (these are 'static str from the schema)
        let field_segments: Vec<&'static str> = segments
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
            .take_while(|(seg, field_name)| seg.name == **field_name)
            .count();

        // Close segments that are no longer needed (in reverse order)
        while open_segments.len() > common_len {
            let seg = open_segments.pop().unwrap();
            if seg.is_option {
                wip = wip.end()?;
            }
            wip = wip.end()?;
        }

        // Open new segments needed for the catch-all map path
        for &segment in &field_segments[common_len..] {
            wip = wip.begin_field(segment)?;
            let is_option = matches!(wip.shape().def, Def::Option(_));
            if is_option {
                wip = wip.begin_some()?;
            }
            open_segments.push(OpenSegment {
                name: segment,
                is_option,
                has_variant: false,
            });
        }

        // Initialize the map if this is our first time
        let map_field_name = catch_all_info.serialized_name;
        let is_dynamic_value = matches!(wip.shape().def, Def::DynamicValue(_));
        if !fields_set.contains(map_field_name) {
            wip = wip.init_map()?;
            fields_set.insert(map_field_name);
        }

        // Insert the key-value pair - use different API for DynamicValue vs Map
        if is_dynamic_value {
            let key_owned = key.into_owned();
            wip = wip
                .begin_object_entry(&key_owned)?
                .with(|w| self.deserialize_into(w))?
                .end()?;
        } else {
            // Map uses begin_key() + set value + end() + begin_value() + deserialize + end()
            wip = wip.begin_key()?;
            wip = self.set_string_value(wip, Cow::Owned(key.into_owned()))?;
            wip = wip.end()?;

            wip = wip
                .begin_value()?
                .with(|w| self.deserialize_into(w))?
                .end()?;
        }

        Ok(wip)
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
        let mut opened_segments: Vec<bool> = Vec::new();

        // Navigate to the catch-all field
        for &segment in &field_segments {
            wip = wip.begin_field(segment)?;
            let is_option = matches!(wip.shape().def, Def::Option(_));
            if is_option {
                wip = wip.begin_some()?;
            }
            opened_segments.push(is_option);
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
        for is_option in opened_segments.into_iter().rev() {
            if is_option {
                wip = wip.end()?;
            }
            wip = wip.end()?;
        }

        Ok(wip)
    }
}
