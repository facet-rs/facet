//! Cargo.toml manifest types.

use std::collections::HashMap;

use facet::Facet;

/// Re-export Spanned from facet_reflect.
pub use facet_reflect::Spanned;

/// A parsed `Cargo.toml` manifest.
///
/// This struct represents the complete structure of a Cargo manifest file,
/// including package metadata, dependencies, build targets, and workspace configuration.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct CargoToml {
    /// The `[package]` section containing crate metadata.
    pub package: Option<Package>,

    /// The `[workspace]` section for multi-crate workspaces.
    pub workspace: Option<Workspace>,

    /// Regular dependencies from `[dependencies]`.
    pub dependencies: Option<HashMap<String, Dependency>>,

    /// Development dependencies from `[dev-dependencies]`.
    pub dev_dependencies: Option<HashMap<String, Dependency>>,

    /// Build script dependencies from `[build-dependencies]`.
    pub build_dependencies: Option<HashMap<String, Dependency>>,

    /// Target-specific dependencies from `[target.'cfg(...)'.dependencies]`.
    pub target: Option<HashMap<String, TargetSpec>>,

    /// Library target configuration from `[lib]`.
    #[facet(rename = "lib")]
    pub lib: Option<LibTarget>,

    /// Binary targets from `[[bin]]`.
    #[facet(rename = "bin")]
    pub bin: Option<Vec<BinTarget>>,

    /// Test targets from `[[test]]`.
    pub test: Option<Vec<TestTarget>>,

    /// Benchmark targets from `[[bench]]`.
    pub bench: Option<Vec<BenchTarget>>,

    /// Example targets from `[[example]]`.
    pub example: Option<Vec<ExampleTarget>>,

    /// Feature flags from `[features]`.
    pub features: Option<HashMap<String, Vec<String>>>,

    /// Dependency patches from `[patch]`.
    pub patch: Option<HashMap<String, HashMap<String, Dependency>>>,

    /// Build profiles from `[profile.*]`.
    pub profile: Option<HashMap<String, Profile>>,

    /// Lint configuration from `[lints]`.
    pub lints: Option<Lints>,

    /// Badges from `[badges]` (deprecated).
    pub badges: Option<HashMap<String, Badge>>,
}

/// The `[package]` section of a Cargo.toml.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct Package {
    /// The package identifier used in dependencies and as the default name for targets.
    pub name: Option<Spanned<String>>,
    /// The package version following SemVer format (e.g., `1.0.0`).
    pub version: Option<StringOrWorkspace>,
    /// People or organizations considered package authors (deprecated).
    pub authors: Option<VecOrWorkspace>,
    /// The Rust edition used for compilation (e.g., `2021`, `2024`).
    pub edition: Option<EditionOrWorkspace>,
    /// The minimum supported Rust toolchain version for the package.
    pub rust_version: Option<StringOrWorkspace>,
    /// A short text blurb about the package displayed on registries.
    pub description: Option<StringOrWorkspace>,
    /// URL to the crate's documentation website.
    pub documentation: Option<StringOrWorkspace>,
    /// Path to the README file relative to Cargo.toml, or `false` to disable.
    pub readme: Option<StringOrBoolOrWorkspace>,
    /// URL of the package's home page.
    pub homepage: Option<StringOrWorkspace>,
    /// URL to the package's source repository.
    pub repository: Option<StringOrWorkspace>,
    /// SPDX 2.3 license expression (e.g., `MIT OR Apache-2.0`).
    pub license: Option<StringOrWorkspace>,
    /// Path to a license text file for nonstandard licenses.
    pub license_file: Option<StringOrWorkspace>,
    /// Up to 5 searchable keywords for registry discoverability.
    pub keywords: Option<VecOrWorkspace>,
    /// Up to 5 categories from crates.io's predefined list.
    pub categories: Option<VecOrWorkspace>,
    /// Path to the workspace root directory.
    pub workspace: Option<StringOrWorkspace>,
    /// Path to the build script file, or `false` to disable auto-detection.
    pub build: Option<StringOrBool>,
    /// Name of the native library being linked by a build script.
    pub links: Option<Spanned<String>>,
    /// Gitignore-style patterns for files to exclude when publishing.
    pub exclude: Option<Spanned<Vec<String>>>,
    /// Gitignore-style patterns for files to explicitly include when publishing.
    pub include: Option<Spanned<Vec<String>>>,
    /// Controls publishing to registries; array of registry names or `false` to prevent publishing.
    pub publish: Option<BoolOrVec>,
    /// A table for external tool configuration, ignored by Cargo.
    pub metadata: Option<facet_value::Value>,
    /// The default binary selected by `cargo run` when multiple binaries exist.
    pub default_run: Option<Spanned<String>>,
    /// Disables automatic library target discovery.
    pub autolib: Option<Spanned<bool>>,
    /// Disables automatic binary target discovery.
    pub autobins: Option<Spanned<bool>>,
    /// Disables automatic example target discovery.
    pub autoexamples: Option<Spanned<bool>>,
    /// Disables automatic test target discovery.
    pub autotests: Option<Spanned<bool>>,
    /// Disables automatic benchmark target discovery.
    pub autobenches: Option<Spanned<bool>>,
    /// Sets which dependency resolver version to use.
    pub resolver: Option<Resolver>,
}

/// A value that can be a direct edition or inherited from workspace.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum EditionOrWorkspace {
    /// Direct edition value.
    Edition(Spanned<Edition>),
    /// Inherited from `[workspace.package]`.
    Workspace(WorkspaceRef),
}

/// A value that can be a direct string or inherited from workspace.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum StringOrWorkspace {
    /// Inherited from `[workspace.package]`.
    Workspace(WorkspaceRef),
    /// Direct string value.
    String(Spanned<String>),
}

/// A value that can be a direct array or inherited from workspace.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum VecOrWorkspace {
    /// Inherited from `[workspace.package]`.
    Workspace(WorkspaceRef),
    /// Direct array value.
    Values(Spanned<Vec<String>>),
}

/// Workspace inheritance marker (`{ workspace = true }`).
#[derive(Facet, Debug, Clone)]
pub struct WorkspaceRef {
    /// Must be `true` to indicate workspace inheritance.
    pub workspace: Spanned<bool>,
}

/// A value that can be a string, boolean, or inherited from workspace.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum StringOrBoolOrWorkspace {
    /// Inherited from `[workspace.package]`.
    Workspace(WorkspaceRef),
    /// A boolean value (e.g., `false` to disable).
    Bool(Spanned<bool>),
    /// A string value (typically a path).
    String(Spanned<String>),
}

/// A value that can be a string path or boolean.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum StringOrBool {
    /// A string value (typically a path).
    String(Spanned<String>),
    /// A boolean value.
    Bool(Spanned<bool>),
}

/// A value that can be a boolean or array of strings.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum BoolOrVec {
    /// A boolean value.
    Bool(Spanned<bool>),
    /// An array of strings.
    Vec(Spanned<Vec<String>>),
}

/// Rust edition year.
#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Edition {
    /// Rust 2015 edition.
    #[facet(rename = "2015")]
    E2015,
    /// Rust 2018 edition.
    #[facet(rename = "2018")]
    E2018,
    /// Rust 2021 edition.
    #[facet(rename = "2021")]
    E2021,
    /// Rust 2024 edition.
    #[facet(rename = "2024")]
    E2024,
}

/// Cargo dependency resolver version.
#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Resolver {
    /// Version 1 resolver.
    #[facet(rename = "1")]
    V1,
    /// Version 2 resolver (default for edition 2021+).
    #[facet(rename = "2")]
    V2,
    /// Version 3 resolver (default for edition 2024+).
    #[facet(rename = "3")]
    V3,
}

/// The `[workspace]` section of a Cargo.toml.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct Workspace {
    /// Packages to include in the workspace.
    pub members: Option<Spanned<Vec<String>>>,
    /// Packages to exclude from the workspace.
    pub exclude: Option<Spanned<Vec<String>>>,
    /// Packages to operate on when in workspace root without package selection flags.
    pub default_members: Option<Spanned<Vec<String>>>,
    /// Sets the dependency resolver to use.
    pub resolver: Option<Spanned<Resolver>>,
    /// Extra settings for external tools (ignored by Cargo).
    pub metadata: Option<facet_value::Value>,
    /// Shared dependencies for workspace members to inherit.
    pub dependencies: Option<HashMap<String, Dependency>>,
    /// Shared package metadata for workspace members to inherit.
    pub package: Option<WorkspacePackage>,
    /// Shared lint configuration for workspace members to inherit.
    pub lints: Option<Lints>,
}

/// Inheritable package metadata in `[workspace.package]`.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct WorkspacePackage {
    /// The package version following SemVer format.
    pub version: Option<Spanned<String>>,
    /// People or organizations considered package authors.
    pub authors: Option<Spanned<Vec<String>>>,
    /// The Rust edition used for compilation.
    pub edition: Option<Spanned<Edition>>,
    /// The minimum supported Rust toolchain version.
    pub rust_version: Option<Spanned<String>>,
    /// A short text blurb about the package.
    pub description: Option<Spanned<String>>,
    /// URL to the crate's documentation website.
    pub documentation: Option<Spanned<String>>,
    /// Path to the README file, or `false` to disable.
    pub readme: Option<StringOrBool>,
    /// URL of the package's home page.
    pub homepage: Option<Spanned<String>>,
    /// URL to the package's source repository.
    pub repository: Option<Spanned<String>>,
    /// SPDX 2.3 license expression.
    pub license: Option<Spanned<String>>,
    /// Path to a license text file.
    pub license_file: Option<Spanned<String>>,
    /// Searchable keywords for registry discoverability.
    pub keywords: Option<Spanned<Vec<String>>>,
    /// Categories from crates.io's predefined list.
    pub categories: Option<Spanned<Vec<String>>>,
    /// Controls publishing to registries.
    pub publish: Option<BoolOrVec>,
}

/// A dependency specification.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum Dependency {
    /// Simple version string: `aho-corasick = "1.0"`.
    Version(Spanned<String>),
    /// Workspace inheritance: `aho-corasick = { workspace = true }`.
    Workspace(WorkspaceDependency),
    /// Detailed specification: `aho-corasick = { version = "1.0", features = [...] }`.
    Detailed(DependencyDetail),
}

/// Detailed dependency specification.
#[derive(Facet, Debug, Clone, Default)]
#[facet(rename_all = "kebab-case")]
pub struct DependencyDetail {
    /// The version requirement string (e.g., `"1.2.3"`, `"^1.2"`, `">=1, <2"`).
    pub version: Option<Spanned<String>>,
    /// A file system path to a local crate directory.
    pub path: Option<Spanned<String>>,
    /// A URL to a Git repository containing the crate source code.
    pub git: Option<Spanned<String>>,
    /// The Git branch to use when fetching a Git dependency.
    pub branch: Option<Spanned<String>>,
    /// A Git tag specifying an exact release or commit.
    pub tag: Option<Spanned<String>>,
    /// A Git revision such as a commit hash or named reference.
    pub rev: Option<Spanned<String>>,
    /// The name of an alternative registry to fetch the dependency from.
    pub registry: Option<Spanned<String>>,
    /// The URL of a registry index to use directly.
    pub registry_index: Option<Spanned<String>>,
    /// The actual crate name on the registry when renaming locally.
    pub package: Option<Spanned<String>>,
    /// A list of crate features to enable for this dependency.
    pub features: Option<Spanned<Vec<String>>>,
    /// Whether to include the dependency's default features.
    pub default_features: Option<Spanned<bool>>,
    /// Whether this dependency is optional (enabled via features).
    pub optional: Option<Spanned<bool>>,
    /// Whether this dependency is part of the crate's public API (unstable).
    pub public: Option<Spanned<bool>>,
    /// Additional metadata for external tools.
    pub metadata: Option<facet_value::Value>,
}

/// Workspace dependency inheritance with optional overrides.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case", deny_unknown_fields)]
pub struct WorkspaceDependency {
    /// Must be `true` to indicate workspace inheritance.
    pub workspace: Spanned<bool>,
    /// Override features from the workspace dependency.
    pub features: Option<Spanned<Vec<String>>>,
    /// Override the optional setting from the workspace dependency.
    pub optional: Option<Spanned<bool>>,
    /// Override the default-features setting from the workspace dependency.
    pub default_features: Option<Spanned<bool>>,
}

/// Target-specific configuration from `[target.'cfg(...)']`.
#[derive(Facet, Debug, Clone, Default)]
#[facet(rename_all = "kebab-case")]
pub struct TargetSpec {
    /// Target-specific regular dependencies.
    #[facet(default)]
    pub dependencies: Option<HashMap<String, Dependency>>,
    /// Target-specific development dependencies.
    #[facet(default)]
    pub dev_dependencies: Option<HashMap<String, Dependency>>,
    /// Target-specific build dependencies.
    #[facet(default)]
    pub build_dependencies: Option<HashMap<String, Dependency>>,
}

/// Library target configuration from `[lib]`.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct LibTarget {
    /// The name of the library (defaults to package name with hyphens replaced by underscores).
    pub name: Option<Spanned<String>>,
    /// The source file of the target, relative to Cargo.toml.
    pub path: Option<Spanned<String>>,
    /// Whether the target is tested by default by `cargo test`.
    pub test: Option<Spanned<bool>>,
    /// Whether documentation examples are tested by `cargo test`.
    pub doctest: Option<Spanned<bool>>,
    /// Whether the target is benchmarked by default by `cargo bench`.
    pub bench: Option<Spanned<bool>>,
    /// Whether the target is included in `cargo doc` output.
    pub doc: Option<Spanned<bool>>,
    /// Deprecated and unused.
    pub plugin: Option<Spanned<bool>>,
    /// Whether the library is a procedural macro.
    pub proc_macro: Option<Spanned<bool>>,
    /// Whether to use the libtest harness for `#[test]` functions.
    pub harness: Option<Spanned<bool>>,
    /// The Rust edition the target will use.
    pub edition: Option<Spanned<Edition>>,
    /// The crate types to generate (e.g., `lib`, `rlib`, `dylib`, `cdylib`, `staticlib`).
    pub crate_type: Option<Spanned<Vec<String>>>,
    /// Features required for the target to be built.
    pub required_features: Option<Spanned<Vec<String>>>,
    /// Whether Rustdoc should scrape examples from this target.
    pub doc_scrape_examples: Option<Spanned<bool>>,
}

/// Binary target configuration from `[[bin]]`.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct BinTarget {
    /// The name of the binary (used as the executable filename).
    pub name: Option<Spanned<String>>,
    /// The source file of the target, relative to Cargo.toml.
    pub path: Option<Spanned<String>>,
    /// Whether the target is tested by default by `cargo test`.
    pub test: Option<Spanned<bool>>,
    /// Whether documentation examples are tested by `cargo test`.
    pub doctest: Option<Spanned<bool>>,
    /// Whether the target is benchmarked by default by `cargo bench`.
    pub bench: Option<Spanned<bool>>,
    /// Whether the target is included in `cargo doc` output.
    pub doc: Option<Spanned<bool>>,
    /// Deprecated and unused.
    pub plugin: Option<Spanned<bool>>,
    /// Whether to use the libtest harness for `#[test]` functions.
    pub harness: Option<Spanned<bool>>,
    /// The Rust edition the target will use.
    pub edition: Option<Spanned<Edition>>,
    /// Features required for the target to be built.
    pub required_features: Option<Spanned<Vec<String>>>,
}

/// Test target configuration from `[[test]]`.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct TestTarget {
    /// The name of the test target.
    pub name: Option<Spanned<String>>,
    /// The source file of the target, relative to Cargo.toml.
    pub path: Option<Spanned<String>>,
    /// Whether the target is tested by default by `cargo test`.
    pub test: Option<Spanned<bool>>,
    /// Whether documentation examples are tested by `cargo test`.
    pub doctest: Option<Spanned<bool>>,
    /// Whether the target is benchmarked by default by `cargo bench`.
    pub bench: Option<Spanned<bool>>,
    /// Whether the target is included in `cargo doc` output.
    pub doc: Option<Spanned<bool>>,
    /// Deprecated and unused.
    pub plugin: Option<Spanned<bool>>,
    /// Whether to use the libtest harness for `#[test]` functions.
    pub harness: Option<Spanned<bool>>,
    /// The Rust edition the target will use.
    pub edition: Option<Spanned<Edition>>,
    /// Features required for the target to be built.
    pub required_features: Option<Spanned<Vec<String>>>,
}

/// Benchmark target configuration from `[[bench]]`.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct BenchTarget {
    /// The name of the benchmark target.
    pub name: Option<Spanned<String>>,
    /// The source file of the target, relative to Cargo.toml.
    pub path: Option<Spanned<String>>,
    /// Whether the target is tested by default by `cargo test`.
    pub test: Option<Spanned<bool>>,
    /// Whether documentation examples are tested by `cargo test`.
    pub doctest: Option<Spanned<bool>>,
    /// Whether the target is benchmarked by default by `cargo bench`.
    pub bench: Option<Spanned<bool>>,
    /// Whether the target is included in `cargo doc` output.
    pub doc: Option<Spanned<bool>>,
    /// Deprecated and unused.
    pub plugin: Option<Spanned<bool>>,
    /// Whether to use the libtest harness for `#[test]` functions.
    pub harness: Option<Spanned<bool>>,
    /// The Rust edition the target will use.
    pub edition: Option<Spanned<Edition>>,
    /// Features required for the target to be built.
    pub required_features: Option<Spanned<Vec<String>>>,
}

/// Example target configuration from `[[example]]`.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct ExampleTarget {
    /// The name of the example target.
    pub name: Option<Spanned<String>>,
    /// The source file of the target, relative to Cargo.toml.
    pub path: Option<Spanned<String>>,
    /// Whether the target is tested by default by `cargo test`.
    pub test: Option<Spanned<bool>>,
    /// Whether documentation examples are tested by `cargo test`.
    pub doctest: Option<Spanned<bool>>,
    /// Whether the target is benchmarked by default by `cargo bench`.
    pub bench: Option<Spanned<bool>>,
    /// Whether the target is included in `cargo doc` output.
    pub doc: Option<Spanned<bool>>,
    /// Deprecated and unused.
    pub plugin: Option<Spanned<bool>>,
    /// Whether to use the libtest harness for `#[test]` functions.
    pub harness: Option<Spanned<bool>>,
    /// The Rust edition the target will use.
    pub edition: Option<Spanned<Edition>>,
    /// Features required for the target to be built.
    pub required_features: Option<Spanned<Vec<String>>>,
    /// The crate types to generate for this example.
    pub crate_type: Option<Spanned<Vec<String>>>,
}

/// Build profile configuration from `[profile.*]`.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct Profile {
    /// Controls the optimization level (0-3, "s", or "z").
    pub opt_level: Option<OptLevel>,
    /// Controls the amount of debug information in the binary.
    pub debug: Option<DebugLevel>,
    /// Enables or disables `cfg(debug_assertions)` conditional compilation.
    pub debug_assertions: Option<Spanned<bool>>,
    /// Enables or disables runtime integer overflow checks.
    pub overflow_checks: Option<Spanned<bool>>,
    /// Controls LLVM link time optimizations.
    pub lto: Option<Lto>,
    /// Controls the panic strategy ("unwind" or "abort").
    pub panic: Option<Spanned<PanicStrategy>>,
    /// Enables or disables incremental compilation.
    pub incremental: Option<Spanned<bool>>,
    /// Controls how many code generation units a crate is split into.
    pub codegen_units: Option<Spanned<u32>>,
    /// Enables or disables rpath for dynamic library loading.
    pub rpath: Option<Spanned<bool>>,
    /// Directs rustc to strip symbols or debuginfo from the binary.
    pub strip: Option<StripLevel>,
    /// Controls whether debug information is in the executable or separate file.
    pub split_debuginfo: Option<Spanned<String>>,
    /// Specifies which built-in profile this custom profile inherits from.
    pub inherits: Option<Spanned<String>>,
    /// Per-package profile overrides.
    pub package: Option<HashMap<String, PackageProfile>>,
    /// Settings for build scripts and proc-macros.
    #[facet(rename = "build-override")]
    pub build_override: Option<BuildOverride>,
}

/// Optimization level (0-3, "s", or "z").
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum OptLevel {
    /// Numeric optimization level (0-3).
    Number(Spanned<u8>),
    /// Size optimization ("s" or "z").
    String(Spanned<String>),
}

/// Debug information level.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum DebugLevel {
    /// Boolean debug info (true = full, false = none).
    Bool(Spanned<bool>),
    /// Numeric debug level (0, 1, or 2).
    Number(Spanned<u8>),
    /// Named debug level ("line-tables-only", "line-directives-only").
    String(Spanned<String>),
}

/// Link-time optimization setting.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum Lto {
    /// Boolean LTO (true = "fat", false = disabled).
    Bool(Spanned<bool>),
    /// Named LTO mode ("thin", "fat", "off").
    String(Spanned<String>),
}

/// Panic strategy.
#[derive(Facet, Debug, Clone, Copy)]
#[repr(u8)]
pub enum PanicStrategy {
    /// Unwind the stack on panic.
    #[facet(rename = "unwind")]
    Unwind,
    /// Abort the process on panic.
    #[facet(rename = "abort")]
    Abort,
    /// Immediately abort the process on panic using hardware instructions. Requires a nightly compiler
    #[facet(rename = "immediate-abort")],
    ImmediateAbort
}

/// Symbol stripping level.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum StripLevel {
    /// Boolean strip (true = all symbols, false = none).
    Bool(Spanned<bool>),
    /// Named strip level ("none", "debuginfo", "symbols").
    String(Spanned<String>),
}

/// Per-package profile overrides.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct PackageProfile {
    /// Controls the optimization level.
    pub opt_level: Option<OptLevel>,
    /// Controls the amount of debug information.
    pub debug: Option<DebugLevel>,
    /// Enables or disables debug assertions.
    pub debug_assertions: Option<Spanned<bool>>,
    /// Enables or disables overflow checks.
    pub overflow_checks: Option<Spanned<bool>>,
    /// Controls code generation units.
    pub codegen_units: Option<Spanned<u32>>,
}

/// Build script and proc-macro profile overrides.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct BuildOverride {
    /// Controls the optimization level.
    pub opt_level: Option<OptLevel>,
    /// Controls the amount of debug information.
    pub debug: Option<DebugLevel>,
    /// Enables or disables debug assertions.
    pub debug_assertions: Option<Spanned<bool>>,
    /// Enables or disables overflow checks.
    pub overflow_checks: Option<Spanned<bool>>,
    /// Controls code generation units.
    pub codegen_units: Option<Spanned<u32>>,
    /// Enables or disables incremental compilation.
    pub incremental: Option<Spanned<bool>>,
}

/// The `[lints]` section for configuring compiler lints.
#[derive(Facet, Debug, Clone)]
pub struct Lints {
    /// Inherit lint configuration from workspace.
    pub workspace: Option<Spanned<bool>>,
    /// Rust compiler lint levels.
    pub rust: Option<HashMap<String, LintLevel>>,
    /// Clippy lint levels.
    pub clippy: Option<HashMap<String, LintLevel>>,
    /// Rustdoc lint levels.
    pub rustdoc: Option<HashMap<String, LintLevel>>,
}

/// Lint level configuration.
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
#[facet(untagged)]
pub enum LintLevel {
    /// Detailed config with priority.
    Config(LintConfig),
    /// Forbid the lint (error, cannot be overridden).
    #[facet(rename = "forbid")]
    Forbid,
    /// Deny the lint (error).
    #[facet(rename = "deny")]
    Deny,
    /// Warn on the lint.
    #[facet(rename = "warn")]
    Warn,
    /// Allow the lint (silence it).
    #[facet(rename = "allow")]
    Allow,
}

/// Detailed lint configuration with priority.
#[derive(Facet, Debug, Clone)]
#[facet(rename_all = "kebab-case")]
pub struct LintConfig {
    /// The lint level.
    pub level: Spanned<LintLevelString>,
    /// Priority for ordering lint table application.
    pub priority: Option<Spanned<i32>>,
    /// Custom cfg conditions to check.
    pub check_cfg: Option<Spanned<Vec<String>>>,
}

/// Simple lint level string.
#[derive(Facet, Debug, Clone, Copy)]
#[repr(u8)]
pub enum LintLevelString {
    /// Forbid the lint.
    #[facet(rename = "forbid")]
    Forbid,
    /// Deny the lint.
    #[facet(rename = "deny")]
    Deny,
    /// Warn on the lint.
    #[facet(rename = "warn")]
    Warn,
    /// Allow the lint.
    #[facet(rename = "allow")]
    Allow,
}

/// Badge configuration (deprecated).
#[derive(Facet, Debug, Clone)]
pub struct Badge {
    /// Badge-specific attributes (varies by badge type).
    #[facet(flatten)]
    pub attributes: facet_value::Value,
}

impl CargoToml {
    /// Parse a `Cargo.toml` from a string.
    pub fn parse(contents: &str) -> Result<Self, crate::Error> {
        facet_toml::from_str(contents).map_err(|e| crate::Error::Parse {
            message: e.to_string(),
        })
    }

    /// Parse a `Cargo.toml` from a file path.
    pub fn from_path(path: impl AsRef<camino::Utf8Path>) -> Result<Self, crate::Error> {
        let path = path.as_ref();
        let contents = std::fs::read_to_string(path).map_err(|source| crate::Error::Io {
            path: path.to_owned(),
            source: crate::IoError::from(source),
        })?;
        Self::parse(&contents)
    }
}
