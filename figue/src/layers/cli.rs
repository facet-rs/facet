//! Schema-driven CLI argument parser that outputs ConfigValue with provenance.
//!
//! # ConfigValue Model
//!
//! This parser produces ConfigValue trees that follow these rules:
//!
//! 1. **Effective names**: All field names in ConfigValue are the *serialized* names
//!    (after `#[facet(rename = "...")]`), not the original Rust field names.
//!    Example: if a field is `#[facet(rename = "cores")] concurrency: i64`,
//!    the ConfigValue key is `"cores"`, not `"concurrency"`.
//!
//! 2. **Flat structure for flatten**: Flattened fields appear at the CURRENT level,
//!    not nested under the flattened field name. Example: if `FigueBuiltins` has
//!    `help` and `version` fields, they appear as top-level keys, not under `"builtins"`.
//!
//! 3. **No "0" wrapper for flattened tuple variants**: For tuple variants like
//!    `Install(#[facet(flatten)] InstallOptions)`, the inner struct's fields appear
//!    directly in the variant's fields, NOT wrapped in a `"0"` key.
//!
//! The deserializer (facet-format) expects this flat structure and handles routing
//! fields to their correct nested locations based on the type's Shape.
//!
//! # Architecture
//!
//! This parser:
//! - Uses the pre-built Schema (not raw attribute lookups)
//! - Outputs LayerOutput (ConfigValue + diagnostics), not a Partial
//! - Does NOT set defaults (that's the driver's job)
//! - Reports errors properly (no silent skipping)

use std::hash::RandomState;
use std::string::{String, ToString};
use std::vec::Vec;

use heck::ToKebabCase;
use indexmap::IndexMap;

use crate::config_value::{ConfigValue, EnumValue, Sourced};
use crate::driver::{Diagnostic, LayerOutput, Severity};
use crate::provenance::Provenance;
use crate::schema::{ArgKind, ArgLevelSchema, ArgSchema, Schema, Subcommand};
use crate::value_builder::{LeafValue, ValueBuilder};

// ============================================================================
// CliConfig
// ============================================================================

/// Configuration for CLI argument parsing.
#[derive(Debug, Clone, Default)]
pub struct CliConfig {
    /// Raw CLI arguments.
    args: Vec<String>,
    /// Whether to error on unknown arguments.
    strict: bool,
}

impl CliConfig {
    /// Get the CLI arguments.
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// Check if strict mode is enabled.
    pub fn strict(&self) -> bool {
        self.strict
    }
}

/// Builder for CLI configuration.
#[derive(Debug, Default)]
pub struct CliConfigBuilder {
    config: CliConfig,
}

impl CliConfigBuilder {
    /// Create a new CLI config builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the CLI arguments to parse.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.config.args = args.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Set CLI arguments from OsString iterator (e.g., std::env::args_os()).
    pub fn args_os<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.config.args = args
            .into_iter()
            .filter_map(|s| s.as_ref().to_str().map(|s| s.to_string()))
            .collect();
        self
    }

    /// Enable strict mode - error on unknown arguments.
    pub fn strict(mut self) -> Self {
        self.config.strict = true;
        self
    }

    /// Build the CLI configuration.
    pub fn build(self) -> CliConfig {
        self.config
    }
}

/// Parse CLI arguments using the schema, returning a LayerOutput.
///
/// This handles both:
/// - `schema.args`: top-level flags, positionals, subcommands
/// - `schema.config` overrides: `--config.port 8080` style dotted paths
pub fn parse_cli(schema: &Schema, cli_config: &CliConfig) -> LayerOutput {
    let args: Vec<&str> = cli_config.args().iter().map(|s| s.as_str()).collect();
    let mut ctx = ParseContext::new(&args, schema);
    ctx.parse();
    ctx.into_output()
}

/// Accumulator for a counted flag.
struct CountedAccumulator {
    /// Number of times the flag was seen.
    count: u64,
}

/// A parent level in the parse stack for the adoption agency algorithm.
/// When a flag isn't found at the current subcommand level, we search
/// parent levels and store the value in the appropriate parent's result map.
struct ParentLevel<'a> {
    /// The argument schema for this level
    args: &'a ArgLevelSchema,
    /// The result map for this level (values parsed at this level go here)
    result: IndexMap<String, ConfigValue, RandomState>,
    /// Counted flag accumulators for this level
    counted: IndexMap<String, CountedAccumulator, RandomState>,
}

/// Target for inserting parsed values - either the current level or a parent level.
#[derive(Clone, Copy)]
enum InsertTarget {
    /// Insert into the current level's result map
    Current,
    /// Insert into a parent level's result map (adoption agency)
    Parent(usize),
}

/// Result of looking up a flag in parent levels.
/// Contains owned/copied data to avoid borrow conflicts.
struct ParentFlagLookup {
    /// Index into parent_stack
    parent_idx: usize,
    /// Effective name of the argument (owned to avoid borrow)
    effective_name: String,
    /// Whether this is a bool flag
    is_bool: bool,
    /// Whether this is a counted flag
    is_counted: bool,
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
    /// Counted flag accumulators: field_name -> accumulator
    counted: IndexMap<String, CountedAccumulator, RandomState>,
    /// Byte offset where each argument starts in the flattened string (args joined by spaces)
    arg_offsets: Vec<usize>,
    /// Stack of parent levels for the adoption agency algorithm.
    /// When parsing a subcommand, parent levels are pushed here so flags can bubble up.
    parent_stack: Vec<ParentLevel<'a>>,
    /// ValueBuilder for config overrides (--config.foo.bar style).
    /// Only present if the schema has a config field.
    config_builder: Option<ValueBuilder<'a>>,
}

impl<'a> ParseContext<'a> {
    fn new(args: &'a [&'a str], schema: &'a Schema) -> Self {
        // Compute byte offsets for each argument in the flattened string
        // When args are joined with spaces: "arg0 arg1 arg2"
        // arg0 starts at 0, arg1 starts at len(arg0)+1, etc.
        let mut arg_offsets = Vec::with_capacity(args.len());
        let mut offset = 0;
        for (i, arg) in args.iter().enumerate() {
            arg_offsets.push(offset);
            offset += arg.len();
            if i < args.len() - 1 {
                offset += 1; // space separator
            }
        }

        let config_builder = schema.config().map(ValueBuilder::new);

        Self {
            args,
            index: 0,
            schema,
            result: IndexMap::default(),
            diagnostics: Vec::new(),
            positional_only: false,
            counted: IndexMap::default(),
            arg_offsets,
            parent_stack: Vec::new(),
            config_builder,
        }
    }

    /// Get the span (byte offset and length) for argument at the given index.
    fn span_for_arg(&self, arg_index: usize) -> facet_reflect::Span {
        let offset = self.arg_offsets.get(arg_index).copied().unwrap_or(0);
        let len = self.args.get(arg_index).map(|s| s.len()).unwrap_or(0);
        facet_reflect::Span::new(offset, len)
    }

    /// Get the span for the current argument.
    fn current_span(&self) -> facet_reflect::Span {
        self.span_for_arg(self.index)
    }

    /// Get a span covering a substring within the current argument.
    /// `sub_offset` is the byte offset within the argument, `sub_len` is the length.
    fn span_within_current(&self, sub_offset: usize, sub_len: usize) -> facet_reflect::Span {
        let base = self.arg_offsets.get(self.index).copied().unwrap_or(0);
        facet_reflect::Span::new(base + sub_offset, sub_len)
    }

    fn parse(&mut self) {
        self.parse_level(self.schema.args());
        self.apply_counted_fields();
    }

    fn parse_level(&mut self, level: &'a ArgLevelSchema) {
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
        if let Some(config_schema) = self.schema.config()
            && let Some(config_field_name) = config_schema.field_name()
            && flag_name.starts_with(config_field_name)
            && flag_name.len() > config_field_name.len()
            && flag_name.as_bytes()[config_field_name.len()] == b'.'
        {
            self.parse_config_override(arg, flag_name, inline_value, config_field_name);
            return;
        }

        // Look up in schema - Args::get converts schema keys to kebab-case for comparison
        // and returns the original effective_name for storage in ConfigValue
        if let Some((effective_name, arg_schema)) = level.args().get(flag_name) {
            if let ArgKind::Named { counted, .. } = arg_schema.kind()
                && *counted
            {
                self.increment_counted(effective_name);
                self.index += 1;
                return;
            }
            self.parse_flag_value(arg, effective_name, arg_schema, inline_value);
        } else if let Some(lookup) = self.find_long_flag_in_parents(flag_name) {
            // Adoption agency: flag found in parent level, bubble up
            let target = InsertTarget::Parent(lookup.parent_idx);
            if lookup.is_counted {
                self.increment_counted_to(target, &lookup.effective_name);
                self.index += 1;
                return;
            }
            self.parse_flag_value_simple(
                arg,
                target,
                &lookup.effective_name,
                lookup.is_bool,
                inline_value,
            );
        } else {
            self.emit_error(format!("unknown flag: --{}", flag_name));
            self.index += 1;
        }
    }

    fn parse_short_flag(&mut self, arg: &str, level: &ArgLevelSchema) {
        let flag_part = &arg[1..]; // strip "-"

        // Check for `-k=value` syntax (single short flag with equals)
        if let Some(eq_pos) = flag_part.find('=') {
            let flag_char = flag_part[..eq_pos].chars().next();
            if eq_pos == 1 {
                // Valid: -k=value (single char before =)
                if let Some(ch) = flag_char {
                    let value_str = &flag_part[eq_pos + 1..];
                    self.parse_short_flag_with_value(ch, value_str, level);
                    self.index += 1;
                    return;
                }
            }
            // Invalid: -abc=value or -=value - fall through to error handling
        }

        let chars: Vec<char> = flag_part.chars().collect();

        for (i, ch) in chars.iter().enumerate() {
            // Find argument with this short flag at current level
            let found = level.args().iter().find(|(_, schema)| {
                if let ArgKind::Named { short: Some(s), .. } = schema.kind() {
                    *s == *ch
                } else {
                    false
                }
            });

            // Determine target and flag info
            let (target, name, is_bool, is_counted) = if let Some((name, arg_schema)) = found {
                let is_counted = matches!(arg_schema.kind(), ArgKind::Named { counted: true, .. });
                let is_bool = arg_schema
                    .value()
                    .inner_if_option()
                    .is_bool_or_vec_of_bool();
                (InsertTarget::Current, name.to_string(), is_bool, is_counted)
            } else if let Some(lookup) = self.find_short_flag_in_parents(*ch) {
                // Adoption agency: flag found in parent level
                (
                    InsertTarget::Parent(lookup.parent_idx),
                    lookup.effective_name,
                    lookup.is_bool,
                    lookup.is_counted,
                )
            } else {
                self.emit_error(format!("unknown flag: -{}", ch));
                continue;
            };

            if is_counted {
                self.increment_counted_to(target, &name);
                continue;
            }

            let is_last = i == chars.len() - 1;

            if is_bool {
                // Bool flag: set to true
                let prov = Provenance::cli(format!("-{}", ch), "true");
                self.insert_value_to(
                    target,
                    &name,
                    ConfigValue::Bool(Sourced {
                        value: true,
                        span: None,
                        provenance: Some(prov),
                    }),
                );
            } else if is_last {
                // Non-bool flag at end: look for value
                let flag_span = self.current_span(); // Save span before incrementing
                self.index += 1;
                if self.index < self.args.len() {
                    let value_span = self.current_span();
                    let value_str = self.args[self.index];
                    let prov_arg = format!("-{}", ch);
                    let value = self.parse_value_string(value_str, &prov_arg, value_span);
                    self.insert_value_to(target, &name, value);
                } else {
                    self.emit_error_at(format!("flag -{} requires a value", ch), flag_span);
                }
            } else {
                // Non-bool flag with attached value: -p8080
                // Span for attached value is within the current arg
                let rest: String = chars[i + 1..].iter().collect();
                let value_span = self.span_within_current(i + 1, rest.len());
                let prov_arg = format!("-{}", ch);
                let value = self.parse_value_string(&rest, &prov_arg, value_span);
                self.insert_value_to(target, &name, value);
                break; // Consumed rest of chars
            }
        }
        self.index += 1;
    }

    /// Parse a short flag with an inline value (e.g., -k=3)
    fn parse_short_flag_with_value(&mut self, ch: char, value_str: &str, level: &ArgLevelSchema) {
        let found = level.args().iter().find(|(_, schema)| {
            if let ArgKind::Named { short: Some(s), .. } = schema.kind() {
                *s == ch
            } else {
                false
            }
        });

        // Determine target and flag info
        let (target, name, is_bool, is_counted) = if let Some((name, arg_schema)) = found {
            let is_counted = matches!(arg_schema.kind(), ArgKind::Named { counted: true, .. });
            let is_bool = arg_schema.value().inner_if_option().is_bool();
            (InsertTarget::Current, name.to_string(), is_bool, is_counted)
        } else if let Some(lookup) = self.find_short_flag_in_parents(ch) {
            // Adoption agency: flag found in parent level
            (
                InsertTarget::Parent(lookup.parent_idx),
                lookup.effective_name,
                lookup.is_bool,
                lookup.is_counted,
            )
        } else {
            self.emit_error(format!("unknown flag: -{}", ch));
            return;
        };

        if is_counted {
            self.increment_counted_to(target, &name);
            return;
        }

        let prov_arg = format!("-{}", ch);

        if is_bool {
            // -k=true or -k=false
            let value = matches!(
                value_str.to_lowercase().as_str(),
                "true" | "yes" | "1" | "on" | ""
            );
            let prov = Provenance::cli(&prov_arg, value.to_string());
            self.insert_value_to(
                target,
                &name,
                ConfigValue::Bool(Sourced {
                    value,
                    span: None,
                    provenance: Some(prov),
                }),
            );
        } else {
            // Value starts after the '=' which is at position 2 (after -k)
            let value_span = self.span_within_current(3, value_str.len());
            let value = self.parse_value_string(value_str, &prov_arg, value_span);
            self.insert_value_to(target, &name, value);
        }
    }

    fn parse_flag_value(
        &mut self,
        arg: &str,
        name: &str,
        schema: &ArgSchema,
        inline_value: Option<&str>,
    ) {
        let is_bool = schema.value().inner_if_option().is_bool();
        self.parse_flag_value_simple(arg, InsertTarget::Current, name, is_bool, inline_value);
    }

    /// Parse a flag value with explicit is_bool (avoids borrow issues with schema).
    fn parse_flag_value_simple(
        &mut self,
        arg: &str,
        target: InsertTarget,
        name: &str,
        is_bool: bool,
        inline_value: Option<&str>,
    ) {
        if is_bool {
            // Bool flag: presence means true
            let value = if let Some(v) = inline_value {
                // --flag=true or --flag=false
                matches!(v.to_lowercase().as_str(), "true" | "yes" | "1" | "on" | "")
            } else {
                true
            };
            let prov = Provenance::cli(arg, value.to_string());
            self.insert_value_to(
                target,
                name,
                ConfigValue::Bool(Sourced {
                    value,
                    span: None,
                    provenance: Some(prov),
                }),
            );
            self.index += 1;
        } else {
            // Non-bool: need a value
            let flag_span = self.current_span();
            let (value_str, value_span) = if let Some(v) = inline_value {
                // --flag=value: value starts after the '='
                let eq_pos = arg.find('=').unwrap_or(0) + 1;
                let span = self.span_within_current(eq_pos, v.len());
                self.index += 1;
                (v, span)
            } else {
                self.index += 1;
                if self.index < self.args.len() {
                    let span = self.current_span();
                    let v = self.args[self.index];
                    self.index += 1;
                    (v, span)
                } else {
                    self.emit_error_at(format!("flag {} requires a value", arg), flag_span);
                    return;
                }
            };

            let prov_arg = arg.split('=').next().unwrap_or(arg);
            let value = self.parse_value_string(value_str, prov_arg, value_span);
            self.insert_value_to(target, name, value);
        }
    }

    fn parse_config_override(
        &mut self,
        _arg: &str,
        flag_name: &str,
        inline_value: Option<&str>,
        config_field_name: &str,
    ) {
        // Extract the path after "config."
        let path_str = &flag_name[config_field_name.len() + 1..];
        let parts: Vec<&str> = path_str.split('.').collect();

        // Get the value
        let flag_span = self.current_span(); // Save span before incrementing
        let (value_str, value_span) = if let Some(v) = inline_value {
            // --config.key=value: value starts after the '='
            let eq_pos = flag_name.len() + 3 + 1; // "--" + flag_name + "="
            let span = self.span_within_current(eq_pos, v.len());
            self.index += 1;
            (v, span)
        } else {
            self.index += 1;
            if self.index < self.args.len() {
                let span = self.current_span();
                let v = self.args[self.index];
                self.index += 1;
                (v, span)
            } else {
                self.emit_error_at(format!("flag --{} requires a value", flag_name), flag_span);
                return;
            }
        };

        let prov_arg = format!("--{}", flag_name);
        let provenance = Provenance::cli(&prov_arg, value_str);
        let path: Vec<String> = parts.iter().map(|s| (*s).to_string()).collect();
        let leaf_value = LeafValue::String(value_str.to_string());

        self.config_builder
            .as_mut()
            .expect("config_builder must exist when parsing config overrides")
            .set(&path, leaf_value, Some(value_span), provenance);
    }

    fn try_parse_subcommand(&mut self, level: &'a ArgLevelSchema) -> bool {
        let Some(field_name) = level.subcommand_field_name() else {
            return false;
        };

        let arg = self.args[self.index];

        // Find subcommand by comparing user input with kebab-case of effective_name
        let subcommand = level
            .subcommands()
            .iter()
            .find(|(name, _)| name.to_kebab_case() == arg);

        if let Some((_, subcommand)) = subcommand {
            self.index += 1;
            let fields = self.parse_subcommand_args(level, subcommand);

            // For flattened tuple variants like `Install(#[facet(flatten)] InstallOptions)`,
            // the fields are already flat (they came from the inner struct's schema).
            // We do NOT wrap them in a "0" key - the deserializer handles the routing.
            // See module-level docs for the ConfigValue model.
            let _ = subcommand.is_flattened_tuple(); // Acknowledge the flag exists but don't use it here

            // Use the effective name for deserialization - facet-format expects
            // the effective name (respecting `#[facet(rename = "...")]`), e.g., "rm" for a
            // variant named `Remove` with `#[facet(rename = "rm")]`.
            let enum_value = ConfigValue::Enum(Sourced {
                value: EnumValue {
                    variant: subcommand.effective_name().to_string(),
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
        parent_level: &'a ArgLevelSchema,
        subcommand: &'a Subcommand,
    ) -> IndexMap<String, ConfigValue, RandomState> {
        // Push current level onto parent stack for adoption agency algorithm.
        // This allows flags not found in the subcommand to bubble up to parent levels.
        let parent = ParentLevel {
            args: parent_level,
            result: core::mem::take(&mut self.result),
            counted: core::mem::take(&mut self.counted),
        };
        self.parent_stack.push(parent);

        // Parse subcommand arguments
        self.parse_level(subcommand.args());
        self.apply_counted_fields();

        // Pop parent level and restore state
        let mut parent = self
            .parent_stack
            .pop()
            .expect("parent stack should not be empty");

        // Any values that were "adopted" by the parent during subcommand parsing
        // are now in parent.result. Restore them to self.result.
        let subcommand_result = core::mem::replace(&mut self.result, parent.result);

        // Also apply any counted fields that bubbled up to the parent
        self.apply_counted_from(&mut parent.counted);

        subcommand_result
    }

    /// Apply counted accumulators from another map (used for adoption agency).
    fn apply_counted_from(
        &mut self,
        counted: &mut IndexMap<String, CountedAccumulator, RandomState>,
    ) {
        for (name, acc) in counted.drain(..) {
            let prov =
                Provenance::cli(format!("-{} (x{})", name, acc.count), acc.count.to_string());
            self.result.insert(
                name,
                ConfigValue::Integer(Sourced {
                    value: acc.count as i64,
                    span: None,
                    provenance: Some(prov),
                }),
            );
        }
    }

    /// Look up a long flag in parent levels (adoption agency algorithm).
    /// Returns owned data to avoid borrow conflicts during mutation.
    fn find_long_flag_in_parents(&self, flag_name: &str) -> Option<ParentFlagLookup> {
        // Search parent stack from innermost (most recent) to outermost
        for (idx, parent) in self.parent_stack.iter().enumerate().rev() {
            if let Some((effective_name, arg_schema)) = parent.args.args().get(flag_name) {
                let is_counted = matches!(arg_schema.kind(), ArgKind::Named { counted: true, .. });
                let is_bool = arg_schema.value().inner_if_option().is_bool();
                return Some(ParentFlagLookup {
                    parent_idx: idx,
                    effective_name: effective_name.to_string(),
                    is_bool,
                    is_counted,
                });
            }
        }
        None
    }

    /// Look up a short flag in parent levels (adoption agency algorithm).
    /// Returns owned data to avoid borrow conflicts during mutation.
    fn find_short_flag_in_parents(&self, ch: char) -> Option<ParentFlagLookup> {
        // Search parent stack from innermost (most recent) to outermost
        for (idx, parent) in self.parent_stack.iter().enumerate().rev() {
            for (name, schema) in parent.args.args().iter() {
                if let ArgKind::Named { short: Some(s), .. } = schema.kind()
                    && *s == ch
                {
                    let is_counted = matches!(schema.kind(), ArgKind::Named { counted: true, .. });
                    let is_bool = schema.value().inner_if_option().is_bool();
                    return Some(ParentFlagLookup {
                        parent_idx: idx,
                        effective_name: name.to_string(),
                        is_bool,
                        is_counted,
                    });
                }
            }
        }
        None
    }

    fn try_parse_positional(&mut self, level: &ArgLevelSchema) -> bool {
        let arg = self.args[self.index];

        // Find the next unset positional argument
        for (name, schema) in level.args() {
            if !matches!(schema.kind(), ArgKind::Positional) {
                continue;
            }

            // Check if already set (unless it's multiple/list)
            if self.has_value(name) && !schema.multiple() {
                continue;
            }

            let value_span = self.current_span();
            let value = self.parse_value_string(arg, arg, value_span);
            self.insert_value(name, value);
            self.index += 1;
            return true;
        }

        false
    }

    fn parse_value_string(
        &self,
        s: &str,
        arg_name: &str,
        span: facet_reflect::Span,
    ) -> ConfigValue {
        let prov = Some(Provenance::cli(arg_name, s));

        // Keep values as strings - type coercion happens during deserialization
        // based on the schema's expected types. This is consistent with the env layer.
        ConfigValue::String(Sourced {
            value: s.to_string(),
            span: Some(span),
            provenance: prov,
        })
    }

    /// Insert a value at the arg's effective name (flat, not nested).
    /// For repeated flags (Vec fields), accumulates values into an array.
    fn insert_value(&mut self, name: &str, value: ConfigValue) {
        self.insert_value_to(InsertTarget::Current, name, value);
    }

    /// Insert a value to a specific target (current level or parent level).
    fn insert_value_to(&mut self, target: InsertTarget, name: &str, value: ConfigValue) {
        let result_map = match target {
            InsertTarget::Current => &mut self.result,
            InsertTarget::Parent(idx) => &mut self.parent_stack[idx].result,
        };

        match result_map.entry(name.to_string()) {
            indexmap::map::Entry::Vacant(entry) => {
                entry.insert(value);
            }
            indexmap::map::Entry::Occupied(mut entry) => {
                // Accumulate into array for repeated flags
                let existing = entry.get_mut();
                if let ConfigValue::Array(arr) = existing {
                    arr.value.push(value);
                } else {
                    // Convert to array with both values
                    let placeholder = ConfigValue::Null(Sourced {
                        value: (),
                        span: None,
                        provenance: None,
                    });
                    let old = core::mem::replace(existing, placeholder);
                    *existing = ConfigValue::Array(Sourced {
                        value: vec![old, value],
                        span: None,
                        provenance: None,
                    });
                }
            }
        }
    }

    /// Check if a value exists at the given name.
    fn has_value(&self, name: &str) -> bool {
        self.result.contains_key(name)
    }

    fn increment_counted(&mut self, name: &str) {
        self.increment_counted_to(InsertTarget::Current, name);
    }

    fn increment_counted_to(&mut self, target: InsertTarget, name: &str) {
        let counted_map = match target {
            InsertTarget::Current => &mut self.counted,
            InsertTarget::Parent(idx) => &mut self.parent_stack[idx].counted,
        };
        let acc = counted_map
            .entry(name.to_string())
            .or_insert_with(|| CountedAccumulator { count: 0 });
        acc.count += 1;
    }

    fn apply_counted_fields(&mut self) {
        for (name, acc) in &self.counted {
            let prov = Provenance::cli(
                format!("-{}", name.chars().next().unwrap_or('?')),
                acc.count.to_string(),
            );
            let value = ConfigValue::Integer(Sourced {
                value: acc.count as i64,
                span: None,
                provenance: Some(prov),
            });
            // Insert directly at the field name (flat)
            self.result.insert(name.clone(), value);
        }
    }

    /// Emit an error diagnostic for the current argument.
    fn emit_error(&mut self, message: String) {
        self.emit_error_at(message, self.current_span());
    }

    /// Emit an error diagnostic at a specific span.
    fn emit_error_at(&mut self, message: String, span: facet_reflect::Span) {
        self.diagnostics.push(Diagnostic {
            message,
            path: None,
            span: Some(crate::span::Span::new(span.offset, span.len)),
            severity: Severity::Error,
        });
    }

    fn into_output(mut self) -> LayerOutput {
        // Merge config builder output if present
        let mut unused_keys = Vec::new();
        let mut diagnostics = self.diagnostics;

        if let Some(builder) = self.config_builder
            && let Some(config_schema) = self.schema.config()
        {
            let config_field_name = config_schema.field_name();
            let builder_output = builder.into_output(None);

            // Merge builder's value into result under the config field name
            if let Some(config_value) = builder_output.value
                && let Some(name) = config_field_name
            {
                self.result.insert(name.to_string(), config_value);
            }

            unused_keys.extend(builder_output.unused_keys);
            diagnostics.extend(builder_output.diagnostics);
        }

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
            unused_keys,
            diagnostics,
            source_text: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as args;
    use facet::Facet;
    use facet_testhelpers::test;

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

    /// Build a CliConfig from a slice of string slices (for tests)
    fn cli_config(args: &[&str]) -> CliConfig {
        CliConfigBuilder::new()
            .args(args.iter().map(|s| s.to_string()))
            .build()
    }

    /// Assert that parsing produces the expected ConfigValue
    fn assert_parses_to<T: Facet<'static>>(args: &[&str], expected: ConfigValue) {
        let schema = Schema::from_shape(T::SHAPE).expect("valid schema");
        tracing::debug!(args = ?schema.args().args().keys().collect::<Vec<_>>(), "Schema args");
        let config = cli_config(args);
        let output = parse_cli(&schema, &config);

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
        let config = cli_config(args);
        let output = parse_cli(&schema, &config);

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
                let mut actual_keys: Vec<_> = a.value.keys().collect();
                let mut expected_keys: Vec<_> = e.value.keys().collect();
                actual_keys.sort();
                expected_keys.sort();
                assert_eq!(actual_keys, expected_keys, "object keys mismatch");
                for (key, expected_val) in &e.value {
                    let actual_val = a
                        .value
                        .get(key)
                        .unwrap_or_else(|| panic!("missing key: {}", key));
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
                // Sort keys for order-independent comparison
                let mut actual_keys: Vec<_> = a.value.fields.keys().collect();
                let mut expected_keys: Vec<_> = e.value.fields.keys().collect();
                actual_keys.sort();
                expected_keys.sort();
                assert_eq!(actual_keys, expected_keys, "enum fields mismatch");
                for (key, expected_val) in &e.value.fields {
                    let actual_val = a
                        .value
                        .fields
                        .get(key)
                        .unwrap_or_else(|| panic!("missing enum field: {}", key));
                    assert_config_value_eq(actual_val, expected_val);
                }
            }
            (ConfigValue::Null(_), ConfigValue::Null(_)) => {}
            _ => panic!(
                "ConfigValue variant mismatch: {:?} vs {:?}",
                core::mem::discriminant(actual),
                core::mem::discriminant(expected)
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
    #[allow(dead_code)]
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
            cv::object([("port", cv::string("8080", "--port"))]),
        );
    }

    #[test]
    fn test_long_flag_with_equals() {
        assert_parses_to::<SimpleArgs>(
            &["--port=8080"],
            cv::object([("port", cv::string("8080", "--port"))]),
        );
    }

    #[test]
    fn test_short_flag_with_value() {
        assert_parses_to::<SimpleArgs>(
            &["-p", "8080"],
            cv::object([("port", cv::string("8080", "-p"))]),
        );
    }

    #[test]
    fn test_short_flag_attached_value() {
        // -p8080 (no space)
        assert_parses_to::<SimpleArgs>(
            &["-p8080"],
            cv::object([("port", cv::string("8080", "-p"))]),
        );
    }

    #[test]
    fn test_multiple_flags() {
        assert_parses_to::<SimpleArgs>(
            &["--verbose", "--port", "8080"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                ("port", cv::string("8080", "--port")),
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
                cv::object([("port", cv::string("8080", "--config.port"))]),
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
                    cv::object([("port", cv::string("8080", "--config.port"))]),
                ),
            ]),
        );
    }

    // ========================================================================
    // Tests: Config overrides with flattened config
    // ========================================================================

    /// Common configuration shared across services
    #[derive(Facet)]
    struct CommonConfigCli {
        log_level: String,
        debug: bool,
    }

    /// Server config with flattened common settings
    #[derive(Facet)]
    struct ServerConfigWithFlattenCli {
        port: u16,
        #[facet(flatten)]
        common: CommonConfigCli,
    }

    #[derive(Facet)]
    struct ArgsWithFlattenedConfigCli {
        #[facet(args::named)]
        verbose: bool,

        #[facet(args::config)]
        config: ServerConfigWithFlattenCli,
    }

    #[test]
    fn test_config_override_with_flattened_struct() {
        // Flattened fields are accessed directly without the wrapper field name.
        // Since `common` is flattened, `log_level` appears at the top level of the config.
        assert_parses_to::<ArgsWithFlattenedConfigCli>(
            &["--config.log_level", "debug"],
            cv::object([(
                "config",
                cv::object([("log_level", cv::string("debug", "--config.log_level"))]),
            )]),
        );
    }

    #[test]
    fn test_config_override_flattened_and_regular_fields() {
        // Mix of regular (port) and flattened (debug) fields.
        // Since `common` is flattened, its fields appear directly in the config.
        assert_parses_to::<ArgsWithFlattenedConfigCli>(
            &["--config.port", "8080", "--config.debug", "true"],
            cv::object([(
                "config",
                cv::object([
                    ("port", cv::string("8080", "--config.port")),
                    ("debug", cv::string("true", "--config.debug")),
                ]),
            )]),
        );
    }

    // ========================================================================
    // Tests: Subcommands
    // ========================================================================

    #[test]
    fn test_subcommand_basic() {
        // Tuple variant with no args passed -> empty fields
        // (the inner struct's fields have defaults, handled by deserializer)
        assert_parses_to::<ArgsWithSubcommand>(
            &["build"],
            cv::object([("command", cv::enumv("Build", []))]),
        );
    }

    #[test]
    fn test_subcommand_with_args() {
        // Tuple variant's inner struct fields appear directly in the variant
        // (no "0" wrapper - the deserializer handles routing to tuple position)
        assert_parses_to::<ArgsWithSubcommand>(
            &["build", "--release"],
            cv::object([(
                "command",
                cv::enumv("Build", [("release", cv::bool(true, "--release"))]),
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
                    // Variant name is PascalCase for facet-format deserialization
                    // Flattened tuple variants have their fields directly (not wrapped in "0")
                    // because the CLI layer emits flat ConfigValue - the deserializer handles routing
                    cv::enumv("Build", [("release", cv::bool(true, "--release"))]),
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

    // ========================================================================
    // Flatten tests
    // ========================================================================

    /// Common arguments that can be flattened into other structs
    #[derive(Facet)]
    struct CommonArgs {
        /// Enable verbose output
        #[facet(args::named, args::short = 'v')]
        verbose: bool,

        /// Enable quiet mode
        #[facet(args::named, args::short = 'q')]
        quiet: bool,
    }

    /// Args struct with flattened common args
    #[derive(Facet)]
    struct ArgsWithFlatten {
        /// Input file
        #[facet(args::positional)]
        input: String,

        /// Common options
        #[facet(flatten)]
        common: CommonArgs,
    }

    #[test]
    fn test_flatten_parses_flags_flat() {
        // Flattened fields appear at the CURRENT level, not nested
        let expected = cv::object([
            ("input", cv::string("file.txt", "file.txt")),
            ("verbose", cv::bool(true, "-v")),
        ]);
        assert_parses_to::<ArgsWithFlatten>(&["-v", "file.txt"], expected);
    }

    #[test]
    fn test_flatten_multiple_flags() {
        // Multiple flattened flags all appear at current level
        let expected = cv::object([
            ("input", cv::string("test.txt", "test.txt")),
            ("verbose", cv::bool(true, "-v")),
            ("quiet", cv::bool(true, "-q")),
        ]);
        assert_parses_to::<ArgsWithFlatten>(&["-v", "-q", "test.txt"], expected);
    }

    #[test]
    fn test_flatten_long_flags() {
        let expected = cv::object([
            ("input", cv::string("data.txt", "data.txt")),
            ("verbose", cv::bool(true, "--verbose")),
        ]);
        assert_parses_to::<ArgsWithFlatten>(&["--verbose", "data.txt"], expected);
    }

    /// Nested flatten - common inside options inside top-level
    #[derive(Facet)]
    struct NestedCommon {
        /// Enable debug mode
        #[facet(args::named)]
        debug: bool,
    }

    #[derive(Facet)]
    struct MiddleLayer {
        /// Nested common args
        #[facet(flatten)]
        nested: NestedCommon,

        /// Log level
        #[facet(args::named)]
        log_level: Option<String>,
    }

    #[derive(Facet)]
    struct DeepFlatten {
        /// Input path
        #[facet(args::positional)]
        path: String,

        /// Middle layer options
        #[facet(flatten)]
        options: MiddleLayer,
    }

    #[test]
    fn test_deeply_nested_flatten() {
        // Two levels of flatten: options is flattened, and options.nested is flattened
        // So "debug" appears at the TOP level, not nested
        let expected = cv::object([
            ("path", cv::string("file.txt", "file.txt")),
            ("debug", cv::bool(true, "--debug")),
        ]);
        assert_parses_to::<DeepFlatten>(&["--debug", "file.txt"], expected);
    }

    // ========================================================================
    // Tests: ConfigValue structure expectations
    // ========================================================================
    //
    // These tests encode our expectations about what ConfigValue structure
    // the CLI layer produces. The deserializer then routes these values
    // to the correct locations based on the type's Shape.
    //
    // KEY PRINCIPLE: The CLI layer produces ConfigValue that matches the
    // SCHEMA structure, NOT the Rust struct layout. For flattened fields,
    // the values appear at the current level. For non-flattened nested
    // structs, values are nested under the field name.

    // ------------------------------------------------------------------------
    // Simple top-level flags produce flat structure
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct SimpleFlags {
        #[facet(args::named)]
        verbose: bool,
        #[facet(args::named)]
        quiet: bool,
    }

    #[test]
    fn test_cv_simple_flags_flat() {
        // Simple flags at top level -> flat ConfigValue
        assert_parses_to::<SimpleFlags>(
            &["--verbose"],
            cv::object([("verbose", cv::bool(true, "--verbose"))]),
        );
    }

    #[test]
    fn test_cv_multiple_simple_flags_flat() {
        assert_parses_to::<SimpleFlags>(
            &["--verbose", "--quiet"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                ("quiet", cv::bool(true, "--quiet")),
            ]),
        );
    }

    // ------------------------------------------------------------------------
    // Flattened struct fields appear at CURRENT level
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct InnerOpts {
        #[facet(args::named)]
        debug: bool,
        #[facet(args::named)]
        trace: bool,
    }

    #[derive(Facet)]
    struct OuterWithFlatten {
        #[facet(args::named)]
        verbose: bool,
        #[facet(flatten)]
        inner: InnerOpts,
    }

    #[test]
    fn test_cv_flattened_fields_at_current_level() {
        // --debug comes from flattened InnerOpts
        // It should appear at top level, NOT nested under "inner"
        assert_parses_to::<OuterWithFlatten>(
            &["--debug"],
            cv::object([("debug", cv::bool(true, "--debug"))]),
        );
    }

    #[test]
    fn test_cv_flattened_mixed_with_outer() {
        // Mix of outer field (--verbose) and flattened field (--trace)
        assert_parses_to::<OuterWithFlatten>(
            &["--verbose", "--trace"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                ("trace", cv::bool(true, "--trace")),
            ]),
        );
    }

    // ------------------------------------------------------------------------
    // Non-flattened nested struct via config produces nested structure
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct DatabaseConfig {
        host: String,
        port: u16,
    }

    #[derive(Facet)]
    struct AppWithConfig {
        #[facet(args::named)]
        verbose: bool,
        #[facet(args::config)]
        db: Option<DatabaseConfig>,
    }

    #[test]
    fn test_cv_config_override_nested() {
        // --db.host=localhost should produce nested structure
        assert_parses_to::<AppWithConfig>(
            &["--db.host", "localhost"],
            cv::object([(
                "db",
                cv::object([("host", cv::string("localhost", "--db.host"))]),
            )]),
        );
    }

    #[test]
    fn test_cv_config_override_deeply_nested() {
        // Multiple levels of nesting
        assert_parses_to::<AppWithConfig>(
            &["--db.host", "localhost", "--db.port", "5432"],
            cv::object([(
                "db",
                cv::object([
                    ("host", cv::string("localhost", "--db.host")),
                    ("port", cv::string("5432", "--db.port")),
                ]),
            )]),
        );
    }

    // ------------------------------------------------------------------------
    // Subcommand fields are nested under the variant
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct CvBuildOpts {
        #[facet(args::named)]
        release: bool,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum CvCommand {
        Build {
            #[facet(flatten)]
            opts: CvBuildOpts,
        },
        Test {
            #[facet(args::named)]
            coverage: bool,
        },
    }

    #[derive(Facet)]
    struct AppWithSubcommand {
        #[facet(args::named)]
        verbose: bool,
        #[facet(args::subcommand)]
        cmd: CvCommand,
    }

    #[test]
    fn test_cv_subcommand_enum_structure() {
        // Subcommand produces Enum variant with fields
        // --release is from flattened CvBuildOpts, appears directly in variant fields
        assert_parses_to::<AppWithSubcommand>(
            &["build", "--release"],
            cv::object([(
                "cmd",
                cv::enumv("Build", [("release", cv::bool(true, "--release"))]),
            )]),
        );
    }

    #[test]
    fn test_cv_subcommand_with_outer_flag() {
        // Outer flag + subcommand
        assert_parses_to::<AppWithSubcommand>(
            &["--verbose", "test", "--coverage"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                (
                    "cmd",
                    cv::enumv("Test", [("coverage", cv::bool(true, "--coverage"))]),
                ),
            ]),
        );
    }

    // ------------------------------------------------------------------------
    // Deeply nested flatten (flatten inside flatten)
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct Level3 {
        #[facet(args::named)]
        deep_flag: bool,
    }

    #[derive(Facet)]
    struct Level2 {
        #[facet(flatten)]
        level3: Level3,
    }

    #[derive(Facet)]
    struct Level1 {
        #[facet(flatten)]
        level2: Level2,
    }

    #[test]
    fn test_cv_deeply_nested_flatten_still_flat() {
        // Even with flatten inside flatten, the field appears at top level
        assert_parses_to::<Level1>(
            &["--deep-flag"],
            cv::object([("deep_flag", cv::bool(true, "--deep-flag"))]),
        );
    }

    // ------------------------------------------------------------------------
    // Renamed fields use effective name in ConfigValue
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct RenamedFields {
        #[facet(args::named, rename = "output-dir")]
        output: String,
    }

    #[test]
    fn test_cv_renamed_field_uses_effective_name() {
        // The CLI flag is --output-dir, and the ConfigValue key is "output-dir"
        assert_parses_to::<RenamedFields>(
            &["--output-dir", "/tmp"],
            cv::object([("output-dir", cv::string("/tmp", "--output-dir"))]),
        );
    }

    // ------------------------------------------------------------------------
    // Config with flattened inner struct
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct CvLoggingConfig {
        level: String,
    }

    #[derive(Facet)]
    struct CvServerConfig {
        port: u16,
        #[facet(flatten)]
        logging: CvLoggingConfig,
    }

    #[derive(Facet)]
    struct AppWithFlattenedConfig {
        #[facet(args::config)]
        server: Option<CvServerConfig>,
    }

    #[test]
    fn test_cv_config_with_flattened_inner() {
        // --server.level should work because logging is flattened in CvServerConfig
        // The path in ConfigValue is server.level (not server.logging.level)
        assert_parses_to::<AppWithFlattenedConfig>(
            &["--server.level", "debug"],
            cv::object([(
                "server",
                cv::object([("level", cv::string("debug", "--server.level"))]),
            )]),
        );
    }

    #[test]
    fn test_cv_config_non_flattened_field() {
        // --server.port is a direct field
        assert_parses_to::<AppWithFlattenedConfig>(
            &["--server.port", "8080"],
            cv::object([(
                "server",
                cv::object([("port", cv::string("8080", "--server.port"))]),
            )]),
        );
    }

    // ------------------------------------------------------------------------
    // Mix of everything
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct GlobalOpts {
        #[facet(args::named, args::short = 'v')]
        verbose: bool,
    }

    #[derive(Facet)]
    struct CvComplexApp {
        #[facet(flatten)]
        global: GlobalOpts,
        #[facet(args::config)]
        config: Option<CvServerConfig>,
        #[facet(args::subcommand)]
        command: Option<CvCommand>,
    }

    #[test]
    fn test_cv_complex_mix() {
        // Global flag (flattened) + config override + subcommand
        assert_parses_to::<CvComplexApp>(
            &["-v", "--config.port", "9000", "build", "--release"],
            cv::object([
                ("verbose", cv::bool(true, "-v")),
                (
                    "config",
                    cv::object([("port", cv::string("9000", "--config.port"))]),
                ),
                (
                    "command",
                    cv::enumv("Build", [("release", cv::bool(true, "--release"))]),
                ),
            ]),
        );
    }

    // ========================================================================
    // Tests: Subcommand nesting behavior
    // ========================================================================
    //
    // Subcommands create REAL nesting - the variant's fields are inside the
    // enum value, not at the top level. This is different from flatten which
    // hoists fields UP.

    // ------------------------------------------------------------------------
    // Subcommand with flattened struct field
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct SubBuildConfig {
        #[facet(args::named)]
        jobs: Option<u32>,
        #[facet(args::named)]
        target: Option<String>,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SubCommand1 {
        Build {
            #[facet(flatten)]
            config: SubBuildConfig,
        },
    }

    #[derive(Facet)]
    struct AppWithSubcommandFlattenedField {
        #[facet(args::subcommand)]
        cmd: SubCommand1,
    }

    #[test]
    fn test_cv_subcommand_flattened_struct_field() {
        // When a subcommand has a flattened struct field,
        // the struct's fields appear directly in the variant (flat)
        assert_parses_to::<AppWithSubcommandFlattenedField>(
            &["build", "--jobs", "4"],
            cv::object([(
                "cmd",
                cv::enumv("Build", [("jobs", cv::string("4", "--jobs"))]),
            )]),
        );
    }

    // ------------------------------------------------------------------------
    // Nested subcommands (subcommand within subcommand)
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum DbSubCommand {
        Create {
            #[facet(args::positional)]
            name: String,
        },
        Drop {
            #[facet(args::positional)]
            name: String,
            #[facet(args::named)]
            force: bool,
        },
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum TopCommand {
        Db {
            #[facet(args::subcommand)]
            action: DbSubCommand,
        },
        Version,
    }

    #[derive(Facet)]
    struct AppWithNestedSubcommands {
        #[facet(args::subcommand)]
        cmd: TopCommand,
    }

    #[test]
    fn test_cv_nested_subcommands() {
        // db create mydb -> nested enums
        assert_parses_to::<AppWithNestedSubcommands>(
            &["db", "create", "mydb"],
            cv::object([(
                "cmd",
                cv::enumv(
                    "Db",
                    [(
                        "action",
                        cv::enumv("Create", [("name", cv::string("mydb", "mydb"))]),
                    )],
                ),
            )]),
        );
    }

    #[test]
    fn test_cv_nested_subcommands_with_flags() {
        // db drop mydb --force
        assert_parses_to::<AppWithNestedSubcommands>(
            &["db", "drop", "mydb", "--force"],
            cv::object([(
                "cmd",
                cv::enumv(
                    "Db",
                    [(
                        "action",
                        cv::enumv(
                            "Drop",
                            [
                                ("name", cv::string("mydb", "mydb")),
                                ("force", cv::bool(true, "--force")),
                            ],
                        ),
                    )],
                ),
            )]),
        );
    }

    // ------------------------------------------------------------------------
    // Subcommand with tuple variant (single flattened field)
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct InstallOpts {
        #[facet(args::named)]
        global: bool,
        #[facet(args::positional)]
        package: String,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum PkgCommand {
        Install(#[facet(flatten)] InstallOpts),
        Uninstall {
            #[facet(args::positional)]
            package: String,
        },
    }

    #[derive(Facet)]
    struct AppWithTupleVariant {
        #[facet(args::subcommand)]
        cmd: PkgCommand,
    }

    #[test]
    fn test_cv_tuple_variant_flattened() {
        // Tuple variant with flatten - fields appear directly in variant
        // NOT wrapped in "0"
        assert_parses_to::<AppWithTupleVariant>(
            &["install", "--global", "lodash"],
            cv::object([(
                "cmd",
                cv::enumv(
                    "Install",
                    [
                        ("global", cv::bool(true, "--global")),
                        ("package", cv::string("lodash", "lodash")),
                    ],
                ),
            )]),
        );
    }

    #[test]
    fn test_cv_struct_variant_after_tuple() {
        // Struct variant in same enum as tuple variant
        assert_parses_to::<AppWithTupleVariant>(
            &["uninstall", "lodash"],
            cv::object([(
                "cmd",
                cv::enumv("Uninstall", [("package", cv::string("lodash", "lodash"))]),
            )]),
        );
    }

    // ------------------------------------------------------------------------
    // Subcommand with flatten inside flatten
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct DeepOpts {
        #[facet(args::named)]
        deep: bool,
    }

    #[derive(Facet)]
    struct MiddleOpts {
        #[facet(flatten)]
        deep_opts: DeepOpts,
        #[facet(args::named)]
        middle: bool,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum DeepFlattenCmd {
        Run {
            #[facet(flatten)]
            opts: MiddleOpts,
            #[facet(args::positional)]
            target: String,
        },
    }

    #[derive(Facet)]
    struct AppWithDeepFlattenSubcmd {
        #[facet(args::subcommand)]
        cmd: DeepFlattenCmd,
    }

    #[test]
    fn test_cv_subcommand_deep_flatten() {
        // flatten inside flatten inside subcommand
        // All flattened fields appear directly in variant fields
        assert_parses_to::<AppWithDeepFlattenSubcmd>(
            &["run", "--deep", "--middle", "mytarget"],
            cv::object([(
                "cmd",
                cv::enumv(
                    "Run",
                    [
                        ("deep", cv::bool(true, "--deep")),
                        ("middle", cv::bool(true, "--middle")),
                        ("target", cv::string("mytarget", "mytarget")),
                    ],
                ),
            )]),
        );
    }

    // ------------------------------------------------------------------------
    // Unit variant subcommand
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum SimpleCmd {
        Start,
        Stop,
        Status {
            #[facet(args::named)]
            verbose: bool,
        },
    }

    #[derive(Facet)]
    struct AppWithUnitVariant {
        #[facet(args::subcommand)]
        cmd: SimpleCmd,
    }

    #[test]
    fn test_cv_unit_variant_subcommand() {
        // Unit variant - no fields
        assert_parses_to::<AppWithUnitVariant>(
            &["start"],
            cv::object([("cmd", cv::enumv("Start", []))]),
        );
    }

    #[test]
    fn test_cv_unit_variant_then_struct_variant() {
        // Struct variant in same enum
        assert_parses_to::<AppWithUnitVariant>(
            &["status", "--verbose"],
            cv::object([(
                "cmd",
                cv::enumv("Status", [("verbose", cv::bool(true, "--verbose"))]),
            )]),
        );
    }

    // ------------------------------------------------------------------------
    // Renamed subcommand variant
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum RenamedCmd {
        #[facet(rename = "ls")]
        List {
            #[facet(args::named)]
            all: bool,
        },
        #[facet(rename = "rm")]
        Remove {
            #[facet(args::positional)]
            path: String,
        },
    }

    #[derive(Facet)]
    struct AppWithRenamedVariants {
        #[facet(args::subcommand)]
        cmd: RenamedCmd,
    }

    #[test]
    fn test_cv_renamed_subcommand_variant() {
        // CLI uses "ls", ConfigValue uses effective_name "ls"
        assert_parses_to::<AppWithRenamedVariants>(
            &["ls", "--all"],
            cv::object([("cmd", cv::enumv("ls", [("all", cv::bool(true, "--all"))]))]),
        );
    }

    #[test]
    fn test_cv_another_renamed_variant() {
        assert_parses_to::<AppWithRenamedVariants>(
            &["rm", "/tmp/foo"],
            cv::object([(
                "cmd",
                cv::enumv("rm", [("path", cv::string("/tmp/foo", "/tmp/foo"))]),
            )]),
        );
    }

    // ------------------------------------------------------------------------
    // Global flags before subcommand
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    struct GlobalFlags {
        #[facet(args::named, args::short = 'v')]
        verbose: bool,
        #[facet(args::named, args::short = 'q')]
        quiet: bool,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum ActionCmd {
        Run {
            #[facet(args::positional)]
            script: String,
        },
    }

    #[derive(Facet)]
    struct AppWithGlobalFlags {
        #[facet(flatten)]
        globals: GlobalFlags,
        #[facet(args::subcommand)]
        action: ActionCmd,
    }

    #[test]
    fn test_cv_global_flags_before_subcommand() {
        // Global flags (flattened) appear at top level
        // Subcommand fields are nested in enum
        assert_parses_to::<AppWithGlobalFlags>(
            &["-v", "run", "script.sh"],
            cv::object([
                ("verbose", cv::bool(true, "-v")),
                (
                    "action",
                    cv::enumv("Run", [("script", cv::string("script.sh", "script.sh"))]),
                ),
            ]),
        );
    }

    #[test]
    fn test_cv_multiple_global_flags() {
        assert_parses_to::<AppWithGlobalFlags>(
            &["-v", "-q", "run", "test.sh"],
            cv::object([
                ("verbose", cv::bool(true, "-v")),
                ("quiet", cv::bool(true, "-q")),
                (
                    "action",
                    cv::enumv("Run", [("script", cv::string("test.sh", "test.sh"))]),
                ),
            ]),
        );
    }

    // ------------------------------------------------------------------------
    // Optional subcommand
    // ------------------------------------------------------------------------
    #[derive(Facet)]
    #[repr(u8)]
    enum OptionalCmd {
        Init,
        Build,
    }

    #[derive(Facet)]
    struct AppWithOptionalSubcommand {
        #[facet(args::named)]
        version: bool,
        #[facet(args::subcommand)]
        cmd: Option<OptionalCmd>,
    }

    #[test]
    fn test_cv_optional_subcommand_present() {
        assert_parses_to::<AppWithOptionalSubcommand>(
            &["init"],
            cv::object([("cmd", cv::enumv("Init", []))]),
        );
    }

    #[test]
    fn test_cv_optional_subcommand_absent() {
        // No subcommand - just flags
        assert_parses_to::<AppWithOptionalSubcommand>(
            &["--version"],
            cv::object([("version", cv::bool(true, "--version"))]),
        );
    }

    // ------------------------------------------------------------------------
    // Adoption agency: flags bubbling up to parent levels
    // ------------------------------------------------------------------------
    // When a flag like `--help` or `--verbose` is specified after a subcommand,
    // but doesn't exist in the subcommand's schema, it should "bubble up" to
    // the parent level if it exists there.

    #[derive(Facet)]
    struct ParentWithGlobalFlag {
        /// Global verbose flag at parent level
        #[facet(args::named, args::short = 'v')]
        verbose: bool,

        #[facet(args::subcommand)]
        command: ChildCommand,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum ChildCommand {
        /// Build subcommand - no verbose flag here
        Build {
            #[facet(args::named)]
            release: bool,
        },
        /// Test subcommand - no verbose flag here
        Test {
            #[facet(args::positional)]
            filter: Option<String>,
        },
    }

    #[test]
    fn test_adoption_flag_after_subcommand_bubbles_up() {
        // `myapp build --verbose` - verbose doesn't exist in Build, should bubble to parent
        assert_parses_to::<ParentWithGlobalFlag>(
            &["build", "--verbose"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                ("command", cv::enumv("Build", [])),
            ]),
        );
    }

    #[test]
    fn test_adoption_short_flag_after_subcommand_bubbles_up() {
        // `myapp build -v` - short flag should also bubble up
        assert_parses_to::<ParentWithGlobalFlag>(
            &["build", "-v"],
            cv::object([
                ("verbose", cv::bool(true, "-v")),
                ("command", cv::enumv("Build", [])),
            ]),
        );
    }

    #[test]
    fn test_adoption_mixed_flags_some_bubble() {
        // `myapp build --release --verbose` - release stays in subcommand, verbose bubbles
        assert_parses_to::<ParentWithGlobalFlag>(
            &["build", "--release", "--verbose"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                (
                    "command",
                    cv::enumv("Build", [("release", cv::bool(true, "--release"))]),
                ),
            ]),
        );
    }

    #[test]
    fn test_adoption_flag_before_subcommand_stays_at_parent() {
        // `myapp --verbose build` - verbose is at parent level, should stay there
        assert_parses_to::<ParentWithGlobalFlag>(
            &["--verbose", "build"],
            cv::object([
                ("verbose", cv::bool(true, "--verbose")),
                ("command", cv::enumv("Build", [])),
            ]),
        );
    }

    // Nested subcommands with adoption
    #[derive(Facet)]
    struct GrandparentWithFlag {
        /// Global flag at grandparent level
        #[facet(args::named)]
        debug: bool,

        #[facet(args::subcommand)]
        command: ParentCommand,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum ParentCommand {
        Repo {
            /// Parent-level flag
            #[facet(args::named)]
            quiet: bool,

            #[facet(args::subcommand)]
            action: RepoAction,
        },
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum RepoAction {
        Clone {
            #[facet(args::positional)]
            url: String,
        },
        Push {
            #[facet(args::named)]
            force: bool,
        },
    }

    #[test]
    fn test_adoption_nested_bubbles_to_immediate_parent() {
        // `myapp repo clone https://... --quiet` - quiet should bubble to repo level
        assert_parses_to::<GrandparentWithFlag>(
            &["repo", "clone", "https://example.com", "--quiet"],
            cv::object([(
                "command",
                cv::enumv(
                    "Repo",
                    [
                        ("quiet", cv::bool(true, "--quiet")),
                        (
                            "action",
                            cv::enumv(
                                "Clone",
                                [(
                                    "url",
                                    cv::string("https://example.com", "https://example.com"),
                                )],
                            ),
                        ),
                    ],
                ),
            )]),
        );
    }

    #[test]
    fn test_adoption_nested_bubbles_to_grandparent() {
        // `myapp repo clone https://... --debug` - debug should bubble all the way to grandparent
        assert_parses_to::<GrandparentWithFlag>(
            &["repo", "clone", "https://example.com", "--debug"],
            cv::object([
                ("debug", cv::bool(true, "--debug")),
                (
                    "command",
                    cv::enumv(
                        "Repo",
                        [(
                            "action",
                            cv::enumv(
                                "Clone",
                                [(
                                    "url",
                                    cv::string("https://example.com", "https://example.com"),
                                )],
                            ),
                        )],
                    ),
                ),
            ]),
        );
    }

    #[test]
    fn test_adoption_nested_mixed_levels() {
        // `myapp repo push --force --quiet --debug`
        // force stays in Push, quiet bubbles to Repo, debug bubbles to grandparent
        assert_parses_to::<GrandparentWithFlag>(
            &["repo", "push", "--force", "--quiet", "--debug"],
            cv::object([
                ("debug", cv::bool(true, "--debug")),
                (
                    "command",
                    cv::enumv(
                        "Repo",
                        [
                            ("quiet", cv::bool(true, "--quiet")),
                            (
                                "action",
                                cv::enumv("Push", [("force", cv::bool(true, "--force"))]),
                            ),
                        ],
                    ),
                ),
            ]),
        );
    }

    // Test that local flags shadow parent flags (no adoption when flag exists locally)
    #[derive(Facet)]
    struct ParentWithShadowedFlag {
        #[facet(args::named, args::short = 'v')]
        verbose: bool,

        #[facet(args::subcommand)]
        command: ChildWithSameFlag,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum ChildWithSameFlag {
        Run {
            /// Local verbose flag - shadows parent
            #[facet(args::named, args::short = 'v')]
            verbose: bool,

            #[facet(args::positional)]
            script: String,
        },
    }

    #[test]
    fn test_adoption_local_flag_shadows_parent() {
        // `myapp run script.sh --verbose` - verbose exists in Run, should NOT bubble
        assert_parses_to::<ParentWithShadowedFlag>(
            &["run", "script.sh", "--verbose"],
            cv::object([(
                "command",
                cv::enumv(
                    "Run",
                    [
                        ("verbose", cv::bool(true, "--verbose")),
                        ("script", cv::string("script.sh", "script.sh")),
                    ],
                ),
            )]),
        );
    }

    #[test]
    fn test_adoption_short_flag_shadows_parent() {
        // `myapp run script.sh -v` - short flag also shadows
        assert_parses_to::<ParentWithShadowedFlag>(
            &["run", "script.sh", "-v"],
            cv::object([(
                "command",
                cv::enumv(
                    "Run",
                    [
                        ("verbose", cv::bool(true, "-v")),
                        ("script", cv::string("script.sh", "script.sh")),
                    ],
                ),
            )]),
        );
    }

    // Test with FigueBuiltins (realistic scenario)
    #[derive(Facet)]
    struct AppWithBuiltins {
        #[facet(flatten)]
        builtins: crate::FigueBuiltins,

        #[facet(args::subcommand)]
        command: AppCommand,
    }

    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum AppCommand {
        Install {
            #[facet(args::positional)]
            package: String,
        },
        Uninstall {
            #[facet(args::positional)]
            package: String,
            #[facet(args::named)]
            force: bool,
        },
    }

    #[test]
    fn test_adoption_help_flag_bubbles_from_subcommand() {
        // `myapp install foo --help` - help doesn't exist in Install, bubbles to builtins
        assert_parses_to::<AppWithBuiltins>(
            &["install", "foo", "--help"],
            cv::object([
                ("help", cv::bool(true, "--help")),
                (
                    "command",
                    cv::enumv("Install", [("package", cv::string("foo", "foo"))]),
                ),
            ]),
        );
    }

    #[test]
    fn test_adoption_version_flag_bubbles_from_subcommand() {
        // `myapp uninstall bar --version` - version bubbles to builtins
        assert_parses_to::<AppWithBuiltins>(
            &["uninstall", "bar", "--version"],
            cv::object([
                ("version", cv::bool(true, "--version")),
                (
                    "command",
                    cv::enumv("Uninstall", [("package", cv::string("bar", "bar"))]),
                ),
            ]),
        );
    }

    #[test]
    fn test_adoption_help_short_flag_bubbles() {
        // `myapp install foo -h` - short help flag bubbles
        assert_parses_to::<AppWithBuiltins>(
            &["install", "foo", "-h"],
            cv::object([
                ("help", cv::bool(true, "-h")),
                (
                    "command",
                    cv::enumv("Install", [("package", cv::string("foo", "foo"))]),
                ),
            ]),
        );
    }
}
