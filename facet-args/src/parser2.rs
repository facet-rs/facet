//! Schema-driven CLI argument parser that outputs ConfigValue with provenance.
//!
//! This parser:
//! - Uses the pre-built Schema (not raw attribute lookups)
//! - Outputs LayerOutput (ConfigValue + diagnostics), not a Partial
//! - Does NOT set defaults (that's the driver's job)
//! - Reports errors properly (no silent skipping)

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use std::hash::RandomState;

use heck::{ToKebabCase, ToSnakeCase};
use indexmap::IndexMap;

use crate::config_value::{ConfigValue, EnumValue, Sourced};
use crate::driver::{Diagnostic, LayerOutput, Severity};
use crate::provenance::Provenance;
use crate::schema::{ArgKind, ArgLevelSchema, ArgSchema, Schema, Subcommand};

/// Parse CLI arguments using the schema, returning a LayerOutput.
///
/// This handles both:
/// - `schema.args`: top-level flags, positionals, subcommands
/// - `schema.config` overrides: `--config.port 8080` style dotted paths
pub fn parse_cli(args: &[&str], schema: &Schema) -> LayerOutput {
    let mut ctx = ParseContext::new(args, schema);
    ctx.parse();
    ctx.into_output()
}

/// Parser context holding state during CLI parsing.
struct ParseContext<'a> {
    /// Input arguments
    args: &'a [&'a str],
    /// Current position in args
    index: usize,
    /// Schema being parsed against
    schema: &'a Schema,
    /// Result object being built
    result: IndexMap<String, ConfigValue, RandomState>,
    /// Diagnostics collected during parsing
    diagnostics: Vec<Diagnostic>,
    /// Whether we've seen `--` (positional-only mode)
    positional_only: bool,
    /// Counted flag accumulators: field_name -> count
    counted: IndexMap<String, u64, RandomState>,
}

impl<'a> ParseContext<'a> {
    fn new(args: &'a [&'a str], schema: &'a Schema) -> Self {
        Self {
            args,
            index: 0,
            schema,
            result: IndexMap::default(),
            diagnostics: Vec::new(),
            positional_only: false,
            counted: IndexMap::default(),
        }
    }

    fn parse(&mut self) {
        self.parse_level(self.schema.args());
        self.apply_counted_fields();
    }

    fn parse_level(&mut self, level: &ArgLevelSchema) {
        while self.index < self.args.len() {
            let arg = self.args[self.index];

            // Handle `--` separator
            if arg == "--" {
                self.positional_only = true;
                self.index += 1;
                continue;
            }

            // In positional-only mode, everything is a positional
            if self.positional_only {
                if !self.try_parse_positional(level) && !self.try_parse_subcommand(level) {
                    self.emit_error(format!("unexpected positional argument: {}", arg));
                    self.index += 1;
                }
                continue;
            }

            // Try to parse as a flag or positional
            if arg.starts_with("--") {
                self.parse_long_flag(arg, level);
            } else if arg.starts_with('-') && arg.len() > 1 {
                self.parse_short_flag(arg, level);
            } else {
                // Positional or subcommand
                if !self.try_parse_subcommand(level) && !self.try_parse_positional(level) {
                    self.emit_error(format!("unexpected argument: {}", arg));
                    self.index += 1;
                }
            }
        }
    }

    fn parse_long_flag(&mut self, arg: &str, level: &ArgLevelSchema) {
        let flag = &arg[2..]; // strip "--"

        // Reject flags that start with another dash (e.g., ---verbose)
        if flag.starts_with('-') {
            self.emit_error(format!("unknown flag: {}", arg));
            self.index += 1;
            return;
        }

        // Check for `--flag=value` syntax
        let (flag_name, inline_value) = if let Some(eq_pos) = flag.find('=') {
            (&flag[..eq_pos], Some(&flag[eq_pos + 1..]))
        } else {
            (flag, None)
        };

        // Check if this is a config override (e.g., --config.port)
        if let Some(config_schema) = self.schema.config() {
            if let Some(config_field_name) = config_schema.field_name() {
                if flag_name.starts_with(config_field_name)
                    && flag_name.len() > config_field_name.len()
                    && flag_name.as_bytes()[config_field_name.len()] == b'.'
                {
                    self.parse_config_override(arg, flag_name, inline_value, config_field_name);
                    return;
                }
            }
        }

        // Look up in schema
        let flag_snake = flag_name.to_snake_case();
        if let Some(arg_schema) = level.args().get(&flag_snake) {
            if let ArgKind::Named { counted, .. } = arg_schema.kind() {
                if *counted {
                    self.increment_counted(&flag_snake);
                    self.index += 1;
                    return;
                }
            }
            self.parse_flag_value(arg, &flag_snake, arg_schema, inline_value);
        } else {
            self.emit_error(format!("unknown flag: --{}", flag_name));
            self.index += 1;
        }
    }

    fn parse_short_flag(&mut self, arg: &str, level: &ArgLevelSchema) {
        let chars: Vec<char> = arg[1..].chars().collect();

        for (i, ch) in chars.iter().enumerate() {
            // Find argument with this short flag
            let found = level.args().iter().find(|(_, schema)| {
                if let ArgKind::Named { short: Some(s), .. } = schema.kind() {
                    *s == *ch
                } else {
                    false
                }
            });

            if let Some((name, arg_schema)) = found {
                if let ArgKind::Named { counted, .. } = arg_schema.kind() {
                    if *counted {
                        self.increment_counted(name);
                        continue;
                    }
                }

                let is_bool = arg_schema.value().inner_if_option().is_bool();
                let is_last = i == chars.len() - 1;

                if is_bool {
                    // Bool flag: set to true
                    let prov = Provenance::cli(format!("-{}", ch), "true");
                    self.result.insert(
                        name.clone(),
                        ConfigValue::Bool(Sourced {
                            value: true,
                            span: None,
                            provenance: Some(prov),
                        }),
                    );
                } else if is_last {
                    // Non-bool flag at end: look for value
                    self.index += 1;
                    if self.index < self.args.len() {
                        let value_str = self.args[self.index];
                        let prov_arg = format!("-{}", ch);
                        let value = self.parse_value_string(value_str, &prov_arg);
                        self.result.insert(name.clone(), value);
                    } else {
                        self.emit_error(format!("flag -{} requires a value", ch));
                    }
                } else {
                    // Non-bool flag with attached value: -p8080
                    let rest: String = chars[i + 1..].iter().collect();
                    let prov_arg = format!("-{}", ch);
                    let value = self.parse_value_string(&rest, &prov_arg);
                    self.result.insert(name.clone(), value);
                    break; // Consumed rest of chars
                }
            } else {
                self.emit_error(format!("unknown flag: -{}", ch));
            }
        }
        self.index += 1;
    }

    fn parse_flag_value(
        &mut self,
        arg: &str,
        name: &str,
        schema: &ArgSchema,
        inline_value: Option<&str>,
    ) {
        let is_bool = schema.value().inner_if_option().is_bool();

        if is_bool {
            // Bool flag: presence means true
            let value = if let Some(v) = inline_value {
                // --flag=true or --flag=false
                matches!(v.to_lowercase().as_str(), "true" | "yes" | "1" | "on" | "")
            } else {
                true
            };
            let prov = Provenance::cli(arg, value.to_string());
            self.result.insert(
                name.to_string(),
                ConfigValue::Bool(Sourced {
                    value,
                    span: None,
                    provenance: Some(prov),
                }),
            );
            self.index += 1;
        } else {
            // Non-bool: need a value
            let value_str = if let Some(v) = inline_value {
                self.index += 1;
                v
            } else {
                self.index += 1;
                if self.index < self.args.len() {
                    let v = self.args[self.index];
                    self.index += 1;
                    v
                } else {
                    self.emit_error(format!("flag {} requires a value", arg));
                    return;
                }
            };

            let prov_arg = arg.split('=').next().unwrap_or(arg);
            let value = self.parse_value_string(value_str, prov_arg);
            self.result.insert(name.to_string(), value);
        }
    }

    fn parse_config_override(
        &mut self,
        arg: &str,
        flag_name: &str,
        inline_value: Option<&str>,
        config_field_name: &str,
    ) {
        // Extract the path after "config."
        let path = &flag_name[config_field_name.len() + 1..];
        let parts: Vec<&str> = path.split('.').collect();

        // Get the value
        let value_str = if let Some(v) = inline_value {
            self.index += 1;
            v
        } else {
            self.index += 1;
            if self.index < self.args.len() {
                let v = self.args[self.index];
                self.index += 1;
                v
            } else {
                self.emit_error(format!("flag --{} requires a value", flag_name));
                return;
            }
        };

        let prov_arg = format!("--{}", flag_name);
        let value = self.parse_value_string(value_str, &prov_arg);

        // Insert into nested config structure
        self.insert_config_value(config_field_name, &parts, value);
    }

    fn insert_config_value(&mut self, config_field_name: &str, path: &[&str], value: ConfigValue) {
        // Get or create the config object
        let config_obj = self
            .result
            .entry(config_field_name.to_string())
            .or_insert_with(|| {
                ConfigValue::Object(Sourced {
                    value: IndexMap::default(),
                    span: None,
                    provenance: None,
                })
            });

        if let ConfigValue::Object(sourced) = config_obj {
            insert_nested(&mut sourced.value, path, value);
        }
    }

    fn try_parse_subcommand(&mut self, level: &ArgLevelSchema) -> bool {
        let Some(field_name) = level.subcommand_field_name() else {
            return false;
        };

        let arg = self.args[self.index];
        let arg_kebab = arg.to_kebab_case();

        if let Some(subcommand) = level.subcommands().get(&arg_kebab) {
            self.index += 1;
            let fields = self.parse_subcommand_args(subcommand);

            let enum_value = ConfigValue::Enum(Sourced {
                value: EnumValue {
                    variant: arg_kebab,
                    fields,
                },
                span: None,
                provenance: Some(Provenance::cli(arg, "")),
            });

            self.result.insert(field_name.to_string(), enum_value);
            return true;
        }

        false
    }

    fn parse_subcommand_args(
        &mut self,
        subcommand: &Subcommand,
    ) -> IndexMap<String, ConfigValue, RandomState> {
        // Save current state
        let old_result = core::mem::take(&mut self.result);
        let old_counted = core::mem::take(&mut self.counted);

        // Parse subcommand arguments
        self.parse_level(subcommand.args());
        self.apply_counted_fields();

        // Restore and return
        let subcommand_result = core::mem::replace(&mut self.result, old_result);
        self.counted = old_counted;

        subcommand_result
    }

    fn try_parse_positional(&mut self, level: &ArgLevelSchema) -> bool {
        let arg = self.args[self.index];

        // Find the next unset positional argument
        for (name, schema) in level.args() {
            if !matches!(schema.kind(), ArgKind::Positional) {
                continue;
            }

            // Check if already set (unless it's multiple/list)
            if self.result.contains_key(name) && !schema.multiple() {
                continue;
            }

            let prov = Provenance::cli(arg, arg);
            let value = self.parse_value_string(arg, arg);
            self.result.insert(name.clone(), value);
            self.index += 1;
            return true;
        }

        false
    }

    fn parse_value_string(&self, s: &str, arg_name: &str) -> ConfigValue {
        let prov = Some(Provenance::cli(arg_name, s));

        // Try to parse as different types
        if s == "true" {
            return ConfigValue::Bool(Sourced {
                value: true,
                span: None,
                provenance: prov,
            });
        }
        if s == "false" {
            return ConfigValue::Bool(Sourced {
                value: false,
                span: None,
                provenance: prov,
            });
        }
        if let Ok(i) = s.parse::<i64>() {
            return ConfigValue::Integer(Sourced {
                value: i,
                span: None,
                provenance: prov,
            });
        }
        if let Ok(f) = s.parse::<f64>() {
            return ConfigValue::Float(Sourced {
                value: f,
                span: None,
                provenance: prov,
            });
        }

        ConfigValue::String(Sourced {
            value: s.to_string(),
            span: None,
            provenance: prov,
        })
    }

    fn increment_counted(&mut self, name: &str) {
        let count = self.counted.entry(name.to_string()).or_insert(0);
        *count += 1;
    }

    fn apply_counted_fields(&mut self) {
        for (name, count) in &self.counted {
            let prov = Provenance::cli(
                format!("-{}", name.chars().next().unwrap_or('?')),
                count.to_string(),
            );
            self.result.insert(
                name.clone(),
                ConfigValue::Integer(Sourced {
                    value: *count as i64,
                    span: None,
                    provenance: Some(prov),
                }),
            );
        }
    }

    fn emit_error(&mut self, message: String) {
        self.diagnostics.push(Diagnostic {
            message,
            path: None,
            span: None,
            severity: Severity::Error,
        });
    }

    fn into_output(self) -> LayerOutput {
        let value = if self.result.is_empty() {
            Some(ConfigValue::Object(Sourced {
                value: IndexMap::default(),
                span: None,
                provenance: None,
            }))
        } else {
            Some(ConfigValue::Object(Sourced {
                value: self.result,
                span: None,
                provenance: None,
            }))
        };

        LayerOutput {
            value,
            unused_keys: Vec::new(),
            diagnostics: self.diagnostics,
        }
    }
}

/// Insert a value at a nested path in an IndexMap.
fn insert_nested(
    map: &mut IndexMap<String, ConfigValue, RandomState>,
    path: &[&str],
    value: ConfigValue,
) {
    if path.is_empty() {
        return;
    }

    if path.len() == 1 {
        map.insert(path[0].to_string(), value);
        return;
    }

    let key = path[0].to_string();
    let entry = map.entry(key).or_insert_with(|| {
        ConfigValue::Object(Sourced {
            value: IndexMap::default(),
            span: None,
            provenance: None,
        })
    });

    if let ConfigValue::Object(sourced) = entry {
        insert_nested(&mut sourced.value, &path[1..], value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as args;
    use facet::Facet;

    // ========================================================================
    // Test helpers for building ConfigValue concisely
    // ========================================================================

    /// Build a ConfigValue with CLI provenance
    mod cv {
        use super::*;

        pub fn bool(value: bool, arg: &str) -> ConfigValue {
            ConfigValue::Bool(Sourced {
                value,
                span: None,
                provenance: Some(Provenance::cli(arg, value.to_string())),
            })
        }

        pub fn int(value: i64, arg: &str) -> ConfigValue {
            ConfigValue::Integer(Sourced {
                value,
                span: None,
                provenance: Some(Provenance::cli(arg, value.to_string())),
            })
        }

        pub fn string(value: impl Into<String>, arg: &str) -> ConfigValue {
            let s = value.into();
            ConfigValue::String(Sourced {
                value: s.clone(),
                span: None,
                provenance: Some(Provenance::cli(arg, s)),
            })
        }

        pub fn object(
            fields: impl IntoIterator<Item = (&'static str, ConfigValue)>,
        ) -> ConfigValue {
            let map: IndexMap<String, ConfigValue, std::hash::RandomState> = fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
            ConfigValue::Object(Sourced {
                value: map,
                span: None,
                provenance: Some(Provenance::Cli {
                    arg: String::new(),
                    value: String::new(),
                }),
            })
        }

        pub fn enumv(
            variant: &str,
            fields: impl IntoIterator<Item = (&'static str, ConfigValue)>,
        ) -> ConfigValue {
            use crate::config_value::EnumValue;
            let fields_map: IndexMap<String, ConfigValue, std::hash::RandomState> = fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
            ConfigValue::Enum(Sourced {
                value: EnumValue {
                    variant: variant.to_string(),
                    fields: fields_map,
                },
                span: None,
                provenance: Some(Provenance::Cli {
                    arg: variant.to_string(),
                    value: String::new(),
                }),
            })
        }
    }

    /// Assert that parsing produces the expected ConfigValue
    fn assert_parses_to<T: Facet<'static>>(args: &[&str], expected: ConfigValue) {
        let schema = Schema::from_shape(T::SHAPE).expect("valid schema");
        let output = parse_cli(args, &schema);

        assert!(
            output.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            output.diagnostics
        );

        let value = output.value.expect("expected a value");
        assert_config_value_eq(&value, &expected);
    }

    /// Assert that parsing produces a diagnostic containing the given message
    fn assert_diagnostic_contains<T: Facet<'static>>(args: &[&str], expected_msg: &str) {
        let schema = Schema::from_shape(T::SHAPE).expect("valid schema");
        let output = parse_cli(args, &schema);

        assert!(
            output
                .diagnostics
                .iter()
                .any(|d| d.message.contains(expected_msg)),
            "expected diagnostic containing {:?}, got: {:?}",
            expected_msg,
            output.diagnostics
        );
    }

    /// Deep equality check for ConfigValue (ignoring provenance details)
    fn assert_config_value_eq(actual: &ConfigValue, expected: &ConfigValue) {
        match (actual, expected) {
            (ConfigValue::Bool(a), ConfigValue::Bool(e)) => {
                assert_eq!(a.value, e.value, "bool mismatch");
            }
            (ConfigValue::Integer(a), ConfigValue::Integer(e)) => {
                assert_eq!(a.value, e.value, "integer mismatch");
            }
            (ConfigValue::String(a), ConfigValue::String(e)) => {
                assert_eq!(a.value, e.value, "string mismatch");
            }
            (ConfigValue::Object(a), ConfigValue::Object(e)) => {
                assert_eq!(
                    a.value.keys().collect::<Vec<_>>(),
                    e.value.keys().collect::<Vec<_>>(),
                    "object keys mismatch"
                );
                for (key, expected_val) in &e.value {
                    let actual_val = a.value.get(key).expect(&format!("missing key: {}", key));
                    assert_config_value_eq(actual_val, expected_val);
                }
            }
            (ConfigValue::Array(a), ConfigValue::Array(e)) => {
                assert_eq!(a.value.len(), e.value.len(), "array length mismatch");
                for (av, ev) in a.value.iter().zip(e.value.iter()) {
                    assert_config_value_eq(av, ev);
                }
            }
            (ConfigValue::Enum(a), ConfigValue::Enum(e)) => {
                assert_eq!(a.value.variant, e.value.variant, "enum variant mismatch");
                assert_eq!(
                    a.value.fields.keys().collect::<Vec<_>>(),
                    e.value.fields.keys().collect::<Vec<_>>(),
                    "enum fields mismatch"
                );
                for (key, expected_val) in &e.value.fields {
                    let actual_val = a
                        .value
                        .fields
                        .get(key)
                        .expect(&format!("missing enum field: {}", key));
                    assert_config_value_eq(actual_val, expected_val);
                }
            }
            (ConfigValue::Null(_), ConfigValue::Null(_)) => {}
            _ => panic!(
                "ConfigValue variant mismatch: {:?} vs {:?}",
                std::mem::discriminant(actual),
                std::mem::discriminant(expected)
            ),
        }
    }

    // ========================================================================
    // Test schemas
    // ========================================================================

    #[derive(Facet)]
    struct SimpleArgs {
        /// Enable verbose output
        #[facet(args::named, args::short = 'v')]
        verbose: bool,

        /// Port number
        #[facet(args::named, args::short = 'p')]
        port: Option<u16>,
    }

    #[derive(Facet)]
    struct ArgsWithPositional {
        /// The input file
        #[facet(args::positional)]
        input: String,

        /// The output file
        #[facet(args::positional)]
        output: Option<String>,
    }

    #[derive(Facet)]
    struct ArgsWithConfig {
        #[facet(args::named, args::short = 'v')]
        verbose: bool,

        #[facet(args::config)]
        config: ServerConfig,
    }

    #[derive(Facet)]
    struct ServerConfig {
        port: u16,
        host: String,
    }

    #[derive(Facet)]
    struct ArgsWithSubcommand {
        #[facet(args::named, args::short = 'v')]
        verbose: bool,

        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Facet)]
    #[repr(u8)]
    enum Command {
        /// Build the project
        Build(BuildArgs),
        /// Run tests
        Test(TestArgs),
    }

    #[derive(Facet)]
    struct BuildArgs {
        /// Build in release mode
        #[facet(args::named, args::short = 'r')]
        release: bool,
    }

    #[derive(Facet)]
    struct TestArgs {
        /// Filter tests by name
        #[facet(args::named)]
        filter: Option<String>,
    }

    #[derive(Facet)]
    struct CountedArgs {
        /// Verbosity level
        #[facet(args::named, args::short = 'v', args::counted)]
        verbose: u8,
    }

    // ========================================================================
    // Tests: Basic flag parsing
    // ========================================================================

    #[test]
    fn test_empty_args() {
        assert_parses_to::<SimpleArgs>(&[], cv::object([]));
    }

    #[test]
    fn test_long_bool_flag() {
        assert_parses_to::<SimpleArgs>(
            &["--verbose"],
            cv::object([("verbose", cv::bool(true, "--verbose"))]),
        );
    }

    #[test]
    fn test_short_bool_flag() {
        assert_parses_to::<SimpleArgs>(&["-v"], cv::object([("verbose", cv::bool(true, "-v"))]));
    }

    #[test]
    fn test_long_flag_with_value() {
        assert_parses_to::<SimpleArgs>(
            &["--port", "8080"],
            cv::object([("port", cv::int(8080, "--port"))]),
        );
    }

    #[test]
    fn test_long_flag_with_equals() {
        assert_parses_to::<SimpleArgs>(
            &["--port=8080"],
            cv::object([("port", cv::int(8080, "--port"))]),
        );
    }

    #[test]
    fn test_short_flag_with_value() {
        assert_parses_to::<SimpleArgs>(
            &["-p", "8080"],
            cv::object([("port", cv::int(8080, "-p"))]),
        );
    }

    #[test]
    fn test_short_flag_attached_value() {
        // -p8080 (no space)
        assert_parses_to::<SimpleArgs>(&["-p8080"], cv::object([("port", cv::int(8080, "-p"))]));
    }

    #[test]
    fn test_multiple_flags() {
        assert_parses_to::<SimpleArgs>(
            &["--verbose", "--port", "8080"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                ("port", cv::int(8080, "--port")),
            ]),
        );
    }

    // ========================================================================
    // Tests: Positional arguments
    // ========================================================================

    #[test]
    fn test_single_positional() {
        assert_parses_to::<ArgsWithPositional>(
            &["input.txt"],
            cv::object([("input", cv::string("input.txt", "input.txt"))]),
        );
    }

    #[test]
    fn test_multiple_positionals() {
        assert_parses_to::<ArgsWithPositional>(
            &["input.txt", "output.txt"],
            cv::object([
                ("input", cv::string("input.txt", "input.txt")),
                ("output", cv::string("output.txt", "output.txt")),
            ]),
        );
    }

    #[test]
    fn test_positional_after_double_dash() {
        // -- means everything after is positional
        assert_parses_to::<ArgsWithPositional>(
            &["--", "--input.txt"],
            cv::object([("input", cv::string("--input.txt", "--input.txt"))]),
        );
    }

    // ========================================================================
    // Tests: Config overrides via CLI
    // ========================================================================

    #[test]
    fn test_config_dotted_path() {
        assert_parses_to::<ArgsWithConfig>(
            &["--config.port", "8080"],
            cv::object([(
                "config",
                cv::object([("port", cv::int(8080, "--config.port"))]),
            )]),
        );
    }

    #[test]
    fn test_config_nested_dotted_path() {
        // If config had nested structs: --config.server.host
        assert_parses_to::<ArgsWithConfig>(
            &["--config.host", "localhost"],
            cv::object([(
                "config",
                cv::object([("host", cv::string("localhost", "--config.host"))]),
            )]),
        );
    }

    #[test]
    fn test_args_and_config_together() {
        assert_parses_to::<ArgsWithConfig>(
            &["--verbose", "--config.port", "8080"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                (
                    "config",
                    cv::object([("port", cv::int(8080, "--config.port"))]),
                ),
            ]),
        );
    }

    // ========================================================================
    // Tests: Subcommands
    // ========================================================================

    #[test]
    fn test_subcommand_basic() {
        assert_parses_to::<ArgsWithSubcommand>(
            &["build"],
            cv::object([("command", cv::enumv("build", []))]),
        );
    }

    #[test]
    fn test_subcommand_with_args() {
        assert_parses_to::<ArgsWithSubcommand>(
            &["build", "--release"],
            cv::object([(
                "command",
                cv::enumv("build", [("release", cv::bool(true, "--release"))]),
            )]),
        );
    }

    #[test]
    fn test_global_flag_before_subcommand() {
        assert_parses_to::<ArgsWithSubcommand>(
            &["--verbose", "build", "--release"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                (
                    "command",
                    cv::enumv("build", [("release", cv::bool(true, "--release"))]),
                ),
            ]),
        );
    }

    // ========================================================================
    // Tests: Counted flags
    // ========================================================================

    #[test]
    fn test_counted_single() {
        assert_parses_to::<CountedArgs>(&["-v"], cv::object([("verbose", cv::int(1, "-v"))]));
    }

    #[test]
    fn test_counted_chained() {
        assert_parses_to::<CountedArgs>(&["-vvv"], cv::object([("verbose", cv::int(3, "-vvv"))]));
    }

    #[test]
    fn test_counted_repeated() {
        assert_parses_to::<CountedArgs>(
            &["-v", "-v", "-v"],
            cv::object([("verbose", cv::int(3, "-v"))]),
        );
    }

    // ========================================================================
    // Tests: Error cases
    // ========================================================================

    #[test]
    fn test_unknown_long_flag() {
        assert_diagnostic_contains::<SimpleArgs>(&["--unknown"], "unknown");
    }

    #[test]
    fn test_unknown_short_flag() {
        assert_diagnostic_contains::<SimpleArgs>(&["-x"], "unknown");
    }

    #[test]
    fn test_missing_value_for_flag() {
        assert_diagnostic_contains::<SimpleArgs>(&["--port"], "requires a value");
    }

    #[test]
    fn test_triple_dash_flag() {
        assert_diagnostic_contains::<SimpleArgs>(&["---verbose"], "unknown");
    }

    // Note: parser2 does NOT validate types - it parses strings/ints/floats generically.
    // Type validation is the driver's responsibility when emitting values.
    // Note: parser2 does NOT handle -h/--help specially - that's the driver's job.
    // If -h or --help is needed, they should be defined in the schema.
}
