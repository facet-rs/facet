//! Deep-merge functionality for layered configuration.
//!
//! This module provides the ability to merge multiple [`ConfigValue`] trees
//! together, with later values taking precedence over earlier ones.
//!
//! # Merge Strategy
//!
//! - **Objects**: Merged recursively - keys from both sides are combined,
//!   with the "upper" (higher priority) value winning for duplicate keys.
//! - **Arrays**: Not merged - upper value replaces lower entirely.
//! - **Scalars**: Upper value replaces lower entirely.
//!
//! # Provenance Tracking
//!
//! When a value from a higher-priority layer overrides a lower-priority one,
//! the override is recorded in the returned [`MergeResult`].

use alloc::vec::Vec;

use indexmap::IndexMap;

use crate::config_value::{ConfigValue, Sourced};
use crate::provenance::{Override, Provenance};

/// Result of merging multiple configuration layers.
#[derive(Debug)]
pub struct MergeResult {
    /// The merged configuration value.
    pub value: ConfigValue,
    /// Records of values that were overridden during the merge.
    pub overrides: Vec<Override>,
}

impl MergeResult {
    /// Create a new merge result with no overrides.
    pub fn new(value: ConfigValue) -> Self {
        Self {
            value,
            overrides: Vec::new(),
        }
    }
}

/// Merge two ConfigValue trees, with `upper` taking precedence over `lower`.
///
/// This performs a deep merge for objects - keys from both trees are combined,
/// with `upper` winning when both have the same key.
///
/// For arrays and scalars, `upper` replaces `lower` entirely.
///
/// # Arguments
///
/// * `lower` - The lower-priority value (e.g., from config file)
/// * `upper` - The higher-priority value (e.g., from env vars)
/// * `path` - The current path in the tree (for override tracking)
///
/// # Returns
///
/// A [`MergeResult`] containing the merged value and any override records.
pub fn merge(lower: ConfigValue, upper: ConfigValue, path: &str) -> MergeResult {
    let mut overrides = Vec::new();
    let value = merge_inner(lower, upper, path, &mut overrides);
    MergeResult { value, overrides }
}

/// Merge multiple ConfigValue layers in order (first = lowest priority).
///
/// This is equivalent to repeatedly calling [`merge`] with each successive layer.
///
/// # Arguments
///
/// * `layers` - Iterator of ConfigValue layers, from lowest to highest priority
///
/// # Returns
///
/// A [`MergeResult`] containing the final merged value and all override records.
pub fn merge_layers<I>(layers: I) -> MergeResult
where
    I: IntoIterator<Item = ConfigValue>,
{
    let mut iter = layers.into_iter();

    let Some(first) = iter.next() else {
        // Empty iterator - return empty object
        return MergeResult::new(ConfigValue::Object(Sourced::new(IndexMap::default())));
    };

    let mut result = MergeResult::new(first);

    for upper in iter {
        let merged = merge(result.value, upper, "");
        result.value = merged.value;
        result.overrides.extend(merged.overrides);
    }

    result
}

/// Internal merge implementation that accumulates overrides.
fn merge_inner(
    lower: ConfigValue,
    upper: ConfigValue,
    path: &str,
    overrides: &mut Vec<Override>,
) -> ConfigValue {
    match (lower, upper) {
        // Both are objects - merge recursively
        (ConfigValue::Object(mut lower_obj), ConfigValue::Object(upper_obj)) => {
            for (key, upper_value) in upper_obj.value {
                let key_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };

                if let Some(lower_value) = lower_obj.value.shift_remove(&key) {
                    // Key exists in both - recurse
                    let merged = merge_inner(lower_value, upper_value, &key_path, overrides);
                    lower_obj.value.insert(key, merged);
                } else {
                    // Key only in upper - just insert
                    lower_obj.value.insert(key, upper_value);
                }
            }
            // Keys only in lower are already in lower_obj

            // Use upper's provenance for the merged object if available
            if upper_obj.provenance.is_some() {
                lower_obj.provenance = upper_obj.provenance;
            }

            ConfigValue::Object(lower_obj)
        }

        // Upper is not an object, or lower is not an object - upper wins
        (lower, upper) => {
            // Record the override if both have provenance
            if let (Some(lower_prov), Some(upper_prov)) =
                (get_provenance(&lower), get_provenance(&upper))
            {
                overrides.push(Override::new(path, upper_prov.clone(), lower_prov.clone()));
            }
            upper
        }
    }
}

/// Extract provenance from a ConfigValue.
fn get_provenance(value: &ConfigValue) -> Option<&Provenance> {
    match value {
        ConfigValue::Null(s) => s.provenance.as_ref(),
        ConfigValue::Bool(s) => s.provenance.as_ref(),
        ConfigValue::Integer(s) => s.provenance.as_ref(),
        ConfigValue::Float(s) => s.provenance.as_ref(),
        ConfigValue::String(s) => s.provenance.as_ref(),
        ConfigValue::Array(s) => s.provenance.as_ref(),
        ConfigValue::Object(s) => s.provenance.as_ref(),
        ConfigValue::Missing(_) => None, // Missing values have no provenance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;

    use crate::provenance::ConfigFile;

    /// Helper to create a string ConfigValue with provenance.
    fn string_with_prov(value: &str, prov: Provenance) -> ConfigValue {
        ConfigValue::String(Sourced {
            value: value.to_string(),
            span: None,
            provenance: Some(prov),
        })
    }

    /// Helper to create an integer ConfigValue with provenance.
    fn int_with_prov(value: i64, prov: Provenance) -> ConfigValue {
        ConfigValue::Integer(Sourced {
            value,
            span: None,
            provenance: Some(prov),
        })
    }

    /// Helper to create an object ConfigValue.
    fn object(entries: Vec<(&str, ConfigValue)>) -> ConfigValue {
        let map: IndexMap<String, ConfigValue, std::hash::RandomState> = entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ConfigValue::Object(Sourced::new(map))
    }

    #[test]
    fn test_merge_disjoint_objects() {
        let lower = object(vec![("a", int_with_prov(1, Provenance::Default))]);
        let upper = object(vec![("b", int_with_prov(2, Provenance::Default))]);

        let result = merge(lower, upper, "");

        if let ConfigValue::Object(obj) = result.value {
            assert_eq!(obj.value.len(), 2);
            assert!(obj.value.contains_key("a"));
            assert!(obj.value.contains_key("b"));
        } else {
            panic!("expected object");
        }

        assert!(result.overrides.is_empty());
    }

    #[test]
    fn test_merge_overlapping_objects() {
        let file = Arc::new(ConfigFile::new("config.json", "{}"));
        let file_prov = Provenance::file(file, "port", 0, 4);
        let env_prov = Provenance::env("REEF__PORT", "9000");

        let lower = object(vec![("port", int_with_prov(8080, file_prov))]);
        let upper = object(vec![("port", int_with_prov(9000, env_prov))]);

        let result = merge(lower, upper, "");

        if let ConfigValue::Object(obj) = result.value {
            assert_eq!(obj.value.len(), 1);
            if let Some(ConfigValue::Integer(port)) = obj.value.get("port") {
                assert_eq!(port.value, 9000); // Upper wins
            } else {
                panic!("expected integer");
            }
        } else {
            panic!("expected object");
        }

        // Should have recorded the override
        assert_eq!(result.overrides.len(), 1);
        assert_eq!(result.overrides[0].path, "port");
    }

    #[test]
    fn test_merge_nested_objects() {
        let file_prov = || Provenance::Default;

        let lower = object(vec![(
            "smtp",
            object(vec![
                ("host", string_with_prov("mail.example.com", file_prov())),
                ("port", int_with_prov(587, file_prov())),
            ]),
        )]);

        let upper = object(vec![(
            "smtp",
            object(vec![(
                "host",
                string_with_prov("override.com", file_prov()),
            )]),
        )]);

        let result = merge(lower, upper, "");

        if let ConfigValue::Object(obj) = result.value {
            if let Some(ConfigValue::Object(smtp)) = obj.value.get("smtp") {
                assert_eq!(smtp.value.len(), 2); // Both host and port

                if let Some(ConfigValue::String(host)) = smtp.value.get("host") {
                    assert_eq!(host.value, "override.com"); // Upper wins
                }
                if let Some(ConfigValue::Integer(port)) = smtp.value.get("port") {
                    assert_eq!(port.value, 587); // Lower preserved
                }
            } else {
                panic!("expected smtp object");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_merge_scalar_replaces() {
        let lower = int_with_prov(1, Provenance::Default);
        let upper = int_with_prov(2, Provenance::env("VAR", "2"));

        let result = merge(lower, upper, "value");

        if let ConfigValue::Integer(i) = result.value {
            assert_eq!(i.value, 2);
        } else {
            panic!("expected integer");
        }

        assert_eq!(result.overrides.len(), 1);
    }

    #[test]
    fn test_merge_layers_empty() {
        let result = merge_layers(Vec::<ConfigValue>::new());

        if let ConfigValue::Object(obj) = result.value {
            assert!(obj.value.is_empty());
        } else {
            panic!("expected empty object");
        }
    }

    #[test]
    fn test_merge_layers_single() {
        let layer = object(vec![("port", int_with_prov(8080, Provenance::Default))]);
        let result = merge_layers(vec![layer]);

        if let ConfigValue::Object(obj) = result.value {
            assert_eq!(obj.value.len(), 1);
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_merge_layers_multiple() {
        // file < env < cli
        let file_prov = Provenance::Default;
        let env_prov = Provenance::env("REEF__PORT", "9000");
        let cli_prov = Provenance::cli("--config.port", "8080");

        let file_layer = object(vec![
            ("port", int_with_prov(80, file_prov.clone())),
            ("host", string_with_prov("file.com", file_prov)),
        ]);

        let env_layer = object(vec![("port", int_with_prov(9000, env_prov))]);

        let cli_layer = object(vec![("port", int_with_prov(8080, cli_prov))]);

        let result = merge_layers(vec![file_layer, env_layer, cli_layer]);

        if let ConfigValue::Object(obj) = result.value {
            // Port should be 8080 (CLI wins)
            if let Some(ConfigValue::Integer(port)) = obj.value.get("port") {
                assert_eq!(port.value, 8080);
            }
            // Host should be from file (not overridden)
            if let Some(ConfigValue::String(host)) = obj.value.get("host") {
                assert_eq!(host.value, "file.com");
            }
        } else {
            panic!("expected object");
        }

        // Should have 2 overrides (env over file, cli over env)
        assert_eq!(result.overrides.len(), 2);
    }

    #[test]
    fn test_merge_object_over_scalar() {
        // If lower has a scalar and upper has an object, upper wins entirely
        let lower = object(vec![(
            "smtp",
            string_with_prov("legacy", Provenance::Default),
        )]);

        let upper = object(vec![(
            "smtp",
            object(vec![(
                "host",
                string_with_prov("mail.com", Provenance::Default),
            )]),
        )]);

        let result = merge(lower, upper, "");

        if let ConfigValue::Object(obj) = result.value {
            if let Some(ConfigValue::Object(smtp)) = obj.value.get("smtp") {
                assert_eq!(smtp.value.len(), 1);
                assert!(smtp.value.contains_key("host"));
            } else {
                panic!("expected smtp to be object");
            }
        } else {
            panic!("expected object");
        }
    }

    #[test]
    fn test_merge_scalar_over_object() {
        // If lower has an object and upper has a scalar, upper wins entirely
        let lower = object(vec![(
            "smtp",
            object(vec![(
                "host",
                string_with_prov("mail.com", Provenance::Default),
            )]),
        )]);

        let upper = object(vec![(
            "smtp",
            string_with_prov("disabled", Provenance::Default),
        )]);

        let result = merge(lower, upper, "");

        if let ConfigValue::Object(obj) = result.value {
            if let Some(ConfigValue::String(smtp)) = obj.value.get("smtp") {
                assert_eq!(smtp.value, "disabled");
            } else {
                panic!("expected smtp to be string");
            }
        } else {
            panic!("expected object");
        }
    }
}
