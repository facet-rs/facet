//! Driver API for orchestrating layered configuration parsing, validation, and diagnostics.
//!
//! # Phases
//! 1. **Parse layers**: CLI, env, file (defaults filled during deserialization)
//! 2. **Check special fields**: If help/version/completions was requested, short-circuit
//! 3. **Merge** layers by priority (CLI > env > file > defaults)
//! 4. **Deserialize** merged ConfigValue into the target Facet type
//!
//! # TODO
//! - [x] Wire override tracking from merge result into DriverReport
//! - [x] Define DriverError enum (Failed, Help, Completions, Version)
//! - [x] Implement unwrap() on DriverResult
//! - [x] Add figue::help, figue::completions, figue::version attribute detection
//! - [x] Handle special fields in Driver::run() before deserialization
//! - [ ] Collect unused keys from layer parsers into LayerOutput
//! - [ ] Add facet-validate pass after deserialization
//! - [ ] Improve render_pretty() with Ariadne integration
//! - [x] Migrate build_traced tests to driver API (removed - functionality covered by driver tests)
#![allow(clippy::result_large_err)]

use std::marker::PhantomData;
use std::string::String;
use std::vec::Vec;

use crate::builder::Config;
use crate::completions::{Shell, generate_completions_for_shape};
use crate::config_value::ConfigValue;
use crate::config_value_parser::{fill_defaults_from_schema, from_config_value};
use crate::dump::dump_config_with_schema;
use crate::enum_conflicts::detect_enum_conflicts;
use crate::env_subst::{EnvSubstError, RealEnv, substitute_env_vars};
use crate::help::generate_help_for_subcommand;
use crate::layers::{cli::parse_cli, env::parse_env, file::parse_file};
use crate::merge::merge_layers;
use crate::missing::{collect_missing_fields, format_missing_fields_summary};
use crate::path::Path;
use crate::provenance::{FileResolution, Override, Provenance};
use crate::span::Span;
use crate::span_registry::assign_virtual_spans;
use facet_core::Facet;

/// Diagnostics for a single layer.
#[derive(Debug, Default)]
pub struct LayerOutput {
    /// Parsed value for this layer (if any).
    pub value: Option<ConfigValue>,
    /// Keys provided by this layer but unused by the schema.
    pub unused_keys: Vec<UnusedKey>,
    /// Layer-specific diagnostics collected while parsing.
    pub diagnostics: Vec<Diagnostic>,
    /// Virtual source text for this layer (for error reporting with Ariadne).
    /// For env layers, this is a synthetic document like `VAR="value"\n`.
    /// For file layers, this is the file contents.
    /// For CLI, this is the concatenated args.
    pub source_text: Option<String>,
}

/// A key that was unused by the schema, with provenance.
#[derive(Debug)]
pub struct UnusedKey {
    /// The unused key path.
    pub key: Path,
    /// Provenance for where it came from (CLI/env/file/default).
    pub provenance: Provenance,
}

/// Layered config values from CLI/env/file/defaults, with diagnostics.
#[derive(Debug, Default)]
pub struct ConfigLayers {
    /// Default layer (lowest priority).
    pub defaults: LayerOutput,
    /// File layer.
    pub file: LayerOutput,
    /// Environment layer.
    pub env: LayerOutput,
    /// CLI layer (highest priority).
    pub cli: LayerOutput,
}

/// Primary driver type that orchestrates parsing and validation.
///
/// This is generic over `T`, with a non-generic core for future optimization.
pub struct Driver<T> {
    config: Config<T>,
    core: DriverCore,
    _phantom: PhantomData<T>,
}

/// Non-generic driver core (placeholder for future monomorphization reduction).
#[derive(Debug, Default)]
pub struct DriverCore;

impl DriverCore {
    fn new() -> Self {
        Self
    }
}

impl<T: Facet<'static>> Driver<T> {
    /// Create a driver from a fully built config.
    pub fn new(config: Config<T>) -> Self {
        Self {
            config,
            core: DriverCore::new(),
            _phantom: PhantomData,
        }
    }

    /// Execute the driver and return an outcome.
    ///
    /// The returned `DriverOutcome` must be handled explicitly:
    /// - Use `.unwrap()` for automatic exit handling (recommended)
    /// - Use `.into_result()` if you need manual control
    pub fn run(self) -> DriverOutcome<T> {
        let _ = self.core;

        let mut layers = ConfigLayers::default();
        let mut all_diagnostics = Vec::new();
        let mut file_resolution = None;

        // Get CLI args source for Ariadne error display
        let cli_args_source = self
            .config
            .cli_config
            .as_ref()
            .map(|c| {
                let args = c.args().join(" ");
                if args.is_empty() {
                    "<no arguments>".to_string()
                } else {
                    args
                }
            })
            .unwrap_or_else(|| "<no arguments>".to_string());

        // Phase 1: Parse each layer
        // Priority order (lowest to highest): defaults < file < env < cli

        // 1a. Defaults layer (TODO: extract defaults from schema)
        // For now, defaults is empty - this will be filled in when we implement
        // default value extraction from the schema

        // 1b. File layer
        if let Some(ref file_config) = self.config.file_config {
            let result = parse_file(&self.config.schema, file_config);
            layers.file = result.output;
            file_resolution = Some(result.resolution);
            all_diagnostics.extend(layers.file.diagnostics.iter().cloned());
        }

        // 1c. Environment layer
        if let Some(ref env_config) = self.config.env_config {
            layers.env = parse_env(&self.config.schema, env_config, env_config.source());
            all_diagnostics.extend(layers.env.diagnostics.iter().cloned());
        }

        // 1d. CLI layer
        if let Some(ref cli_config) = self.config.cli_config {
            layers.cli = parse_cli(&self.config.schema, cli_config);
            tracing::debug!(cli_value = ?layers.cli.value, "driver: parsed CLI layer");
            all_diagnostics.extend(layers.cli.diagnostics.iter().cloned());
        }

        // Phase 1.5: Check special fields (help/version/completions)
        // These short-circuit before merge/deserialization
        if let Some(cli_value) = &layers.cli.value {
            let special = self.config.schema.special();

            // Check for --help
            if let Some(ref help_path) = special.help
                && let Some(ConfigValue::Bool(b)) = cli_value.get_by_path(help_path)
                && b.value
            {
                let help_config = self
                    .config
                    .help_config
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();

                // Extract subcommand path for subcommand-aware help
                let subcommand_path = if let Some(subcommand_field) =
                    self.config.schema.args().subcommand_field_name()
                {
                    cli_value.extract_subcommand_path(subcommand_field)
                } else {
                    Vec::new()
                };

                let text = generate_help_for_subcommand(
                    &self.config.schema,
                    &subcommand_path,
                    &help_config,
                );
                return DriverOutcome::err(DriverError::Help { text });
            }

            // Check for --version
            if let Some(ref version_path) = special.version
                && let Some(ConfigValue::Bool(b)) = cli_value.get_by_path(version_path)
                && b.value
            {
                let version = self
                    .config
                    .help_config
                    .as_ref()
                    .and_then(|h| h.version.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                let program_name = self
                    .config
                    .help_config
                    .as_ref()
                    .and_then(|h| h.program_name.clone())
                    .or_else(|| std::env::args().next())
                    .unwrap_or_else(|| "program".to_string());
                let text = format!("{} {}", program_name, version);
                return DriverOutcome::err(DriverError::Version { text });
            }

            // Check for --completions <shell>
            if let Some(ref completions_path) = special.completions
                && let Some(value) = cli_value.get_by_path(completions_path)
            {
                // The value should be a string representing the shell name
                if let Some(shell) = extract_shell_from_value(value) {
                    let program_name = self
                        .config
                        .help_config
                        .as_ref()
                        .and_then(|h| h.program_name.clone())
                        .or_else(|| std::env::args().next())
                        .unwrap_or_else(|| "program".to_string());
                    let script = generate_completions_for_shape(T::SHAPE, shell, &program_name);
                    return DriverOutcome::err(DriverError::Completions { script });
                }
            }
        }

        // Check for errors before proceeding
        let has_errors = all_diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
        if has_errors {
            return DriverOutcome::err(DriverError::Failed {
                report: Box::new(DriverReport {
                    diagnostics: all_diagnostics,
                    layers,
                    file_resolution,
                    overrides: Vec::new(),
                    cli_args_source,
                    source_name: "<cli>".to_string(),
                }),
            });
        }

        // Phase 2: Merge layers by priority
        let values_to_merge: Vec<ConfigValue> = [
            layers.defaults.value.clone(),
            layers.file.value.clone(),
            layers.env.value.clone(),
            layers.cli.value.clone(),
        ]
        .into_iter()
        .flatten()
        .collect();

        let merged = merge_layers(values_to_merge);
        tracing::debug!(merged_value = ?merged.value, "driver: merged layers");
        let overrides = merged.overrides;

        // Phase 2.5: Environment variable substitution
        // Substitute ${VAR} patterns in string values where env_subst is enabled
        let mut merged_value = merged.value;
        if let Some(config_schema) = self.config.schema.config()
            && let ConfigValue::Object(ref mut sourced_fields) = merged_value
            && let Some(config_field_name) = config_schema.field_name()
            && let Some(config_value) = sourced_fields.value.get_mut(&config_field_name.to_string())
            && let Err(e) = substitute_env_vars(config_value, config_schema, &RealEnv)
        {
            return DriverOutcome::err(DriverError::EnvSubst { error: e });
        }
        tracing::debug!(merged_value = ?merged_value, "driver: after env_subst");

        // Phase 2.75: Check for conflicting enum variants
        // E.g., setting both storage.s3.bucket and storage.gcp.project when storage is an enum
        let enum_conflicts = detect_enum_conflicts(&merged_value, &self.config.schema);
        if !enum_conflicts.is_empty() {
            let messages: Vec<String> = enum_conflicts.iter().map(|c| c.format()).collect();
            let message = messages.join("\n\n");

            return DriverOutcome::err(DriverError::Failed {
                report: Box::new(DriverReport {
                    diagnostics: vec![Diagnostic {
                        message,
                        path: None,
                        span: None,
                        severity: Severity::Error,
                    }],
                    layers,
                    file_resolution,
                    overrides,
                    cli_args_source,
                    source_name: "<cli>".to_string(),
                }),
            });
        }

        // Phase 3: Fill defaults and check for missing required fields
        // This must happen BEFORE deserialization so we can show all missing fields at once
        let value_with_defaults = fill_defaults_from_schema(&merged_value, &self.config.schema);
        tracing::debug!(value_with_defaults = ?value_with_defaults, "driver: after fill_defaults_from_schema");

        // Check for missing required fields by walking the schema
        let mut missing_fields = Vec::new();
        collect_missing_fields(
            &value_with_defaults,
            &self.config.schema,
            &mut missing_fields,
        );

        if !missing_fields.is_empty() {
            // If the only missing field is the subcommand, show help instead of "missing fields"
            let subcommand_field_name = self.config.schema.args().subcommand_field_name();
            let only_missing_subcommand = subcommand_field_name.is_some()
                && missing_fields.len() == 1
                && missing_fields[0].field_name == subcommand_field_name.unwrap();

            if only_missing_subcommand {
                // Show help instead of "missing required fields"
                let help_config = self
                    .config
                    .help_config
                    .as_ref()
                    .cloned()
                    .unwrap_or_default();

                let help = generate_help_for_subcommand(&self.config.schema, &[], &help_config);
                return DriverOutcome::err(DriverError::Help { text: help });
            }

            // Show dump with missing field markers (includes Sources header)
            let mut dump_buf = Vec::new();
            let resolution = file_resolution.as_ref().cloned().unwrap_or_default();
            dump_config_with_schema(
                &mut dump_buf,
                &value_with_defaults,
                &resolution,
                &self.config.schema,
            );
            let dump =
                String::from_utf8(dump_buf).unwrap_or_else(|_| "error rendering dump".into());

            // Format the summary of missing fields
            let summary = format_missing_fields_summary(&missing_fields);

            let message = format!(
                "Missing required fields:\n\n{}\nMissing:\n{}\nRun with --help for usage information.",
                dump, summary
            );

            return DriverOutcome::err(DriverError::Failed {
                report: Box::new(DriverReport {
                    diagnostics: vec![Diagnostic {
                        message,
                        path: None,
                        span: None,
                        severity: Severity::Error,
                    }],
                    layers,
                    file_resolution,
                    overrides,
                    cli_args_source,
                    source_name: "<cli>".to_string(),
                }),
            });
        }

        // Phase 4: Assign virtual spans and deserialize into T
        // The span registry maps virtual spans back to real source locations
        let mut value_with_virtual_spans = value_with_defaults;
        let span_registry = assign_virtual_spans(&mut value_with_virtual_spans);

        let value: T = match from_config_value(&value_with_virtual_spans) {
            Ok(v) => v,
            Err(e) => {
                // Extract virtual span from the error, then look up real location
                let (span, source_name, source_contents) = if let Some(virtual_span) = e.span() {
                    if let Some(entry) = span_registry.lookup_by_offset(virtual_span.offset) {
                        let real_span = Span::new(entry.real_span.offset, entry.real_span.len);
                        let (name, contents) =
                            get_source_for_provenance(&entry.provenance, &cli_args_source, &layers);
                        (Some(real_span), name, contents)
                    } else {
                        (None, "<unknown>".to_string(), cli_args_source.clone())
                    }
                } else {
                    (None, "<unknown>".to_string(), cli_args_source.clone())
                };

                return DriverOutcome::err(DriverError::Failed {
                    report: Box::new(DriverReport {
                        diagnostics: vec![Diagnostic {
                            message: e.to_string(),
                            path: None,
                            span,
                            severity: Severity::Error,
                        }],
                        layers,
                        file_resolution,
                        overrides,
                        cli_args_source: source_contents,
                        source_name,
                    }),
                });
            }
        };

        DriverOutcome::ok(DriverOutput {
            value,
            report: DriverReport {
                diagnostics: all_diagnostics,
                layers,
                file_resolution,
                overrides,
                cli_args_source,
                source_name: "<cli>".to_string(),
            },
        })
    }
}

/// Get the source name and contents for a provenance.
fn get_source_for_provenance(
    provenance: &Provenance,
    cli_args_source: &str,
    layers: &ConfigLayers,
) -> (String, String) {
    match provenance {
        Provenance::Cli { .. } => ("<cli>".to_string(), cli_args_source.to_string()),
        Provenance::Env { .. } => {
            // Use the virtual env document from the env layer
            let source_text = layers.env.source_text.as_ref().cloned().unwrap_or_default();
            ("<env>".to_string(), source_text)
        }
        Provenance::File { file, .. } => (file.path.to_string(), file.contents.clone()),
        Provenance::Default => ("<default>".to_string(), String::new()),
    }
}

/// Opaque result type for driver operations.
///
/// This type intentionally does NOT implement `Try`, so you cannot use `?` on it directly.
/// This prevents accidentally propagating help/version/completions as errors (which would
/// cause exit code 1 instead of 0).
///
/// Use one of the following methods to extract the value:
/// - `.unwrap()` - handles exits correctly, returns `T` (recommended for most cases)
/// - `.into_result()` - for advanced users who want to handle everything themselves
#[must_use = "this `DriverOutcome` may contain a help/version request that should be handled"]
pub struct DriverOutcome<T>(Result<DriverOutput<T>, DriverError>);

impl<T: std::fmt::Debug> std::fmt::Debug for DriverOutcome<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Ok(output) => f
                .debug_tuple("DriverOutcome::Ok")
                .field(&output.value)
                .finish(),
            Err(e) => f.debug_tuple("DriverOutcome::Err").field(e).finish(),
        }
    }
}

impl<T> DriverOutcome<T> {
    /// Create a successful outcome.
    pub fn ok(output: DriverOutput<T>) -> Self {
        Self(Ok(output))
    }

    /// Create an error outcome.
    pub fn err(error: DriverError) -> Self {
        Self(Err(error))
    }

    /// Convert to a standard `Result` for manual handling.
    ///
    /// **Warning**: If you use `?` on this result and the error is `Help`, `Version`,
    /// or `Completions`, Rust's default error handling will exit with code 1 instead of 0.
    /// Consider using `.unwrap()` instead for correct exit behavior.
    pub fn into_result(self) -> Result<DriverOutput<T>, DriverError> {
        self.0
    }

    /// Returns `true` if this is a successful parse (not help/version/error).
    pub fn is_ok(&self) -> bool {
        self.0.is_ok()
    }

    /// Returns `true` if this is an error or early exit request.
    pub fn is_err(&self) -> bool {
        self.0.is_err()
    }

    /// Get the value, or print output and exit.
    ///
    /// - On success: prints warnings to stderr, returns value
    /// - On help/completions/version: prints to stdout, exits with code 0
    /// - On error: prints diagnostics to stderr, exits with code 1
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn main() {
    ///     let args = figue::from_std_args::<Args>().unwrap();
    ///     // use args...
    /// }
    /// ```
    pub fn unwrap(self) -> T {
        match self.0 {
            Ok(output) => output.get(),
            Err(DriverError::Help { text }) => {
                println!("{}", text);
                std::process::exit(0);
            }
            Err(DriverError::Completions { script }) => {
                println!("{}", script);
                std::process::exit(0);
            }
            Err(DriverError::Version { text }) => {
                println!("{}", text);
                std::process::exit(0);
            }
            Err(DriverError::Failed { report }) => {
                eprintln!("{}", report.render_pretty());
                std::process::exit(1);
            }
            Err(DriverError::Builder { error }) => {
                eprintln!("{}", error);
                std::process::exit(1);
            }
            Err(DriverError::EnvSubst { error }) => {
                eprintln!("error: {}", error);
                std::process::exit(1);
            }
        }
    }

    /// Unwrap the error, panicking if this is a success.
    ///
    /// Useful for testing error cases.
    ///
    /// # Panics
    ///
    /// Panics if this is a successful parse.
    pub fn unwrap_err(self) -> DriverError {
        match self.0 {
            Ok(_) => panic!("called `DriverOutcome::unwrap_err()` on a success"),
            Err(e) => e,
        }
    }
}

/// Successful driver output: a typed value plus an execution report.
#[derive(Debug)]
pub struct DriverOutput<T> {
    /// The fully-typed value produced by deserialization.
    pub value: T,
    /// Diagnostics and metadata produced by the driver.
    pub report: DriverReport,
}

impl<T> DriverOutput<T> {
    /// Get the value, printing any warnings to stderr.
    pub fn get(self) -> T {
        self.print_warnings();
        self.value
    }

    /// Get the value silently (no warning output).
    pub fn get_silent(self) -> T {
        self.value
    }

    /// Get value and report separately.
    pub fn into_parts(self) -> (T, DriverReport) {
        (self.value, self.report)
    }

    /// Print any warnings to stderr.
    pub fn print_warnings(&self) {
        for diagnostic in &self.report.diagnostics {
            if diagnostic.severity == Severity::Warning {
                eprintln!("{}: {}", diagnostic.severity.as_str(), diagnostic.message);
            }
        }
    }
}

/// Full report of the driver execution.
///
/// The report should be pretty-renderable and capture all diagnostics,
/// plus optional supporting metadata (merge overrides, spans, etc).
#[derive(Default)]
pub struct DriverReport {
    /// Diagnostics emitted by the driver.
    pub diagnostics: Vec<Diagnostic>,
    /// Per-layer outputs, including unused keys and layer diagnostics.
    pub layers: ConfigLayers,
    /// File resolution metadata (paths tried, picked, etc).
    pub file_resolution: Option<FileResolution>,
    /// Records of values that were overridden during merge.
    pub overrides: Vec<Override>,
    /// Source contents for error display (CLI args, env var value, or file contents).
    pub cli_args_source: String,
    /// Name of the source for error display (e.g., `<cli>`, `$VAR`, `config.toml`).
    pub source_name: String,
}

/// A simple cache that wraps a Source and provides a display name.
struct NamedSource {
    name: String,
    source: ariadne::Source<String>,
}

impl ariadne::Cache<()> for NamedSource {
    type Storage = String;

    fn fetch(&mut self, _: &()) -> Result<&ariadne::Source<Self::Storage>, impl std::fmt::Debug> {
        Ok::<_, std::convert::Infallible>(&self.source)
    }

    fn display<'a>(&self, _: &'a ()) -> Option<impl std::fmt::Display + 'a> {
        Some(self.name.clone())
    }
}

impl DriverReport {
    /// Render the report using Ariadne for pretty error display.
    pub fn render_pretty(&self) -> String {
        use ariadne::{Color, Label, Report, ReportKind, Source};

        if self.diagnostics.is_empty() {
            return String::new();
        }

        let mut output = Vec::new();
        let mut cache = NamedSource {
            name: self.source_name.clone(),
            source: Source::from(self.cli_args_source.clone()),
        };

        for diagnostic in &self.diagnostics {
            // For diagnostics without a span, just print the message directly
            // (e.g., missing required fields error doesn't point to a specific location)
            if diagnostic.span.is_none() {
                let prefix = match diagnostic.severity {
                    Severity::Error => "Error: ",
                    Severity::Warning => "Warning: ",
                    Severity::Note => "Note: ",
                };
                output.extend_from_slice(prefix.as_bytes());
                output.extend_from_slice(diagnostic.message.as_bytes());
                output.push(b'\n');
                continue;
            }

            let span = diagnostic
                .span
                .map(|s| s.start..(s.start + s.len))
                .unwrap_or(0..0);

            let report_kind = match diagnostic.severity {
                Severity::Error => ReportKind::Error,
                Severity::Warning => ReportKind::Warning,
                Severity::Note => ReportKind::Advice,
            };

            let color = match diagnostic.severity {
                Severity::Error => Color::Red,
                Severity::Warning => Color::Yellow,
                Severity::Note => Color::Cyan,
            };

            let report = Report::build(report_kind, span.clone())
                .with_message(&diagnostic.message)
                .with_label(
                    Label::new(span)
                        .with_message(&diagnostic.message)
                        .with_color(color),
                )
                .finish();

            report.write(&mut cache, &mut output).ok();
        }

        String::from_utf8(output).unwrap_or_else(|_| "error rendering diagnostics".to_string())
    }
}

impl core::fmt::Display for DriverReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.render_pretty())
    }
}

impl core::fmt::Debug for DriverReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.render_pretty())
    }
}

/// A diagnostic message produced by the driver.
///
/// This is intentionally minimal and will grow as we integrate facet-pretty
/// spans and Ariadne rendering.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Human-readable message.
    pub message: String,
    /// Optional path within the schema or config.
    pub path: Option<Path>,
    /// Optional byte span within a formatted shape or source file.
    pub span: Option<Span>,
    /// Diagnostic severity.
    pub severity: Severity,
}

/// Severity for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Error that prevents producing a value.
    Error,
    /// Warning that allows a value to be produced.
    Warning,
    /// Informational note.
    Note,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
        }
    }
}

/// Extract a Shell value from a ConfigValue.
///
/// The completions field is `Option<Shell>`, so after CLI parsing we get
/// either nothing (None) or a string like "bash", "zsh", "fish".
fn extract_shell_from_value(value: &ConfigValue) -> Option<Shell> {
    match value {
        ConfigValue::String(s) => match s.value.to_lowercase().as_str() {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            _ => None,
        },
        // Could also be an enum variant name directly
        ConfigValue::Enum(e) => match e.value.variant.to_lowercase().as_str() {
            "bash" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            _ => None,
        },
        _ => None,
    }
}

/// Error returned by the driver.
///
/// Not all variants are "errors" in the traditional sense - Help, Completions,
/// and Version are successful operations that just don't produce a config value.
pub enum DriverError {
    /// Builder failed (e.g., schema validation, file not found) - exit code 1
    Builder {
        /// The builder error
        error: crate::builder::BuilderError,
    },

    /// Parsing or validation failed - exit code 1
    Failed {
        /// Report containing all diagnostics
        report: Box<DriverReport>,
    },

    /// Help was requested (via `#[facet(figue::help)]` field) - exit code 0
    Help {
        /// Formatted help text
        text: String,
    },

    /// Shell completions were requested (via `#[facet(figue::completions)]` field) - exit code 0
    Completions {
        /// Generated completion script
        script: String,
    },

    /// Version was requested (via `#[facet(figue::version)]` field) - exit code 0
    Version {
        /// Version string
        text: String,
    },

    /// Environment variable substitution failed - exit code 1
    EnvSubst {
        /// The substitution error
        error: EnvSubstError,
    },
}

impl DriverError {
    /// Returns the appropriate exit code for this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            DriverError::Builder { .. } => 1,
            DriverError::Failed { .. } => 1,
            DriverError::Help { .. } => 0,
            DriverError::Completions { .. } => 0,
            DriverError::Version { .. } => 0,
            DriverError::EnvSubst { .. } => 1,
        }
    }

    /// Returns true if this is a "success" error (help, completions, version).
    pub fn is_success(&self) -> bool {
        self.exit_code() == 0
    }

    /// Returns true if this is a help request.
    pub fn is_help(&self) -> bool {
        matches!(self, DriverError::Help { .. })
    }

    /// Returns the help text if this is a help request.
    pub fn help_text(&self) -> Option<&str> {
        match self {
            DriverError::Help { text } => Some(text),
            _ => None,
        }
    }
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverError::Builder { error } => write!(f, "{}", error),
            DriverError::Failed { report } => write!(f, "{}", report),
            DriverError::Help { text } => write!(f, "{}", text),
            DriverError::Completions { script } => write!(f, "{}", script),
            DriverError::Version { text } => write!(f, "{}", text),
            DriverError::EnvSubst { error } => write!(f, "{}", error),
        }
    }
}

impl std::fmt::Debug for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl std::error::Error for DriverError {}

impl std::process::Termination for DriverError {
    fn report(self) -> std::process::ExitCode {
        // Print the appropriate output
        match &self {
            DriverError::Help { text } | DriverError::Version { text } => {
                println!("{}", text);
            }
            DriverError::Completions { script } => {
                println!("{}", script);
            }
            DriverError::Failed { report } => {
                eprintln!("{}", report.render_pretty());
            }
            DriverError::Builder { error } => {
                eprintln!("{}", error);
            }
            DriverError::EnvSubst { error } => {
                eprintln!("error: {}", error);
            }
        }
        std::process::ExitCode::from(self.exit_code() as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as figue;
    use crate::FigueBuiltins;
    use crate::builder::builder;
    use facet::Facet;
    use facet_testhelpers::test;

    /// Args struct with FigueBuiltins flattened in
    #[derive(Facet, Debug)]
    struct ArgsWithBuiltins {
        /// Input file
        #[facet(figue::positional)]
        input: Option<String>,

        /// Standard CLI options
        #[facet(flatten)]
        builtins: FigueBuiltins,
    }

    #[test]
    fn test_driver_help_flag() {
        let config = builder::<ArgsWithBuiltins>()
            .unwrap()
            .cli(|cli| cli.args(["--help"]))
            .help(|h| h.program_name("test-app").version("1.0.0"))
            .build();

        let driver = Driver::new(config);
        let result = driver.run().into_result();

        match result {
            Err(DriverError::Help { text }) => {
                assert!(
                    text.contains("test-app"),
                    "help should contain program name"
                );
                assert!(text.contains("--help"), "help should mention --help flag");
            }
            other => panic!("expected DriverError::Help, got {:?}", other),
        }
    }

    #[test]
    fn test_driver_help_short_flag() {
        let config = builder::<ArgsWithBuiltins>()
            .unwrap()
            .cli(|cli| cli.args(["-h"]))
            .help(|h| h.program_name("test-app"))
            .build();

        let driver = Driver::new(config);
        let result = driver.run().into_result();

        assert!(
            matches!(result, Err(DriverError::Help { .. })),
            "expected DriverError::Help"
        );
    }

    #[test]
    fn test_driver_version_flag() {
        let config = builder::<ArgsWithBuiltins>()
            .unwrap()
            .cli(|cli| cli.args(["--version"]))
            .help(|h| h.program_name("test-app").version("2.0.0"))
            .build();

        let driver = Driver::new(config);
        let result = driver.run().into_result();

        match result {
            Err(DriverError::Version { text }) => {
                assert!(
                    text.contains("test-app"),
                    "version should contain program name"
                );
                assert!(
                    text.contains("2.0.0"),
                    "version should contain version number"
                );
            }
            other => panic!("expected DriverError::Version, got {:?}", other),
        }
    }

    #[test]
    fn test_driver_version_short_flag() {
        let config = builder::<ArgsWithBuiltins>()
            .unwrap()
            .cli(|cli| cli.args(["-V"]))
            .help(|h| h.program_name("test-app").version("3.0.0"))
            .build();

        let driver = Driver::new(config);
        let result = driver.run().into_result();

        match result {
            Err(DriverError::Version { text }) => {
                assert!(
                    text.contains("3.0.0"),
                    "version should contain version number"
                );
            }
            other => panic!("expected DriverError::Version, got {:?}", other),
        }
    }

    #[test]
    fn test_driver_completions_bash() {
        let config = builder::<ArgsWithBuiltins>()
            .unwrap()
            .cli(|cli| cli.args(["--completions", "bash"]))
            .help(|h| h.program_name("test-app"))
            .build();

        let driver = Driver::new(config);
        let result = driver.run().into_result();

        match result {
            Err(DriverError::Completions { script }) => {
                assert!(
                    script.contains("_test-app"),
                    "bash completions should contain function name"
                );
                assert!(
                    script.contains("complete"),
                    "bash completions should contain 'complete'"
                );
            }
            other => panic!("expected DriverError::Completions, got {:?}", other),
        }
    }

    #[test]
    fn test_driver_completions_zsh() {
        let config = builder::<ArgsWithBuiltins>()
            .unwrap()
            .cli(|cli| cli.args(["--completions", "zsh"]))
            .help(|h| h.program_name("myapp"))
            .build();

        let driver = Driver::new(config);
        let result = driver.run().into_result();

        match result {
            Err(DriverError::Completions { script }) => {
                assert!(
                    script.contains("#compdef myapp"),
                    "zsh completions should contain #compdef"
                );
            }
            other => panic!("expected DriverError::Completions, got {:?}", other),
        }
    }

    #[test]
    fn test_driver_completions_fish() {
        let config = builder::<ArgsWithBuiltins>()
            .unwrap()
            .cli(|cli| cli.args(["--completions", "fish"]))
            .help(|h| h.program_name("myapp"))
            .build();

        let driver = Driver::new(config);
        let result = driver.run().into_result();

        match result {
            Err(DriverError::Completions { script }) => {
                assert!(
                    script.contains("complete -c myapp"),
                    "fish completions should contain 'complete -c myapp'"
                );
            }
            other => panic!("expected DriverError::Completions, got {:?}", other),
        }
    }

    #[test]
    fn test_driver_normal_execution() {
        let config = builder::<ArgsWithBuiltins>()
            .unwrap()
            .cli(|cli| cli.args(["myfile.txt"]))
            .build();

        let driver = Driver::new(config);
        let result = driver.run().into_result();

        match result {
            Ok(output) => {
                assert_eq!(output.value.input, Some("myfile.txt".to_string()));
                assert!(!output.value.builtins.help);
                assert!(!output.value.builtins.version);
                assert!(output.value.builtins.completions.is_none());
            }
            Err(e) => panic!("expected success, got error: {:?}", e),
        }
    }

    #[test]
    fn test_driver_error_exit_codes() {
        let help_err = DriverError::Help {
            text: "help".to_string(),
        };
        let version_err = DriverError::Version {
            text: "1.0".to_string(),
        };
        let completions_err = DriverError::Completions {
            script: "script".to_string(),
        };
        let failed_err = DriverError::Failed {
            report: Box::new(DriverReport::default()),
        };

        assert_eq!(help_err.exit_code(), 0);
        assert_eq!(version_err.exit_code(), 0);
        assert_eq!(completions_err.exit_code(), 0);
        assert_eq!(failed_err.exit_code(), 1);

        assert!(help_err.is_success());
        assert!(version_err.is_success());
        assert!(completions_err.is_success());
        assert!(!failed_err.is_success());
    }

    // ========================================================================
    // Tests: Builder API with subcommands (regression test for issue #3)
    // ========================================================================

    /// Subcommand enum for testing
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum TestCommand {
        /// Build the project
        Build {
            /// Build in release mode
            #[facet(figue::named, figue::short = 'r')]
            release: bool,
        },
        /// Run the project
        Run {
            /// Arguments to pass
            #[facet(figue::positional)]
            args: Vec<String>,
        },
    }

    /// Args struct with subcommand only (minimal reproduction)
    #[derive(Facet, Debug)]
    struct ArgsWithSubcommandOnly {
        #[facet(figue::subcommand)]
        command: TestCommand,
    }

    /// Args struct with subcommand AND FigueBuiltins (issue report pattern)
    #[derive(Facet, Debug)]
    struct ArgsWithSubcommandAndBuiltins {
        #[facet(figue::subcommand)]
        command: TestCommand,

        #[facet(flatten)]
        builtins: FigueBuiltins,
    }

    // Test the minimal case - subcommand only, no builtins
    #[test]
    fn test_builder_api_with_subcommand_minimal() {
        let config = builder::<ArgsWithSubcommandOnly>()
            .expect("failed to build args schema")
            .cli(|cli| cli.args(["build", "--release"]))
            .build();

        let result = Driver::new(config).run().into_result();

        match result {
            Ok(output) => match &output.value.command {
                TestCommand::Build { release } => {
                    assert!(*release, "release flag should be true");
                }
                TestCommand::Run { .. } => {
                    panic!("expected Build subcommand, got Run");
                }
            },
            Err(e) => panic!("expected success, got error: {:?}", e),
        }
    }

    // Test with FigueBuiltins (the pattern from issue report)
    #[test]
    fn test_builder_api_with_subcommand_and_builtins() {
        let config = builder::<ArgsWithSubcommandAndBuiltins>()
            .expect("failed to build args schema")
            .cli(|cli| cli.args(["build", "--release"]))
            .help(|h| h.program_name("test-app").version("1.0.0"))
            .build();

        let result = Driver::new(config).run().into_result();

        match result {
            Ok(output) => match &output.value.command {
                TestCommand::Build { release } => {
                    assert!(*release, "release flag should be true");
                }
                TestCommand::Run { .. } => {
                    panic!("expected Build subcommand, got Run");
                }
            },
            Err(e) => panic!("expected success, got error: {:?}", e),
        }
    }

    #[test]
    fn test_builder_api_with_subcommand_no_args() {
        let config = builder::<ArgsWithSubcommandOnly>()
            .expect("failed to build args schema")
            .cli(|cli| cli.args(["build"]))
            .build();

        let result = Driver::new(config).run().into_result();

        match result {
            Ok(output) => match &output.value.command {
                TestCommand::Build { release } => {
                    assert!(!*release, "release flag should be false by default");
                }
                TestCommand::Run { .. } => {
                    panic!("expected Build subcommand, got Run");
                }
            },
            Err(e) => panic!("expected success, got error: {:?}", e),
        }
    }

    #[test]
    fn test_builder_api_with_run_subcommand() {
        let config = builder::<ArgsWithSubcommandOnly>()
            .expect("failed to build args schema")
            .cli(|cli| cli.args(["run", "arg1", "arg2"]))
            .build();

        let result = Driver::new(config).run().into_result();

        match result {
            Ok(output) => match &output.value.command {
                TestCommand::Run { args } => {
                    assert_eq!(args, &["arg1".to_string(), "arg2".to_string()]);
                }
                TestCommand::Build { .. } => {
                    panic!("expected Run subcommand, got Build");
                }
            },
            Err(e) => panic!("expected success, got error: {:?}", e),
        }
    }

    #[test]
    fn test_from_slice_with_subcommand() {
        // Test that from_slice (which uses builder internally) works with subcommands
        let args: ArgsWithSubcommandOnly = crate::from_slice(&["build", "--release"]).unwrap();

        match &args.command {
            TestCommand::Build { release } => {
                assert!(*release, "release flag should be true");
            }
            TestCommand::Run { .. } => {
                panic!("expected Build subcommand, got Run");
            }
        }
    }

    #[test]
    fn test_from_slice_with_subcommand_and_builtins() {
        // Test that from_slice works with subcommands AND FigueBuiltins
        let args: ArgsWithSubcommandAndBuiltins =
            crate::from_slice(&["build", "--release"]).unwrap();

        match &args.command {
            TestCommand::Build { release } => {
                assert!(*release, "release flag should be true");
            }
            TestCommand::Run { .. } => {
                panic!("expected Build subcommand, got Run");
            }
        }
    }

    #[test]
    fn test_subcommand_with_builtins_parsing() {
        use crate::config_value_parser::fill_defaults_from_shape;
        use crate::layers::cli::CliConfigBuilder;
        use crate::layers::cli::parse_cli;
        use crate::missing::collect_missing_fields;
        use crate::schema::Schema;

        // Build schema for the type with subcommand + builtins
        let schema =
            Schema::from_shape(ArgsWithSubcommandAndBuiltins::SHAPE).expect("schema should build");

        // Parse CLI args
        let cli_config = CliConfigBuilder::new().args(["build", "--release"]).build();
        let output = parse_cli(&schema, &cli_config);

        assert!(output.diagnostics.is_empty(), "should have no diagnostics");

        // Now test fill_defaults_from_shape
        let cli_value = output.value.unwrap();
        let with_defaults =
            fill_defaults_from_shape(&cli_value, ArgsWithSubcommandAndBuiltins::SHAPE);

        // Check for missing fields
        let mut missing_fields = Vec::new();
        collect_missing_fields(&with_defaults, &schema, &mut missing_fields);

        assert!(
            missing_fields.is_empty(),
            "should have no missing fields, got: {:?}",
            missing_fields
        );

        // Now try the full deserialization
        let result: Result<ArgsWithSubcommandAndBuiltins, _> =
            crate::config_value_parser::from_config_value(&with_defaults);

        assert!(
            result.is_ok(),
            "deserialization should succeed: {:?}",
            result
        );
    }

    // ========================================================================
    // More comprehensive builder API tests for issue #3
    // ========================================================================

    /// Nested subcommand enum (inner level)
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum DatabaseAction {
        /// Create a new migration
        Create {
            /// Migration name
            #[facet(figue::positional)]
            name: String,
        },
        /// Run pending migrations
        Run {
            /// Run in dry-run mode
            #[facet(figue::named)]
            dry_run: bool,
        },
        /// Rollback last migration
        Rollback {
            /// Number of migrations to rollback
            #[facet(figue::named, default)]
            count: Option<u32>,
        },
    }

    /// Top-level command with nested subcommand
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum TopLevelCommand {
        /// Database management commands
        Db {
            #[facet(figue::subcommand)]
            action: DatabaseAction,
        },
        /// Start the server
        Serve {
            /// Port to listen on
            #[facet(figue::named, default)]
            port: Option<u16>,
            /// Host to bind to
            #[facet(figue::named, default)]
            host: Option<String>,
        },
        /// Show version info (unit variant)
        Version,
    }

    /// Args with nested subcommands
    #[derive(Facet, Debug)]
    struct ArgsWithNestedSubcommands {
        /// Global verbose flag
        #[facet(figue::named, figue::short = 'v')]
        verbose: bool,

        #[facet(figue::subcommand)]
        command: TopLevelCommand,

        #[facet(flatten)]
        builtins: FigueBuiltins,
    }

    #[test]
    fn test_builder_nested_subcommand_db_create() {
        let config = builder::<ArgsWithNestedSubcommands>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["db", "create", "add_users_table"]))
            .help(|h| h.program_name("test-app"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => {
                assert!(!output.value.verbose);
                match &output.value.command {
                    TopLevelCommand::Db { action } => match action {
                        DatabaseAction::Create { name } => {
                            assert_eq!(name, "add_users_table");
                        }
                        _ => panic!("expected Create action"),
                    },
                    _ => panic!("expected Db command"),
                }
            }
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    #[test]
    fn test_builder_nested_subcommand_db_run_with_flag() {
        let config = builder::<ArgsWithNestedSubcommands>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["-v", "db", "run", "--dry-run"]))
            .help(|h| h.program_name("test-app"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => {
                assert!(output.value.verbose, "verbose should be true");
                match &output.value.command {
                    TopLevelCommand::Db { action } => match action {
                        DatabaseAction::Run { dry_run } => {
                            assert!(*dry_run, "dry_run should be true");
                        }
                        _ => panic!("expected Run action"),
                    },
                    _ => panic!("expected Db command"),
                }
            }
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    #[test]
    fn test_builder_nested_subcommand_db_rollback_default() {
        let config = builder::<ArgsWithNestedSubcommands>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["db", "rollback"]))
            .help(|h| h.program_name("test-app"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                TopLevelCommand::Db { action } => match action {
                    DatabaseAction::Rollback { count } => {
                        assert_eq!(*count, None, "count should default to None");
                    }
                    _ => panic!("expected Rollback action"),
                },
                _ => panic!("expected Db command"),
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    #[test]
    fn test_builder_nested_subcommand_db_rollback_with_count() {
        let config = builder::<ArgsWithNestedSubcommands>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["db", "rollback", "--count", "3"]))
            .help(|h| h.program_name("test-app"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                TopLevelCommand::Db { action } => match action {
                    DatabaseAction::Rollback { count } => {
                        assert_eq!(*count, Some(3));
                    }
                    _ => panic!("expected Rollback action"),
                },
                _ => panic!("expected Db command"),
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    #[test]
    fn test_builder_serve_with_defaults() {
        let config = builder::<ArgsWithNestedSubcommands>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["serve"]))
            .help(|h| h.program_name("test-app"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                TopLevelCommand::Serve { port, host } => {
                    assert_eq!(*port, None);
                    assert_eq!(*host, None);
                }
                _ => panic!("expected Serve command"),
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    #[test]
    fn test_builder_serve_with_options() {
        let config = builder::<ArgsWithNestedSubcommands>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["serve", "--port", "8080", "--host", "0.0.0.0"]))
            .help(|h| h.program_name("test-app"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                TopLevelCommand::Serve { port, host } => {
                    assert_eq!(*port, Some(8080));
                    assert_eq!(host.as_deref(), Some("0.0.0.0"));
                }
                _ => panic!("expected Serve command"),
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    #[test]
    fn test_builder_unit_variant_subcommand() {
        let config = builder::<ArgsWithNestedSubcommands>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["version"]))
            .help(|h| h.program_name("test-app"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                TopLevelCommand::Version => {
                    // Success - unit variant parsed correctly
                }
                _ => panic!("expected Version command"),
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    /// Test tuple variant subcommands with the builder API
    #[derive(Facet, Debug, PartialEq, Default)]
    struct InstallOptions {
        /// Install globally
        #[facet(figue::named)]
        global: bool,
        /// Force reinstall
        #[facet(figue::named)]
        force: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum PackageCommand {
        /// Install a package (tuple variant with flattened struct)
        Install(#[facet(flatten)] InstallOptions),
        /// Uninstall a package
        Uninstall {
            /// Package name
            #[facet(figue::positional)]
            name: String,
        },
    }

    #[derive(Facet, Debug)]
    struct ArgsWithTupleVariant {
        #[facet(figue::subcommand)]
        command: PackageCommand,

        #[facet(flatten)]
        builtins: FigueBuiltins,
    }

    #[test]
    fn test_builder_tuple_variant_with_flatten() {
        let config = builder::<ArgsWithTupleVariant>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["install", "--global", "--force"]))
            .help(|h| h.program_name("pkg-manager"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                PackageCommand::Install(opts) => {
                    assert!(opts.global, "global should be true");
                    assert!(opts.force, "force should be true");
                }
                _ => panic!("expected Install command"),
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    #[test]
    fn test_builder_tuple_variant_defaults() {
        let config = builder::<ArgsWithTupleVariant>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["install"]))
            .help(|h| h.program_name("pkg-manager"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                PackageCommand::Install(opts) => {
                    assert!(!opts.global, "global should default to false");
                    assert!(!opts.force, "force should default to false");
                }
                _ => panic!("expected Install command"),
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    /// Test renamed subcommand variants
    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum RenamedCommand {
        /// List items (renamed to 'ls')
        #[facet(rename = "ls")]
        List {
            /// Show all files
            #[facet(figue::named, figue::short = 'a')]
            all: bool,
        },
        /// Remove items (renamed to 'rm')
        #[facet(rename = "rm")]
        Remove {
            /// Force removal
            #[facet(figue::named, figue::short = 'f')]
            force: bool,
            /// Target path
            #[facet(figue::positional)]
            path: String,
        },
    }

    #[derive(Facet, Debug)]
    struct ArgsWithRenamedSubcommands {
        #[facet(figue::subcommand)]
        command: RenamedCommand,

        #[facet(flatten)]
        builtins: FigueBuiltins,
    }

    #[test]
    fn test_builder_renamed_subcommand_ls() {
        let config = builder::<ArgsWithRenamedSubcommands>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["ls", "-a"]))
            .help(|h| h.program_name("file-tool"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                RenamedCommand::List { all } => {
                    assert!(*all, "all should be true");
                }
                _ => panic!("expected List command"),
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    #[test]
    fn test_builder_renamed_subcommand_rm() {
        let config = builder::<ArgsWithRenamedSubcommands>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["rm", "-f", "/tmp/file.txt"]))
            .help(|h| h.program_name("file-tool"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                RenamedCommand::Remove { force, path } => {
                    assert!(*force, "force should be true");
                    assert_eq!(path, "/tmp/file.txt");
                }
                _ => panic!("expected Remove command"),
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    /// Test deeply flattened structs within subcommands
    #[derive(Facet, Debug, PartialEq, Default)]
    struct LoggingOpts {
        /// Enable debug logging
        #[facet(figue::named)]
        debug: bool,
        /// Log to file
        #[facet(figue::named, default)]
        log_file: Option<String>,
    }

    #[derive(Facet, Debug, PartialEq, Default)]
    struct CommonOpts {
        /// Verbose output
        #[facet(figue::named, figue::short = 'v')]
        verbose: bool,
        /// Quiet mode
        #[facet(figue::named, figue::short = 'q')]
        quiet: bool,
        #[facet(flatten)]
        logging: LoggingOpts,
    }

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum DeepCommand {
        /// Run with common options
        Execute {
            #[facet(flatten)]
            common: CommonOpts,
            /// Target to execute
            #[facet(figue::positional)]
            target: String,
        },
    }

    #[derive(Facet, Debug)]
    struct ArgsWithDeepFlatten {
        #[facet(figue::subcommand)]
        command: DeepCommand,

        #[facet(flatten)]
        builtins: FigueBuiltins,
    }

    #[test]
    fn test_builder_deep_flatten_all_flags() {
        let config = builder::<ArgsWithDeepFlatten>()
            .expect("failed to build schema")
            .cli(|cli| {
                cli.args([
                    "execute",
                    "-v",
                    "--debug",
                    "--log-file",
                    "/var/log/app.log",
                    "my-target",
                ])
            })
            .help(|h| h.program_name("deep-app"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                DeepCommand::Execute { common, target } => {
                    assert!(common.verbose, "verbose should be true");
                    assert!(!common.quiet, "quiet should be false");
                    assert!(common.logging.debug, "debug should be true");
                    assert_eq!(common.logging.log_file.as_deref(), Some("/var/log/app.log"));
                    assert_eq!(target, "my-target");
                }
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    #[test]
    fn test_builder_deep_flatten_defaults() {
        let config = builder::<ArgsWithDeepFlatten>()
            .expect("failed to build schema")
            .cli(|cli| cli.args(["execute", "simple-target"]))
            .help(|h| h.program_name("deep-app"))
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.command {
                DeepCommand::Execute { common, target } => {
                    assert!(!common.verbose);
                    assert!(!common.quiet);
                    assert!(!common.logging.debug);
                    assert_eq!(common.logging.log_file, None);
                    assert_eq!(target, "simple-target");
                }
            },
            Err(e) => panic!("expected success: {:?}", e),
        }
    }

    // ========================================================================
    // Enum variant conflict tests
    // ========================================================================

    #[derive(Facet, Debug)]
    #[facet(rename_all = "kebab-case")]
    #[repr(u8)]
    #[allow(dead_code)]
    enum StorageBackend {
        S3 { bucket: String, region: String },
        Gcp { project: String, zone: String },
        Local { path: String },
    }

    #[derive(Facet, Debug)]
    struct StorageConfig {
        storage: StorageBackend,
    }

    #[derive(Facet, Debug)]
    struct ArgsWithStorageConfig {
        #[facet(figue::config)]
        config: StorageConfig,
    }

    #[test]
    #[ignore = "env parser doesn't support enum variant paths yet (issue #37)"]
    fn test_enum_variant_conflict_from_env() {
        use crate::layers::env::MockEnv;

        // Set both s3 and gcp variants via env vars - should error
        let config = builder::<ArgsWithStorageConfig>()
            .expect("failed to build schema")
            .env(|env| {
                env.prefix("MYAPP").source(MockEnv::from_pairs([
                    ("MYAPP__STORAGE__S3__BUCKET", "my-bucket"),
                    ("MYAPP__STORAGE__S3__REGION", "us-east-1"),
                    ("MYAPP__STORAGE__GCP__PROJECT", "my-project"),
                ]))
            })
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Err(DriverError::Failed { report }) => {
                let msg = format!("{}", report);
                assert!(
                    msg.contains("Conflicting enum variants"),
                    "should report enum conflict: {msg}"
                );
                assert!(msg.contains("s3"), "should mention s3 variant: {msg}");
                assert!(msg.contains("gcp"), "should mention gcp variant: {msg}");
            }
            Ok(_) => panic!("expected conflict error, got success"),
            Err(e) => panic!("expected conflict error, got {:?}", e),
        }
    }

    #[test]
    #[ignore = "env parser doesn't support enum variant paths yet (issue #37)"]
    fn test_enum_no_conflict_single_variant_from_env() {
        use crate::layers::env::MockEnv;

        // Set only s3 variant via env vars - should succeed
        let config = builder::<ArgsWithStorageConfig>()
            .expect("failed to build schema")
            .env(|env| {
                env.prefix("MYAPP").source(MockEnv::from_pairs([
                    ("MYAPP__STORAGE__S3__BUCKET", "my-bucket"),
                    ("MYAPP__STORAGE__S3__REGION", "us-east-1"),
                ]))
            })
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.config.storage {
                StorageBackend::S3 { bucket, region } => {
                    assert_eq!(bucket, "my-bucket");
                    assert_eq!(region, "us-east-1");
                }
                other => panic!("expected S3 variant, got {:?}", other),
            },
            Err(e) => panic!("expected success, got {:?}", e),
        }
    }

    #[test]
    #[ignore = "env parser doesn't support enum variant paths yet (issue #37)"]
    fn test_enum_variant_conflict_cross_source() {
        use crate::layers::env::MockEnv;

        // Set s3 from env, gcp from CLI - should error
        let config = builder::<ArgsWithStorageConfig>()
            .expect("failed to build schema")
            .env(|env| {
                env.prefix("MYAPP").source(MockEnv::from_pairs([
                    ("MYAPP__STORAGE__S3__BUCKET", "my-bucket"),
                    ("MYAPP__STORAGE__S3__REGION", "us-east-1"),
                ]))
            })
            .cli(|cli| {
                cli.args([
                    "--config.storage.gcp.project",
                    "my-project",
                    "--config.storage.gcp.zone",
                    "us-central1-a",
                ])
            })
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Err(DriverError::Failed { report }) => {
                let msg = format!("{}", report);
                assert!(
                    msg.contains("Conflicting enum variants"),
                    "should report enum conflict: {msg}"
                );
                // Should mention both variants
                assert!(msg.contains("s3"), "should mention s3: {msg}");
                assert!(msg.contains("gcp"), "should mention gcp: {msg}");
            }
            Ok(_) => panic!("expected conflict error, got success"),
            Err(e) => panic!("expected conflict error, got {:?}", e),
        }
    }

    #[test]
    fn test_enum_variant_conflict_from_cli() {
        // Set both s3 and gcp variants via CLI args - should error
        let config = builder::<ArgsWithStorageConfig>()
            .expect("failed to build schema")
            .cli(|cli| {
                cli.args([
                    "--config.storage.s3.bucket",
                    "my-bucket",
                    "--config.storage.gcp.project",
                    "my-project",
                ])
            })
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Err(DriverError::Failed { report }) => {
                let msg = format!("{}", report);
                assert!(
                    msg.contains("Conflicting enum variants"),
                    "should report enum conflict: {msg}"
                );
            }
            Ok(_) => panic!("expected conflict error, got success"),
            Err(e) => panic!("expected conflict error, got {:?}", e),
        }
    }

    #[test]
    fn test_enum_no_conflict_single_variant_from_cli() {
        // Set only s3 variant via CLI args - should succeed
        let config = builder::<ArgsWithStorageConfig>()
            .expect("failed to build schema")
            .cli(|cli| {
                cli.args([
                    "--config.storage.s3.bucket",
                    "my-bucket",
                    "--config.storage.s3.region",
                    "us-east-1",
                ])
            })
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Ok(output) => match &output.value.config.storage {
                StorageBackend::S3 { bucket, region } => {
                    assert_eq!(bucket, "my-bucket");
                    assert_eq!(region, "us-east-1");
                }
                other => panic!("expected S3 variant, got {:?}", other),
            },
            Err(e) => panic!("expected success, got {:?}", e),
        }
    }

    #[test]
    fn test_enum_variant_conflict_three_variants() {
        // Set all three variants via CLI args - should error
        let config = builder::<ArgsWithStorageConfig>()
            .expect("failed to build schema")
            .cli(|cli| {
                cli.args([
                    "--config.storage.s3.bucket",
                    "my-bucket",
                    "--config.storage.gcp.project",
                    "my-project",
                    "--config.storage.local.path",
                    "/data",
                ])
            })
            .build();

        let result = Driver::new(config).run().into_result();
        match result {
            Err(DriverError::Failed { report }) => {
                let msg = format!("{}", report);
                assert!(
                    msg.contains("Conflicting enum variants"),
                    "should report enum conflict: {msg}"
                );
                // Should mention all three variants
                assert!(msg.contains("s3"), "should mention s3: {msg}");
                assert!(msg.contains("gcp"), "should mention gcp: {msg}");
                assert!(msg.contains("local"), "should mention local: {msg}");
            }
            Ok(_) => panic!("expected conflict error, got success"),
            Err(e) => panic!("expected conflict error, got {:?}", e),
        }
    }
}
