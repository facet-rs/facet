#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;
use alloc::borrow::Cow;

mod error;

use error::{ArgsError, ArgsErrorKind};
use facet_core::{Def, Facet, FieldAttribute};
use facet_reflect::{ReflectError, Wip};

fn parse_field<'facet>(wip: Wip<'facet>, value: &'facet str) -> Result<Wip<'facet>, ArgsError> {
    let shape = wip.shape();
    match shape.def {
        Def::Scalar(_) => {
            if shape.is_type::<String>() {
                wip.put(value.to_string())
            } else if shape.is_type::<&str>() {
                wip.put(value)
            } else if shape.is_type::<bool>() {
                log::trace!("Boolean field detected, setting to true");
                wip.put(value.to_lowercase() == "true")
            } else {
                wip.parse(value)
            }
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
    let Def::Struct(sd) = wip.shape().def else {
        return Err(ArgsError::new(ArgsErrorKind::GenericArgsError(
            "Expected struct defintion".to_string(),
        )));
    };

    while let Some(token) = s.first() {
        log::trace!("Processing token: {}", token);
        s = &s[1..];

        if let Some(key) = token.strip_prefix("--") {
            let key = kebab_to_snake(key);
            let field_index = match wip.field_index(&key) {
                Some(index) => index,
                None => {
                    return Err(ArgsError::new(ArgsErrorKind::GenericArgsError(format!(
                        "Unknown argument `{key}`",
                    ))));
                }
            };
            log::trace!("Found named argument: {}", key);

            let field = wip
                .field(field_index)
                .expect("field_index should be a valid field bound");

            if field.shape().is_type::<bool>() {
                // TODO: absence i.e "false" case is not handled
                wip = parse_field(field, "true")?;
            } else {
                let value = s
                    .first()
                    .ok_or(ArgsError::new(ArgsErrorKind::GenericArgsError(format!(
                        "expected value after argument `{key}`"
                    ))))?;
                log::trace!("Field value: {}", value);
                s = &s[1..];
                wip = parse_field(field, value)?;
            }
        } else if let Some(key) = token.strip_prefix("-") {
            log::trace!("Found short named argument: {}", key);
            for (field_index, f) in sd.fields.iter().enumerate() {
                if f.attributes
                    .iter()
                    .any(|a| matches!(a, FieldAttribute::Arbitrary(a) if a.contains("short") && a.contains(key))
                   )
                {
                    log::trace!("Found field matching short_code: {} for field {}", key, f.name);
                    let field = wip.field(field_index).expect("field_index is in bounds");
                    if field.shape().is_type::<bool>() {
                        wip = parse_field(field, "true")?;
                    } else {
                        let value = s
                            .first()
                            .ok_or(ArgsError::new(ArgsErrorKind::GenericArgsError(format!(
                                "expected value after argument `{key}`"
                            ))))?;
                        log::trace!("Field value: {}", value);
                        s = &s[1..];
                        wip = parse_field(field, value)?;
                    }
                    break;
                }
            }
        } else {
            log::trace!("Encountered positional argument: {}", token);
            for (field_index, f) in sd.fields.iter().enumerate() {
                if f.attributes
                    .iter()
                    .any(|a| matches!(a, FieldAttribute::Arbitrary(a) if a.contains("positional")))
                {
                    if wip
                        .is_field_set(field_index)
                        .expect("field_index is in bounds")
                    {
                        continue;
                    }
                    let field = wip.field(field_index).expect("field_index is in bounds");
                    wip = parse_field(field, token)?;
                    break;
                }
            }
        }
    }

    // If a boolean field is unset the value is set to `false`
    // This behaviour means `#[facet(default = false)]` does not need to be explicitly set
    // on each boolean field specified on a Command struct
    for (field_index, f) in sd.fields.iter().enumerate() {
        if f.shape().is_type::<bool>() && !wip.is_field_set(field_index).expect("in bounds") {
            let field = wip.field(field_index).expect("field_index is in bounds");
            wip = parse_field(field, "false")?;
        }
    }

    let heap_vale = wip
        .build()
        .map_err(|e| ArgsError::new(ArgsErrorKind::GenericReflect(e)))?;
    let result = heap_vale
        .materialize()
        .map_err(|e| ArgsError::new(ArgsErrorKind::GenericReflect(e)))?;
    Ok(result)
}
