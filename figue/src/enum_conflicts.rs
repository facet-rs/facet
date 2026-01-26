//! Detection of conflicting enum variant initialization.
//!
//! This module detects when a user tries to initialize multiple variants of the same
//! enum simultaneously (e.g., setting both `storage.s3.bucket` and `storage.gcp.project`
//! when `storage` is an enum with `S3` and `Gcp` variants).
//!
//! # Example error
//!
//! ```text
//! Error: Conflicting enum variants for `config.storage`
//!
//! Multiple variants are being initialized:
//!   - S3 (from MYAPP__STORAGE__S3__BUCKET)
//!   - Gcp (from MYAPP__STORAGE__GCP__PROJECT)
//!
//! An enum can only have one active variant.
//! ```

use std::collections::HashMap;
use std::vec::Vec;

use crate::config_value::ConfigValue;
use crate::path::Path;
use crate::provenance::Provenance;
use crate::schema::{ConfigEnumSchema, ConfigStructSchema, ConfigValueSchema, Schema};

/// A detected enum variant conflict.
#[derive(Debug)]
pub struct EnumConflict {
    /// Path to the enum field (e.g., ["config", "storage"]).
    pub path: Path,
    /// Variants that are being initialized, with their sources.
    pub variants: Vec<VariantSource>,
}

/// A variant being initialized with its provenance.
#[derive(Debug)]
pub struct VariantSource {
    /// The variant name (e.g., "S3", "Gcp").
    pub variant_name: String,
    /// Paths of fields being set for this variant.
    pub field_paths: Vec<Path>,
    /// Provenance for one of the fields (for display).
    pub provenance: Option<Provenance>,
}

impl EnumConflict {
    /// Format this conflict as a user-friendly error message.
    pub fn format(&self) -> String {
        let path_str = self.path.join(".");
        let mut msg = format!("Conflicting enum variants for `{path_str}`\n\n");
        msg.push_str("Multiple variants are being initialized:\n");

        for vs in &self.variants {
            let source = vs
                .provenance
                .as_ref()
                .map(|p| format!(" (from {})", p.source_description()))
                .unwrap_or_default();
            msg.push_str(&format!("  - {}{}\n", vs.variant_name, source));
        }

        msg.push_str("\nAn enum can only have one active variant.");
        msg
    }
}

/// Check for enum variant conflicts in a merged ConfigValue.
///
/// Returns a list of all detected conflicts.
pub fn detect_enum_conflicts(value: &ConfigValue, schema: &Schema) -> Vec<EnumConflict> {
    let mut conflicts = Vec::new();

    // Check config struct if present
    if let Some(config_schema) = schema.config()
        && let Some(field_name) = config_schema.field_name()
        && let ConfigValue::Object(sourced) = value
        && let Some(config_value) = sourced.value.get(field_name)
    {
        let mut path = vec![field_name.to_string()];
        check_struct_for_conflicts(config_value, config_schema, &mut path, &mut conflicts);
    }

    conflicts
}

/// Recursively check a struct for enum conflicts.
fn check_struct_for_conflicts(
    value: &ConfigValue,
    schema: &ConfigStructSchema,
    path: &mut Path,
    conflicts: &mut Vec<EnumConflict>,
) {
    let ConfigValue::Object(sourced) = value else {
        return;
    };

    for (field_name, field_schema) in schema.fields() {
        path.push(field_name.clone());

        if let Some(field_value) = sourced.value.get(field_name) {
            check_value_for_conflicts(field_value, field_schema.value(), path, conflicts);
        }

        path.pop();
    }
}

/// Check a value against its schema for enum conflicts.
fn check_value_for_conflicts(
    value: &ConfigValue,
    schema: &ConfigValueSchema,
    path: &mut Path,
    conflicts: &mut Vec<EnumConflict>,
) {
    match schema {
        ConfigValueSchema::Struct(struct_schema) => {
            check_struct_for_conflicts(value, struct_schema, path, conflicts);
        }
        ConfigValueSchema::Option { value: inner, .. } => {
            // Unwrap option and check inner
            check_value_for_conflicts(value, inner, path, conflicts);
        }
        ConfigValueSchema::Vec(vec_schema) => {
            // Check each element
            if let ConfigValue::Array(sourced) = value {
                for (i, elem) in sourced.value.iter().enumerate() {
                    path.push(i.to_string());
                    check_value_for_conflicts(elem, vec_schema.element(), path, conflicts);
                    path.pop();
                }
            }
        }
        ConfigValueSchema::Enum(enum_schema) => {
            // This is where the magic happens - check for conflicting variants
            if let Some(conflict) = check_enum_for_conflict(value, enum_schema, path) {
                conflicts.push(conflict);
            }
        }
        ConfigValueSchema::Leaf(_) => {
            // Leaf values can't have conflicts
        }
    }
}

/// Check an enum value for conflicting variants.
///
/// If the value is an object with keys matching multiple variants, that's a conflict.
fn check_enum_for_conflict(
    value: &ConfigValue,
    enum_schema: &ConfigEnumSchema,
    path: &Path,
) -> Option<EnumConflict> {
    // If it's already an Enum value, no conflict (variant was explicitly selected)
    if let ConfigValue::Enum(_) = value {
        return None;
    }

    // For objects, check if keys match multiple variant names
    let ConfigValue::Object(sourced) = value else {
        return None;
    };

    // Collect which variants have keys set
    let mut variants_with_data: HashMap<String, VariantSource> = HashMap::new();

    for (key, key_value) in &sourced.value {
        // Check if this key is a variant name
        if enum_schema.get_variant(key).is_some() {
            // This key is a variant name - check if it has data
            if let ConfigValue::Object(variant_obj) = key_value {
                if !variant_obj.value.is_empty() {
                    let provenance = get_first_provenance(key_value);
                    let field_paths = collect_field_paths(key_value, &mut path.clone(), key);

                    variants_with_data.insert(
                        key.clone(),
                        VariantSource {
                            variant_name: key.clone(),
                            field_paths,
                            provenance,
                        },
                    );
                }
            } else {
                // Non-empty non-object value for a variant
                let provenance = get_provenance(key_value);
                variants_with_data.insert(
                    key.clone(),
                    VariantSource {
                        variant_name: key.clone(),
                        field_paths: vec![{
                            let mut p = path.clone();
                            p.push(key.clone());
                            p
                        }],
                        provenance,
                    },
                );
            }
        } else {
            // This key might be a field within a variant
            // Check all variants to see if this field belongs to them
            for (variant_name, variant_schema) in enum_schema.variants() {
                if variant_schema.fields().contains_key(key) {
                    let provenance = get_provenance(key_value);
                    let entry = variants_with_data
                        .entry(variant_name.clone())
                        .or_insert_with(|| VariantSource {
                            variant_name: variant_name.clone(),
                            field_paths: Vec::new(),
                            provenance: None,
                        });
                    entry.field_paths.push({
                        let mut p = path.clone();
                        p.push(key.clone());
                        p
                    });
                    if entry.provenance.is_none() {
                        entry.provenance = provenance;
                    }
                }
            }
        }
    }

    // If more than one variant has data, that's a conflict
    if variants_with_data.len() > 1 {
        Some(EnumConflict {
            path: path.clone(),
            variants: variants_with_data.into_values().collect(),
        })
    } else {
        None
    }
}

/// Get provenance from a ConfigValue.
fn get_provenance(value: &ConfigValue) -> Option<Provenance> {
    match value {
        ConfigValue::Null(s) => s.provenance.clone(),
        ConfigValue::Bool(s) => s.provenance.clone(),
        ConfigValue::Integer(s) => s.provenance.clone(),
        ConfigValue::Float(s) => s.provenance.clone(),
        ConfigValue::String(s) => s.provenance.clone(),
        ConfigValue::Array(s) => s.provenance.clone(),
        ConfigValue::Object(s) => s.provenance.clone(),
        ConfigValue::Enum(s) => s.provenance.clone(),
    }
}

/// Get the first provenance from a nested value.
fn get_first_provenance(value: &ConfigValue) -> Option<Provenance> {
    if let Some(prov) = get_provenance(value) {
        return Some(prov);
    }

    match value {
        ConfigValue::Object(sourced) => {
            for v in sourced.value.values() {
                if let Some(prov) = get_first_provenance(v) {
                    return Some(prov);
                }
            }
            None
        }
        ConfigValue::Array(sourced) => {
            for v in &sourced.value {
                if let Some(prov) = get_first_provenance(v) {
                    return Some(prov);
                }
            }
            None
        }
        _ => None,
    }
}

/// Collect all field paths under a value.
fn collect_field_paths(value: &ConfigValue, base_path: &mut Path, key: &str) -> Vec<Path> {
    let mut paths = Vec::new();
    base_path.push(key.to_string());

    match value {
        ConfigValue::Object(sourced) => {
            if sourced.value.is_empty() {
                paths.push(base_path.clone());
            } else {
                for (k, v) in &sourced.value {
                    paths.extend(collect_field_paths(v, base_path, k));
                }
            }
        }
        _ => {
            paths.push(base_path.clone());
        }
    }

    base_path.pop();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as figue;
    use crate::config_value::Sourced;
    use facet::Facet;
    use indexmap::IndexMap;

    // ========================================================================
    // Test schemas
    // ========================================================================

    #[derive(Facet)]
    #[facet(rename_all = "kebab-case")]
    #[repr(u8)]
    #[allow(dead_code)]
    enum Storage {
        S3 { bucket: String, region: String },
        Gcp { project: String, zone: String },
        Local { path: String },
    }

    #[derive(Facet)]
    struct ConfigWithEnum {
        storage: Storage,
        port: u16,
    }

    #[derive(Facet)]
    struct ArgsWithEnumConfig {
        #[facet(figue::config)]
        config: ConfigWithEnum,
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    fn make_object(entries: Vec<(&str, ConfigValue)>) -> ConfigValue {
        let map: IndexMap<String, ConfigValue, std::hash::RandomState> = entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ConfigValue::Object(Sourced::new(map))
    }

    fn make_string(value: &str, prov: Option<Provenance>) -> ConfigValue {
        ConfigValue::String(Sourced {
            value: value.to_string(),
            span: None,
            provenance: prov,
        })
    }

    // ========================================================================
    // Tests
    // ========================================================================

    #[test]
    fn test_no_conflict_single_variant() {
        // Only s3 variant is set - no conflict
        let value = make_object(vec![(
            "config",
            make_object(vec![
                (
                    "s3",
                    make_object(vec![
                        ("bucket", make_string("my-bucket", None)),
                        ("region", make_string("us-east-1", None)),
                    ]),
                ),
                ("port", ConfigValue::Integer(Sourced::new(8080))),
            ]),
        )]);

        let schema = Schema::from_shape(ArgsWithEnumConfig::SHAPE).unwrap();
        let conflicts = detect_enum_conflicts(&value, &schema);

        assert!(
            conflicts.is_empty(),
            "should have no conflicts: {conflicts:?}"
        );
    }

    #[test]
    fn test_conflict_two_variants() {
        // Both s3 and gcp variants are set - conflict!
        let value = make_object(vec![(
            "config",
            make_object(vec![
                (
                    "storage",
                    make_object(vec![
                        (
                            "s3",
                            make_object(vec![("bucket", make_string("my-bucket", None))]),
                        ),
                        (
                            "gcp",
                            make_object(vec![("project", make_string("my-project", None))]),
                        ),
                    ]),
                ),
                ("port", ConfigValue::Integer(Sourced::new(8080))),
            ]),
        )]);

        let schema = Schema::from_shape(ArgsWithEnumConfig::SHAPE).unwrap();
        let conflicts = detect_enum_conflicts(&value, &schema);

        assert_eq!(conflicts.len(), 1, "should have one conflict");
        let conflict = &conflicts[0];
        assert_eq!(conflict.path, vec!["config", "storage"]);
        assert_eq!(conflict.variants.len(), 2);

        // Check format includes both variants
        let msg = conflict.format();
        assert!(msg.contains("s3"), "error should mention s3: {msg}");
        assert!(msg.contains("gcp"), "error should mention gcp: {msg}");
    }

    #[test]
    fn test_conflict_with_provenance() {
        // Conflict with provenance tracking
        let env_prov = Provenance::env("MYAPP__STORAGE__S3__BUCKET", "my-bucket");
        let cli_prov = Provenance::cli("--config.storage.gcp.project", "my-project");

        let value = make_object(vec![(
            "config",
            make_object(vec![(
                "storage",
                make_object(vec![
                    (
                        "s3",
                        make_object(vec![("bucket", make_string("my-bucket", Some(env_prov)))]),
                    ),
                    (
                        "gcp",
                        make_object(vec![("project", make_string("my-project", Some(cli_prov)))]),
                    ),
                ]),
            )]),
        )]);

        let schema = Schema::from_shape(ArgsWithEnumConfig::SHAPE).unwrap();
        let conflicts = detect_enum_conflicts(&value, &schema);

        assert_eq!(conflicts.len(), 1);
        let msg = conflicts[0].format();

        // Should mention provenance sources
        assert!(
            msg.contains("MYAPP__STORAGE__S3__BUCKET")
                || msg.contains("--config.storage.gcp.project"),
            "error should mention provenance: {msg}"
        );
    }

    #[test]
    fn test_conflict_three_variants() {
        // All three variants are set - conflict!
        let value = make_object(vec![(
            "config",
            make_object(vec![(
                "storage",
                make_object(vec![
                    (
                        "s3",
                        make_object(vec![("bucket", make_string("my-bucket", None))]),
                    ),
                    (
                        "gcp",
                        make_object(vec![("project", make_string("my-project", None))]),
                    ),
                    (
                        "local",
                        make_object(vec![("path", make_string("/data", None))]),
                    ),
                ]),
            )]),
        )]);

        let schema = Schema::from_shape(ArgsWithEnumConfig::SHAPE).unwrap();
        let conflicts = detect_enum_conflicts(&value, &schema);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].variants.len(), 3);
    }

    #[derive(Facet)]
    struct ConfigWithOptionalEnum {
        storage: Option<Storage>,
    }

    #[derive(Facet)]
    struct ArgsWithOptionalEnumConfig {
        #[facet(figue::config)]
        config: ConfigWithOptionalEnum,
    }

    #[test]
    fn test_conflict_optional_enum() {
        // Optional enum with conflict
        let value = make_object(vec![(
            "config",
            make_object(vec![(
                "storage",
                make_object(vec![
                    (
                        "s3",
                        make_object(vec![("bucket", make_string("my-bucket", None))]),
                    ),
                    (
                        "gcp",
                        make_object(vec![("project", make_string("my-project", None))]),
                    ),
                ]),
            )]),
        )]);

        let schema = Schema::from_shape(ArgsWithOptionalEnumConfig::SHAPE).unwrap();
        let conflicts = detect_enum_conflicts(&value, &schema);

        assert_eq!(
            conflicts.len(),
            1,
            "optional enum should also detect conflicts"
        );
    }

    #[derive(Facet)]
    struct NestedConfig {
        inner: InnerConfig,
    }

    #[derive(Facet)]
    struct InnerConfig {
        storage: Storage,
    }

    #[derive(Facet)]
    struct ArgsWithNestedEnumConfig {
        #[facet(figue::config)]
        config: NestedConfig,
    }

    #[test]
    fn test_conflict_nested_enum() {
        // Nested enum with conflict
        let value = make_object(vec![(
            "config",
            make_object(vec![(
                "inner",
                make_object(vec![(
                    "storage",
                    make_object(vec![
                        (
                            "s3",
                            make_object(vec![("bucket", make_string("my-bucket", None))]),
                        ),
                        (
                            "gcp",
                            make_object(vec![("project", make_string("my-project", None))]),
                        ),
                    ]),
                )]),
            )]),
        )]);

        let schema = Schema::from_shape(ArgsWithNestedEnumConfig::SHAPE).unwrap();
        let conflicts = detect_enum_conflicts(&value, &schema);

        assert_eq!(conflicts.len(), 1);
        assert_eq!(
            conflicts[0].path,
            vec!["config", "inner", "storage"],
            "should report correct nested path"
        );
    }
}
