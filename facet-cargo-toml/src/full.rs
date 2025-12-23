//! Comprehensive Cargo.toml parser using typed facet structs/enums
//!
//! This module provides a complete type-safe representation of Cargo.toml files,
//! supporting all features found in real-world manifests from ~/bearcove/.
//!
//! Unlike the v0.3 subset parser in lib.rs, this accepts everything and uses
//! proper types instead of facet_value::Value.

use std::collections::HashMap;

use facet::Facet;
use facet_toml::Spanned;

// ============================================================================
// Top-level manifest structure
// ============================================================================

/// Complete Cargo.toml manifest structure
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct CargoManifest {
    /// Package metadata
    pub package: Option<Package>,

    /// Workspace configuration
    pub workspace: Option<Workspace>,

    /// Regular dependencies
    pub dependencies: Option<HashMap<String, Dependency>>,

    /// Development dependencies
    pub dev_dependencies: Option<HashMap<String, Dependency>>,

    /// Build dependencies
    pub build_dependencies: Option<HashMap<String, Dependency>>,

    /// Target-specific dependencies (e.g., `[target.'cfg(unix)'.dependencies]`)
    pub target: Option<HashMap<String, TargetSpec>>,

    /// Library target configuration
    #[facet(rename = "lib")]
    pub lib: Option<LibTarget>,

    /// Binary targets
    #[facet(rename = "bin")]
    pub bin: Option<Vec<BinTarget>>,

    /// Test targets
    pub test: Option<Vec<TestTarget>>,

    /// Benchmark targets
    pub bench: Option<Vec<BenchTarget>>,

    /// Example targets
    pub example: Option<Vec<ExampleTarget>>,

    /// Feature flags
    pub features: Option<HashMap<String, Vec<String>>>,

    /// Patches for dependencies
    pub patch: Option<HashMap<String, HashMap<String, Dependency>>>,

    /// Build profiles
    pub profile: Option<HashMap<String, Profile>>,

    /// Lint configuration
    pub lints: Option<Lints>,

    /// Badges (deprecated but still used)
    pub badges: Option<HashMap<String, Badge>>,
}

// ============================================================================
// Package metadata
// ============================================================================

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct Package {
    pub name: Option<String>,
    pub version: Option<StringOrWorkspace>,
    pub authors: Option<VecOrWorkspace>,
    pub edition: Option<EditionOrWorkspace>,
    pub rust_version: Option<StringOrWorkspace>,
    pub description: Option<StringOrWorkspace>,
    pub documentation: Option<StringOrWorkspace>,
    pub readme: Option<StringOrWorkspace>,
    pub homepage: Option<StringOrWorkspace>,
    pub repository: Option<StringOrWorkspace>,
    pub license: Option<StringOrWorkspace>,
    pub license_file: Option<StringOrWorkspace>,
    pub keywords: Option<VecOrWorkspace>,
    pub categories: Option<VecOrWorkspace>,
    pub workspace: Option<StringOrWorkspace>,
    pub build: Option<StringOrBool>,
    pub links: Option<String>,
    pub exclude: Option<Vec<String>>,
    pub include: Option<Vec<String>>,
    pub publish: Option<BoolOrVec>,
    pub metadata: Option<facet_value::Value>,
    pub default_run: Option<String>,
    pub autolib: Option<bool>,
    pub autobins: Option<bool>,
    pub autoexamples: Option<bool>,
    pub autotests: Option<bool>,
    pub autobenches: Option<bool>,
    pub resolver: Option<Resolver>,
}

/// Edition can be a direct value or `workspace = true`
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum EditionOrWorkspace {
    Workspace(WorkspaceRef),
    Edition(Edition),
}

/// String field that can inherit from workspace
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum StringOrWorkspace {
    String(String),
    Workspace(WorkspaceRef),
}

/// Vec<String> field that can inherit from workspace
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum VecOrWorkspace {
    Values(Vec<String>),
    Workspace(WorkspaceRef),
}

/// Workspace inheritance marker
#[derive(Facet, Debug, Clone)]
pub struct WorkspaceRef {
    pub workspace: bool,
}

/// Build field can be string (path) or bool (false to disable)
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum StringOrBool {
    String(String),
    Bool(bool),
}

/// Publish field can be bool or array of registries
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum BoolOrVec {
    Bool(bool),
    Vec(Vec<String>),
}

/// Rust edition
#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Edition {
    #[facet(rename = "2015")]
    E2015,
    #[facet(rename = "2018")]
    E2018,
    #[facet(rename = "2021")]
    E2021,
    #[facet(rename = "2024")]
    E2024,
}

/// Dependency resolver version
#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Resolver {
    #[facet(rename = "1")]
    V1,
    #[facet(rename = "2")]
    V2,
    #[facet(rename = "3")]
    V3,
}

// ============================================================================
// Workspace
// ============================================================================

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct Workspace {
    pub members: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub default_members: Option<Vec<String>>,
    pub resolver: Option<Resolver>,
    pub metadata: Option<facet_value::Value>,

    /// Shared dependencies across workspace
    pub dependencies: Option<HashMap<String, Dependency>>,

    /// Shared package metadata
    pub package: Option<WorkspacePackage>,

    /// Shared lints
    pub lints: Option<Lints>,
}

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct WorkspacePackage {
    pub version: Option<String>,
    pub authors: Option<Vec<String>>,
    pub edition: Option<Edition>,
    pub rust_version: Option<String>,
    pub description: Option<String>,
    pub documentation: Option<String>,
    pub readme: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub license_file: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub categories: Option<Vec<String>>,
    pub publish: Option<BoolOrVec>,
}

// ============================================================================
// Dependencies
// ============================================================================

/// Dependency specification - can be version string or detailed table
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum Dependency {
    /// Simple version: `serde = "1.0"`
    Version(String),

    /// Workspace inheritance: `serde = { workspace = true }`
    /// Must come before Detailed to match workspace deps first
    Workspace(WorkspaceDependency),

    /// Detailed specification: `serde = { version = "1.0", features = [...] }`
    Detailed(DependencyDetail),
}

#[derive(Facet, Debug, Clone, Default)]
#[facet(rename_all = "kebab-case")]
pub struct DependencyDetail {
    /// Version requirement
    pub version: Option<String>,

    /// Path to local dependency
    pub path: Option<String>,

    /// Git repository URL (wrapped in Spanned for error reporting)
    pub git: Option<Spanned<String>>,

    /// Git branch
    pub branch: Option<String>,

    /// Git tag
    pub tag: Option<String>,

    /// Git revision (commit hash)
    pub rev: Option<String>,

    /// Alternative registry (wrapped in Spanned for error reporting)
    pub registry: Option<Spanned<String>>,

    /// Registry index URL (wrapped in Spanned for error reporting)
    pub registry_index: Option<Spanned<String>>,

    /// Package name if different from dependency key (wrapped in Spanned for error reporting)
    pub package: Option<Spanned<String>>,

    /// Enable specific features (wrapped in Spanned for error reporting)
    pub features: Option<Spanned<Vec<String>>>,

    /// Disable default features (wrapped in Spanned for error reporting)
    pub default_features: Option<Spanned<bool>>,

    /// Optional dependency (wrapped in Spanned for error reporting)
    pub optional: Option<Spanned<bool>>,

    /// Public dependency (used by public API)
    pub public: Option<bool>,

    /// Additional metadata
    pub metadata: Option<facet_value::Value>,
}

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case", deny_unknown_fields)]
pub struct WorkspaceDependency {
    /// Must be true - this is how we distinguish workspace deps from detailed deps
    pub workspace: bool,

    /// Override features from workspace
    pub features: Option<Vec<String>>,

    /// Override optional
    pub optional: Option<bool>,

    /// Override default-features
    pub default_features: Option<bool>,
}

// ============================================================================
// Target-specific configuration
// ============================================================================

#[derive(Facet, Debug, Clone, Default)]
#[facet(rename_all = "kebab-case")]
pub struct TargetSpec {
    /// Target-specific dependencies
    #[facet(default)]
    pub dependencies: Option<HashMap<String, Dependency>>,

    /// Target-specific dev dependencies
    #[facet(default)]
    pub dev_dependencies: Option<HashMap<String, Dependency>>,

    /// Target-specific build dependencies
    #[facet(default)]
    pub build_dependencies: Option<HashMap<String, Dependency>>,
}

// ============================================================================
// Build targets
// ============================================================================

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct LibTarget {
    pub name: Option<String>,
    pub path: Option<String>,
    pub test: Option<bool>,
    pub doctest: Option<bool>,
    pub bench: Option<bool>,
    pub doc: Option<bool>,
    pub plugin: Option<bool>,
    pub proc_macro: Option<bool>,
    pub harness: Option<bool>,
    pub edition: Option<Edition>,
    pub crate_type: Option<Vec<String>>,
    pub required_features: Option<Vec<String>>,
    pub doc_scrape_examples: Option<bool>,
}

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct BinTarget {
    pub name: Option<String>,
    pub path: Option<String>,
    pub test: Option<bool>,
    pub doctest: Option<bool>,
    pub bench: Option<bool>,
    pub doc: Option<bool>,
    pub plugin: Option<bool>,
    pub harness: Option<bool>,
    pub edition: Option<Edition>,
    pub required_features: Option<Vec<String>>,
}

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct TestTarget {
    pub name: Option<String>,
    pub path: Option<String>,
    pub test: Option<bool>,
    pub doctest: Option<bool>,
    pub bench: Option<bool>,
    pub doc: Option<bool>,
    pub plugin: Option<bool>,
    pub harness: Option<bool>,
    pub edition: Option<Edition>,
    pub required_features: Option<Vec<String>>,
}

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct BenchTarget {
    pub name: Option<String>,
    pub path: Option<String>,
    pub test: Option<bool>,
    pub doctest: Option<bool>,
    pub bench: Option<bool>,
    pub doc: Option<bool>,
    pub plugin: Option<bool>,
    pub harness: Option<bool>,
    pub edition: Option<Edition>,
    pub required_features: Option<Vec<String>>,
}

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct ExampleTarget {
    pub name: Option<String>,
    pub path: Option<String>,
    pub test: Option<bool>,
    pub doctest: Option<bool>,
    pub bench: Option<bool>,
    pub doc: Option<bool>,
    pub plugin: Option<bool>,
    pub harness: Option<bool>,
    pub edition: Option<Edition>,
    pub required_features: Option<Vec<String>>,
    pub crate_type: Option<Vec<String>>,
}

// ============================================================================
// Profiles
// ============================================================================

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct Profile {
    /// Optimization level (0-3, "s", "z")
    pub opt_level: Option<OptLevel>,

    /// Debug info level
    pub debug: Option<DebugLevel>,

    /// Enable debug assertions
    pub debug_assertions: Option<bool>,

    /// Overflow checks
    pub overflow_checks: Option<bool>,

    /// Link-time optimization
    pub lto: Option<Lto>,

    /// Panic strategy
    pub panic: Option<PanicStrategy>,

    /// Incremental compilation
    pub incremental: Option<bool>,

    /// Code generation units
    pub codegen_units: Option<u32>,

    /// Rpath
    pub rpath: Option<bool>,

    /// Strip symbols
    pub strip: Option<StripLevel>,

    /// Split debuginfo
    pub split_debuginfo: Option<String>,

    /// Inherit from another profile
    pub inherits: Option<String>,

    /// Per-package overrides
    pub package: Option<HashMap<String, PackageProfile>>,

    /// Build override settings
    #[facet(rename = "build-override")]
    pub build_override: Option<BuildOverride>,
}

#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum OptLevel {
    Number(u8),
    String(String), // "s" or "z"
}

#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum DebugLevel {
    Bool(bool),
    Number(u8),
    String(String), // "line-tables-only", "line-directives-only"
}

#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum Lto {
    Bool(bool),
    String(String), // "thin", "fat"
}

#[derive(Facet, Debug, Clone, Copy)]
#[repr(u8)]
pub enum PanicStrategy {
    #[facet(rename = "unwind")]
    Unwind,
    #[facet(rename = "abort")]
    Abort,
}

#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum StripLevel {
    Bool(bool),
    String(String), // "symbols", "debuginfo"
}

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct PackageProfile {
    pub opt_level: Option<OptLevel>,
    pub debug: Option<DebugLevel>,
    pub debug_assertions: Option<bool>,
    pub overflow_checks: Option<bool>,
    pub codegen_units: Option<u32>,
}

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct BuildOverride {
    pub opt_level: Option<OptLevel>,
    pub debug: Option<DebugLevel>,
    pub debug_assertions: Option<bool>,
    pub overflow_checks: Option<bool>,
    pub codegen_units: Option<u32>,
    pub incremental: Option<bool>,
}

// ============================================================================
// Lints
// ============================================================================

#[derive(Facet, Debug, Clone)]
pub struct Lints {
    pub workspace: Option<bool>,
    pub rust: Option<HashMap<String, LintLevel>>,
    pub clippy: Option<HashMap<String, LintLevel>>,
    pub rustdoc: Option<HashMap<String, LintLevel>>,
}

#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum LintLevel {
    /// Detailed config with priority (table format)
    Config(LintConfig),

    /// Simple string level: "forbid", "deny", "warn", or "allow"
    #[facet(rename = "forbid")]
    Forbid,
    #[facet(rename = "deny")]
    Deny,
    #[facet(rename = "warn")]
    Warn,
    #[facet(rename = "allow")]
    Allow,
}

#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct LintConfig {
    pub level: LintLevelString,
    pub priority: Option<i32>,
    pub check_cfg: Option<Vec<String>>,
}

#[derive(Facet, Debug, Clone, Copy)]
#[repr(u8)]
pub enum LintLevelString {
    #[facet(rename = "forbid")]
    Forbid,
    #[facet(rename = "deny")]
    Deny,
    #[facet(rename = "warn")]
    Warn,
    #[facet(rename = "allow")]
    Allow,
}

// ============================================================================
// Badges
// ============================================================================

#[derive(Facet, Debug, Clone)]
pub struct Badge {
    /// Badge-specific attributes (varies by badge type)
    #[facet(flatten)]
    pub attributes: facet_value::Value,
}

// ============================================================================
// Parsing API
// ============================================================================

impl CargoManifest {
    /// Parse Cargo.toml from a string
    pub fn parse(contents: &str) -> Result<Self, String> {
        facet_toml::from_str(contents).map_err(|e| e.to_string())
    }

    /// Parse Cargo.toml from a file path
    pub fn from_path(
        path: impl AsRef<std::path::Path>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        Ok(Self::parse(&contents)?)
    }
}
