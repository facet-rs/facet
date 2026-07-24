use super::*;
use crate as args;
use facet::Facet;
use facet_testhelpers::test;

macro_rules! assert_schema_snapshot {
    ($result:expr) => {{
        match $result {
            Ok(value) => insta::assert_snapshot!(facet_json::to_string_pretty(&value).unwrap()),
            Err(err) => {
                let rendered = err.to_string();
                let stripped = strip_ansi_escapes::strip(rendered.as_bytes());
                let stripped = String::from_utf8_lossy(&stripped).replace('\\', "/");
                insta::assert_snapshot!(stripped);
            }
        }
    }};
}

#[derive(Facet)]
struct BasicArgs {
    /// Verbose output
    #[facet(args::named, args::short = 'v')]
    verbose: bool,
    /// Input file
    #[facet(args::positional)]
    input: String,
    /// Include list
    #[facet(args::named)]
    include: Vec<String>,
    /// Quiet count
    #[facet(args::named, args::short = 'q', args::counted)]
    quiet: u32,
    /// Subcommand
    #[facet(args::subcommand)]
    command: Option<Command>,
    /// Config
    #[facet(args::config, args::env_prefix = "APP")]
    config: Option<AppConfig>,
}

#[derive(Facet)]
#[repr(u8)]
enum Command {
    /// Build stuff
    Build(BuildArgs),
    /// Clean
    #[facet(rename = "clean-all")]
    Clean,
}

#[derive(Facet)]
struct BuildArgs {
    /// Release build
    #[facet(args::named, args::short = 'r')]
    release: bool,
}

#[derive(Facet)]
struct AppConfig {
    host: String,
    port: u16,
}

#[derive(Facet)]
struct MissingArgsAnnotation {
    foo: String,
}

#[derive(Facet)]
#[repr(u8)]
enum SubA {
    A,
}

#[derive(Facet)]
#[repr(u8)]
enum SubB {
    B,
}

#[derive(Facet)]
struct MultipleSubcommands {
    #[facet(args::subcommand)]
    a: SubA,
    #[facet(args::subcommand)]
    b: SubB,
}

#[derive(Facet)]
struct SubcommandOnNonEnum {
    #[facet(args::subcommand)]
    value: String,
}

#[derive(Facet)]
struct CountedOnNonInteger {
    #[facet(args::named, args::counted)]
    value: bool,
}

#[derive(Facet)]
struct ShortOnPositional {
    #[facet(args::positional, args::short = 'p')]
    value: String,
}

#[derive(Facet)]
struct EnvPrefixWithoutConfig {
    #[facet(args::env_prefix = "APP")]
    value: String,
}

#[derive(Facet)]
struct ConflictingLongFlags {
    #[facet(args::named, rename = "dup")]
    a: bool,
    #[facet(args::named, rename = "dup")]
    b: bool,
}

#[derive(Facet)]
struct ConflictingShortFlags {
    #[facet(args::named, args::short = 'v')]
    a: bool,
    #[facet(args::named, args::short = 'v')]
    b: bool,
}

#[derive(Facet)]
#[repr(u8)]
enum SubcommandWithShort {
    #[facet(args::short = 'd')]
    Daemon,
    Doctor,
}

#[derive(Facet)]
struct ArgsWithSubcommandShort {
    #[facet(args::subcommand)]
    command: SubcommandWithShort,
}

#[derive(Facet)]
#[repr(u8)]
enum SubcommandShortConflictsWithFlagCommand {
    #[facet(args::short = 'd')]
    Daemon,
}

#[derive(Facet)]
struct SubcommandShortConflictsWithFlag {
    #[facet(args::named, args::short = 'd')]
    debug: bool,
    #[facet(args::subcommand)]
    command: SubcommandShortConflictsWithFlagCommand,
}

#[derive(Facet)]
#[repr(u8)]
enum SubcommandShortConflictsCommand {
    #[facet(args::short = 'd')]
    Daemon,
    #[facet(args::short = 'd')]
    Doctor,
}

#[derive(Facet)]
struct SubcommandShortConflicts {
    #[facet(args::subcommand)]
    command: SubcommandShortConflictsCommand,
}

#[derive(Facet)]
struct BadConfigField {
    #[facet(args::config)]
    config: String,
}

#[derive(Facet)]
#[repr(u8)]
enum TopLevelEnum {
    Foo,
}

#[test]
fn snapshot_schema_basic() {
    assert_schema_snapshot!(Schema::from_shape(BasicArgs::SHAPE));
}

#[test]
fn snapshot_schema_top_level_enum() {
    assert_schema_snapshot!(Schema::from_shape(TopLevelEnum::SHAPE));
}

#[test]
fn snapshot_schema_missing_args_annotation() {
    assert_schema_snapshot!(Schema::from_shape(MissingArgsAnnotation::SHAPE));
}

#[test]
fn snapshot_schema_multiple_subcommands() {
    assert_schema_snapshot!(Schema::from_shape(MultipleSubcommands::SHAPE));
}

#[test]
fn snapshot_schema_subcommand_on_non_enum() {
    assert_schema_snapshot!(Schema::from_shape(SubcommandOnNonEnum::SHAPE));
}

#[test]
fn snapshot_schema_counted_on_non_integer() {
    assert_schema_snapshot!(Schema::from_shape(CountedOnNonInteger::SHAPE));
}

#[test]
fn snapshot_schema_short_on_positional() {
    assert_schema_snapshot!(Schema::from_shape(ShortOnPositional::SHAPE));
}

#[test]
fn snapshot_schema_env_prefix_without_config() {
    assert_schema_snapshot!(Schema::from_shape(EnvPrefixWithoutConfig::SHAPE));
}

#[test]
fn snapshot_schema_conflicting_long_flags() {
    assert_schema_snapshot!(Schema::from_shape(ConflictingLongFlags::SHAPE));
}

#[test]
fn snapshot_schema_conflicting_short_flags() {
    assert_schema_snapshot!(Schema::from_shape(ConflictingShortFlags::SHAPE));
}

#[test]
fn test_schema_subcommand_short_stored() {
    let schema = Schema::from_shape(ArgsWithSubcommandShort::SHAPE).unwrap();
    let daemon = schema
        .args()
        .subcommands()
        .values()
        .find(|sub| sub.cli_name() == "daemon")
        .unwrap();
    assert_eq!(daemon.short(), Some('d'));
}

#[test]
fn test_schema_subcommand_short_conflicts_with_flag() {
    Schema::from_shape(SubcommandShortConflictsWithFlag::SHAPE)
        .expect("subcommand short alias 'd' should not conflict with flag short '-d'");
}

#[test]
fn test_schema_subcommand_short_conflicts_with_subcommand_short() {
    let result = Schema::from_shape(SubcommandShortConflicts::SHAPE);
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("duplicate subcommand short alias `d`"),
        "unexpected error: {err}"
    );
}

#[test]
fn snapshot_schema_bad_config_field() {
    assert_schema_snapshot!(Schema::from_shape(BadConfigField::SHAPE));
}

// ============================================================================
// Flatten tests
// ============================================================================

/// Common args that can be flattened into other structs.
#[derive(Facet)]
struct CommonArgs {
    #[facet(args::named, args::short = 'v')]
    verbose: bool,
    #[facet(args::named, args::short = 'q')]
    quiet: bool,
}

/// Args struct that flattens CommonArgs.
#[derive(Facet)]
struct ArgsWithFlatten {
    #[facet(args::positional)]
    input: String,
    #[facet(flatten)]
    common: CommonArgs,
}

#[test]
fn test_flatten_schema_builds() {
    let schema = Schema::from_shape(ArgsWithFlatten::SHAPE).expect("schema should build");

    // The flattened args should appear at top level
    let args = schema.args();
    assert!(
        args.args.contains_key("verbose"),
        "verbose should be in args"
    );
    assert!(args.args.contains_key("quiet"), "quiet should be in args");
    assert!(args.args.contains_key("input"), "input should be in args");
}

/// Nested flattening test structs
#[derive(Facet)]
struct OutputArgs {
    #[facet(args::named, args::short = 'f')]
    format: Option<String>,
}

#[derive(Facet)]
struct ExtendedCommonArgs {
    #[facet(flatten)]
    common: CommonArgs,
    #[facet(flatten)]
    output: OutputArgs,
}

#[derive(Facet)]
struct ArgsWithNestedFlatten {
    #[facet(args::positional)]
    input: String,
    #[facet(flatten)]
    extended: ExtendedCommonArgs,
}

/// Test conflicting flags from flatten
#[derive(Facet)]
struct ConflictingFlattenArgs {
    #[facet(args::named, args::short = 'v')]
    version: bool,
    #[facet(flatten)]
    common: CommonArgs, // CommonArgs also has -v for verbose
}

#[test]
fn test_flatten_conflict_detected() {
    let result = Schema::from_shape(ConflictingFlattenArgs::SHAPE);
    assert!(result.is_err(), "should detect duplicate -v flag");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("duplicate") || err.contains("-v"),
        "error should mention duplicate: {}",
        err
    );
}

// ============================================================================
// Config-level flatten tests
// ============================================================================

/// Common config fields that can be flattened
#[derive(Facet)]
struct CommonConfig {
    /// Log level
    log_level: Option<String>,
    /// Debug mode
    debug: bool,
}

/// Database config
#[derive(Facet)]
struct DatabaseConfig {
    /// Database host
    host: String,
    /// Database port
    port: u16,
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

/// Args with config that has flatten
#[derive(Facet)]
struct ArgsWithFlattenedConfig {
    #[facet(args::positional)]
    input: String,
    #[facet(args::config)]
    config: ConfigWithFlatten,
}

#[test]
fn test_config_flatten_schema_builds() {
    let schema = Schema::from_shape(ArgsWithFlattenedConfig::SHAPE).expect("schema should build");
    let config = schema.configs().first().expect("should have config");
    let fields = config.fields();

    // Should have 3 fields: name, log_level, debug (flattened from common)
    assert_eq!(fields.len(), 3, "should have 3 fields after flatten");
    assert!(fields.contains_key("name"), "should have name field");
    assert!(
        fields.contains_key("log_level"),
        "should have log_level from flattened common"
    );
    assert!(
        fields.contains_key("debug"),
        "should have debug from flattened common"
    );
}

#[test]
fn test_config_skipped_field_is_omitted() {
    #[derive(Facet)]
    struct ConfigWithSkippedField {
        visible: String,
        #[facet(skip, default)]
        skipped: String,
    }

    #[derive(Facet)]
    struct ArgsWithSkippedConfigField {
        #[facet(args::config)]
        config: ConfigWithSkippedField,
    }
    assert_schema_snapshot!(Schema::from_shape(ArgsWithSkippedConfigField::SHAPE))
}

#[test]
fn test_config_skipped_field_errors_when_not_defaulted() {
    #[derive(Facet)]
    struct ConfigWithSkippedNonDefaultField {
        visible: String,
        #[facet(skip)]
        skipped: String,
    }

    #[derive(Facet)]
    struct ArgsWithSkippedNonDefaultConfigField {
        #[facet(args::config)]
        config: ConfigWithSkippedNonDefaultField,
    }
    assert_schema_snapshot!(Schema::from_shape(
        ArgsWithSkippedNonDefaultConfigField::SHAPE
    ))
}

/// Deeply nested config flatten: common inside extended
#[derive(Facet)]
struct ExtendedConfig {
    #[facet(flatten)]
    common: CommonConfig,
    #[facet(flatten)]
    database: DatabaseConfig,
}

#[derive(Facet)]
struct ConfigWithNestedFlatten {
    app_name: String,
    #[facet(flatten)]
    extended: ExtendedConfig,
}

#[derive(Facet)]
struct ArgsWithNestedFlattenConfig {
    #[facet(args::positional)]
    input: String,
    #[facet(args::config)]
    config: ConfigWithNestedFlatten,
}

#[test]
fn test_config_nested_flatten_schema_builds() {
    let schema =
        Schema::from_shape(ArgsWithNestedFlattenConfig::SHAPE).expect("schema should build");
    let config = schema.configs().first().expect("should have config");
    let fields = config.fields();

    // Should have 5 fields: app_name + log_level, debug (from common) + host, port (from database)
    assert_eq!(fields.len(), 5, "should have 5 fields after nested flatten");
    assert!(fields.contains_key("app_name"), "should have app_name");
    assert!(fields.contains_key("log_level"), "should have log_level");
    assert!(fields.contains_key("debug"), "should have debug");
    assert!(fields.contains_key("host"), "should have host");
    assert!(fields.contains_key("port"), "should have port");
}

/// Test conflict detection in config flatten
#[derive(Facet)]
struct ConflictingConfigA {
    name: String,
}

#[derive(Facet)]
struct ConflictingConfigB {
    name: String, // Same field name as ConflictingConfigA
}

#[derive(Facet)]
struct ConfigWithConflictingFlatten {
    #[facet(flatten)]
    a: ConflictingConfigA,
    #[facet(flatten)]
    b: ConflictingConfigB,
}

#[derive(Facet)]
struct ArgsWithConflictingConfigFlatten {
    #[facet(args::positional)]
    input: String,
    #[facet(args::config)]
    config: ConfigWithConflictingFlatten,
}

#[test]
fn test_config_flatten_conflict_detected() {
    let result = Schema::from_shape(ArgsWithConflictingConfigFlatten::SHAPE);
    assert!(result.is_err(), "should detect duplicate config field");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("duplicate") || err.contains("name"),
        "error should mention duplicate: {}",
        err
    );
}

// ============================================================================
// Struct fields in args must be flattened
// ============================================================================

#[derive(Facet)]
struct NestedOptions {
    #[facet(args::named)]
    verbose: bool,
}

#[derive(Facet)]
struct ArgsWithUnflattenedStruct {
    #[facet(args::named)]
    options: NestedOptions, // ERROR: struct fields must use flatten
}

#[test]
fn test_struct_field_without_flatten_is_error() {
    let result = Schema::from_shape(ArgsWithUnflattenedStruct::SHAPE);
    assert!(result.is_err(), "struct field without flatten should error");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("flatten"),
        "error should mention flatten: {}",
        err
    );
}

// ============================================================================
// Env alias conflict detection
// ============================================================================

#[derive(Facet)]
struct ConfigWithConflictingAliases {
    #[facet(args::env_alias = "DATABASE_URL")]
    db_url: String,
    #[facet(args::env_alias = "DATABASE_URL")]
    connection_string: String,
}

#[derive(Facet)]
struct ArgsWithConflictingAliases {
    #[facet(args::config)]
    config: ConfigWithConflictingAliases,
}

#[test]
fn test_env_alias_conflict_detected() {
    let result = Schema::from_shape(ArgsWithConflictingAliases::SHAPE);
    assert!(result.is_err(), "should detect duplicate env alias");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("DATABASE_URL") && err.contains("db_url") && err.contains("connection_string"),
        "error should mention the alias and both fields: {}",
        err
    );
}

// ============================================================================
// Alias support
// ============================================================================

#[derive(Facet)]
struct ArgsWithAlias {
    #[facet(args::named, rename = "drive", args::alias = "drive-letter-pattern")]
    drive_letter_pattern: String,
}

#[derive(Facet)]
struct ConflictingAliasAndCanonical {
    #[facet(args::named, args::alias = "port")]
    host: String,
    #[facet(args::named)]
    port: String,
}

#[derive(Facet)]
struct DuplicateAliasOnField {
    #[facet(
        args::named,
        args::alias = "drive-letter-pattern",
        args::alias = "drive-letter-pattern"
    )]
    drive: String,
}

#[derive(Facet)]
struct ConflictingAliases {
    #[facet(args::named, args::alias = "drive-letter-pattern")]
    drive: String,
    #[facet(args::named, args::alias = "drive-letter-pattern")]
    path: String,
}

#[derive(Facet)]
#[repr(u8)]
enum SubcommandWithDuplicateAlias {
    #[facet(args::alias = "profiles", args::alias = "profiles")]
    Profile,
}

#[derive(Facet)]
struct ArgsWithDuplicateSubcommandAlias {
    #[facet(args::subcommand)]
    command: SubcommandWithDuplicateAlias,
}

#[derive(Facet)]
#[repr(u8)]
enum SubcommandAliasCanonicalConflict {
    Profile,
    #[facet(args::alias = "profile")]
    Profiles,
}

#[derive(Facet)]
struct ArgsWithSubcommandAliasCanonicalConflict {
    #[facet(args::subcommand)]
    command: SubcommandAliasCanonicalConflict,
}

#[derive(Facet)]
#[repr(u8)]
enum SubcommandAliasAliasConflict {
    #[facet(args::alias = "profiles")]
    Profile,
    #[facet(args::alias = "profiles")]
    User,
}

#[derive(Facet)]
struct ArgsWithSubcommandAliasAliasConflict {
    #[facet(args::subcommand)]
    command: SubcommandAliasAliasConflict,
}

#[derive(Facet)]
#[repr(u8)]
enum SubcommandWithCasedAlias {
    #[facet(args::alias = "Profiles")]
    UserProfiles,
}

#[derive(Facet)]
struct ArgsWithCasedSubcommandAlias {
    #[facet(args::subcommand)]
    command: SubcommandWithCasedAlias,
}

#[derive(Facet)]
#[repr(u8)]
enum SubcommandAliasCaseConflict {
    Profiles,
    #[facet(args::alias = "Profiles")]
    User,
}

#[derive(Facet)]
struct ArgsWithSubcommandAliasCaseConflict {
    #[facet(args::subcommand)]
    command: SubcommandAliasCaseConflict,
}

#[test]
fn test_schema_aliases_are_stored() {
    let schema = Schema::from_shape(ArgsWithAlias::SHAPE).expect("schema should build");
    let (_, arg) = schema
        .args()
        .args()
        .get("drive")
        .expect("drive arg should be present");
    assert_eq!(arg.aliases(), &["drive-letter-pattern".to_string()]);
}

#[test]
fn test_schema_alias_conflicts_with_canonical_flag() {
    let result = Schema::from_shape(ConflictingAliasAndCanonical::SHAPE);
    assert!(result.is_err(), "should detect alias/canonical conflict");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("duplicate flag `--port`"), "unexpected error: {err}");
}

#[test]
fn test_schema_duplicate_alias_on_same_field_is_rejected() {
    let result = Schema::from_shape(DuplicateAliasOnField::SHAPE);
    assert!(result.is_err(), "should detect duplicate alias on one field");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("duplicate alias `--drive-letter-pattern`"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_schema_duplicate_alias_across_fields_is_rejected() {
    let result = Schema::from_shape(ConflictingAliases::SHAPE);
    assert!(result.is_err(), "should detect duplicate alias across fields");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("duplicate flag `--drive-letter-pattern`"), "unexpected error: {err}");
}

#[test]
fn test_subcommand_duplicate_alias_on_same_variant_detected() {
    let result = Schema::from_shape(ArgsWithDuplicateSubcommandAlias::SHAPE);
    assert!(result.is_err(), "should detect duplicate alias on one variant");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("duplicate subcommand alias") && err.contains("profiles"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_subcommand_alias_conflict_with_canonical_name_detected() {
    let result = Schema::from_shape(ArgsWithSubcommandAliasCanonicalConflict::SHAPE);
    assert!(result.is_err(), "should detect alias/canonical conflict");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("duplicate subcommand name `profile`"), "unexpected error: {err}");
}

#[test]
fn test_subcommand_alias_conflict_with_other_alias_detected() {
    let result = Schema::from_shape(ArgsWithSubcommandAliasAliasConflict::SHAPE);
    assert!(result.is_err(), "should detect alias/alias conflict");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("duplicate subcommand name `profiles`"), "unexpected error: {err}");
}

#[test]
fn test_subcommand_aliases_are_normalized_to_kebab_case() {
    let schema = Schema::from_shape(ArgsWithCasedSubcommandAlias::SHAPE)
        .expect("schema should build");
    let subcommand = schema
        .args()
        .subcommands()
        .values()
        .next()
        .expect("subcommand should be present");

    assert_eq!(subcommand.aliases(), &["profiles".to_string()]);
}

#[test]
fn test_subcommand_alias_conflict_after_case_normalization_detected() {
    let result = Schema::from_shape(ArgsWithSubcommandAliasCaseConflict::SHAPE);
    assert!(
        result.is_err(),
        "should detect alias/canonical conflict after kebab-case normalization"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("duplicate subcommand name `profiles`"),
        "unexpected error: {err}"
    );
}

