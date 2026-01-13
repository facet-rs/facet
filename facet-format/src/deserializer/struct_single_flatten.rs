extern crate alloc;

use facet_core::{Def, Type, UserType};
use facet_reflect::Partial;

use crate::{
    DeserializeError, FieldLocationHint, FormatDeserializer, FormatParser, ParseEvent, ScalarValue,
    deserializer::VariantMatch,
};

impl<'input, const BORROW: bool, P> FormatDeserializer<'input, BORROW, P>
where
    P: FormatParser<'input>,
{
    /// Deserialize a struct with single-level flattened fields (original approach).
    /// This handles simple flatten cases where there's no nested flatten or enum flatten.
    pub(crate) fn deserialize_struct_single_flatten(
        &mut self,
        mut wip: Partial<'input, BORROW>,
    ) -> Result<Partial<'input, BORROW>, DeserializeError<P::Error>> {
        use alloc::collections::BTreeMap;
        use facet_core::Characteristic;

        // Get struct fields for lookup
        let struct_type_name = wip.shape().type_identifier;
        let struct_def = match &wip.shape().ty {
            Type::User(UserType::Struct(def)) => def,
            _ => {
                return Err(DeserializeError::Unsupported(format!(
                    "expected struct type but got {:?}",
                    wip.shape().ty
                )));
            }
        };

        let struct_has_default = wip.shape().has_default_attr();
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
            // If the struct has a text field, set it from the scalar and default the rest.
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

                // Set defaults for other fields (including flattened ones)
                for (other_idx, other_field) in struct_def.fields.iter().enumerate() {
                    if other_idx == idx {
                        continue;
                    }

                    let field_has_default = other_field.has_default();
                    let field_type_has_default =
                        other_field.shape().is(facet_core::Characteristic::Default);
                    let field_is_option = matches!(other_field.shape().def, Def::Option(_));

                    if field_has_default || (struct_has_default && field_type_has_default) {
                        wip = wip
                            .set_nth_field_to_default(other_idx)
                            .map_err(DeserializeError::reflect)?;
                    } else if field_is_option {
                        wip = wip
                            .begin_field(other_field.name)
                            .map_err(DeserializeError::reflect)?;
                        wip = wip.set_default().map_err(DeserializeError::reflect)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else if other_field.should_skip_deserializing() {
                        // Skip fields that are marked for skip deserializing
                        continue;
                    } else {
                        return Err(DeserializeError::MissingField {
                            field: other_field.name,
                            type_name: struct_type_name,
                            span: self.last_span,
                            path: None,
                        });
                    }
                }

                return Ok(wip);
            }
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

        // Build flatten info: for each flattened field, get its inner struct fields
        // and track which inner fields have been set
        let mut flatten_info: alloc::vec::Vec<
            Option<(&'static [facet_core::Field], alloc::vec::Vec<bool>)>,
        > = alloc::vec![None; num_fields];

        // Track which fields are DynamicValue flattens (like facet_value::Value)
        let mut dynamic_value_flattens: alloc::vec::Vec<bool> = alloc::vec![false; num_fields];

        // Track flattened map field index (for collecting unknown keys)
        // Can be either:
        // - (outer_idx, None) for a directly flattened map
        // - (outer_idx, Some(inner_idx)) for a map nested inside a flattened struct
        let mut flatten_map_idx: Option<(usize, Option<usize>)> = None;

        // Track field names across flattened structs to detect duplicates
        let mut flatten_field_names: BTreeMap<&str, usize> = BTreeMap::new();

        for (idx, field) in struct_def.fields.iter().enumerate() {
            if field.is_flattened() {
                // Handle Option<T> flatten by unwrapping to inner type
                let inner_shape = match field.shape().def {
                    Def::Option(opt) => opt.t,
                    _ => field.shape(),
                };

                // Check if this is a DynamicValue flatten (like facet_value::Value)
                if matches!(inner_shape.def, Def::DynamicValue(_)) {
                    dynamic_value_flattens[idx] = true;
                } else if matches!(inner_shape.def, Def::Map(_)) {
                    // Flattened map - collects unknown keys
                    flatten_map_idx = Some((idx, None));
                } else if let Type::User(UserType::Struct(inner_def)) = &inner_shape.ty {
                    let inner_fields = inner_def.fields;
                    let inner_set = alloc::vec![false; inner_fields.len()];
                    flatten_info[idx] = Some((inner_fields, inner_set));

                    // Check for duplicate field names across flattened structs
                    for inner_field in inner_fields.iter() {
                        let field_name = inner_field.rename.unwrap_or(inner_field.name);
                        if let Some(_prev_idx) = flatten_field_names.insert(field_name, idx) {
                            return Err(DeserializeError::Unsupported(format!(
                                "duplicate field `{}` in flattened structs",
                                field_name
                            )));
                        }
                    }

                    // Also check for nested flattened maps inside this struct
                    // (e.g., GlobalAttrs has a flattened HashMap for unknown attributes)
                    if flatten_map_idx.is_none() {
                        for (inner_idx, inner_field) in inner_fields.iter().enumerate() {
                            if inner_field.is_flattened() {
                                let inner_inner_shape = match inner_field.shape().def {
                                    Def::Option(opt) => opt.t,
                                    _ => inner_field.shape(),
                                };
                                if matches!(inner_inner_shape.def, Def::Map(_)) {
                                    flatten_map_idx = Some((idx, Some(inner_idx)));
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Enter deferred mode for flatten handling (if not already in deferred mode)
        let already_deferred = wip.is_deferred();
        if !already_deferred {
            wip = wip.begin_deferred().map_err(DeserializeError::reflect)?;
        }

        // Track xml::elements field state for collecting child elements into lists
        // (field_idx, is_open)
        let mut elements_field_state: Option<(usize, bool)> = None;

        loop {
            let event = self.expect_event("value")?;
            match event {
                ParseEvent::StructEnd => {
                    // End any open xml::elements field
                    if let Some((_, true)) = elements_field_state {
                        wip = wip.end().map_err(DeserializeError::reflect)?; // end field only
                    }
                    break;
                }
                ParseEvent::FieldKey(key) => {
                    // First, look up field in direct struct fields (non-flattened, non-elements)
                    // Exclude xml::elements fields - they accumulate repeated child elements
                    // and must be handled via find_elements_field_for_element below
                    let direct_field_info = struct_def.fields.iter().enumerate().find(|(_, f)| {
                        !f.is_flattened()
                            && !f.is_elements()
                            && Self::field_matches_with_namespace(
                                f,
                                key.name.as_ref(),
                                key.namespace.as_deref(),
                                key.location,
                                ns_all,
                            )
                    });

                    if let Some((idx, _field)) = direct_field_info {
                        // End any open xml::elements field before switching to a different field
                        if let Some((elem_idx, true)) = elements_field_state
                            && elem_idx != idx
                        {
                            wip = wip.end().map_err(DeserializeError::reflect)?; // end field only
                            elements_field_state = None;
                        }

                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                        fields_set[idx] = true;
                        continue;
                    }

                    // Check if this child element or text node should go into an xml::elements field
                    // This handles both child elements and text nodes in mixed content
                    if matches!(
                        key.location,
                        FieldLocationHint::Child | FieldLocationHint::Text
                    ) && let Some((idx, field)) = self.find_elements_field_for_element(
                        struct_def.fields,
                        key.name.as_ref(),
                        key.namespace.as_deref(),
                        ns_all,
                    ) {
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

                    // Check flattened fields for a match
                    let mut found_flatten = false;
                    for (flatten_idx, field) in struct_def.fields.iter().enumerate() {
                        if !field.is_flattened() {
                            continue;
                        }
                        if let Some((inner_fields, inner_set)) = flatten_info[flatten_idx].as_mut()
                        {
                            let inner_match =
                                inner_fields.iter().enumerate().find(|(inner_idx, f)| {
                                    // Nested flattened map is handled separately, even if its name
                                    // matches the key name it must be skipped.
                                    let is_flatten_map =
                                        Some((flatten_idx, Some(*inner_idx))) == flatten_map_idx;

                                    !is_flatten_map
                                        && Self::field_matches_with_namespace(
                                            f,
                                            key.name.as_ref(),
                                            key.namespace.as_deref(),
                                            key.location,
                                            ns_all,
                                        )
                                });

                            if let Some((inner_idx, _inner_field)) = inner_match {
                                // Check if flatten field is Option - if so, wrap in Some
                                let is_option = matches!(field.shape().def, Def::Option(_));
                                wip = wip
                                    .begin_nth_field(flatten_idx)
                                    .map_err(DeserializeError::reflect)?;
                                if is_option {
                                    wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                                }
                                wip = wip
                                    .begin_nth_field(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                                wip = self.deserialize_into(wip)?;
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                                if is_option {
                                    wip = wip.end().map_err(DeserializeError::reflect)?;
                                }
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                                inner_set[inner_idx] = true;
                                fields_set[flatten_idx] = true;
                                found_flatten = true;
                                break;
                            }
                        }
                    }

                    if found_flatten {
                        continue;
                    }

                    // Check if this unknown field should go to a DynamicValue flatten
                    let mut found_dynamic = false;
                    for (flatten_idx, _field) in struct_def.fields.iter().enumerate() {
                        if !dynamic_value_flattens[flatten_idx] {
                            continue;
                        }

                        // This is a DynamicValue flatten - insert the field into it
                        // First, ensure the DynamicValue is initialized as an object
                        let is_option =
                            matches!(struct_def.fields[flatten_idx].shape().def, Def::Option(_));

                        // Navigate to the DynamicValue field
                        wip = wip
                            .begin_nth_field(flatten_idx)
                            .map_err(DeserializeError::reflect)?;
                        if is_option {
                            wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                        }
                        // Initialize or re-enter the DynamicValue as an object.
                        // begin_map() is idempotent - it returns Ok if already in Object state.
                        // We always call it because in deferred mode inside collections (like HashMap),
                        // the frame might not be stored/restored, so we can't rely on fields_set alone.
                        wip = wip.begin_map().map_err(DeserializeError::reflect)?;
                        fields_set[flatten_idx] = true;

                        // Insert the key-value pair into the object
                        wip = wip
                            .begin_object_entry(key.name.as_ref())
                            .map_err(DeserializeError::reflect)?;
                        wip = self.deserialize_into(wip)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;

                        // Navigate back out (Note: we close the map when we're done with ALL fields, not per-field)
                        if is_option {
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        wip = wip.end().map_err(DeserializeError::reflect)?;

                        found_dynamic = true;
                        break;
                    }

                    if found_dynamic {
                        continue;
                    }

                    // Skip _tag fields that have no matching is_tag() field - they should be silently ignored
                    // (Tag location hint is used by custom elements to capture the element name,
                    // but for regular elements it should just be dropped, not added to extra attributes)
                    if key.location == FieldLocationHint::Tag {
                        self.parser.skip_value().map_err(DeserializeError::Parser)?;
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

                    // Check if this unknown field should go to a flattened map
                    // flatten_map_idx is (outer_idx, Option<inner_idx>):
                    // - (outer_idx, None): direct flattened map at struct_def.fields[outer_idx]
                    // - (outer_idx, Some(inner_idx)): nested map inside a flattened struct
                    //
                    // For nested maps, we only insert if the value is a scalar. This is important
                    // for HTML/XML where child elements also appear as object keys, but should not
                    // be inserted into an attribute map (which expects String values).
                    let should_insert_into_map = if let Some((_, inner_idx_opt)) = flatten_map_idx {
                        if inner_idx_opt.is_some() {
                            // Nested case: only insert if value is a scalar
                            matches!(
                                self.parser.peek_event().ok().flatten(),
                                Some(ParseEvent::Scalar(_))
                            )
                        } else {
                            // Direct case: always insert
                            true
                        }
                    } else {
                        false
                    };

                    if should_insert_into_map {
                        let (outer_idx, inner_idx_opt) = flatten_map_idx.unwrap();
                        let outer_field = &struct_def.fields[outer_idx];
                        let outer_is_option = matches!(outer_field.shape().def, Def::Option(_));

                        // Navigate to the outer field first
                        if !fields_set[outer_idx] {
                            // First time - need to initialize
                            wip = wip
                                .begin_nth_field(outer_idx)
                                .map_err(DeserializeError::reflect)?;
                            if outer_is_option {
                                wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                            }

                            if let Some(inner_idx) = inner_idx_opt {
                                // Nested case: navigate to the inner map field
                                let inner_field = flatten_info[outer_idx]
                                    .as_ref()
                                    .map(|(fields, _)| &fields[inner_idx])
                                    .expect("inner field should exist");
                                let inner_is_option =
                                    matches!(inner_field.shape().def, Def::Option(_));

                                wip = wip
                                    .begin_nth_field(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                                if inner_is_option {
                                    wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                                }
                                // Initialize the map
                                wip = wip.begin_map().map_err(DeserializeError::reflect)?;
                            } else {
                                // Direct case: initialize the map
                                wip = wip.begin_map().map_err(DeserializeError::reflect)?;
                            }
                            fields_set[outer_idx] = true;
                        } else {
                            // Already initialized - navigate to it
                            wip = wip
                                .begin_nth_field(outer_idx)
                                .map_err(DeserializeError::reflect)?;
                            if outer_is_option {
                                wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                            }

                            if let Some(inner_idx) = inner_idx_opt {
                                // Nested case: navigate to the inner map field
                                let inner_field = flatten_info[outer_idx]
                                    .as_ref()
                                    .map(|(fields, _)| &fields[inner_idx])
                                    .expect("inner field should exist");
                                let inner_is_option =
                                    matches!(inner_field.shape().def, Def::Option(_));

                                wip = wip
                                    .begin_nth_field(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                                if inner_is_option {
                                    wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                                }
                                // In deferred mode, the map frame might not be stored/restored,
                                // so we always need to call begin_map() to re-enter it
                                wip = wip.begin_map().map_err(DeserializeError::reflect)?;
                            } else {
                                // Direct case: in deferred mode (e.g., within an array element),
                                // we need to re-enter the map even if already initialized
                                wip = wip.begin_map().map_err(DeserializeError::reflect)?;
                            }
                        }

                        // Insert the key-value pair into the map using begin_key/begin_value
                        // Clone the key to an owned String since we need it beyond the parse event lifetime
                        let key_owned: alloc::string::String = key.name.clone().into_owned();
                        // First: push key frame
                        wip = wip.begin_key().map_err(DeserializeError::reflect)?;
                        // Set the key (it's a string)
                        wip = wip.set(key_owned).map_err(DeserializeError::reflect)?;
                        // Pop key frame
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                        // Push value frame
                        wip = wip.begin_value().map_err(DeserializeError::reflect)?;
                        // Deserialize value
                        wip = self.deserialize_into(wip)?;
                        // Pop value frame
                        wip = wip.end().map_err(DeserializeError::reflect)?;

                        // Navigate back out
                        if let Some(inner_idx) = inner_idx_opt {
                            // Nested case: need to pop inner field frames too
                            let inner_field = flatten_info[outer_idx]
                                .as_ref()
                                .map(|(fields, _)| &fields[inner_idx])
                                .expect("inner field should exist");
                            let inner_is_option = matches!(inner_field.shape().def, Def::Option(_));

                            if inner_is_option {
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                            }
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }

                        if outer_is_option {
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        wip = wip.end().map_err(DeserializeError::reflect)?;

                        // Mark the nested map field as set so defaults won't overwrite it
                        if let Some(inner_idx) = inner_idx_opt
                            && let Some((_, inner_set)) = flatten_info[outer_idx].as_mut()
                        {
                            inner_set[inner_idx] = true;
                        }

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

        // Apply defaults for missing fields
        for (idx, field) in struct_def.fields.iter().enumerate() {
            if field.is_flattened() {
                // Handle DynamicValue flattens that received no fields
                if dynamic_value_flattens[idx] && !fields_set[idx] {
                    let is_option = matches!(field.shape().def, Def::Option(_));

                    if is_option {
                        // Option<DynamicValue> with no fields -> set to None
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = wip.set_default().map_err(DeserializeError::reflect)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else {
                        // DynamicValue with no fields -> initialize as empty object
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        // Initialize as object (for DynamicValue, begin_map creates an object)
                        wip = wip.begin_map().map_err(DeserializeError::reflect)?;
                        // The map is now initialized and empty, just end the field
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    }
                    continue;
                }

                // Handle flattened map that received no unknown keys
                // Only applies to direct flattened maps (outer_idx, None), not nested ones
                if flatten_map_idx == Some((idx, None)) && !fields_set[idx] {
                    let is_option = matches!(field.shape().def, Def::Option(_));
                    let field_has_default = field.has_default();
                    let field_type_has_default =
                        field.shape().is(facet_core::Characteristic::Default);

                    if is_option {
                        // Option<HashMap> with no fields -> set to None
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = wip.set_default().map_err(DeserializeError::reflect)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else if field_has_default || (struct_has_default && field_type_has_default) {
                        // Has default - use it
                        wip = wip
                            .set_nth_field_to_default(idx)
                            .map_err(DeserializeError::reflect)?;
                    } else {
                        // No default - initialize as empty map
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = wip.begin_map().map_err(DeserializeError::reflect)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    }
                    continue;
                }

                if let Some((inner_fields, inner_set)) = flatten_info[idx].as_ref() {
                    let any_inner_set = inner_set.iter().any(|&s| s);
                    let is_option = matches!(field.shape().def, Def::Option(_));

                    if any_inner_set {
                        // Some inner fields were set - apply defaults to missing ones
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        if is_option {
                            wip = wip.begin_some().map_err(DeserializeError::reflect)?;
                        }
                        for (inner_idx, inner_field) in inner_fields.iter().enumerate() {
                            if inner_set[inner_idx] {
                                continue;
                            }
                            let inner_has_default = inner_field.has_default();
                            let inner_type_has_default =
                                inner_field.shape().is(Characteristic::Default);
                            let inner_is_option = matches!(inner_field.shape().def, Def::Option(_));

                            if inner_has_default || inner_type_has_default {
                                wip = wip
                                    .set_nth_field_to_default(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                            } else if inner_is_option {
                                wip = wip
                                    .begin_nth_field(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                                wip = wip.set_default().map_err(DeserializeError::reflect)?;
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                            } else if inner_field.should_skip_deserializing() {
                                wip = wip
                                    .set_nth_field_to_default(inner_idx)
                                    .map_err(DeserializeError::reflect)?;
                            } else {
                                return Err(DeserializeError::TypeMismatch {
                                    expected: "field to be present or have default",
                                    got: format!("missing field '{}'", inner_field.name),
                                    span: self.last_span,
                                    path: None,
                                });
                            }
                        }
                        if is_option {
                            wip = wip.end().map_err(DeserializeError::reflect)?;
                        }
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else if is_option {
                        // No inner fields set and field is Option - set to None
                        wip = wip
                            .begin_nth_field(idx)
                            .map_err(DeserializeError::reflect)?;
                        wip = wip.set_default().map_err(DeserializeError::reflect)?;
                        wip = wip.end().map_err(DeserializeError::reflect)?;
                    } else {
                        // No inner fields set - try to default the whole flattened field
                        let field_has_default = field.has_default();
                        let field_type_has_default = field.shape().is(Characteristic::Default);
                        if field_has_default || (struct_has_default && field_type_has_default) {
                            wip = wip
                                .set_nth_field_to_default(idx)
                                .map_err(DeserializeError::reflect)?;
                        } else {
                            let all_inner_can_default = inner_fields.iter().all(|f| {
                                f.has_default()
                                    || f.shape().is(Characteristic::Default)
                                    || matches!(f.shape().def, Def::Option(_))
                                    || f.should_skip_deserializing()
                            });
                            if all_inner_can_default {
                                wip = wip
                                    .begin_nth_field(idx)
                                    .map_err(DeserializeError::reflect)?;
                                for (inner_idx, inner_field) in inner_fields.iter().enumerate() {
                                    let inner_has_default = inner_field.has_default();
                                    let inner_type_has_default =
                                        inner_field.shape().is(Characteristic::Default);
                                    let inner_is_option =
                                        matches!(inner_field.shape().def, Def::Option(_));

                                    if inner_has_default || inner_type_has_default {
                                        wip = wip
                                            .set_nth_field_to_default(inner_idx)
                                            .map_err(DeserializeError::reflect)?;
                                    } else if inner_is_option {
                                        wip = wip
                                            .begin_nth_field(inner_idx)
                                            .map_err(DeserializeError::reflect)?;
                                        wip =
                                            wip.set_default().map_err(DeserializeError::reflect)?;
                                        wip = wip.end().map_err(DeserializeError::reflect)?;
                                    } else if inner_field.should_skip_deserializing() {
                                        wip = wip
                                            .set_nth_field_to_default(inner_idx)
                                            .map_err(DeserializeError::reflect)?;
                                    }
                                }
                                wip = wip.end().map_err(DeserializeError::reflect)?;
                            } else {
                                return Err(DeserializeError::TypeMismatch {
                                    expected: "field to be present or have default",
                                    got: format!("missing flattened field '{}'", field.name),
                                    span: self.last_span,
                                    path: None,
                                });
                            }
                        }
                    }
                }
                continue;
            }

            if fields_set[idx] {
                continue;
            }

            let field_has_default = field.has_default();
            let field_type_has_default = field.shape().is(Characteristic::Default);
            let field_is_option = matches!(field.shape().def, Def::Option(_));

            if field_has_default || (struct_has_default && field_type_has_default) {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else if field_is_option {
                wip = wip
                    .begin_field(field.name)
                    .map_err(DeserializeError::reflect)?;
                wip = wip.set_default().map_err(DeserializeError::reflect)?;
                wip = wip.end().map_err(DeserializeError::reflect)?;
            } else if field.should_skip_deserializing() {
                wip = wip
                    .set_nth_field_to_default(idx)
                    .map_err(DeserializeError::reflect)?;
            } else {
                return Err(DeserializeError::TypeMismatch {
                    expected: "field to be present or have default",
                    got: format!("missing field '{}'", field.name),
                    span: self.last_span,
                    path: None,
                });
            }
        }

        // Finish deferred mode (only if we started it)
        if !already_deferred {
            wip = wip.finish_deferred().map_err(DeserializeError::reflect)?;
        }

        Ok(wip)
    }
}
