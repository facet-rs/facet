//! Schema-driven config file parser that outputs ConfigValue with provenance.
//!
//! This module is under active development and not yet wired into the main API.
//!
//! This parser:
//! - Uses the pre-built Schema to validate config structure
//! - Outputs LayerOutput (ConfigValue + diagnostics), not a Partial
//! - Supports multiple file formats via FormatRegistry
//! - Reports unused keys (keys in file that don't match schema)
//! - Tracks provenance for all values
//!
//! # Example
//!
//! ```rust,ignore
//! use figue::layers::file::{parse_file, FileConfig, FormatRegistry};
//! use figue::schema::Schema;
//!
//! let schema = Schema::from_shape(MyConfig::SHAPE)?;
//! let config = FileConfig::new()
//!     .path("config.json")
//!     .registry(FormatRegistry::with_defaults());
//!
//! let output = parse_file(&schema, &config)?;
//! ```

use std::boxed::Box;
use std::string::{String, ToString};
use std::sync::Arc;
use std::vec::Vec;

use camino::{Utf8Path, Utf8PathBuf};

use std::collections::HashSet;

use crate::config_format::{ConfigFormat, ConfigFormatError, JsonFormat};
use crate::config_value::ConfigValue;
use crate::driver::{Diagnostic, LayerOutput, Severity, UnusedKey};
use crate::provenance::{ConfigFile, FilePathStatus, FileResolution, Provenance};
use crate::schema::{ConfigStructSchema, ConfigValueSchema, Schema};

// ============================================================================
// Valid Paths Helper
// ============================================================================

/// Tracks valid paths for config file validation.
///
/// Distinguishes between:
/// - Container paths: intermediate objects (like "common" when "common.log_level" exists)
/// - Leaf paths: actual field values
#[derive(Default)]
struct ValidPaths {
    /// All valid paths (both containers and leaves)
    paths: HashSet<Vec<String>>,
}

impl ValidPaths {
    fn new() -> Self {
        Self::default()
    }

    fn add_leaf(&mut self, path: Vec<String>) {
        self.paths.insert(path);
    }

    fn is_valid(&self, path: &[String]) -> bool {
        self.paths.contains(path)
    }
}

// ============================================================================
// Format Registry
// ============================================================================

/// A registry of config file formats.
///
/// This allows registering multiple formats and selecting the appropriate
/// one based on file extension.
#[derive(Default)]
pub struct FormatRegistry {
    formats: Vec<Box<dyn ConfigFormat>>,
}

impl FormatRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            formats: Vec::new(),
        }
    }

    /// Create a registry with the default JSON format.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(JsonFormat);
        registry
    }

    /// Register a new format.
    pub fn register<F: ConfigFormat + 'static>(&mut self, format: F) {
        self.formats.push(Box::new(format));
    }

    /// Find a format that handles the given file extension.
    ///
    /// The extension should not include the leading dot.
    pub fn find_by_extension(&self, extension: &str) -> Option<&dyn ConfigFormat> {
        let ext_lower = extension.to_lowercase();
        self.formats
            .iter()
            .find(|f| {
                f.extensions()
                    .iter()
                    .any(|e| e.eq_ignore_ascii_case(&ext_lower))
            })
            .map(|f| f.as_ref())
    }

    /// Parse a config file, automatically selecting the format based on extension.
    pub fn parse(&self, contents: &str, extension: &str) -> Result<ConfigValue, ConfigFormatError> {
        let format = self.find_by_extension(extension).ok_or_else(|| {
            ConfigFormatError::new(format!("unsupported file extension: .{extension}"))
        })?;
        format.parse(contents)
    }

    /// Parse a config file and set provenance on all values.
    ///
    /// This is the preferred method for loading config files, as it ensures
    /// all values have proper provenance tracking for error messages.
    pub fn parse_file(
        &self,
        path: &Utf8Path,
        contents: &str,
    ) -> Result<ConfigValue, ConfigFormatError> {
        let extension = path.extension().unwrap_or("");
        let mut value = self.parse(contents, extension)?;

        // Create config file and set provenance recursively
        let file = Arc::new(ConfigFile::new(path, contents));
        value.set_file_provenance_recursive(&file, "");

        Ok(value)
    }

    /// Get all registered extensions.
    pub fn extensions(&self) -> Vec<&str> {
        self.formats
            .iter()
            .flat_map(|f| f.extensions().iter().copied())
            .collect()
    }
}

// ============================================================================
// File Configuration
// ============================================================================

/// Configuration for config file parsing.
pub struct FileConfig {
    /// Explicit path provided via CLI (e.g., --config path.json).
    pub explicit_path: Option<Utf8PathBuf>,

    /// Default paths to check if no explicit path is provided.
    pub default_paths: Vec<Utf8PathBuf>,

    /// Format registry for parsing different file types.
    pub registry: FormatRegistry,

    /// Whether to error on unknown keys in the config file.
    pub strict: bool,

    /// Inline content for testing (avoids disk I/O).
    /// When set, this content is used instead of reading from disk.
    /// The tuple is (content, filename_for_format_detection).
    pub inline_content: Option<(String, String)>,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            explicit_path: None,
            default_paths: Vec::new(),
            registry: FormatRegistry::with_defaults(),
            strict: false,
            inline_content: None,
        }
    }
}

impl FileConfig {
    /// Create a new file config with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set an explicit config file path.
    pub fn path(mut self, path: impl Into<Utf8PathBuf>) -> Self {
        self.explicit_path = Some(path.into());
        self
    }

    /// Set default paths to check for config files.
    pub fn default_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<Utf8PathBuf>,
    {
        self.default_paths = paths.into_iter().map(|p| p.into()).collect();
        self
    }

    /// Set the format registry.
    pub fn registry(mut self, registry: FormatRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// Enable strict mode - error on unknown keys.
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// Set inline content for testing (avoids disk I/O).
    ///
    /// The filename is used for format detection (e.g., "config.toml" or "settings.json").
    pub fn content(mut self, content: impl Into<String>, filename: impl Into<String>) -> Self {
        self.inline_content = Some((content.into(), filename.into()));
        self
    }
}

// ============================================================================
// File Parsing Result
// ============================================================================

/// Result of file parsing, including resolution info.
pub struct FileParseResult {
    /// The layer output with parsed values and diagnostics.
    pub output: LayerOutput,
    /// Information about which files were tried.
    pub resolution: FileResolution,
}

// ============================================================================
// Main Parse Function
// ============================================================================

/// Parse a config file using the schema, returning a LayerOutput.
///
/// This resolves the file path, reads the file, parses it using the appropriate
/// format, validates against the schema, and tracks provenance.
pub fn parse_file(schema: &Schema, config: &FileConfig) -> FileParseResult {
    let mut ctx = FileParseContext::new(schema, config);
    ctx.parse();
    ctx.into_result()
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
        ConfigValue::Enum(s) => s.provenance.as_ref(),
    }
}

/// Context for parsing config files.
struct FileParseContext<'a> {
    config: &'a FileConfig,
    /// The config struct schema, if present
    config_schema: Option<&'a ConfigStructSchema>,
    /// The config field name from schema (e.g., "config" or "settings")
    config_field_name: Option<&'a str>,
    /// Parsed config value (if successful)
    value: Option<ConfigValue>,
    /// Unused keys found in the file
    unused_keys: Vec<UnusedKey>,
    /// Diagnostics collected during parsing
    diagnostics: Vec<Diagnostic>,
    /// File resolution tracking
    resolution: FileResolution,
}

impl<'a> FileParseContext<'a> {
    fn new(schema: &'a Schema, config: &'a FileConfig) -> Self {
        let (config_field_name, config_schema) = if let Some(cs) = schema.config() {
            (cs.field_name(), Some(cs))
        } else {
            (None, None)
        };

        Self {
            config,
            config_schema,
            config_field_name,
            value: None,
            unused_keys: Vec::new(),
            diagnostics: Vec::new(),
            resolution: FileResolution::new(),
        }
    }

    fn parse(&mut self) {
        // Check for inline content first (used for testing)
        let (path, contents) = if let Some((content, filename)) = &self.config.inline_content {
            let path = Utf8PathBuf::from(filename);
            // Record this as a "picked" file in resolution for display purposes
            self.resolution.add_explicit(path.clone(), true);
            (path, content.clone())
        } else {
            // Resolve which file to load
            let path = match self.resolve_path() {
                Some(p) => p,
                None => return, // No file to load (not an error if no explicit path)
            };

            // Read the file
            let contents = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    self.emit_error(format!("failed to read {}: {}", path, e));
                    return;
                }
            };
            (path, contents)
        };

        // Parse the file
        let parsed = match self.config.registry.parse_file(&path, &contents) {
            Ok(v) => v,
            Err(e) => {
                self.emit_error(format!("failed to parse {}: {}", path, e));
                return;
            }
        };

        // Validate against schema and collect unused keys
        if let Some(config_schema) = self.config_schema {
            self.validate_and_collect_unused(&parsed, config_schema, Vec::new());
        }

        self.value = Some(parsed);
    }

    /// Resolve which file path to use.
    ///
    /// Returns Some(path) if a file should be loaded, None otherwise.
    fn resolve_path(&mut self) -> Option<Utf8PathBuf> {
        // If explicit path provided, use it (error if not found)
        if let Some(explicit) = &self.config.explicit_path {
            let exists = explicit.exists();
            self.resolution.add_explicit(explicit.clone(), exists);

            if exists {
                // Mark defaults as not tried
                self.resolution
                    .mark_defaults_not_tried(&self.config.default_paths);
                return Some(explicit.clone());
            } else {
                self.emit_error(format!("config file not found: {}", explicit));
                return None;
            }
        }

        // Try default paths in order
        for default_path in &self.config.default_paths {
            if default_path.exists() {
                self.resolution
                    .add_default(default_path.clone(), FilePathStatus::Picked);
                return Some(default_path.clone());
            } else {
                self.resolution
                    .add_default(default_path.clone(), FilePathStatus::Absent);
            }
        }

        // No file found - this is not an error (file layer is optional)
        None
    }

    /// Validate parsed values against the schema and collect unused keys.
    ///
    /// This function handles flattened fields by using `target_path` to understand
    /// the expected nested structure. The schema's fields are flattened, but the
    /// JSON/config file will have the original nested structure.
    fn validate_and_collect_unused(
        &mut self,
        value: &ConfigValue,
        schema: &ConfigStructSchema,
        path: Vec<String>,
    ) {
        // Build a set of valid paths from the schema's target_paths
        let valid_paths = self.build_valid_paths(schema);

        // Recursively validate against the valid paths
        self.validate_recursive(value, &valid_paths, path);
    }

    /// Build a set of all valid key paths from the schema.
    ///
    /// Each field in the schema is indexed by its effective name (after rename).
    /// Flattened fields appear directly in the schema at their effective name.
    fn build_valid_paths(&self, schema: &ConfigStructSchema) -> ValidPaths {
        let mut valid = ValidPaths::new();

        for (field_name, field_schema) in schema.fields() {
            // Add this field as a valid leaf path
            valid.add_leaf(vec![field_name.clone()]);

            // If it's a struct, recurse to add nested paths
            self.add_nested_paths(field_schema.value(), vec![field_name.clone()], &mut valid);
        }

        valid
    }

    /// Recursively add nested paths for struct fields.
    fn add_nested_paths(
        &self,
        value_schema: &ConfigValueSchema,
        prefix: Vec<String>,
        valid: &mut ValidPaths,
    ) {
        match value_schema {
            ConfigValueSchema::Struct(nested) => {
                for (name, field) in nested.fields() {
                    let mut path = prefix.clone();
                    path.push(name.clone());
                    valid.add_leaf(path.clone());
                    self.add_nested_paths(field.value(), path, valid);
                }
            }
            ConfigValueSchema::Option { value, .. } => {
                self.add_nested_paths(value, prefix, valid);
            }
            ConfigValueSchema::Vec(vec_schema) => {
                // For vec, we can't predict indices, but we mark the prefix as valid
                self.add_nested_paths(vec_schema.element(), prefix, valid);
            }
            ConfigValueSchema::Enum(enum_schema) => {
                // For enums, add paths for all variant fields
                for (_variant_name, variant_schema) in enum_schema.variants() {
                    for (field_name, field_schema) in variant_schema.fields() {
                        let mut path = prefix.clone();
                        path.push(field_name.clone());
                        valid.add_leaf(path.clone());
                        self.add_nested_paths(field_schema.value(), path, valid);
                    }
                }
            }
            ConfigValueSchema::Leaf(_) => {
                // Nothing more to add
            }
        }
    }

    /// Recursively validate config values against valid paths.
    fn validate_recursive(
        &mut self,
        value: &ConfigValue,
        valid_paths: &ValidPaths,
        path: Vec<String>,
    ) {
        if let ConfigValue::Object(obj) = value {
            for (key, val) in &obj.value {
                let mut key_path = path.clone();
                key_path.push(key.clone());

                if valid_paths.is_valid(&key_path) {
                    // Valid key - validate enum values and recurse if it's an object
                    self.validate_enum_value(val, &key_path);
                    if matches!(val, ConfigValue::Object(_)) {
                        self.validate_recursive(val, valid_paths, key_path);
                    }
                } else {
                    // Unknown key
                    let prov = get_provenance(val).cloned().unwrap_or(Provenance::Default);
                    self.unused_keys.push(UnusedKey {
                        key: key_path.clone(),
                        provenance: prov,
                    });

                    if self.config.strict {
                        self.emit_error(format!(
                            "unknown configuration key: {}",
                            key_path.join(".")
                        ));
                    }
                }
            }
        }
    }

    /// Validate that a string value at an enum path is a valid variant.
    fn validate_enum_value(&mut self, value: &ConfigValue, path: &[String]) {
        // Only check string values - other types will be caught by deserialization
        let string_value = match value {
            ConfigValue::String(s) => &s.value,
            _ => return,
        };

        // Get the schema for this path
        let Some(config_schema) = self.config_schema else {
            return;
        };
        let Some(value_schema) = config_schema.get_by_path(&path.to_vec()) else {
            return;
        };

        // Unwrap Option wrapper if present
        let inner_schema = match value_schema {
            ConfigValueSchema::Option { value: inner, .. } => inner.as_ref(),
            other => other,
        };

        // For enum fields, validate the value is a known variant
        if let ConfigValueSchema::Enum(enum_schema) = inner_schema {
            let variants = enum_schema.variants();
            if !variants.contains_key(string_value) {
                let valid_variants: Vec<&String> = variants.keys().collect();
                self.emit_warning(format!(
                    "unknown variant '{}' for {}. Valid variants are: {}",
                    string_value,
                    path.join("."),
                    valid_variants
                        .iter()
                        .map(|v| format!("'{}'", v))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
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

    fn emit_warning(&mut self, message: String) {
        self.diagnostics.push(Diagnostic {
            message,
            path: None,
            span: None,
            severity: Severity::Warning,
        });
    }

    fn into_result(self) -> FileParseResult {
        // Wrap the value under the config field name if needed
        let value = if let Some(parsed) = self.value {
            if let Some(field_name) = self.config_field_name {
                // Wrap: {config_field_name: parsed_value}
                let mut root = indexmap::IndexMap::default();
                root.insert(field_name.to_string(), parsed);
                Some(ConfigValue::Object(crate::config_value::Sourced {
                    value: root,
                    span: None,
                    provenance: None,
                }))
            } else {
                Some(parsed)
            }
        } else {
            None
        };

        FileParseResult {
            output: LayerOutput {
                value,
                unused_keys: self.unused_keys,
                diagnostics: self.diagnostics,
            },
            resolution: self.resolution,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as figue;
    use facet::Facet;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // ========================================================================
    // Test schemas
    // ========================================================================

    #[derive(Facet)]
    struct ArgsWithConfig {
        #[facet(figue::named)]
        verbose: bool,

        #[facet(figue::config)]
        config: ServerConfig,
    }

    #[derive(Facet)]
    struct ServerConfig {
        port: u16,
        host: String,
    }

    #[derive(Facet)]
    struct ArgsWithNestedConfig {
        #[facet(figue::config)]
        settings: AppSettings,
    }

    #[derive(Facet)]
    struct AppSettings {
        port: u16,
        smtp: SmtpConfig,
    }

    #[derive(Facet)]
    struct SmtpConfig {
        host: String,
        connection_timeout: u64,
    }

    // ========================================================================
    // Helper functions
    // ========================================================================

    fn create_temp_json(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::with_suffix(".json").unwrap();
        write!(file, "{}", content).unwrap();
        file
    }

    fn get_nested<'a>(cv: &'a ConfigValue, path: &[&str]) -> Option<&'a ConfigValue> {
        let mut current = cv;
        for key in path {
            match current {
                ConfigValue::Object(obj) => {
                    current = obj.value.get(*key)?;
                }
                _ => return None,
            }
        }
        Some(current)
    }

    fn get_integer(cv: &ConfigValue) -> Option<i64> {
        match cv {
            ConfigValue::Integer(i) => Some(i.value),
            _ => None,
        }
    }

    fn get_string(cv: &ConfigValue) -> Option<&str> {
        match cv {
            ConfigValue::String(s) => Some(&s.value),
            _ => None,
        }
    }

    // ========================================================================
    // Tests: Basic parsing
    // ========================================================================

    #[test]
    fn test_parse_simple_json() {
        let file = create_temp_json(r#"{"port": 8080, "host": "localhost"}"#);
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let config = FileConfig::new().path(path);

        let result = parse_file(&schema, &config);

        assert!(result.output.diagnostics.is_empty());
        assert!(result.output.unused_keys.is_empty());

        let value = result.output.value.expect("should have value");
        let port = get_nested(&value, &["config", "port"]).expect("config.port");
        assert_eq!(get_integer(port), Some(8080));

        let host = get_nested(&value, &["config", "host"]).expect("config.host");
        assert_eq!(get_string(host), Some("localhost"));
    }

    #[test]
    fn test_parse_nested_json() {
        let file = create_temp_json(
            r#"{"port": 8080, "smtp": {"host": "mail.example.com", "connection_timeout": 30}}"#,
        );
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithNestedConfig::SHAPE).unwrap();
        let config = FileConfig::new().path(path);

        let result = parse_file(&schema, &config);

        assert!(result.output.diagnostics.is_empty());
        let value = result.output.value.expect("should have value");

        let port = get_nested(&value, &["settings", "port"]).expect("settings.port");
        assert_eq!(get_integer(port), Some(8080));

        let smtp_host =
            get_nested(&value, &["settings", "smtp", "host"]).expect("settings.smtp.host");
        assert_eq!(get_string(smtp_host), Some("mail.example.com"));
    }

    // ========================================================================
    // Tests: File resolution
    // ========================================================================

    #[test]
    fn test_explicit_path_not_found() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let config = FileConfig::new().path("/nonexistent/config.json");

        let result = parse_file(&schema, &config);

        assert!(!result.output.diagnostics.is_empty());
        assert!(
            result
                .output
                .diagnostics
                .iter()
                .any(|d| d.message.contains("not found"))
        );
    }

    #[test]
    fn test_no_file_configured() {
        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let config = FileConfig::new(); // No path

        let result = parse_file(&schema, &config);

        // No file = no error, just no value
        assert!(result.output.diagnostics.is_empty());
        assert!(result.output.value.is_none());
    }

    #[test]
    fn test_default_paths_tried_in_order() {
        let file = create_temp_json(r#"{"port": 9000, "host": "default"}"#);
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let config = FileConfig::new().default_paths([
            Utf8PathBuf::from("/nonexistent/first.json"),
            path.clone(),
            Utf8PathBuf::from("/nonexistent/third.json"),
        ]);

        let result = parse_file(&schema, &config);

        assert!(result.output.diagnostics.is_empty());
        assert!(result.output.value.is_some());

        // Check resolution tracking
        assert_eq!(result.resolution.paths.len(), 2); // first absent, second picked
        assert!(matches!(
            result.resolution.paths[0].status,
            FilePathStatus::Absent
        ));
        assert!(matches!(
            result.resolution.paths[1].status,
            FilePathStatus::Picked
        ));
    }

    // ========================================================================
    // Tests: Unused keys
    // ========================================================================

    #[test]
    fn test_unknown_key_tracked() {
        let file = create_temp_json(r#"{"port": 8080, "host": "localhost", "unknown_field": 123}"#);
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let config = FileConfig::new().path(path);

        let result = parse_file(&schema, &config);

        // Should track the unknown key
        assert!(!result.output.unused_keys.is_empty());
        assert!(
            result
                .output
                .unused_keys
                .iter()
                .any(|k| k.key.contains(&"unknown_field".to_string()))
        );

        // But no error in non-strict mode
        assert!(result.output.diagnostics.is_empty());
    }

    #[test]
    fn test_unknown_key_error_in_strict_mode() {
        let file = create_temp_json(r#"{"port": 8080, "host": "localhost", "unknown_field": 123}"#);
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let config = FileConfig::new().path(path).strict();

        let result = parse_file(&schema, &config);

        // Should have error in strict mode
        assert!(!result.output.diagnostics.is_empty());
        assert!(
            result
                .output
                .diagnostics
                .iter()
                .any(|d| d.message.contains("unknown"))
        );
    }

    // ========================================================================
    // Tests: Provenance
    // ========================================================================

    #[test]
    fn test_file_provenance() {
        let file = create_temp_json(r#"{"port": 8080, "host": "localhost"}"#);
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithConfig::SHAPE).unwrap();
        let config = FileConfig::new().path(path.clone());

        let result = parse_file(&schema, &config);

        let value = result.output.value.expect("should have value");
        let port = get_nested(&value, &["config", "port"]).expect("config.port");

        // Check provenance is set
        let prov = get_provenance(port).expect("should have provenance");
        assert!(prov.is_file());
        if let Provenance::File {
            file: config_file, ..
        } = prov
        {
            assert_eq!(config_file.path, path);
        }
    }

    // ========================================================================
    // Tests: Format registry
    // ========================================================================

    #[test]
    fn test_format_registry_with_defaults() {
        let registry = FormatRegistry::with_defaults();
        assert!(registry.find_by_extension("json").is_some());
        assert!(registry.find_by_extension("JSON").is_some()); // case insensitive
        assert!(registry.find_by_extension("toml").is_none());
    }

    #[test]
    fn test_format_registry_extensions() {
        let registry = FormatRegistry::with_defaults();
        let extensions = registry.extensions();
        assert!(extensions.contains(&"json"));
    }

    // ========================================================================
    // Tests: Flatten support
    // ========================================================================

    /// Common config fields that can be flattened
    #[derive(Facet)]
    struct CommonConfig {
        /// Log level
        log_level: Option<String>,
        /// Debug mode
        debug: bool,
    }

    /// Config with flattened common fields
    #[derive(Facet)]
    struct ConfigWithFlatten {
        /// Application name
        name: String,
        /// Common settings
        #[facet(flatten)]
        common: CommonConfig,
    }

    #[derive(Facet)]
    struct ArgsWithFlattenedConfig {
        #[facet(figue::config)]
        config: ConfigWithFlatten,
    }

    #[test]
    fn test_flatten_config_parses_flat_json() {
        // JSON file has FLAT structure - flattened fields appear at the current level
        // NOT nested under "common"
        let file = create_temp_json(r#"{"name": "myapp", "log_level": "debug", "debug": true}"#);
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithFlattenedConfig::SHAPE).unwrap();
        let config = FileConfig::new().path(path);

        let result = parse_file(&schema, &config);

        // No errors - flatten should be handled
        assert!(
            result.output.diagnostics.is_empty(),
            "should have no errors: {:?}",
            result.output.diagnostics
        );
        assert!(
            result.output.unused_keys.is_empty(),
            "should have no unused keys: {:?}",
            result.output.unused_keys
        );

        let value = result.output.value.expect("should have value");

        // All fields appear at the config level (flat)
        let name = get_nested(&value, &["config", "name"]).expect("config.name");
        assert_eq!(get_string(name), Some("myapp"));

        let log_level = get_nested(&value, &["config", "log_level"]).expect("config.log_level");
        assert_eq!(get_string(log_level), Some("debug"));

        let debug = get_nested(&value, &["config", "debug"]).expect("config.debug");
        assert!(matches!(debug, ConfigValue::Bool(b) if b.value));
    }

    #[test]
    fn test_flatten_config_rejects_nested_json() {
        // JSON with nested "common" should be rejected - "common" is not a valid key
        // because it's flattened (its fields are hoisted to the parent level)
        let file = create_temp_json(
            r#"{"name": "myapp", "common": {"log_level": "debug", "debug": true}}"#,
        );
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithFlattenedConfig::SHAPE).unwrap();
        let config = FileConfig::new().path(path);

        let result = parse_file(&schema, &config);

        // Should have unused key for "common"
        assert!(
            result
                .output
                .unused_keys
                .iter()
                .any(|k| k.key.contains(&"common".to_string())),
            "should reject 'common' key: {:?}",
            result.output.unused_keys
        );
    }

    /// Database config for nested flatten test
    #[derive(Facet)]
    struct DatabaseConfig {
        /// Database host
        host: String,
        /// Database port
        port: u16,
    }

    /// Extended config with double flatten
    #[derive(Facet)]
    struct ExtendedConfig {
        #[facet(flatten)]
        common: CommonConfig,
        #[facet(flatten)]
        database: DatabaseConfig,
    }

    /// Config with deeply nested flatten
    #[derive(Facet)]
    struct ConfigWithNestedFlatten {
        app_name: String,
        #[facet(flatten)]
        extended: ExtendedConfig,
    }

    #[derive(Facet)]
    struct ArgsWithNestedFlattenConfig {
        #[facet(figue::config)]
        config: ConfigWithNestedFlatten,
    }

    #[test]
    fn test_two_level_flatten_config() {
        // Two levels of flattening: extended is flattened, and extended contains
        // flattened common and database. So ALL fields appear at the top level.
        let file = create_temp_json(
            r#"{
                "app_name": "super-app",
                "log_level": "info",
                "debug": false,
                "host": "db.example.com",
                "port": 5432
            }"#,
        );
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithNestedFlattenConfig::SHAPE).unwrap();
        let config = FileConfig::new().path(path);

        let result = parse_file(&schema, &config);

        assert!(
            result.output.diagnostics.is_empty(),
            "should have no errors: {:?}",
            result.output.diagnostics
        );
        assert!(
            result.output.unused_keys.is_empty(),
            "should have no unused keys: {:?}",
            result.output.unused_keys
        );

        let value = result.output.value.expect("should have value");

        // All fields appear at the config level (flat due to double flatten)
        let app_name = get_nested(&value, &["config", "app_name"]).expect("config.app_name");
        assert_eq!(get_string(app_name), Some("super-app"));

        let log_level = get_nested(&value, &["config", "log_level"]).expect("config.log_level");
        assert_eq!(get_string(log_level), Some("info"));

        let debug = get_nested(&value, &["config", "debug"]).expect("config.debug");
        assert!(matches!(debug, ConfigValue::Bool(b) if !b.value));

        let host = get_nested(&value, &["config", "host"]).expect("config.host");
        assert_eq!(get_string(host), Some("db.example.com"));

        let port = get_nested(&value, &["config", "port"]).expect("config.port");
        assert_eq!(get_integer(port), Some(5432));
    }

    #[test]
    fn test_flatten_config_unknown_key_detection() {
        // JSON with an unknown key at the flattened level
        let file = create_temp_json(
            r#"{"name": "myapp", "log_level": "debug", "debug": true, "unknown_field": 123}"#,
        );
        let path = Utf8PathBuf::from_path_buf(file.path().to_path_buf()).unwrap();

        let schema = Schema::from_shape(ArgsWithFlattenedConfig::SHAPE).unwrap();
        let config = FileConfig::new().path(path);

        let result = parse_file(&schema, &config);

        // Should detect unknown key
        assert!(
            result
                .output
                .unused_keys
                .iter()
                .any(|k| k.key.contains(&"unknown_field".to_string())),
            "should detect unknown key: {:?}",
            result.output.unused_keys
        );
    }

    // ========================================================================
    // Tests: Schema-guided enum validation
    // ========================================================================

    /// Log level enum for testing enum validation
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum LogLevel {
        Debug,
        Info,
        Warn,
        Error,
    }

    #[derive(Facet)]
    struct ConfigWithEnum {
        log_level: LogLevel,
        port: u16,
    }

    #[derive(Facet)]
    struct ArgsWithEnumConfig {
        #[facet(figue::config)]
        config: ConfigWithEnum,
    }

    #[test]
    fn test_enum_valid_variant_no_warning() {
        // Valid variant should not produce a warning
        let schema = Schema::from_shape(ArgsWithEnumConfig::SHAPE).unwrap();
        let config =
            FileConfig::new().content(r#"{"log_level": "Debug", "port": 8080}"#, "config.json");

        let result = parse_file(&schema, &config);

        assert!(
            result.output.diagnostics.is_empty(),
            "valid enum variant should not produce warnings: {:?}",
            result.output.diagnostics
        );
    }

    #[test]
    fn test_enum_invalid_variant_produces_warning() {
        // Invalid variant should produce a warning with helpful message
        let schema = Schema::from_shape(ArgsWithEnumConfig::SHAPE).unwrap();
        let config =
            FileConfig::new().content(r#"{"log_level": "Debugg", "port": 8080}"#, "config.json"); // typo

        let result = parse_file(&schema, &config);

        // Should have a warning
        assert!(
            !result.output.diagnostics.is_empty(),
            "invalid enum variant should produce a warning"
        );

        // Warning should mention the invalid value and valid variants
        let warning = &result.output.diagnostics[0];
        assert!(
            warning.message.contains("Debugg"),
            "warning should mention the invalid value: {}",
            warning.message
        );
        assert!(
            warning.message.contains("Debug")
                && warning.message.contains("Info")
                && warning.message.contains("Warn")
                && warning.message.contains("Error"),
            "warning should list valid variants: {}",
            warning.message
        );
    }

    #[derive(Facet)]
    struct ConfigWithOptionalEnum {
        log_level: Option<LogLevel>,
    }

    #[derive(Facet)]
    struct ArgsWithOptionalEnumConfig {
        #[facet(figue::config)]
        config: ConfigWithOptionalEnum,
    }

    #[test]
    fn test_optional_enum_validation() {
        // Optional enum should also be validated
        let schema = Schema::from_shape(ArgsWithOptionalEnumConfig::SHAPE).unwrap();
        let config = FileConfig::new().content(r#"{"log_level": "invalid"}"#, "config.json");

        let result = parse_file(&schema, &config);

        // Should have a warning even for optional enum
        assert!(
            !result.output.diagnostics.is_empty(),
            "invalid optional enum variant should produce a warning"
        );
    }

    #[derive(Facet)]
    struct NestedConfigWithEnum {
        logging: LoggingConfig,
    }

    #[derive(Facet)]
    struct LoggingConfig {
        level: LogLevel,
    }

    #[derive(Facet)]
    struct ArgsWithNestedEnumConfig {
        #[facet(figue::config)]
        config: NestedConfigWithEnum,
    }

    #[test]
    fn test_nested_enum_validation() {
        // Enum in nested struct should be validated
        let schema = Schema::from_shape(ArgsWithNestedEnumConfig::SHAPE).unwrap();
        let config =
            FileConfig::new().content(r#"{"logging": {"level": "unknown"}}"#, "config.json");

        let result = parse_file(&schema, &config);

        // Should have a warning
        assert!(
            !result.output.diagnostics.is_empty(),
            "invalid nested enum variant should produce a warning"
        );
    }
}
