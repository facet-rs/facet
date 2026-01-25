//! Provenance tracking for layered configuration.
//!
//! This module provides types for tracking where configuration values came from,
//! enabling rich error messages and debugging output.
//!
//! # Example
//!
//! ```rust,ignore
//! use facet_args::provenance::{Provenance, ConfigFile};
//! use std::sync::Arc;
//!
//! // Track a value from CLI
//! let cli_prov = Provenance::Cli {
//!     arg: "--config.port".into(),
//!     value: "8080".into(),
//! };
//!
//! // Track a value from environment
//! let env_prov = Provenance::Env {
//!     var: "REEF__PORT".into(),
//!     value: "9000".into(),
//! };
//!
//! // Track a value from a config file
//! let file = Arc::new(ConfigFile::new("config.json", r#"{"port": 8080}"#));
//! let file_prov = Provenance::File {
//!     file: file.clone(),
//!     key_path: "port".into(),
//!     offset: 9,
//!     len: 4,
//! };
//! ```

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

use camino::Utf8PathBuf;
use facet::Facet;
use indexmap::IndexMap;

use crate::config_value::ConfigValue;

/// Type alias for IndexMap with default hasher.
type ProvenanceMap = IndexMap<String, Provenance, std::hash::RandomState>;

/// Information about a loaded config file.
///
/// This is reference-counted so it can be shared across all values
/// that originated from the same file without duplicating the path
/// and contents.
#[derive(Debug, Clone, Facet)]
pub struct ConfigFile {
    /// Path to the config file (UTF-8).
    pub path: Utf8PathBuf,
    /// Full contents of the file (kept for error reporting with ariadne).
    pub contents: String,
}

impl ConfigFile {
    /// Create a new ConfigFile from a path and contents.
    pub fn new(path: impl Into<Utf8PathBuf>, contents: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            contents: contents.into(),
        }
    }
}

/// The origin of a configuration value.
///
/// This tracks where a value came from in the layered configuration system,
/// enabling detailed error messages and config dumps.
#[repr(u8)]
#[derive(Debug, Clone, Default, facet::Facet)]
pub enum Provenance {
    /// Value came from a CLI argument.
    Cli {
        /// The CLI argument string, e.g. "--config.port" or "-p".
        arg: String,
        /// The raw value provided, e.g. "8080".
        value: String,
    },

    /// Value came from an environment variable.
    Env {
        /// The environment variable name, e.g. "REEF__PORT".
        var: String,
        /// The raw value from the environment.
        value: String,
    },

    /// Value came from a config file.
    File {
        /// The config file (shared reference).
        file: Arc<ConfigFile>,
        /// The key path within the file, e.g. "smtp.host".
        key_path: String,
        /// Byte offset in the file where the value starts.
        offset: usize,
        /// Length in bytes of the value in the source.
        len: usize,
    },

    /// Value came from a `#[facet(default = ...)]` attribute.
    #[default]
    Default,
}

impl Provenance {
    /// Create a CLI provenance.
    pub fn cli(arg: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Cli {
            arg: arg.into(),
            value: value.into(),
        }
    }

    /// Create an environment variable provenance.
    pub fn env(var: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Env {
            var: var.into(),
            value: value.into(),
        }
    }

    /// Create a file provenance.
    pub fn file(
        file: Arc<ConfigFile>,
        key_path: impl Into<String>,
        offset: usize,
        len: usize,
    ) -> Self {
        Self::File {
            file,
            key_path: key_path.into(),
            offset,
            len,
        }
    }

    /// Check if this provenance is from CLI.
    pub fn is_cli(&self) -> bool {
        matches!(self, Self::Cli { .. })
    }

    /// Check if this provenance is from environment.
    pub fn is_env(&self) -> bool {
        matches!(self, Self::Env { .. })
    }

    /// Check if this provenance is from a file.
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File { .. })
    }

    /// Check if this provenance is a default value.
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }

    /// Get the priority of this provenance source.
    ///
    /// Higher numbers mean higher priority:
    /// - CLI: 3 (highest)
    /// - Env: 2
    /// - File: 1
    /// - Default: 0 (lowest)
    pub fn priority(&self) -> u8 {
        match self {
            Self::Cli { .. } => 3,
            Self::Env { .. } => 2,
            Self::File { .. } => 1,
            Self::Default => 0,
        }
    }

    /// Get a human-readable description of the source.
    pub fn source_description(&self) -> String {
        match self {
            Self::Cli { arg, .. } => format!("CLI: {arg}"),
            Self::Env { var, .. } => format!("env: {var}"),
            Self::File { file, key_path, .. } => format!("{}: {key_path}", file.path),
            Self::Default => "default".into(),
        }
    }
}

impl core::fmt::Display for Provenance {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Cli { arg, .. } => write!(f, "from CLI argument {arg}"),
            Self::Env { var, .. } => write!(f, "from environment variable {var}"),
            Self::File { file, key_path, .. } => {
                write!(f, "from {}: {key_path}", file.path)
            }
            Self::Default => write!(f, "from default"),
        }
    }
}

/// A record of when a higher-priority layer overrode a lower-priority one.
#[derive(Debug, Clone)]
pub struct Override {
    /// The configuration path that was overridden, e.g. "config.port".
    pub path: String,
    /// The winning provenance (higher priority).
    pub winner: Provenance,
    /// The losing provenance (lower priority, was overridden).
    pub loser: Provenance,
}

impl Override {
    /// Create a new override record.
    pub fn new(path: impl Into<String>, winner: Provenance, loser: Provenance) -> Self {
        Self {
            path: path.into(),
            winner,
            loser,
        }
    }
}

impl core::fmt::Display for Override {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}: {} overrides {}",
            self.path,
            self.winner.source_description(),
            self.loser.source_description()
        )
    }
}

/// The result of parsing layered configuration, including provenance tracking.
#[derive(Debug)]
pub struct ConfigResult<T> {
    /// The resolved configuration value.
    pub value: T,

    /// Map from config paths to their provenance.
    ///
    /// Keys are dot-separated paths like "config.port" or "config.smtp.host".
    #[deprecated(
        note = "preovnance is stored inline in ConfigMap, I don't believe it's necessary here"
    )]
    pub provenance: ProvenanceMap,

    /// Records of values that were overridden by higher-priority layers.
    pub overrides: Vec<Override>,

    /// Information about config file path resolution.
    pub file_resolution: FileResolution,
}

impl<T> ConfigResult<T> {
    /// Create a new config result with just a value (no provenance tracking).
    #[deprecated(note = "we always want full provenance")]
    pub fn new(value: T) -> Self {
        Self {
            value,
            provenance: ProvenanceMap::default(),
            overrides: Vec::new(),
            file_resolution: FileResolution::new(),
        }
    }

    /// Create a new config result with provenance tracking.
    #[deprecated(note = "we always want full provenance")]
    pub fn with_provenance(value: T, provenance: ProvenanceMap, overrides: Vec<Override>) -> Self {
        Self {
            value,
            provenance,
            overrides,
            file_resolution: FileResolution::new(),
        }
    }

    /// Create a new config result with full tracking.
    #[deprecated(note = "we always want full provenance")]
    pub fn with_full_tracking(
        value: T,
        provenance: ProvenanceMap,
        overrides: Vec<Override>,
        file_resolution: FileResolution,
    ) -> Self {
        Self {
            value,
            provenance,
            overrides,
            file_resolution,
        }
    }

    /// Get the provenance for a specific path.
    pub fn get_provenance(&self, path: &str) -> Option<&Provenance> {
        self.provenance.get(path)
    }

    /// Check if any values were overridden.
    pub fn has_overrides(&self) -> bool {
        !self.overrides.is_empty()
    }
}

/// Status of a config file path during resolution.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
pub enum FilePathStatus {
    /// Path was picked and loaded successfully.
    Picked,
    /// Path exists but was not tried (explicit --config was provided).
    NotTried,
    /// Path does not exist.
    Absent,
}

/// Information about config file path resolution.
#[derive(Facet, Debug, Clone)]
pub struct FilePathResolution {
    /// The path that was checked.
    pub path: Utf8PathBuf,

    /// The status of this path.
    pub status: FilePathStatus,

    /// Whether this path came from explicit --config flag.
    pub explicit: bool,
}

/// Result of config file resolution, tracking all paths that were considered.
#[derive(Facet, Debug, Clone, Default)]
pub struct FileResolution {
    /// All paths that were considered, in order.
    pub paths: Vec<FilePathResolution>,

    /// Whether an explicit --config path was provided.
    pub had_explicit: bool,
}

impl FileResolution {
    /// Create a new empty file resolution.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an explicit path that was provided via --config.
    pub fn add_explicit(&mut self, path: Utf8PathBuf, exists: bool) {
        self.had_explicit = true;
        self.paths.push(FilePathResolution {
            path,
            status: if exists {
                FilePathStatus::Picked
            } else {
                FilePathStatus::Absent
            },
            explicit: true,
        });
    }

    /// Add a default path that was checked.
    pub fn add_default(&mut self, path: Utf8PathBuf, status: FilePathStatus) {
        self.paths.push(FilePathResolution {
            path,
            status,
            explicit: false,
        });
    }

    /// Mark remaining default paths as not tried (because explicit was used).
    pub fn mark_defaults_not_tried(&mut self, default_paths: &[Utf8PathBuf]) {
        for path in default_paths {
            self.paths.push(FilePathResolution {
                path: path.clone(),
                status: FilePathStatus::NotTried,
                explicit: false,
            });
        }
    }
}

/// Builder for tracking provenance during config resolution.
#[derive(Debug, Default)]
pub struct ProvenanceTracker {
    /// Current provenance map.
    provenance: ProvenanceMap,
    /// Recorded overrides.
    overrides: Vec<Override>,
}

impl ProvenanceTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a provenance entry, potentially recording an override.
    ///
    /// If a value already exists at this path with a lower priority,
    /// it will be recorded as overridden.
    pub fn record(&mut self, path: impl Into<String>, provenance: Provenance) {
        let path = path.into();
        if let Some(existing) = self.provenance.get(&path) {
            // Record override if new provenance has higher priority
            if provenance.priority() > existing.priority() {
                self.overrides.push(Override::new(
                    path.clone(),
                    provenance.clone(),
                    existing.clone(),
                ));
            }
        }
        self.provenance.insert(path, provenance);
    }

    /// Record a provenance entry without checking for overrides.
    ///
    /// Use this when you know the entry doesn't exist yet.
    pub fn record_new(&mut self, path: impl Into<String>, provenance: Provenance) {
        self.provenance.insert(path.into(), provenance);
    }

    /// Finish tracking and produce the final maps.
    pub fn finish(self) -> (ProvenanceMap, Vec<Override>) {
        (self.provenance, self.overrides)
    }

    /// Build a ConfigResult with the tracked provenance.
    pub fn into_result<T>(self, value: T) -> ConfigResult<T> {
        let (provenance, overrides) = self.finish();
        ConfigResult::with_provenance(value, provenance, overrides)
    }
}

/// Collect provenance from all values in a ConfigValue tree.
pub(crate) fn collect_provenance(
    value: &ConfigValue,
    path: &str,
) -> indexmap::IndexMap<String, Provenance, std::hash::RandomState> {
    let mut map = indexmap::IndexMap::default();
    collect_provenance_inner(value, path, &mut map);
    map
}

fn collect_provenance_inner(
    value: &ConfigValue,
    path: &str,
    map: &mut indexmap::IndexMap<String, Provenance, std::hash::RandomState>,
) {
    let prov = match value {
        ConfigValue::Null(s) => s.provenance.as_ref(),
        ConfigValue::Bool(s) => s.provenance.as_ref(),
        ConfigValue::Integer(s) => s.provenance.as_ref(),
        ConfigValue::Float(s) => s.provenance.as_ref(),
        ConfigValue::String(s) => s.provenance.as_ref(),
        ConfigValue::Array(s) => s.provenance.as_ref(),
        ConfigValue::Object(s) => s.provenance.as_ref(),
        ConfigValue::Enum(s) => s.provenance.as_ref(),
        ConfigValue::Missing(_) => None, // Missing values have no provenance
    };

    if let Some(prov) = prov
        && !path.is_empty()
    {
        map.insert(path.to_string(), prov.clone());
    }

    // Recurse into children
    match value {
        ConfigValue::Array(arr) => {
            for (i, item) in arr.value.iter().enumerate() {
                let item_path = if path.is_empty() {
                    format!("{i}")
                } else {
                    format!("{path}[{i}]")
                };
                collect_provenance_inner(item, &item_path, map);
            }
        }
        ConfigValue::Object(obj) => {
            for (key, val) in &obj.value {
                let key_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect_provenance_inner(val, &key_path, map);
            }
        }
        ConfigValue::Enum(e) => {
            for (key, val) in &e.value.fields {
                let key_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                collect_provenance_inner(val, &key_path, map);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provenance_priority() {
        assert!(
            Provenance::cli("--port", "8080").priority()
                > Provenance::env("PORT", "9000").priority()
        );
        assert!(Provenance::env("PORT", "9000").priority() > Provenance::Default.priority());

        let file = Arc::new(ConfigFile::new("config.json", "{}"));
        let file_prov = Provenance::file(file, "port", 0, 4);
        assert!(Provenance::env("PORT", "9000").priority() > file_prov.priority());
        assert!(file_prov.priority() > Provenance::Default.priority());
    }

    #[test]
    fn test_provenance_display() {
        let cli = Provenance::cli("--config.port", "8080");
        assert!(cli.to_string().contains("--config.port"));

        let env = Provenance::env("REEF__PORT", "9000");
        assert!(env.to_string().contains("REEF__PORT"));

        let file = Arc::new(ConfigFile::new("config.json", "{}"));
        let file_prov = Provenance::file(file, "port", 0, 4);
        assert!(file_prov.to_string().contains("config.json"));
        assert!(file_prov.to_string().contains("port"));

        let default = Provenance::Default;
        assert!(default.to_string().contains("default"));
    }

    #[test]
    fn test_provenance_is_checks() {
        assert!(Provenance::cli("--port", "8080").is_cli());
        assert!(!Provenance::cli("--port", "8080").is_env());

        assert!(Provenance::env("PORT", "9000").is_env());
        assert!(!Provenance::env("PORT", "9000").is_cli());

        let file = Arc::new(ConfigFile::new("config.json", "{}"));
        assert!(Provenance::file(file, "port", 0, 4).is_file());

        assert!(Provenance::Default.is_default());
    }

    #[test]
    fn test_config_file() {
        let file = ConfigFile::new("config.json", r#"{"port": 8080}"#);
        assert_eq!(file.path, "config.json");
        assert!(file.contents.contains("port"));
    }

    #[test]
    fn test_override_display() {
        let ovr = Override::new(
            "config.port",
            Provenance::cli("--config.port", "8080"),
            Provenance::env("REEF__PORT", "9000"),
        );
        let display = ovr.to_string();
        assert!(display.contains("config.port"));
        assert!(display.contains("CLI"));
        assert!(display.contains("env"));
    }

    #[test]
    fn test_config_result() {
        let result = ConfigResult::new(42);
        assert_eq!(result.value, 42);
        assert!(result.provenance.is_empty());
        assert!(!result.has_overrides());
    }

    #[test]
    fn test_provenance_tracker() {
        let mut tracker = ProvenanceTracker::new();

        // First, record a file value
        let file = Arc::new(ConfigFile::new("config.json", "{}"));
        tracker.record("config.port", Provenance::file(file, "port", 0, 4));

        // Then override with env
        tracker.record("config.port", Provenance::env("REEF__PORT", "9000"));

        // Then override with CLI
        tracker.record("config.port", Provenance::cli("--config.port", "8080"));

        let (provenance, overrides) = tracker.finish();

        // Final provenance should be CLI
        assert!(provenance.get("config.port").unwrap().is_cli());

        // Should have two override records
        assert_eq!(overrides.len(), 2);
    }

    #[test]
    fn test_provenance_tracker_into_result() {
        let mut tracker = ProvenanceTracker::new();
        tracker.record_new("config.port", Provenance::cli("--port", "8080"));

        let result = tracker.into_result(8080u16);
        assert_eq!(result.value, 8080);
        assert!(result.get_provenance("config.port").is_some());
    }
}
