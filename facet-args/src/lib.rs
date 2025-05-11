#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;
use alloc::borrow::Cow;

/// Apply field default values and function values using facet-deserialize
pub mod defaults;
/// Errors raised when CLI arguments are not parsed or otherwise fail during reflection
pub mod error;
/// Parsing utilities for CLI arguments
pub mod parse;

use defaults::apply_field_defaults;
use error::{ArgsError, ArgsErrorKind};
use facet_core::{Def, Facet, Type, UserType};
use facet_reflect::{ReflectError, Wip};
// use format::CliFormat;
use parse::{parse_named_arg, parse_positional_arg, parse_short_arg};

/// Process a field in the Wip
pub fn parse_field<'facet>(wip: Wip<'facet>, value: &'facet str) -> Result<Wip<'facet>, ArgsError> {
    let shape = wip.shape();

    if shape.is_type::<String>() {
        log::trace!("shape is String");
        wip.put(value.to_string())
    } else if shape.is_type::<&str>() {
        log::trace!("shape is &str");
        wip.put(value)
    } else if shape.is_type::<bool>() {
        log::trace!("shape is bool, setting to true");
        wip.put(value.to_lowercase() == "true")
    } else {
        match shape.def {
            Def::Scalar(_) => {
                log::trace!("shape is nothing known, falling back to parse: {}", shape);
                wip.parse(value)
            }
            _def => {
                return Err(ArgsError::new(ArgsErrorKind::GenericReflect(
                    ReflectError::OperationFailed {
                        shape,
                        operation: "parsing field",
                    },
                )));
            }
        }
    }
    .map_err(|e| ArgsError::new(ArgsErrorKind::GenericReflect(e)))?
    .pop()
    .map_err(|e| ArgsError {
        kind: ArgsErrorKind::GenericReflect(e),
    })
}

fn kebab_to_snake(input: &str) -> Cow<str> {
    // ASSUMPTION: We only support GNU/Unix kebab-case named argument
    // ASSUMPTION: struct fields are snake_case
    if !input.contains('-') {
        return Cow::Borrowed(input);
    }
    Cow::Owned(input.replace('-', "_"))
}

/// Parses command-line arguments
pub fn from_slice<'input, 'facet, T>(s: &[&'input str]) -> Result<T, ArgsError>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!("Entering from_slice function");
    let mut s = s;
    let mut wip =
        Wip::alloc::<T>().map_err(|e| ArgsError::new(ArgsErrorKind::GenericReflect(e)))?;
    log::trace!("Allocated Poke for type T");
    let Type::User(UserType::Struct(st)) = wip.shape().ty else {
        return Err(ArgsError::new(ArgsErrorKind::GenericArgsError(
            "Expected struct type".to_string(),
        )));
    };

    while let Some(token) = s.first() {
        log::trace!("Processing token: {}", token);
        s = &s[1..];

        if let Some(key) = token.strip_prefix("--") {
            let key = kebab_to_snake(key);
            log::trace!("Found named argument: {}", key);
            wip = parse_named_arg(wip, &key, &mut s)?;
        } else if let Some(key) = token.strip_prefix("-") {
            log::trace!("Found short named argument: {}", key);
            wip = parse_short_arg(wip, key, &mut s, &st)?;
        } else {
            log::trace!("Encountered positional argument: {}", token);
            wip = parse_positional_arg(wip, token, &st)?;
        }
    }

    // Apply defaults, except for absent booleans being implicitly default false
    wip = apply_field_defaults(wip)?;

    // If a boolean field is unset the value is set to `false`
    // This behaviour means `#[facet(default = false)]` does not need to be explicitly set
    // on each boolean field specified on a Command struct
    for (field_index, f) in st.fields.iter().enumerate() {
        if f.shape().is_type::<bool>() && !wip.is_field_set(field_index).expect("in bounds") {
            let field = wip.field(field_index).expect("field_index is in bounds");
            wip = parse_field(field, "false")?;
        }
    }

    // Add this right after getting the struct type (st)
    log::trace!("Checking field attributes");
    for (i, field) in st.fields.iter().enumerate() {
        log::trace!(
            "Field {}: {} - Attributes: {:?}",
            i,
            field.name,
            field.attributes
        );
    }

    let heap_vale = wip
        .build()
        .map_err(|e| ArgsError::new(ArgsErrorKind::GenericReflect(e)))?;
    let result = heap_vale
        .materialize()
        .map_err(|e| ArgsError::new(ArgsErrorKind::GenericReflect(e)))?;
    Ok(result)
}
