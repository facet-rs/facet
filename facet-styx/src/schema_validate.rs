//! Schema validation for Styx documents.
//!
//! Validates `styx_tree::Value` instances against `Schema` definitions.

use std::collections::HashSet;

use styx_tree::{Payload, Value};

/// Compute Levenshtein distance between two strings.
fn levenshtein(a: &str, b: &str) -> usize {
    let a_len = a.chars().count();
    let b_len = b.chars().count();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut prev_row: Vec<usize> = (0..=b_len).collect();
    let mut curr_row = vec![0; b_len + 1];

    for (i, a_char) in a.chars().enumerate() {
        curr_row[0] = i + 1;
        for (j, b_char) in b.chars().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            curr_row[j + 1] = (prev_row[j + 1] + 1)
                .min(curr_row[j] + 1)
                .min(prev_row[j] + cost);
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[b_len]
}

/// Find the most similar string from a list, if one is close enough.
fn suggest_similar<'a>(unknown: &str, valid: &'a [String]) -> Option<&'a str> {
    let unknown_lower = unknown.to_lowercase();
    valid
        .iter()
        .filter_map(|v| {
            let v_lower = v.to_lowercase();
            let dist = levenshtein(&unknown_lower, &v_lower);
            // Only suggest if edit distance is at most 2 and less than half the length
            if dist <= 2 && dist < unknown.len().max(1) {
                Some((v.as_str(), dist))
            } else {
                None
            }
        })
        .min_by_key(|(_, d)| *d)
        .map(|(v, _)| v)
}

use crate::schema_error::{
    ValidationError, ValidationErrorKind, ValidationResult, ValidationWarning,
    ValidationWarningKind,
};
use crate::schema_types::{
    DefaultSchema, DeprecatedSchema, Documented, EnumSchema, FlattenSchema, FloatConstraints,
    IntConstraints, MapSchema, ObjectKey, ObjectSchema, OneOfSchema, OptionalSchema, Schema,
    SchemaFile, SeqSchema, StringConstraints, UnionSchema,
};

/// Validator for Styx documents.
pub struct Validator<'a> {
    /// The schema file containing type definitions.
    schema_file: &'a SchemaFile,
}

impl<'a> Validator<'a> {
    /// Create a new validator with the given schema.
    pub fn new(schema_file: &'a SchemaFile) -> Self {
        Self { schema_file }
    }

    /// Validate a document against the schema's root type.
    pub fn validate_document(&self, doc: &Value) -> ValidationResult {
        // Look up the root schema (key None = unit/@)
        match self.schema_file.schema.get(&None) {
            Some(root_schema) => self.validate_value(doc, root_schema, ""),
            None => {
                let mut result = ValidationResult::ok();
                result.error(
                    ValidationError::new(
                        "",
                        ValidationErrorKind::SchemaError {
                            reason: "no root type (@) defined in schema".into(),
                        },
                        "schema has no root type definition",
                    )
                    .with_span(doc.span),
                );
                result
            }
        }
    }

    /// Validate a value against a specific named type.
    pub fn validate_as_type(&self, value: &Value, type_name: &str) -> ValidationResult {
        match self.schema_file.schema.get(&Some(type_name.to_string())) {
            Some(schema) => self.validate_value(value, schema, ""),
            None => {
                let mut result = ValidationResult::ok();
                result.error(
                    ValidationError::new(
                        "",
                        ValidationErrorKind::UnknownType {
                            name: type_name.into(),
                        },
                        format!("unknown type '{type_name}'"),
                    )
                    .with_span(value.span),
                );
                result
            }
        }
    }

    /// Validate a value against a schema.
    pub fn validate_value(&self, value: &Value, schema: &Schema, path: &str) -> ValidationResult {
        match schema {
            // Built-in scalar types
            Schema::String(constraints) => self.validate_string(value, constraints.as_ref(), path),
            Schema::Int(constraints) => self.validate_int(value, constraints.as_ref(), path),
            Schema::Float(constraints) => self.validate_float(value, constraints.as_ref(), path),
            Schema::Bool => self.validate_bool(value, path),
            Schema::Unit => self.validate_unit(value, path),
            Schema::Any => ValidationResult::ok(),

            // Structural types
            Schema::Object(obj_schema) => self.validate_object(value, obj_schema, path),
            Schema::Seq(seq_schema) => self.validate_seq(value, seq_schema, path),
            Schema::Map(map_schema) => self.validate_map(value, map_schema, path),

            // Combinators
            Schema::Union(union_schema) => self.validate_union(value, union_schema, path),
            Schema::Optional(opt_schema) => self.validate_optional(value, opt_schema, path),
            Schema::Enum(enum_schema) => self.validate_enum(value, enum_schema, path),
            Schema::OneOf(oneof_schema) => self.validate_one_of(value, oneof_schema, path),
            Schema::Flatten(flatten_schema) => self.validate_flatten(value, flatten_schema, path),

            // Wrappers
            Schema::Default(default_schema) => self.validate_default(value, default_schema, path),
            Schema::Deprecated(deprecated_schema) => {
                self.validate_deprecated(value, deprecated_schema, path)
            }

            // Other
            Schema::Literal(expected) => self.validate_literal(value, expected, path),
            Schema::Type { name } => self.validate_type_ref(value, name.as_deref(), path),
        }
    }

    // =========================================================================
    // Built-in scalar types
    // =========================================================================

    fn validate_string(
        &self,
        value: &Value,
        constraints: Option<&StringConstraints>,
        path: &str,
    ) -> ValidationResult {
        let mut result = ValidationResult::ok();

        let text = match value.scalar_text() {
            Some(t) => t,
            None => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedScalar,
                        format!("expected string, got {}", value_type_name(value)),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        // Apply constraints if present
        if let Some(c) = constraints {
            if let Some(min) = c.min_len
                && text.len() < min
            {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: format!("string length {} < minimum {}", text.len(), min),
                        },
                        format!("string too short (min length: {})", min),
                    )
                    .with_span(value.span),
                );
            }
            if let Some(max) = c.max_len
                && text.len() > max
            {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: format!("string length {} > maximum {}", text.len(), max),
                        },
                        format!("string too long (max length: {})", max),
                    )
                    .with_span(value.span),
                );
            }
            if let Some(pattern) = &c.pattern {
                // TODO: compile and match regex
                let _ = pattern; // Suppress unused warning for now
            }
        }

        result
    }

    fn validate_int(
        &self,
        value: &Value,
        constraints: Option<&IntConstraints>,
        path: &str,
    ) -> ValidationResult {
        let mut result = ValidationResult::ok();

        let text = match value.scalar_text() {
            Some(t) => t,
            None => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedScalar,
                        format!("expected integer, got {}", value_type_name(value)),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        let parsed = match text.parse::<i128>() {
            Ok(n) => n,
            Err(_) => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: "not a valid integer".into(),
                        },
                        format!("'{}' is not a valid integer", text),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        // Apply constraints
        if let Some(c) = constraints {
            if let Some(min) = c.min
                && parsed < min
            {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: format!("value {} < minimum {}", parsed, min),
                        },
                        format!("value too small (min: {})", min),
                    )
                    .with_span(value.span),
                );
            }
            if let Some(max) = c.max
                && parsed > max
            {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: format!("value {} > maximum {}", parsed, max),
                        },
                        format!("value too large (max: {})", max),
                    )
                    .with_span(value.span),
                );
            }
        }

        result
    }

    fn validate_float(
        &self,
        value: &Value,
        constraints: Option<&FloatConstraints>,
        path: &str,
    ) -> ValidationResult {
        let mut result = ValidationResult::ok();

        let text = match value.scalar_text() {
            Some(t) => t,
            None => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedScalar,
                        format!("expected number, got {}", value_type_name(value)),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        let parsed = match text.parse::<f64>() {
            Ok(n) => n,
            Err(_) => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: "not a valid number".into(),
                        },
                        format!("'{}' is not a valid number", text),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        // Apply constraints
        if let Some(c) = constraints {
            if let Some(min) = c.min
                && parsed < min
            {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: format!("value {} < minimum {}", parsed, min),
                        },
                        format!("value too small (min: {})", min),
                    )
                    .with_span(value.span),
                );
            }
            if let Some(max) = c.max
                && parsed > max
            {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: format!("value {} > maximum {}", parsed, max),
                        },
                        format!("value too large (max: {})", max),
                    )
                    .with_span(value.span),
                );
            }
        }

        result
    }

    fn validate_bool(&self, value: &Value, path: &str) -> ValidationResult {
        let mut result = ValidationResult::ok();

        match value.scalar_text() {
            Some(text) if text == "true" || text == "false" => {}
            Some(text) => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: "not a valid boolean".into(),
                        },
                        format!("'{}' is not a valid boolean (expected true/false)", text),
                    )
                    .with_span(value.span),
                );
            }
            None => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedScalar,
                        format!("expected boolean, got {}", value_type_name(value)),
                    )
                    .with_span(value.span),
                );
            }
        }

        result
    }

    fn validate_unit(&self, value: &Value, path: &str) -> ValidationResult {
        let mut result = ValidationResult::ok();

        if !value.is_unit() {
            result.error(
                ValidationError::new(
                    path,
                    ValidationErrorKind::TypeMismatch {
                        expected: "unit".into(),
                        got: value_type_name(value).into(),
                    },
                    "expected unit value",
                )
                .with_span(value.span),
            );
        }

        result
    }

    // =========================================================================
    // Structural types
    // =========================================================================

    fn validate_object(
        &self,
        value: &Value,
        schema: &ObjectSchema,
        path: &str,
    ) -> ValidationResult {
        let mut result = ValidationResult::ok();

        let obj = match value.as_object() {
            Some(o) => o,
            None => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedObject,
                        format!("expected object, got {}", value_type_name(value)),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        let mut seen_fields: HashSet<Option<&str>> = HashSet::new();
        // Look up catch-all schema - find any key that is a typed pattern or unit
        let additional_schema = schema.0.iter().find_map(|(k, v)| {
            if k.value.tag.is_some() {
                Some(v)
            } else {
                None
            }
        });

        for entry in &obj.entries {
            let key_opt: Option<&str> = if entry.key.is_unit() {
                None
            } else if let Some(s) = entry.key.as_str() {
                Some(s)
            } else {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: "object keys must be scalars or unit".into(),
                        },
                        "invalid object key",
                    )
                    .with_span(entry.key.span),
                );
                continue;
            };

            let key_display = key_opt.unwrap_or("@");
            let field_path = if path.is_empty() {
                key_display.to_string()
            } else {
                format!("{path}.{key_display}")
            };

            seen_fields.insert(key_opt);

            // Look up by Documented<ObjectKey> - for named fields
            let lookup_key = Documented::new(ObjectKey::named(key_opt.unwrap_or("")));
            if let Some(field_schema) = schema.0.get(&lookup_key) {
                result.merge(self.validate_value(&entry.value, field_schema, &field_path));
            } else if let Some(add_schema) = additional_schema {
                result.merge(self.validate_value(&entry.value, add_schema, &field_path));
            } else {
                // Collect valid field names for error message
                let valid_fields: Vec<String> = schema
                    .0
                    .keys()
                    .filter_map(|k| k.value.name().map(|s| s.to_string()))
                    .collect();

                // Try to find a similar field name (typo detection)
                let suggestion = suggest_similar(key_display, &valid_fields).map(String::from);

                result.error(
                    ValidationError::new(
                        &field_path,
                        ValidationErrorKind::UnknownField {
                            field: key_display.into(),
                            valid_fields,
                            suggestion,
                        },
                        format!("unknown field '{key_display}'"),
                    )
                    .with_span(entry.key.span),
                );
            }
        }

        // Check for missing required fields
        for (field_name_doc, field_schema) in &schema.0 {
            // Skip catch-all fields (typed patterns like @string)
            let Some(name) = field_name_doc.value.name() else {
                continue;
            };

            if !seen_fields.contains(&Some(name)) {
                // Optional and Default fields are not required
                if !matches!(field_schema, Schema::Optional(_) | Schema::Default(_)) {
                    let field_path = if path.is_empty() {
                        name.to_string()
                    } else {
                        format!("{path}.{name}")
                    };
                    result.error(
                        ValidationError::new(
                            &field_path,
                            ValidationErrorKind::MissingField {
                                field: name.to_string(),
                            },
                            format!("missing required field '{name}'"),
                        )
                        .with_span(value.span),
                    );
                }
            }
        }

        result
    }

    fn validate_seq(&self, value: &Value, schema: &SeqSchema, path: &str) -> ValidationResult {
        let mut result = ValidationResult::ok();

        let seq = match value.as_sequence() {
            Some(s) => s,
            None => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedSequence,
                        format!("expected sequence, got {}", value_type_name(value)),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        // Validate each element against the inner schema
        let inner_schema = &*schema.0.0.value;
        for (i, item) in seq.items.iter().enumerate() {
            let item_path = format!("{path}[{i}]");
            result.merge(self.validate_value(item, inner_schema, &item_path));
        }

        result
    }

    fn validate_map(&self, value: &Value, schema: &MapSchema, path: &str) -> ValidationResult {
        let mut result = ValidationResult::ok();

        let obj = match value.as_object() {
            Some(o) => o,
            None => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedObject,
                        format!("expected map (object), got {}", value_type_name(value)),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        // @map(@V) has 1 element, @map(@K @V) has 2
        let (key_schema, value_schema) = match schema.0.len() {
            1 => (None, &schema.0[0].value),
            2 => (Some(&schema.0[0].value), &schema.0[1].value),
            n => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::SchemaError {
                            reason: format!("map schema must have 1 or 2 types, got {}", n),
                        },
                        "invalid map schema",
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        for entry in &obj.entries {
            let key_str = match entry.key.as_str() {
                Some(s) => s,
                None => {
                    result.error(
                        ValidationError::new(
                            path,
                            ValidationErrorKind::InvalidValue {
                                reason: "map keys must be scalars".into(),
                            },
                            "invalid map key",
                        )
                        .with_span(entry.key.span),
                    );
                    continue;
                }
            };

            // Validate key if schema provided
            if let Some(ks) = key_schema {
                result.merge(self.validate_value(&entry.key, ks, path));
            }

            // Validate value
            let entry_path = if path.is_empty() {
                key_str.to_string()
            } else {
                format!("{path}.{key_str}")
            };
            result.merge(self.validate_value(&entry.value, value_schema, &entry_path));
        }

        result
    }

    // =========================================================================
    // Combinators
    // =========================================================================

    fn validate_union(&self, value: &Value, schema: &UnionSchema, path: &str) -> ValidationResult {
        let mut result = ValidationResult::ok();

        if schema.0.is_empty() {
            result.error(
                ValidationError::new(
                    path,
                    ValidationErrorKind::SchemaError {
                        reason: "union must have at least one variant".into(),
                    },
                    "invalid union schema: no variants",
                )
                .with_span(value.span),
            );
            return result;
        }

        let mut tried = Vec::new();
        for variant in &schema.0 {
            let variant_result = self.validate_value(value, &variant.value, path);
            if variant_result.is_valid() {
                return ValidationResult::ok();
            }
            tried.push(schema_type_name(&variant.value));
        }

        result.error(
            ValidationError::new(
                path,
                ValidationErrorKind::UnionMismatch { tried },
                format!(
                    "value doesn't match any union variant (tried: {})",
                    schema
                        .0
                        .iter()
                        .map(|d| schema_type_name(&d.value))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
            .with_span(value.span),
        );

        result
    }

    fn validate_optional(
        &self,
        value: &Value,
        schema: &OptionalSchema,
        path: &str,
    ) -> ValidationResult {
        // Unit value represents None - always valid for Optional
        if value.is_unit() {
            return ValidationResult::ok();
        }
        // Otherwise validate the inner type
        self.validate_value(value, &schema.0.0.value, path)
    }

    fn validate_enum(&self, value: &Value, schema: &EnumSchema, path: &str) -> ValidationResult {
        let mut result = ValidationResult::ok();

        // An enum value must have a tag, OR match a fallback variant
        let tag = match &value.tag {
            Some(t) => t.name.as_str(),
            None => {
                // No tag - try to find a fallback variant that accepts this value type
                if let Some(fallback_schema) = self.find_enum_fallback(value, schema) {
                    // Validate against the fallback variant's schema
                    return self.validate_value(value, fallback_schema, path);
                }
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedTagged,
                        format!(
                            "expected tagged value for enum, got {}",
                            value_type_name(value)
                        ),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        // Extract payload as a Value for recursive validation
        let payload_value = value.payload.as_ref().map(|p| Value {
            tag: None,
            payload: Some(p.clone()),
            span: None,
        });

        let expected_variants: Vec<String> = schema.0.keys().map(|k| k.value.clone()).collect();

        match schema.0.get(&Documented::new(tag.to_string())) {
            Some(variant_schema) => {
                match (&payload_value, variant_schema) {
                    (None, Schema::Unit) => {
                        // @variant with unit schema - OK
                    }
                    (None, Schema::Type { name: Some(n) }) if n == "unit" => {
                        // @variant with @unit type ref - OK
                    }
                    (None, Schema::Type { name: None }) => {
                        // @variant with @ schema - OK (unit)
                    }
                    (Some(p), _) => {
                        let variant_path = if path.is_empty() {
                            tag.to_string()
                        } else {
                            format!("{path}.{tag}")
                        };
                        result.merge(self.validate_value(p, variant_schema, &variant_path));
                    }
                    (None, _) => {
                        result.error(
                            ValidationError::new(
                                path,
                                ValidationErrorKind::TypeMismatch {
                                    expected: schema_type_name(variant_schema),
                                    got: "unit".into(),
                                },
                                format!("variant '{tag}' requires a payload"),
                            )
                            .with_span(value.span),
                        );
                    }
                }
            }
            None => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidVariant {
                            expected: expected_variants.clone(),
                            got: tag.into(),
                        },
                        format!(
                            "unknown enum variant '{tag}' (expected one of: {})",
                            expected_variants.join(", ")
                        ),
                    )
                    .with_span(value.span),
                );
            }
        }

        result
    }

    /// Find a fallback variant in an enum that can accept an untagged value.
    ///
    /// For example, if the enum has `eq @string` variant and the value is a bare string,
    /// this returns the `@string` schema so the value can be validated against it.
    fn find_enum_fallback<'s>(&self, value: &Value, schema: &'s EnumSchema) -> Option<&'s Schema> {
        // Only scalars can fall back
        let text = value.scalar_text()?;

        // Look for a variant whose schema matches this value
        for (_variant_name, variant_schema) in &schema.0 {
            match variant_schema {
                // @string accepts any scalar
                Schema::String(_) => return Some(variant_schema),
                // @int accepts scalars that parse as integers
                Schema::Int(_) if text.parse::<i64>().is_ok() => return Some(variant_schema),
                // @float accepts scalars that parse as floats
                Schema::Float(_) if text.parse::<f64>().is_ok() => return Some(variant_schema),
                // @bool accepts "true" or "false"
                Schema::Bool if text == "true" || text == "false" => return Some(variant_schema),
                _ => continue,
            }
        }

        None
    }

    fn validate_one_of(
        &self,
        value: &Value,
        schema: &OneOfSchema,
        path: &str,
    ) -> ValidationResult {
        let mut result = ValidationResult::ok();

        // First validate against the base type
        let base_type = &schema.0.0.value;
        let base_result = self.validate_value(value, base_type, path);
        if !base_result.is_valid() {
            return base_result;
        }

        // Then check if the value matches one of the allowed values
        let allowed_values = &schema.0.1;
        if allowed_values.is_empty() {
            // No values specified means any value of the base type is allowed
            return result;
        }

        // Get the string representation of the value for comparison
        let value_text = match value.scalar_text() {
            Some(t) => t,
            None => {
                // Non-scalar values can't be compared to allowed values
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedScalar,
                        format!(
                            "expected scalar value for one-of constraint, got {}",
                            value_type_name(value)
                        ),
                    )
                    .with_span(value.span),
                );
                return result;
            }
        };

        // Check if the value is in the allowed list
        let allowed_strings: Vec<&str> = allowed_values.iter().map(|v| v.as_str()).collect();
        if !allowed_strings.contains(&value_text) {
            // Try to find a similar value for suggestions
            let allowed_owned: Vec<String> =
                allowed_values.iter().map(|v| v.0.clone()).collect();
            let suggestion = suggest_similar(value_text, &allowed_owned).map(String::from);

            result.error(
                ValidationError::new(
                    path,
                    ValidationErrorKind::InvalidValue {
                        reason: format!(
                            "value '{}' not in allowed set: {}",
                            value_text,
                            allowed_strings.join(", ")
                        ),
                    },
                    format!(
                        "'{}' is not one of: {}{}",
                        value_text,
                        allowed_strings.join(", "),
                        suggestion
                            .map(|s| format!(" (did you mean '{}'?)", s))
                            .unwrap_or_default()
                    ),
                )
                .with_span(value.span),
            );
        }

        result
    }

    fn validate_flatten(
        &self,
        value: &Value,
        schema: &FlattenSchema,
        path: &str,
    ) -> ValidationResult {
        // Flatten just validates against the inner type
        self.validate_value(value, &schema.0.0.value, path)
    }

    // =========================================================================
    // Wrappers
    // =========================================================================

    fn validate_default(
        &self,
        value: &Value,
        schema: &DefaultSchema,
        path: &str,
    ) -> ValidationResult {
        // Default just validates against the inner type
        // (the default value is used at deserialization time, not validation time)
        self.validate_value(value, &schema.0.1.value, path)
    }

    fn validate_deprecated(
        &self,
        value: &Value,
        schema: &DeprecatedSchema,
        path: &str,
    ) -> ValidationResult {
        let (reason, inner) = &schema.0;
        let mut result = self.validate_value(value, &inner.value, path);

        // Add deprecation warning
        result.warning(
            ValidationWarning::new(
                path,
                ValidationWarningKind::Deprecated {
                    reason: reason.clone(),
                },
                format!("deprecated: {}", reason),
            )
            .with_span(value.span),
        );

        result
    }

    // =========================================================================
    // Other
    // =========================================================================

    fn validate_literal(&self, value: &Value, expected: &str, path: &str) -> ValidationResult {
        let mut result = ValidationResult::ok();

        match value.scalar_text() {
            Some(text) if text == expected => {}
            Some(text) => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::InvalidValue {
                            reason: format!("expected literal '{expected}', got '{}'", text),
                        },
                        format!("expected '{expected}', got '{}'", text),
                    )
                    .with_span(value.span),
                );
            }
            None => {
                result.error(
                    ValidationError::new(
                        path,
                        ValidationErrorKind::ExpectedScalar,
                        format!("expected literal '{expected}', got non-scalar"),
                    )
                    .with_span(value.span),
                );
            }
        }

        result
    }

    fn validate_type_ref(
        &self,
        value: &Value,
        type_name: Option<&str>,
        path: &str,
    ) -> ValidationResult {
        let mut result = ValidationResult::ok();

        match type_name {
            None => {
                // Unit type reference (@)
                if !value.is_unit() {
                    result.error(
                        ValidationError::new(
                            path,
                            ValidationErrorKind::TypeMismatch {
                                expected: "unit".into(),
                                got: value_type_name(value).into(),
                            },
                            "expected unit value",
                        )
                        .with_span(value.span),
                    );
                }
            }
            Some(name) => {
                // Named type reference - look up in schema
                if let Some(type_schema) = self.schema_file.schema.get(&Some(name.to_string())) {
                    result.merge(self.validate_value(value, type_schema, path));
                } else {
                    result.error(
                        ValidationError::new(
                            path,
                            ValidationErrorKind::UnknownType { name: name.into() },
                            format!("unknown type '{name}'"),
                        )
                        .with_span(value.span),
                    );
                }
            }
        }

        result
    }
}

/// Get a human-readable name for a value type.
fn value_type_name(value: &Value) -> &'static str {
    if value.is_unit() {
        return "unit";
    }
    if value.tag.is_some() {
        return "tagged";
    }
    match &value.payload {
        None => "unit",
        Some(Payload::Scalar(_)) => "scalar",
        Some(Payload::Sequence(_)) => "sequence",
        Some(Payload::Object(_)) => "object",
    }
}

/// Get a human-readable name for a schema type.
fn schema_type_name(schema: &Schema) -> String {
    match schema {
        Schema::String(_) => "string".into(),
        Schema::Int(_) => "int".into(),
        Schema::Float(_) => "float".into(),
        Schema::Bool => "bool".into(),
        Schema::Unit => "unit".into(),
        Schema::Any => "any".into(),
        Schema::Object(_) => "object".into(),
        Schema::Seq(_) => "seq".into(),
        Schema::Map(_) => "map".into(),
        Schema::Union(_) => "union".into(),
        Schema::Optional(_) => "optional".into(),
        Schema::Enum(_) => "enum".into(),
        Schema::OneOf(_) => "one-of".into(),
        Schema::Flatten(_) => "flatten".into(),
        Schema::Default(_) => "default".into(),
        Schema::Deprecated(_) => "deprecated".into(),
        Schema::Literal(s) => format!("literal({s})"),
        Schema::Type { name: None } => "unit".into(),
        Schema::Type { name: Some(n) } => n.clone(),
    }
}

/// Convenience function to validate a document against a schema.
pub fn validate(doc: &Value, schema: &SchemaFile) -> ValidationResult {
    let validator = Validator::new(schema);
    validator.validate_document(doc)
}

/// Convenience function to validate a value against a named type.
pub fn validate_as(value: &Value, schema: &SchemaFile, type_name: &str) -> ValidationResult {
    let validator = Validator::new(schema);
    validator.validate_as_type(value, type_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typed_catchall_validation() {
        // Schema with @string catch-all (like HashMap<String, i32>)
        let schema_source = r#"meta {id test}
schema {
    @ @object{
        @string @int
    }
}"#;
        let schema: SchemaFile = crate::from_str(schema_source).expect("should parse schema");

        // Check the parsed schema structure
        let root = schema.schema.get(&None).expect("should have root");
        if let Schema::Object(obj) = root {
            tracing::debug!("Object schema has {} entries", obj.0.len());
            for (key, value) in &obj.0 {
                tracing::debug!(
                    "Key: value={:?}, tag={:?} -> {:?}",
                    key.value.value,
                    key.value.tag,
                    value
                );
            }
            // Check if catch-all is found
            let catchall = obj.0.iter().find(|(k, _)| k.value.tag.is_some());
            assert!(
                catchall.is_some(),
                "Schema should have a typed catch-all entry. Keys: {:?}",
                obj.0.keys().map(|k| (&k.value.value, &k.value.tag)).collect::<Vec<_>>()
            );
        } else {
            panic!("Root should be an object schema, got {:?}", root);
        }

        // Document with arbitrary keys
        let doc_source = r#"foo 42
bar 123"#;
        let doc = styx_tree::parse(doc_source).expect("should parse doc");

        let result = validate(&doc, &schema);
        assert!(
            result.is_valid(),
            "Document should validate. Errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_enum_with_fallback_variant() {
        // Schema: enum with explicit variants + fallback `eq @string`
        let schema_source = r#"meta {id test}
schema {
    @ @object{
        filter @enum{
            gt @string
            lt @string
            eq @string
        }
    }
}"#;
        let schema: SchemaFile = crate::from_str(schema_source).expect("should parse schema");

        // Document: bare string should match `eq @string` fallback
        let doc = styx_tree::parse(r#"filter "published""#).expect("should parse doc");

        let result = validate(&doc, &schema);
        assert!(
            result.is_valid(),
            "Bare string should fall back to eq variant. Errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_optional_accepts_unit() {
        // Schema: field is @optional(@string)
        let schema_source = r#"meta {id test}
schema {
    @ @object{
        name @optional(@string)
    }
}"#;
        let schema: SchemaFile = crate::from_str(schema_source).expect("should parse schema");

        // Document: field has unit value (represents None)
        let doc = styx_tree::parse("name").expect("should parse doc");

        let result = validate(&doc, &schema);
        assert!(
            result.is_valid(),
            "Unit value should be valid for @optional. Errors: {:?}",
            result.errors
        );
    }
}
