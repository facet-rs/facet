use std::ffi::OsString;

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

/// Convert a typed CLI value into a shell-friendly command argument string.
///
/// This is equivalent to [`to_os_args`] joined by spaces with lossy UTF-8 conversion.
pub fn to_args_string<T: Facet<'static> + ?Sized>(value: &T) -> Result<String, ToArgsError> {
    let args = to_os_args(value)?;
    Ok(args
        .iter()
        .map(|arg| arg.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" "))
}

/// Convert a typed CLI value into a shell-friendly command string prefixed with
/// the current executable path.
pub fn to_args_string_with_current_exe<T: Facet<'static> + ?Sized>(
    value: &T,
) -> Result<String, ToArgsError> {
    let exe =
        std::env::current_exe().map_err(|error| ToArgsError::CurrentExe(error.to_string()))?;
    let exe_display = exe.to_string_lossy().to_string();
    let args = to_args_string(value)?;

    if args.is_empty() {
        Ok(exe_display)
    } else {
        Ok(format!("{exe_display} {args}"))
    }
}

/// Convenience trait for converting typed CLI values to argument vectors.
pub trait ToArgs: Facet<'static> {
    /// Convert this value into a vector of CLI arguments.
    fn to_args(&self) -> Result<Vec<OsString>, ToArgsError> {
        to_os_args(self)
    }

    /// Convert this value into a shell-friendly command argument string.
    fn to_args_string(&self) -> Result<String, ToArgsError> {
        to_args_string(self)
    }

    /// Convert this value into a shell-friendly command string prefixed with
    /// the current executable path.
    fn to_args_string_with_current_exe(&self) -> Result<String, ToArgsError> {
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
        ArgKind::Named { counted: false, .. } if schema.value().is_bool_or_vec_of_bool() => {
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

fn encode_named_arg(
    name: &str,
    schema: &ArgSchema,
    value: &ConfigValue,
    args: &mut Vec<OsString>,
) -> Result<(), ToArgsError> {
    let flag = format!("--{}", name.to_kebab_case());

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
        if let ConfigValue::Bool(bool_value) = value
            && bool_value.value
        {
            args.push(flag.into());
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
            if matches!(item, ConfigValue::Null(_)) {
                continue;
            }

            args.push(flag.clone().into());
            args.push(value_to_cli_token(name, item, Some(schema.value().inner_if_option()))?.into());
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
    match value {
        ConfigValue::Null(_) => Ok(()),
        ConfigValue::Array(array) => {
            for item in &array.value {
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

fn value_to_cli_token(
    name: &str,
    value: &ConfigValue,
    value_schema: Option<&ValueSchema>,
) -> Result<String, ToArgsError> {
    match value {
        ConfigValue::Bool(sourced) => Ok(sourced.value.to_string()),
        ConfigValue::Integer(sourced) => Ok(integer_to_cli_token(sourced.value, value_schema)),
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


fn integer_to_cli_token(value: i64, value_schema: Option<&ValueSchema>) -> String {
    let scalar = match value_schema {
        Some(ValueSchema::Leaf(leaf)) => leaf.shape.scalar_type(),
        _ => None,
    };

    match scalar {
        Some(FacetScalarType::U8) => (value as u8).to_string(),
        Some(FacetScalarType::U16) => (value as u16).to_string(),
        Some(FacetScalarType::U32) => (value as u32).to_string(),
        Some(FacetScalarType::U64) => (value as u64).to_string(),
        Some(FacetScalarType::U128) => ((value as u64) as u128).to_string(),
        Some(FacetScalarType::USize) => (value as usize).to_string(),
        _ => value.to_string(),
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
        let exe_display = std::env::current_exe()
            .expect("current_exe should resolve")
            .to_string_lossy()
            .to_string();

        assert!(command.starts_with(&exe_display));
        assert!(command.contains("build"));
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
