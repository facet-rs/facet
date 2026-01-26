//! Attribute macros for figue CLI argument parsing.
//!
//! This crate provides the attribute grammar definitions for figue.
//! It exists as a separate crate to work around Rust's restriction on
//! accessing macro-expanded `#[macro_export]` macros by absolute paths
//! within the same crate.
//!
//! Users should depend on `figue` directly, which re-exports everything
//! from this crate.

#![warn(missing_docs)]
#![deny(unsafe_code)]

extern crate self as figue_attrs;

// Args extension attributes for use with #[facet(args::attr)] syntax.
//
// After importing `use figue as args;`, users can write:
//   #[facet(args::positional)]
//   #[facet(args::short = 'v')]
//   #[facet(args::named)]

// Generate args attribute grammar using the grammar DSL.
// This generates:
// - `Attr` enum with all args attribute variants
// - `__attr!` macro that dispatches to attribute handlers and returns ExtensionAttr
// - `__parse_attr!` macro for parsing (internal use)
facet::define_attr_grammar! {
    ns "args";
    crate_path ::figue_attrs;

    /// Args attribute types for field configuration.
    pub enum Attr {
        /// Marks a field as a positional argument.
        ///
        /// Usage: `#[facet(args::positional)]`
        Positional,
        /// Marks a field as a named argument.
        ///
        /// Usage: `#[facet(args::named)]`
        Named,
        /// Specifies a short flag character for the field.
        ///
        /// Usage: `#[facet(args::short = 'v')]` or just `#[facet(args::short)]`
        Short(Option<char>),
        /// Marks a field as a subcommand.
        ///
        /// The field type must be an enum where each variant represents a subcommand.
        /// Variant names are converted to kebab-case for matching.
        ///
        /// Usage: `#[facet(args::subcommand)]`
        Subcommand,
        /// Marks a field as a counted flag.
        ///
        /// Each occurrence of the flag increments the count. Works with both short
        /// flags (`-vvv` or `-v -v -v`) and long flags (`--verbose --verbose`).
        /// The field type must be an integer type (u8, u16, u32, u64, usize, i8, i16, i32, i64, isize).
        /// Uses saturating arithmetic to avoid overflow.
        ///
        /// Usage: `#[facet(args::named, args::short = 'v', args::counted)]`
        Counted,
        /// Marks a field as a layered configuration field.
        ///
        /// The field will be populated from merged configuration sources (CLI overrides,
        /// environment variables, config files) in priority order: CLI > env > file > default.
        ///
        /// This automatically generates:
        /// - `--{field_name} <PATH>` flag to specify config file path
        /// - `--{field_name}.foo.bar <VALUE>` style CLI overrides
        /// - Environment variable parsing
        /// - Config file loading with multiple format support
        ///
        /// Usage: `#[facet(args::config)]`
        Config,
        /// Specifies the environment variable prefix for a config field.
        ///
        /// Must be used together with `#[facet(args::config)]`.
        ///
        /// Usage: `#[facet(args::env_prefix = "MYAPP")]`
        ///
        /// Example: `env_prefix = "MYAPP"` results in `MYAPP__FIELD__NAME` env vars.
        EnvPrefix(Option<&'static str>),
        /// Specifies an additional environment variable name for a config field.
        ///
        /// This allows a field to be read from standard environment variables
        /// like `DATABASE_URL` or `PORT` in addition to the prefixed form.
        ///
        /// The prefixed env var takes priority over aliases when both are set.
        /// Multiple aliases can be specified by using the attribute multiple times.
        ///
        /// Usage: `#[facet(args::env_alias = "DATABASE_URL")]`
        ///
        /// Example:
        /// ```ignore
        /// #[derive(Facet)]
        /// struct Config {
        ///     /// Also reads from $DATABASE_URL
        ///     #[facet(args::env_alias = "DATABASE_URL")]
        ///     database_url: String,
        ///
        ///     /// Reads from $PORT or $HTTP_PORT
        ///     #[facet(args::env_alias = "PORT", args::env_alias = "HTTP_PORT")]
        ///     port: u16,
        /// }
        /// ```
        EnvAlias(&'static str),
        /// Enables environment variable substitution for this field.
        ///
        /// When enabled, `${VAR}` patterns in the field's value will be replaced
        /// with the corresponding environment variable. Supports default values
        /// with `${VAR:-default}` syntax. Use `$$` to escape a literal `$`.
        ///
        /// Usage: `#[facet(args::env_subst)]`
        ///
        /// Example:
        /// ```ignore
        /// #[derive(Facet)]
        /// struct Config {
        ///     #[facet(args::env_subst)]
        ///     data_dir: PathBuf,  // "${BASE_PATH}/data" -> "/var/myapp/data"
        /// }
        /// ```
        EnvSubst,
        /// Enables environment variable substitution for all direct fields in a struct.
        ///
        /// This is equivalent to adding `#[facet(args::env_subst)]` to each direct
        /// field. Does not propagate to nested structs (mirrors `rename_all` behavior),
        /// but does apply to flattened fields since they become direct children.
        ///
        /// Usage: `#[facet(args::env_subst_all)]`
        ///
        /// Example:
        /// ```ignore
        /// #[derive(Facet)]
        /// #[facet(args::env_subst_all)]
        /// struct Config {
        ///     data_dir: PathBuf,   // gets env_subst
        ///     cache_dir: PathBuf,  // gets env_subst
        ///     nested: Other,       // nested.field does NOT get env_subst
        /// }
        /// ```
        EnvSubstAll,
        /// Marks a field as the help flag.
        ///
        /// When this flag is set, the driver shows help and exits with code 0.
        /// The field should be a `bool`.
        ///
        /// Usage: `#[facet(figue::help)]`
        Help,
        /// Marks a field as the version flag.
        ///
        /// When this flag is set, the driver shows version and exits with code 0.
        /// The field should be a `bool`.
        ///
        /// Usage: `#[facet(figue::version)]`
        Version,
        /// Marks a field as the completions flag.
        ///
        /// When this flag is set, the driver generates shell completions and exits with code 0.
        /// The field should be `Option<Shell>`.
        ///
        /// Usage: `#[facet(figue::completions)]`
        Completions,
    }
}
