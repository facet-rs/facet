//! Driver API for orchestrating layered configuration parsing, validation, and diagnostics.
//!
//! # Phases (planned)
//! 1. **Parse layers** using the schema:
//!    - CLI, env, file, defaults
//!    - collect `ConfigValue` trees + unknown keys + layer-specific diagnostics
//! 2. **Merge** layers by priority (CLI > env > file > defaults).
//! 3. **Validate**:
//!    - missing required keys
//!    - type coercion
//!    - deserialize into the target Facet type
//!    - facet-validate pass (if enabled)
//! 4. **Report**:
//!    - collect all diagnostics
//!    - render with pretty formatting (Ariadne + facet-pretty spans)
//!
//! This module is intentionally a skeleton for now. It defines the API surface
//! and types we want to stabilize while moving orchestration out of `builder`.

use alloc::string::String;
use alloc::vec::Vec;
use core::marker::PhantomData;

use crate::builder::Config;
use crate::config_value::ConfigValue;
use crate::path::Path;
use crate::provenance::{FileResolution, Provenance};
use crate::span::Span;
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

    /// Execute the driver and return a fully-typed value plus a report.
    pub fn run(self) -> Result<DriverOutput<T>, DriverError> {
        let _ = self.core;
        let _ = self.config;
        todo!("wire all phases and diagnostics here")
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

/// Full report of the driver execution.
///
/// The report should be pretty-renderable and capture all diagnostics,
/// plus optional supporting metadata (merge overrides, spans, etc).
#[derive(Debug, Default)]
pub struct DriverReport {
    /// Diagnostics emitted by the driver.
    pub diagnostics: Vec<Diagnostic>,
    /// Per-layer outputs, including unused keys and layer diagnostics.
    pub layers: ConfigLayers,
    /// File resolution metadata (paths tried, picked, etc).
    pub file_resolution: Option<FileResolution>,
}

impl DriverReport {
    /// Render the report for user-facing output.
    pub fn render_pretty(&self) -> String {
        let mut out = String::new();
        for diagnostic in &self.diagnostics {
            let _ = core::fmt::write(
                &mut out,
                format_args!("{}: {}\n", diagnostic.severity.as_str(), diagnostic.message),
            );
        }
        out
    }
}

/// A diagnostic message produced by the driver.
///
/// This is intentionally minimal and will grow as we integrate facet-pretty
/// spans and Ariadne rendering.
#[derive(Debug)]
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
#[derive(Debug, Clone, Copy)]
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

/// Error returned by the driver.
///
/// This is a wrapper around a report; both Display and Debug should render
/// the full diagnostics.
pub struct DriverError {
    /// Report that can be rendered for the user.
    pub report: DriverReport,
}

impl core::fmt::Display for DriverError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.report.render_pretty())
    }
}

impl core::fmt::Debug for DriverError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl core::error::Error for DriverError {}
