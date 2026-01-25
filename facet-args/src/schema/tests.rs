use super::*;
use crate as args;
use facet::Facet;

macro_rules! assert_schema_snapshot {
    ($result:expr) => {{
        match $result {
            Ok(value) => insta::assert_debug_snapshot!(value),
            Err(err) => {
                let rendered = err.to_ariadne_string();
                let stripped = strip_ansi_escapes::strip(rendered.as_bytes());
                let stripped = String::from_utf8_lossy(&stripped);
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
fn snapshot_schema_bad_config_field() {
    assert_schema_snapshot!(Schema::from_shape(BadConfigField::SHAPE));
}
