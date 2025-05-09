#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;
use alloc::borrow::Cow;

mod error;

use error::{ArgsError, ArgsErrorKind};
use facet_core::{Def, Facet, FieldAttribute, StructKind};
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
        Def::Enum(_) => {
            log::trace!("Deserializing {} as {}", value, "enum");
            let value = &kebab_to_pascal_case(value);
            let wip = wip.variant_named(value).map_err(|e| ArgsError {
                kind: ArgsErrorKind::GenericReflect(e),
            })?;
            let variant = wip.selected_variant().unwrap();
            if variant.data.kind == StructKind::Unit {
                let wip = wip.pop().map_err(|e| ArgsError {
                    kind: ArgsErrorKind::GenericReflect(e),
                })?;
                return Ok(wip);
            }
            todo!("Only unit-like enums supported")
            // Need to re-work the parsing of args input so that fields present in a enum are grouped
            // Or see if struct enum be iteratively built assuming the input will eventually have all
            // the needed fields
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

fn kebab_to_pascal_case(input: &str) -> String {
    // TODO: See if we can leverage `renamerule.rs` without requiring user
    //       to set rename rules on every field in Args defintion
    input
        .split('-')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut chars = s.chars();
            chars
                .next()
                .map(|c| c.to_ascii_uppercase())
                .into_iter()
                .chain(chars)
                .collect::<String>()
        })
        .collect()
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
