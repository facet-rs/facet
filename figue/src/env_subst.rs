//! Environment variable substitution in config values.
//!
//! This module provides functionality to substitute `${VAR}` patterns in string
//! values from config files with environment variable values.
//!
//! ## Syntax
//!
//! - `${VAR}` - Substitute with the value of environment variable `VAR`. Error if not set.
//! - `${VAR:-default}` - Substitute with the value of `VAR`, or `default` if not set.
//! - `$$` - Escape sequence that produces a literal `$`.
//!
//! ## Example
//!
//! ```text
//! "${BASE_PATH}/data"        -> "/var/myapp/data"  (if BASE_PATH=/var/myapp)
//! "${PORT:-8080}"            -> "8080"             (if PORT is not set)
//! "$${NOT_SUBSTITUTED}"      -> "${NOT_SUBSTITUTED}"
//! ```

use std::borrow::Cow;

use crate::{
    config_value::ConfigValue,
    provenance::Provenance,
    schema::{ConfigFieldSchema, ConfigStructSchema, ConfigValueSchema},
};

/// Error that occurs during environment variable substitution.
///
/// Boxed to keep Result size small (clippy::result_large_err).
#[derive(Debug, Clone)]
pub struct EnvSubstError(Box<EnvSubstErrorInner>);

#[derive(Debug, Clone)]
struct EnvSubstErrorInner {
    /// The name of the missing environment variable.
    var_name: String,
    /// The path to the field in the config (e.g., "database.url").
    field_path: Vec<String>,
    /// The provenance of the value that contained the substitution pattern.
    #[allow(dead_code)] // Useful for future enhanced error messages
    provenance: Option<Provenance>,
    /// The original string that contained the pattern.
    #[allow(dead_code)] // Useful for future enhanced error messages
    original_value: String,
}

impl EnvSubstError {
    fn new(
        var_name: String,
        field_path: Vec<String>,
        provenance: Option<Provenance>,
        original_value: String,
    ) -> Self {
        Self(Box::new(EnvSubstErrorInner {
            var_name,
            field_path,
            provenance,
            original_value,
        }))
    }

    /// The name of the missing environment variable.
    pub fn var_name(&self) -> &str {
        &self.0.var_name
    }
}

impl std::fmt::Display for EnvSubstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "environment variable `{}` is not set (required by field `{}`)",
            self.0.var_name,
            self.0.field_path.join(".")
        )
    }
}

impl std::error::Error for EnvSubstError {}

/// Trait for accessing environment variables, allowing for testing with mock environments.
pub trait EnvSource {
    fn get(&self, key: &str) -> Option<String>;
}

/// Default implementation that uses actual environment variables.
pub struct RealEnv;

impl EnvSource for RealEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
}

/// Substitute environment variables in a single string.
///
/// Returns `Ok(Cow::Borrowed(input))` if no substitution was needed,
/// or `Ok(Cow::Owned(substituted))` if substitution occurred.
/// Returns `Err` if a required variable is not set.
fn substitute_in_string<'a>(
    input: &'a str,
    env: &dyn EnvSource,
    field_path: &[String],
    provenance: Option<&Provenance>,
) -> Result<Cow<'a, str>, EnvSubstError> {
    // Quick check: if there's no $ in the string, no substitution needed
    if !input.contains('$') {
        return Ok(Cow::Borrowed(input));
    }

    let mut result = String::new();
    let mut chars = input.chars().peekable();
    let mut modified = false;

    while let Some(c) = chars.next() {
        if c == '$' {
            match chars.peek() {
                // $$ -> literal $
                Some('$') => {
                    chars.next();
                    result.push('$');
                    modified = true;
                }
                // ${VAR} or ${VAR:-default}
                Some('{') => {
                    chars.next(); // consume '{'
                    modified = true;

                    // Parse until we find '}' or ':-'
                    let mut var_name = String::new();
                    let mut default_value: Option<String> = None;
                    let mut found_close = false;

                    while let Some(&ch) = chars.peek() {
                        if ch == '}' {
                            chars.next();
                            found_close = true;
                            break;
                        } else if ch == ':' {
                            chars.next();
                            // Check for ':-' pattern
                            if chars.peek() == Some(&'-') {
                                chars.next();
                                // Read the default value until '}'
                                let mut default = String::new();
                                while let Some(&dch) = chars.peek() {
                                    if dch == '}' {
                                        chars.next();
                                        found_close = true;
                                        break;
                                    }
                                    default.push(dch);
                                    chars.next();
                                }
                                default_value = Some(default);
                                break;
                            } else {
                                // Just a ':' in the var name (unusual but allowed)
                                var_name.push(':');
                            }
                        } else {
                            var_name.push(ch);
                            chars.next();
                        }
                    }

                    if !found_close {
                        // Malformed: unclosed ${, treat as literal
                        result.push_str("${");
                        result.push_str(&var_name);
                        if let Some(default) = default_value {
                            result.push_str(":-");
                            result.push_str(&default);
                        }
                        continue;
                    }

                    // Look up the environment variable
                    match env.get(&var_name) {
                        Some(value) => result.push_str(&value),
                        None => {
                            if let Some(default) = default_value {
                                result.push_str(&default);
                            } else {
                                return Err(EnvSubstError::new(
                                    var_name,
                                    field_path.to_vec(),
                                    provenance.cloned(),
                                    input.to_string(),
                                ));
                            }
                        }
                    }
                }
                // Just a $ followed by something else - keep as literal
                _ => {
                    result.push('$');
                }
            }
        } else {
            result.push(c);
        }
    }

    if modified {
        Ok(Cow::Owned(result))
    } else {
        Ok(Cow::Borrowed(input))
    }
}

/// Substitute environment variables in a ConfigValue tree according to the schema.
///
/// This function walks the ConfigValue tree alongside the schema, and for any
/// string value where the schema indicates `env_subst = true`, performs
/// environment variable substitution.
pub fn substitute_env_vars(
    value: &mut ConfigValue,
    schema: &ConfigStructSchema,
    env: &dyn EnvSource,
) -> Result<(), EnvSubstError> {
    substitute_in_struct(value, schema, env, &[])
}

fn substitute_in_struct(
    value: &mut ConfigValue,
    schema: &ConfigStructSchema,
    env: &dyn EnvSource,
    path: &[String],
) -> Result<(), EnvSubstError> {
    let ConfigValue::Object(sourced_fields) = value else {
        return Ok(());
    };

    for (field_name, field_value) in sourced_fields.value.iter_mut() {
        let Some(field_schema) = schema.fields().get(field_name) else {
            continue;
        };

        let mut field_path = path.to_vec();
        field_path.push(field_name.to_string());

        substitute_in_field(field_value, field_schema, env, &field_path)?;
    }

    Ok(())
}

fn substitute_in_field(
    value: &mut ConfigValue,
    field_schema: &ConfigFieldSchema,
    env: &dyn EnvSource,
    path: &[String],
) -> Result<(), EnvSubstError> {
    // Clone provenance to avoid borrow conflicts
    let provenance = get_provenance_cloned(value);
    substitute_in_config_value(
        value,
        &field_schema.value,
        field_schema.env_subst(),
        env,
        path,
        provenance.as_ref(),
    )
}

/// Extract and clone provenance from a ConfigValue if available.
fn get_provenance_cloned(value: &ConfigValue) -> Option<Provenance> {
    match value {
        ConfigValue::String(s) => s.provenance.clone(),
        ConfigValue::Bool(s) => s.provenance.clone(),
        ConfigValue::Integer(s) => s.provenance.clone(),
        ConfigValue::Float(s) => s.provenance.clone(),
        ConfigValue::Null(s) => s.provenance.clone(),
        ConfigValue::Array(s) => s.provenance.clone(),
        ConfigValue::Object(s) => s.provenance.clone(),
        ConfigValue::Enum(s) => s.provenance.clone(),
    }
}

fn substitute_in_config_value(
    value: &mut ConfigValue,
    schema: &ConfigValueSchema,
    env_subst: bool,
    env: &dyn EnvSource,
    path: &[String],
    provenance: Option<&Provenance>,
) -> Result<(), EnvSubstError> {
    // Handle Option schema first - unwrap and recurse with inner schema
    if let ConfigValueSchema::Option {
        value: inner_schema,
        ..
    } = schema
    {
        return substitute_in_config_value(value, inner_schema, env_subst, env, path, provenance);
    }

    // String value - perform substitution if enabled
    if env_subst && let ConfigValue::String(s) = value {
        let substituted = substitute_in_string(&s.value, env, path, provenance)?;
        if let Cow::Owned(new_value) = substituted {
            s.value = new_value;
        }
        return Ok(());
    }

    // Nested struct - recurse with struct schema
    if let ConfigValueSchema::Struct(struct_schema) = schema
        && matches!(value, ConfigValue::Object(_))
    {
        return substitute_in_struct(value, struct_schema, env, path);
    }

    // Array - recurse for each element
    if let ConfigValueSchema::Vec(vec_schema) = schema
        && let ConfigValue::Array(sourced_elements) = value
    {
        for (i, element) in sourced_elements.value.iter_mut().enumerate() {
            let mut element_path = path.to_vec();
            element_path.push(i.to_string());
            let element_provenance = get_provenance_cloned(element);
            substitute_in_config_value(
                element,
                vec_schema.element(),
                env_subst,
                env,
                &element_path,
                element_provenance.as_ref(),
            )?;
        }
        return Ok(());
    }

    // Enum - find the active variant and recurse into its fields
    if let ConfigValueSchema::Enum(enum_schema) = schema
        && let ConfigValue::Object(sourced_fields) = value
    {
        // For enums, the object should have a single key which is the variant name
        for (field_name, field_value) in sourced_fields.value.iter_mut() {
            // Check if this is a variant name
            if let Some(variant_schema) = enum_schema.get_variant(field_name) {
                // Recurse into the variant's fields
                if let ConfigValue::Object(sourced_variant_fields) = field_value {
                    for (vf_name, vf_value) in sourced_variant_fields.value.iter_mut() {
                        if let Some(vf_schema) = variant_schema.fields().get(vf_name) {
                            let mut vf_path = path.to_vec();
                            vf_path.push(field_name.to_string());
                            vf_path.push(vf_name.to_string());
                            substitute_in_field(vf_value, vf_schema, env, &vf_path)?;
                        }
                    }
                }
            }
        }
        return Ok(());
    }

    // Leaf values or non-matching types - nothing to do
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockEnv(HashMap<String, String>);

    impl EnvSource for MockEnv {
        fn get(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
    }

    #[test]
    fn test_no_substitution() {
        let env = MockEnv(HashMap::new());
        let result = substitute_in_string("hello world", &env, &[], None).unwrap();
        assert_eq!(result, "hello world");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn test_simple_substitution() {
        let mut env_map = HashMap::new();
        env_map.insert("FOO".to_string(), "bar".to_string());
        let env = MockEnv(env_map);

        let result = substitute_in_string("${FOO}", &env, &[], None).unwrap();
        assert_eq!(result, "bar");
    }

    #[test]
    fn test_substitution_in_path() {
        let mut env_map = HashMap::new();
        env_map.insert("BASE".to_string(), "/var/app".to_string());
        let env = MockEnv(env_map);

        let result = substitute_in_string("${BASE}/data", &env, &[], None).unwrap();
        assert_eq!(result, "/var/app/data");
    }

    #[test]
    fn test_multiple_substitutions() {
        let mut env_map = HashMap::new();
        env_map.insert("A".to_string(), "foo".to_string());
        env_map.insert("B".to_string(), "bar".to_string());
        let env = MockEnv(env_map);

        let result = substitute_in_string("${A}/${B}", &env, &[], None).unwrap();
        assert_eq!(result, "foo/bar");
    }

    #[test]
    fn test_default_value_used() {
        let env = MockEnv(HashMap::new());
        let result = substitute_in_string("${MISSING:-default}", &env, &[], None).unwrap();
        assert_eq!(result, "default");
    }

    #[test]
    fn test_default_value_not_used_when_var_set() {
        let mut env_map = HashMap::new();
        env_map.insert("VAR".to_string(), "actual".to_string());
        let env = MockEnv(env_map);

        let result = substitute_in_string("${VAR:-default}", &env, &[], None).unwrap();
        assert_eq!(result, "actual");
    }

    #[test]
    fn test_escape_dollar() {
        let env = MockEnv(HashMap::new());
        let result = substitute_in_string("$${NOT_SUBST}", &env, &[], None).unwrap();
        assert_eq!(result, "${NOT_SUBST}");
    }

    #[test]
    fn test_double_escape() {
        let env = MockEnv(HashMap::new());
        let result = substitute_in_string("$$$$", &env, &[], None).unwrap();
        assert_eq!(result, "$$");
    }

    #[test]
    fn test_missing_var_error() {
        let env = MockEnv(HashMap::new());
        let result = substitute_in_string(
            "${MISSING}",
            &env,
            &["config".to_string(), "path".to_string()],
            None,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.var_name(), "MISSING");
        assert_eq!(err.0.field_path, vec!["config", "path"]);
    }

    #[test]
    fn test_empty_default() {
        let env = MockEnv(HashMap::new());
        let result = substitute_in_string("${MISSING:-}", &env, &[], None).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_unclosed_brace_literal() {
        let env = MockEnv(HashMap::new());
        // Unclosed ${... should be treated as literal
        let result = substitute_in_string("${UNCLOSED", &env, &[], None).unwrap();
        assert_eq!(result, "${UNCLOSED");
    }

    #[test]
    fn test_bare_dollar() {
        let env = MockEnv(HashMap::new());
        // Just $ without { should be kept as-is
        let result = substitute_in_string("$5.00", &env, &[], None).unwrap();
        assert_eq!(result, "$5.00");
    }
}
