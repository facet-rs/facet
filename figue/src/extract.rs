//! Extract requirements structs from parsed configuration.
//!
//! This module provides the ability to validate and extract subcommand-specific
//! required fields from a successfully parsed configuration.

use crate::config_value::{ConfigValue, ObjectMap, Sourced};
use crate::schema::Schema;
use facet::{Def, Facet, Field, Type, UserType};
use heck::{ToKebabCase, ToShoutySnakeCase};
use indexmap::IndexMap;
use owo_colors::OwoColorize;
use owo_colors::Stream::Stdout;

/// Information about a missing required field during extraction.
#[derive(Debug, Clone)]
pub struct ExtractMissingField {
    /// Field name in the requirements struct.
    pub field_name: String,
    /// Origin path that was looked up.
    pub origin_path: String,
    /// Expected type name.
    pub type_name: String,
    /// CLI hint for setting this field (e.g., "--config.database-url").
    pub cli_hint: Option<String>,
    /// Environment variable hint (e.g., "$MYAPP__DATABASE_URL").
    pub env_hint: Option<String>,
}

/// Error returned when extraction fails.
#[derive(Debug)]
pub struct ExtractError {
    /// List of missing required fields.
    pub missing_fields: Vec<ExtractMissingField>,
}

impl std::fmt::Display for ExtractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Missing required fields for this operation:\n")?;
        for field in &self.missing_fields {
            write!(
                f,
                "  {} <{}> at {}",
                field
                    .field_name
                    .if_supports_color(Stdout, |text| text.bold()),
                field
                    .type_name
                    .if_supports_color(Stdout, |text| text.cyan()),
                field.origin_path
            )?;

            let mut hints = Vec::new();
            if let Some(cli) = &field.cli_hint {
                hints.push(
                    cli.if_supports_color(Stdout, |text| text.green())
                        .to_string(),
                );
            }
            if let Some(env) = &field.env_hint {
                hints.push(
                    env.if_supports_color(Stdout, |text| text.yellow())
                        .to_string(),
                );
            }
            if !hints.is_empty() {
                write!(f, "\n    Set via: {}", hints.join(" or "))?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

impl std::error::Error for ExtractError {}

/// Extract a requirements struct from a ConfigValue.
///
/// The requirements struct should have fields annotated with `#[facet(args::origin = "path")]`
/// to indicate which values from the config should be extracted.
///
/// Returns an error if:
/// - Any required field (non-Option) has a missing origin value
/// - Any field lacks the `args::origin` attribute
pub fn extract_requirements<R: Facet<'static>>(
    config_value: &ConfigValue,
    schema: &Schema,
) -> Result<R, ExtractError> {
    let shape = R::SHAPE;

    // Verify it's a struct
    let struct_type = match &shape.ty {
        Type::User(UserType::Struct(s)) => *s,
        _ => {
            return Err(ExtractError {
                missing_fields: vec![ExtractMissingField {
                    field_name: "<root>".to_string(),
                    origin_path: "<root>".to_string(),
                    type_name: shape.type_identifier.to_string(),
                    cli_hint: None,
                    env_hint: None,
                }],
            });
        }
    };

    let mut missing_fields = Vec::new();
    let mut extracted_values: ObjectMap = IndexMap::default();

    // Get env prefix from schema if available
    let env_prefix = schema.config().and_then(|c| c.env_prefix());

    for field in struct_type.fields {
        let field_name = field.name;

        // Find the args::origin attribute
        let origin_path = find_origin_attribute(field);

        let Some(origin_path) = origin_path else {
            // Field doesn't have args::origin - this is an error
            return Err(ExtractError {
                missing_fields: vec![ExtractMissingField {
                    field_name: field_name.to_string(),
                    origin_path: "<missing args::origin attribute>".to_string(),
                    type_name: field.shape().type_identifier.to_string(),
                    cli_hint: None,
                    env_hint: None,
                }],
            });
        };

        // Parse the origin path
        let path_segments: Vec<&str> = origin_path.split('.').collect();

        // Look up value in config_value
        let value = get_value_by_path(config_value, &path_segments);

        // Check if field is optional (Option<T>)
        let is_optional = matches!(field.shape().def, Def::Option(_));

        match value {
            Some(v) if !is_null_value(v) => {
                // Value exists - add to extracted values
                extracted_values.insert(field_name.to_string(), v.clone());
            }
            _ => {
                // Value is missing or null
                if is_optional {
                    // Optional field - insert null
                    extracted_values
                        .insert(field_name.to_string(), ConfigValue::Null(Sourced::new(())));
                } else {
                    // Required field is missing - collect error info
                    let cli_hint = compute_cli_hint(origin_path);
                    let env_hint = compute_env_hint(origin_path, env_prefix);

                    missing_fields.push(ExtractMissingField {
                        field_name: field_name.to_string(),
                        origin_path: origin_path.to_string(),
                        type_name: field.shape().type_identifier.to_string(),
                        cli_hint,
                        env_hint,
                    });
                }
            }
        }
    }

    if !missing_fields.is_empty() {
        return Err(ExtractError { missing_fields });
    }

    // Build ConfigValue::Object from extracted values and deserialize
    let extracted_config = ConfigValue::Object(Sourced::new(extracted_values));

    crate::config_value_parser::from_config_value(&extracted_config).map_err(|e| ExtractError {
        missing_fields: vec![ExtractMissingField {
            field_name: "<deserialization>".to_string(),
            origin_path: e.to_string(),
            type_name: shape.type_identifier.to_string(),
            cli_hint: None,
            env_hint: None,
        }],
    })
}

/// Find the `args::origin` attribute value from a field.
fn find_origin_attribute(field: &Field) -> Option<&'static str> {
    // The attribute data for args::origin is stored directly as &str
    for field_attr in field.attributes {
        if field_attr.ns == Some("args")
            && field_attr.key == "origin"
            && let Some(s) = field_attr.get_as::<&str>()
        {
            return Some(s);
        }
    }
    None
}

/// Navigate into a ConfigValue by dot-separated path.
fn get_value_by_path<'a>(value: &'a ConfigValue, path: &[&str]) -> Option<&'a ConfigValue> {
    let mut current = value;
    for segment in path {
        match current {
            ConfigValue::Object(obj) => {
                current = obj.value.get(*segment)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

/// Check if a ConfigValue is null.
fn is_null_value(value: &ConfigValue) -> bool {
    matches!(value, ConfigValue::Null(_))
}

/// Compute CLI hint from origin path.
fn compute_cli_hint(origin_path: &str) -> Option<String> {
    let kebab_path = origin_path
        .split('.')
        .map(|s| s.to_kebab_case())
        .collect::<Vec<_>>()
        .join(".");
    Some(format!("--{}", kebab_path))
}

/// Compute environment variable hint from origin path.
fn compute_env_hint(origin_path: &str, env_prefix: Option<&str>) -> Option<String> {
    let shouty_path = origin_path
        .split('.')
        .map(|s| s.to_shouty_snake_case())
        .collect::<Vec<_>>()
        .join("__");

    if let Some(prefix) = env_prefix {
        Some(format!("${}__{}", prefix, shouty_path))
    } else {
        Some(format!("${}", shouty_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_value::Sourced;
    use facet::Facet;
    use figue_attrs as args;

    // Helper to create test config values
    fn cv_object(fields: impl IntoIterator<Item = (&'static str, ConfigValue)>) -> ConfigValue {
        let map: ObjectMap = fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ConfigValue::Object(Sourced::new(map))
    }

    fn cv_string(s: &str) -> ConfigValue {
        ConfigValue::String(Sourced::new(s.to_string()))
    }

    fn cv_int(i: i64) -> ConfigValue {
        ConfigValue::Integer(Sourced::new(i))
    }

    // ========================================================================
    // Test structs
    // ========================================================================

    #[derive(Facet, Debug, PartialEq)]
    struct SimpleRequirements {
        #[facet(args::origin = "config.database_url")]
        database_url: String,

        #[facet(args::origin = "config.port")]
        port: u16,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct RequirementsWithOptional {
        #[facet(args::origin = "config.database_url")]
        database_url: String,

        #[facet(args::origin = "config.timeout")]
        timeout: Option<u32>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct NestedRequirements {
        #[facet(args::origin = "config.server.host")]
        host: String,

        #[facet(args::origin = "config.server.port")]
        port: u16,
    }

    // ========================================================================
    // Tests
    // ========================================================================

    #[test]
    fn test_extract_all_present() {
        let config = cv_object([(
            "config",
            cv_object([
                ("database_url", cv_string("postgres://localhost/db")),
                ("port", cv_int(8080)),
            ]),
        )]);

        // Create a minimal schema for testing
        #[derive(Facet)]
        struct TestConfig {
            database_url: String,
            port: u16,
        }

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::config)]
            config: TestConfig,
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).unwrap();
        let result: Result<SimpleRequirements, _> = extract_requirements(&config, &schema);

        assert!(result.is_ok(), "extraction should succeed: {:?}", result);
        let req = result.unwrap();
        assert_eq!(req.database_url, "postgres://localhost/db");
        assert_eq!(req.port, 8080);
    }

    #[test]
    fn test_extract_missing_required() {
        let config = cv_object([("config", cv_object([("port", cv_int(8080))]))]);

        #[derive(Facet)]
        struct TestConfig {
            database_url: Option<String>,
            port: u16,
        }

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::config)]
            config: TestConfig,
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).unwrap();
        let result: Result<SimpleRequirements, _> = extract_requirements(&config, &schema);

        assert!(result.is_err(), "extraction should fail");
        let err = result.unwrap_err();
        assert_eq!(err.missing_fields.len(), 1);
        assert_eq!(err.missing_fields[0].field_name, "database_url");
        assert_eq!(err.missing_fields[0].origin_path, "config.database_url");
    }

    #[test]
    fn test_extract_optional_missing() {
        let config = cv_object([(
            "config",
            cv_object([("database_url", cv_string("postgres://localhost/db"))]),
        )]);

        #[derive(Facet)]
        struct TestConfig {
            database_url: String,
            timeout: Option<u32>,
        }

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::config)]
            config: TestConfig,
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).unwrap();
        let result: Result<RequirementsWithOptional, _> = extract_requirements(&config, &schema);

        assert!(
            result.is_ok(),
            "extraction should succeed with missing optional: {:?}",
            result
        );
        let req = result.unwrap();
        assert_eq!(req.database_url, "postgres://localhost/db");
        assert_eq!(req.timeout, None);
    }

    #[test]
    fn test_extract_nested_paths() {
        let config = cv_object([(
            "config",
            cv_object([(
                "server",
                cv_object([("host", cv_string("localhost")), ("port", cv_int(3000))]),
            )]),
        )]);

        #[derive(Facet)]
        struct ServerConfig {
            host: String,
            port: u16,
        }

        #[derive(Facet)]
        struct TestConfig {
            server: ServerConfig,
        }

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::config)]
            config: TestConfig,
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).unwrap();
        let result: Result<NestedRequirements, _> = extract_requirements(&config, &schema);

        assert!(
            result.is_ok(),
            "extraction with nested paths should succeed: {:?}",
            result
        );
        let req = result.unwrap();
        assert_eq!(req.host, "localhost");
        assert_eq!(req.port, 3000);
    }

    #[test]
    fn test_extract_multiple_missing() {
        let config = cv_object([("config", cv_object([]))]);

        #[derive(Facet)]
        struct TestConfig {
            database_url: Option<String>,
            port: Option<u16>,
        }

        #[derive(Facet)]
        struct TestArgs {
            #[facet(args::config)]
            config: TestConfig,
        }

        let schema = Schema::from_shape(TestArgs::SHAPE).unwrap();
        let result: Result<SimpleRequirements, _> = extract_requirements(&config, &schema);

        assert!(result.is_err(), "extraction should fail");
        let err = result.unwrap_err();
        assert_eq!(err.missing_fields.len(), 2);

        let field_names: Vec<_> = err
            .missing_fields
            .iter()
            .map(|f| f.field_name.as_str())
            .collect();
        assert!(field_names.contains(&"database_url"));
        assert!(field_names.contains(&"port"));
    }

    #[test]
    fn test_cli_hint_format() {
        let hint = compute_cli_hint("config.database_url");
        assert_eq!(hint, Some("--config.database-url".to_string()));
    }

    #[test]
    fn test_env_hint_format_with_prefix() {
        let hint = compute_env_hint("config.database_url", Some("MYAPP"));
        assert_eq!(hint, Some("$MYAPP__CONFIG__DATABASE_URL".to_string()));
    }

    #[test]
    fn test_env_hint_format_without_prefix() {
        let hint = compute_env_hint("config.database_url", None);
        assert_eq!(hint, Some("$CONFIG__DATABASE_URL".to_string()));
    }

    #[test]
    fn test_missing_origin_attribute_error() {
        #[derive(Facet, Debug)]
        struct BadRequirements {
            // Missing args::origin attribute
            database_url: String,
        }

        let config = cv_object([]);

        #[derive(Facet)]
        struct TestArgs {}

        let schema = Schema::from_shape(TestArgs::SHAPE).unwrap();
        let result: Result<BadRequirements, _> = extract_requirements(&config, &schema);

        assert!(result.is_err(), "should fail for missing origin attribute");
        let err = result.unwrap_err();
        assert!(
            err.missing_fields[0]
                .origin_path
                .contains("missing args::origin")
        );
    }
}
