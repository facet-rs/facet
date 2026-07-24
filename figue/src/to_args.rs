use std::ffi::{OsStr, OsString};

use facet_core::Facet;
use facet_core::ScalarType as FacetScalarType;
use heck::ToKebabCase;

use crate::config_value::{ConfigValue, ObjectMap};
use crate::config_value_parser::ConfigValueSerializer;
use crate::schema::{ArgKind, ArgLevelSchema, ArgSchema, Schema, ValueSchema};

/// Error type for converting a typed CLI value back into command-line arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToArgsError {
    /// Failed to build schema from the type's shape.
    SchemaBuild(String),
    /// Failed to serialize a value into an intermediate config tree.
    Serialize(String),
    /// Top-level value was not an object/map.
    InvalidRootValue,
    /// A required subcommand field contained a non-enum value.
    InvalidSubcommandValue {
        /// Effective field name of the subcommand field.
        field_name: String,
    },
    /// An enum variant did not match any known subcommand.
    UnknownSubcommandVariant {
        /// Effective field name of the subcommand field.
        field_name: String,
        /// Variant name encountered in serialized data.
        variant: String,
    },
    /// A counted flag had a negative count.
    NegativeCount {
        /// Effective field name of the counted argument.
        arg_name: String,
        /// Negative count encountered in serialized data.
        count: i64,
    },
    /// A scalar argument value had an unsupported shape.
    UnsupportedScalarValue {
        /// Effective field name of the argument.
        arg_name: String,
    },
    /// Failed to resolve the current executable path.
    CurrentExe(String),
}

impl core::fmt::Display for ToArgsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ToArgsError::SchemaBuild(message) => write!(f, "failed to build schema: {message}"),
            ToArgsError::Serialize(message) => {
                write!(f, "failed to serialize CLI value: {message}")
            }
            ToArgsError::InvalidRootValue => {
                write!(f, "top-level value must serialize to an object")
            }
            ToArgsError::InvalidSubcommandValue { field_name } => {
                write!(
                    f,
                    "subcommand field `{field_name}` must serialize to an enum"
                )
            }
            ToArgsError::UnknownSubcommandVariant {
                field_name,
                variant,
            } => {
                write!(
                    f,
                    "unknown subcommand variant `{variant}` for field `{field_name}`"
                )
            }
            ToArgsError::NegativeCount { arg_name, count } => {
                write!(
                    f,
                    "counted argument `{arg_name}` cannot have negative count `{count}`"
                )
            }
            ToArgsError::UnsupportedScalarValue { arg_name } => {
                write!(f, "argument `{arg_name}` has an unsupported scalar value")
            }
            ToArgsError::CurrentExe(message) => {
                write!(f, "failed to resolve current executable: {message}")
            }
        }
    }
}

impl std::error::Error for ToArgsError {}

/// Convert a typed CLI value into a vector of CLI arguments.
///
/// This uses figue's schema and Facet serialization metadata, so consumers do not
/// need to hand-write ad-hoc `ToArgs` implementations for each command/subcommand.
pub fn to_os_args<T: Facet<'static> + ?Sized>(value: &T) -> Result<Vec<OsString>, ToArgsError> {
    let schema = Schema::from_shape(T::SHAPE)
        .map_err(|error| ToArgsError::SchemaBuild(error.to_string()))?;
    to_os_args_with_schema(value, &schema)
}

/// Convert a typed CLI value into a display-oriented argument string.
///
/// This joins [`to_os_args`] with spaces as an [`OsString`] and applies basic
/// double-quoting when a token contains spaces or single quotes. It is not a
/// full shell-escaping implementation.
pub fn to_args_string<T: Facet<'static> + ?Sized>(value: &T) -> Result<OsString, ToArgsError> {
    let args = to_os_args(value)?;
    Ok(render_display_command(args.iter().map(OsString::as_os_str)))
}

/// Convert a typed CLI value into a display-oriented command string prefixed
/// with the current executable path.
pub fn to_args_string_with_current_exe<T: Facet<'static> + ?Sized>(
    value: &T,
) -> Result<OsString, ToArgsError> {
    let exe =
        std::env::current_exe().map_err(|error| ToArgsError::CurrentExe(error.to_string()))?;

    let exe_display = exe.to_string_lossy();
    if exe_display.contains(' ') {
        tracing::warn!(
            exe_path = %exe_display,
            "to_args_string_with_current_exe is using basic quoting for an executable path containing spaces"
        );
    }

    let args = to_os_args(value)?;
    Ok(render_display_command(
        std::iter::once(exe.as_os_str()).chain(args.iter().map(OsString::as_os_str)),
    ))
}

/// Convenience trait for converting typed CLI values to argument vectors.
pub trait ToArgs: Facet<'static> {
    /// Convert this value into a vector of CLI arguments.
    fn to_args(&self) -> Result<Vec<OsString>, ToArgsError> {
        to_os_args(self)
    }

    /// Convert this value into a display-oriented argument string.
    fn to_args_string(&self) -> Result<OsString, ToArgsError> {
        to_args_string(self)
    }

    /// Convert this value into a display-oriented command string prefixed with
    /// the current executable path.
    fn to_args_string_with_current_exe(&self) -> Result<OsString, ToArgsError> {
        to_args_string_with_current_exe(self)
    }
}

impl<T: Facet<'static>> ToArgs for T {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NamedArgValueMode {
    CountedFlag,
    BoolFlag,
    RequiredValue,
}

fn named_arg_value_mode(schema: &ArgSchema) -> NamedArgValueMode {
    match schema.kind() {
        ArgKind::Named { counted: true, .. } => NamedArgValueMode::CountedFlag,
        ArgKind::Named { counted: false, .. } if schema.value().inner_if_option().is_bool() => {
            NamedArgValueMode::BoolFlag
        }
        ArgKind::Named { counted: false, .. } => NamedArgValueMode::RequiredValue,
        ArgKind::Positional => panic!("named_arg_value_mode called for a positional argument"),
    }
}

pub(crate) fn to_os_args_with_schema<T: Facet<'static> + ?Sized>(
    value: &T,
    schema: &Schema,
) -> Result<Vec<OsString>, ToArgsError> {
    let config_value = serialize_to_config_value(value)?;
    let ConfigValue::Object(root) = config_value else {
        return Err(ToArgsError::InvalidRootValue);
    };

    let mut args = Vec::new();
    encode_level(schema.args(), &root.value, &mut args)?;
    Ok(args)
}

fn serialize_to_config_value<T: Facet<'static> + ?Sized>(
    value: &T,
) -> Result<ConfigValue, ToArgsError> {
    let mut serializer = ConfigValueSerializer::new();
    facet_format::serialize_root(&mut serializer, facet_reflect::Peek::new(value))
        .map_err(|error| ToArgsError::Serialize(error.to_string()))?;
    Ok(serializer.finish())
}

fn encode_level(
    level: &ArgLevelSchema,
    values: &ObjectMap,
    args: &mut Vec<OsString>,
) -> Result<(), ToArgsError> {
    for (name, schema) in level.args() {
        if !matches!(schema.kind(), ArgKind::Named { .. }) {
            continue;
        }
        let Some(value) = values.get(name) else {
            continue;
        };
        encode_named_arg(name, schema, value, args)?;
    }

    let mut emitted_positional_separator = false;
    for (name, schema) in level.args() {
        if !matches!(schema.kind(), ArgKind::Positional) {
            continue;
        }
        let Some(value) = values.get(name) else {
            continue;
        };
        encode_positional_arg(name, schema, value, args, &mut emitted_positional_separator)?;
    }

    if let Some(field_name) = level.subcommand_field_name()
        && let Some(value) = values.get(field_name)
    {
        let value = unwrap_explicit_some(value);

        if matches!(value, ConfigValue::Null(_)) {
            return Ok(());
        }

        let Some((variant_name, variant_fields)) = as_enum_variant(value) else {
            return Err(ToArgsError::InvalidSubcommandValue {
                field_name: field_name.to_string(),
            });
        };

        let branch = level
            .subcommands()
            .values()
            .find(|candidate| candidate.effective_name() == variant_name)
            .ok_or_else(|| ToArgsError::UnknownSubcommandVariant {
                field_name: field_name.to_string(),
                variant: variant_name.to_string(),
            })?;

        args.push(branch.cli_name().to_string().into());
        encode_level(branch.args(), variant_fields, args)?;
    }

    Ok(())
}

fn unwrap_explicit_some(mut value: &ConfigValue) -> &ConfigValue {
    while let ConfigValue::ExplicitSome(sourced) = value {
        value = sourced.value.as_ref();
    }
    value
}

fn encode_named_arg(
    name: &str,
    schema: &ArgSchema,
    value: &ConfigValue,
    args: &mut Vec<OsString>,
) -> Result<(), ToArgsError> {
    let flag = format!("--{}", name.to_kebab_case());

    // `Some(None)` on an optional-value flag means "flag present, no value"
    // (bare `--flag`). Emit it before unwrapping, or the ExplicitSome(Null)
    // collapses to Null below and the flag is silently dropped.
    if schema.value().optional_value_inner().is_some()
        && let ConfigValue::ExplicitSome(sourced) = value
        && matches!(sourced.value.as_ref(), ConfigValue::Null(_))
    {
        args.push(flag.into());
        return Ok(());
    }

    let value = unwrap_explicit_some(value);

    if matches!(value, ConfigValue::Null(_)) {
        return Ok(());
    }

    let value_mode = named_arg_value_mode(schema);

    if matches!(value_mode, NamedArgValueMode::CountedFlag) {
        let ConfigValue::Integer(count) = value else {
            return Err(ToArgsError::UnsupportedScalarValue {
                arg_name: name.to_string(),
            });
        };

        if count.value < 0 {
            return Err(ToArgsError::NegativeCount {
                arg_name: name.to_string(),
                count: count.value,
            });
        }

        for _ in 0..count.value {
            args.push(flag.clone().into());
        }
        return Ok(());
    }

    if matches!(value_mode, NamedArgValueMode::BoolFlag) {
        let ConfigValue::Bool(bool_value) = value else {
            return Err(ToArgsError::UnsupportedScalarValue {
                arg_name: name.to_string(),
            });
        };

        if bool_value.value {
            args.push(flag.into());
        } else if matches!(schema.default(), Some(ConfigValue::Bool(default)) if default.value) {
            args.push(format!("--no-{}", name.to_kebab_case()).into());
        }
        return Ok(());
    }

    if schema.multiple() {
        let ConfigValue::Array(array) = value else {
            return Err(ToArgsError::UnsupportedScalarValue {
                arg_name: name.to_string(),
            });
        };

        for item in &array.value {
            let item = unwrap_explicit_some(item);

            if matches!(item, ConfigValue::Null(_)) {
                continue;
            }

            args.push(flag.clone().into());
            args.push(
                value_to_cli_token(name, item, Some(schema.value().inner_if_option()))?.into(),
            );
        }

        return Ok(());
    }

    args.push(flag.into());
    args.push(value_to_cli_token(name, value, Some(schema.value().inner_if_option()))?.into());
    Ok(())
}

fn encode_positional_arg(
    name: &str,
    schema: &ArgSchema,
    value: &ConfigValue,
    args: &mut Vec<OsString>,
    emitted_positional_separator: &mut bool,
) -> Result<(), ToArgsError> {
    // `ConfigValueSerializer` wraps `Option::Some` to preserve option nesting.
    // Positional CLI values have no such wrapper, so emit the wrapped value itself.
    let value = unwrap_explicit_some(value);

    match value {
        ConfigValue::Null(_) => Ok(()),
        ConfigValue::Array(array) => {
            for item in &array.value {
                let item = unwrap_explicit_some(item);

                if matches!(item, ConfigValue::Null(_)) {
                    continue;
                }
                let token = value_to_cli_token(name, item, Some(schema.value().inner_if_option()))?;
                maybe_emit_positional_separator(args, &token, emitted_positional_separator);
                args.push(token.into());
            }
            Ok(())
        }
        _ => {
            let token = value_to_cli_token(name, value, Some(schema.value().inner_if_option()))?;
            maybe_emit_positional_separator(args, &token, emitted_positional_separator);
            args.push(token.into());
            Ok(())
        }
    }
}

fn maybe_emit_positional_separator(
    args: &mut Vec<OsString>,
    token: &str,
    emitted_positional_separator: &mut bool,
) {
    if !*emitted_positional_separator && (token == "--" || token.starts_with('-')) {
        args.push("--".into());
        *emitted_positional_separator = true;
    }
}

fn render_display_command<'a>(tokens: impl IntoIterator<Item = &'a OsStr>) -> OsString {
    let mut rendered = OsString::new();

    for token in tokens {
        if !rendered.is_empty() {
            rendered.push(" ");
        }
        rendered.push(render_display_token(token));
    }

    rendered
}

fn render_display_token(token: &OsStr) -> OsString {
    let token_display = token.to_string_lossy();
    if token_display.contains('"') {
        tracing::warn!(
            value = %token_display,
            "to_args_string is using basic quoting for a value containing a double quote"
        );
    }

    if token_display.contains(' ') || token_display.contains('\'') {
        let mut quoted = OsString::from("\"");
        quoted.push(token);
        quoted.push("\"");
        quoted
    } else {
        token.to_os_string()
    }
}

fn value_to_cli_token(
    name: &str,
    value: &ConfigValue,
    value_schema: Option<&ValueSchema>,
) -> Result<String, ToArgsError> {
    match value {
        ConfigValue::Bool(sourced) => Ok(sourced.value.to_string()),
        ConfigValue::Integer(sourced) => integer_to_cli_token(name, sourced.value, value_schema),
        ConfigValue::Float(sourced) => Ok(sourced.value.to_string()),
        ConfigValue::String(sourced) => Ok(sourced.value.clone()),
        ConfigValue::Enum(sourced) if sourced.value.fields.is_empty() => {
            Ok(sourced.value.variant.to_kebab_case())
        }
        ConfigValue::Object(sourced) if sourced.value.len() == 1 => Ok(sourced
            .value
            .first()
            .map(|(variant, _)| variant.to_kebab_case())
            .unwrap_or_default()),
        _ => Err(ToArgsError::UnsupportedScalarValue {
            arg_name: name.to_string(),
        }),
    }
}

fn integer_to_cli_token(
    name: &str,
    value: i64,
    value_schema: Option<&ValueSchema>,
) -> Result<String, ToArgsError> {
    fn unsupported(arg_name: &str) -> Result<String, ToArgsError> {
        Err(ToArgsError::UnsupportedScalarValue {
            arg_name: arg_name.to_string(),
        })
    }

    let scalar = match value_schema {
        Some(ValueSchema::Leaf(leaf)) => leaf.shape.scalar_type(),
        _ => None,
    };

    match scalar {
        Some(FacetScalarType::U8) => match u8::try_from(value) {
            Ok(value) => Ok(value.to_string()),
            Err(_) => unsupported(name),
        },
        Some(FacetScalarType::U16) => match u16::try_from(value) {
            Ok(value) => Ok(value.to_string()),
            Err(_) => unsupported(name),
        },
        Some(FacetScalarType::U32) => match u32::try_from(value) {
            Ok(value) => Ok(value.to_string()),
            Err(_) => unsupported(name),
        },
        Some(FacetScalarType::U64) => Ok((value as u64).to_string()),
        Some(FacetScalarType::U128) => match u128::try_from(value) {
            Ok(value) => Ok(value.to_string()),
            Err(_) => unsupported(name),
        },
        Some(FacetScalarType::USize) => Ok((value as usize).to_string()),
        _ => Ok(value.to_string()),
    }
}

fn as_enum_variant(value: &ConfigValue) -> Option<(&str, &ObjectMap)> {
    match value {
        ConfigValue::Enum(sourced) => Some((&sourced.value.variant, &sourced.value.fields)),
        ConfigValue::String(sourced) => Some((&sourced.value, empty_object_map())),
        ConfigValue::Object(sourced) if sourced.value.len() == 1 => {
            let (variant_name, payload) = sourced.value.first()?;
            match payload {
                ConfigValue::Object(variant_fields) => Some((variant_name, &variant_fields.value)),
                ConfigValue::Null(_) => Some((variant_name, empty_object_map())),
                _ => None,
            }
        }
        _ => None,
    }
}

fn empty_object_map() -> &'static ObjectMap {
    static EMPTY: std::sync::OnceLock<ObjectMap> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Default::default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate as args;
    use crate::config_value::{EnumValue, Sourced};
    use facet::Facet;
    use indexmap::indexmap;

    #[derive(Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Command {
        Build {
            #[facet(args::named)]
            release: bool,
        },
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Cli {
        #[facet(args::named)]
        verbose: bool,

        #[facet(args::subcommand)]
        command: Command,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct UnsignedCli {
        #[facet(args::named)]
        limit: usize,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct Unsigned128Cli {
        #[facet(args::named)]
        limit: u128,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DefaultTrueBoolCli {
        #[facet(args::named, default = true)]
        verbose: bool,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct BoolVecCli {
        #[facet(args::named)]
        verbose: Vec<bool>,
    }

    #[derive(Facet, Debug, PartialEq)]
    struct DashPositionalCli {
        #[facet(args::positional)]
        query: String,
    }

    #[test]
    fn to_args_roundtrip_basic() {
        let cli = Cli {
            verbose: true,
            command: Command::Build { release: true },
        };

        let args = to_os_args(&cli).expect("to_args should succeed");
        let args_as_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        let parsed: Cli =
            crate::from_slice(&args_as_str.iter().map(String::as_str).collect::<Vec<_>>())
                .into_result()
                .expect("roundtrip parse should succeed")
                .get_silent();

        assert_eq!(cli, parsed);
    }

    #[test]
    fn to_args_string_joins_arguments() {
        let cli = Cli {
            verbose: true,
            command: Command::Build { release: true },
        };

        let args_string = to_args_string(&cli).expect("to_args_string should succeed");
        let args_string = args_string.to_string_lossy();

        assert!(args_string.contains("--verbose"));
        assert!(args_string.contains("build"));
        assert!(args_string.contains("--release"));
    }

    #[test]
    fn to_args_string_with_current_exe_prefixes_command() {
        let cli = Cli {
            verbose: false,
            command: Command::Build { release: false },
        };

        let command = to_args_string_with_current_exe(&cli)
            .expect("to_args_string_with_current_exe should succeed");
        let command = command.to_string_lossy();
        let exe_display = std::env::current_exe()
            .expect("current_exe should resolve")
            .to_string_lossy()
            .to_string();

        assert!(command.starts_with(&exe_display));
        assert!(command.contains("build"));
    }

    #[test]
    fn to_args_string_quotes_values_with_spaces_or_single_quotes() {
        let cli = BoolVecCli { verbose: vec![] };

        let rendered = render_display_command([
            OsStr::new("plain"),
            OsStr::new("two words"),
            OsStr::new("it's"),
        ]);

        assert_eq!(rendered.to_string_lossy(), "plain \"two words\" \"it's\"");

        let args_string = to_args_string(&cli).expect("to_args_string should succeed");
        assert!(args_string.is_empty());
    }

    #[test]
    fn to_args_roundtrips_large_usize_values() {
        if usize::BITS < 64 {
            return;
        }

        let cli = UnsignedCli { limit: usize::MAX };

        let args = to_os_args(&cli).expect("to_args should succeed for large usize values");
        let args_as_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert!(
            args_as_str.contains(&usize::MAX.to_string()),
            "generated args should preserve the original unsigned value"
        );

        let parsed: UnsignedCli =
            crate::from_slice(&args_as_str.iter().map(String::as_str).collect::<Vec<_>>())
                .into_result()
                .expect("roundtrip parse should succeed")
                .get_silent();

        assert_eq!(cli, parsed);
    }

    #[test]
    fn to_args_emits_no_flag_for_default_true_bool_set_false() {
        let cli = DefaultTrueBoolCli { verbose: false };

        let args = to_os_args(&cli).expect("to_args should succeed");
        let args_as_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(args_as_str, vec!["--no-verbose"]);

        let parsed: DefaultTrueBoolCli =
            crate::from_slice(&args_as_str.iter().map(String::as_str).collect::<Vec<_>>())
                .into_result()
                .expect("roundtrip parse should succeed")
                .get_silent();

        assert_eq!(cli, parsed);
    }

    #[test]
    fn to_args_emits_explicit_values_for_vec_of_bool() {
        let cli = BoolVecCli {
            verbose: vec![true, false, true],
        };

        let args = to_os_args(&cli).expect("to_args should succeed");
        let args_as_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            args_as_str,
            vec![
                "--verbose",
                "true",
                "--verbose",
                "false",
                "--verbose",
                "true"
            ]
        );

        let parsed: BoolVecCli =
            crate::from_slice(&args_as_str.iter().map(String::as_str).collect::<Vec<_>>())
                .into_result()
                .expect("roundtrip parse should succeed")
                .get_silent();

        assert_eq!(cli, parsed);
    }

    #[test]
    fn to_args_rejects_lossy_u128_intermediate_values() {
        let schema = Schema::from_shape(Unsigned128Cli::SHAPE).expect("schema should be valid");

        let mut root = indexmap! {};
        root.insert("limit".to_string(), ConfigValue::Integer(Sourced::new(-1)));

        let mut args = Vec::new();
        let error = encode_level(schema.args(), &root, &mut args)
            .expect_err("lossy u128 intermediate values should fail");

        assert!(matches!(
            error,
            ToArgsError::UnsupportedScalarValue { arg_name } if arg_name == "limit"
        ));
    }

    #[test]
    fn to_args_inserts_separator_for_dash_prefixed_positionals() {
        let cli = DashPositionalCli {
            query: "-0".to_string(),
        };

        let args = to_os_args(&cli).expect("to_args should succeed");
        let args_as_str = args
            .iter()
            .map(|arg| arg.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(args_as_str, vec!["--", "-0"]);

        let parsed: DashPositionalCli =
            crate::from_slice(&args_as_str.iter().map(String::as_str).collect::<Vec<_>>())
                .into_result()
                .expect("roundtrip parse should succeed")
                .get_silent();

        assert_eq!(cli, parsed);
    }

    #[test]
    fn to_args_fails_for_unknown_subcommand_variant() {
        let schema = Schema::from_shape(Cli::SHAPE).expect("schema should be valid");

        let mut root = indexmap! {};
        root.insert(
            "command".to_string(),
            ConfigValue::Enum(Sourced::new(EnumValue {
                variant: "Unknown".to_string(),
                fields: indexmap! {},
            })),
        );

        let mut args = Vec::new();
        let error = encode_level(schema.args(), &root, &mut args).expect_err("should fail");

        assert!(matches!(
            error,
            ToArgsError::UnknownSubcommandVariant { .. }
        ));
    }

    #[test]
    fn to_args_fails_for_unknown_string_subcommand_value() {
        let schema = Schema::from_shape(Cli::SHAPE).expect("schema should be valid");

        let mut root = indexmap! {};
        root.insert(
            "command".to_string(),
            ConfigValue::String(Sourced::new("build".to_string())),
        );

        let mut args = Vec::new();
        let error = encode_level(schema.args(), &root, &mut args).expect_err("should fail");

        assert!(matches!(
            error,
            ToArgsError::UnknownSubcommandVariant { .. }
        ));
    }
}
