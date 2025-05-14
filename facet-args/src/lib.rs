#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;
use core::fmt::Display;

use alloc::borrow::Cow;

/// Apply field default values and function values using facet-deserialize
pub(crate) mod defaults;
/// Errors raised when CLI arguments are not parsed or otherwise fail during reflection
pub mod error;
/// Parsing utilities for CLI arguments
pub(crate) mod parse;

use defaults::apply_field_defaults;
use error::{ArgsError, ArgsErrorKind};
use facet_core::{Def, Facet, StructKind, Type, UserType};
use facet_reflect::{ReflectError, Wip};
use parse::{parse_named_arg, parse_positional_arg, parse_short_arg};

/// The types of arguments that can be encountered in a CLI command
#[derive(Debug)]
enum ArgumentKind<'input> {
    Positional {
        value: Option<&'input str>,
    },
    Named {
        name: &'input str,
        value: Option<&'input str>,
    },
    ShortName {
        short_name: &'input str,
        value: Option<&'input str>,
    },
}

impl Display for ArgumentKind<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ArgumentKind::Positional { value } => {
                write!(f, "positional arg with value: `{:?}`", value)
            }
            ArgumentKind::Named { name, value } => {
                write!(f, "named arg: `{}` with value: `{:?}`", name, value)
            }
            ArgumentKind::ShortName { short_name, value } => {
                write!(f, "short arg: `{}` with value: `{:?}`", short_name, value)
            }
        }
    }
}

/// A single command line argument
#[derive(Debug)]
struct Argument<'input> {
    kind: ArgumentKind<'input>,
    offset: usize,
}

impl Display for Argument<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Argument of type: `{}` in position `{}`",
            self.kind, self.offset
        )
    }
}

/// Parser for CLI commands
struct CLIParser<'input> {
    rest: &'input [&'input str],
    current_offset: usize,
}

impl<'input> CLIParser<'input> {
    #[must_use]
    fn new(input: &'input [&'input str]) -> Self {
        Self {
            rest: input,
            current_offset: 0,
        }
    }
}

impl<'input> Iterator for CLIParser<'input> {
    type Item = Result<Argument<'input>, ArgsError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }

        let current = self.rest[0];
        let offset = self.current_offset;
        log::trace!("Parsing value in position: {}", offset);
        self.current_offset += 1;
        self.rest = &self.rest[1..];

        if let Some(token) = current.strip_prefix("--") {
            // Named argument
            let name_value: Vec<&str> = token.splitn(2, '=').collect();
            let name = name_value[0];

            if name_value.len() == 2 {
                // --name=value format
                Some(Ok(Argument {
                    kind: ArgumentKind::Named {
                        name,
                        value: Some(name_value[1]),
                    },
                    offset,
                }))
            } else {
                // --name value format
                if !self.rest.is_empty() && !self.rest[0].starts_with('-') {
                    let value = self.rest[0];
                    self.rest = &self.rest[1..];
                    self.current_offset += 1;
                    Some(Ok(Argument {
                        kind: ArgumentKind::Named {
                            name,
                            value: Some(value),
                        },
                        offset,
                    }))
                } else {
                    Some(Ok(Argument {
                        kind: ArgumentKind::Named { name, value: None },
                        offset,
                    }))
                }
            }
        } else if current.starts_with('-') && current != "-" {
            // Short name argument
            let name_value: Vec<&str> = current[1..].splitn(2, '=').collect();
            let short_name = name_value[0];

            if name_value.len() == 2 {
                // -s=value format
                Some(Ok(Argument {
                    kind: ArgumentKind::ShortName {
                        short_name,
                        value: Some(name_value[1]),
                    },
                    offset,
                }))
            } else {
                // -s value format
                if !self.rest.is_empty() && !self.rest[0].starts_with('-') {
                    let value = self.rest[0];
                    self.rest = &self.rest[1..];
                    self.current_offset += 1;
                    Some(Ok(Argument {
                        kind: ArgumentKind::ShortName {
                            short_name,
                            value: Some(value),
                        },
                        offset,
                    }))
                } else {
                    Some(Ok(Argument {
                        kind: ArgumentKind::ShortName {
                            short_name,
                            value: None,
                        },
                        offset,
                    }))
                }
            }
        } else {
            // Positional argument
            Some(Ok(Argument {
                kind: ArgumentKind::Positional {
                    value: Some(current),
                },
                offset,
            }))
        }
    }
}

/// Process a field in the Wip
pub(crate) fn parse_field<'facet>(
    wip: Wip<'facet>,
    value: &'facet str,
) -> Result<Wip<'facet>, ArgsError> {
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
    } else if let Type::User(UserType::Enum(_)) = &shape.ty {
        log::trace!("Deserializing {} as enum", value);
        let value = &kebab_to_pascal_case(value);
        let wip = wip.variant_named(value).map_err(|e| ArgsError {
            kind: ArgsErrorKind::GenericReflect(e),
        })?;

        let variant = wip.selected_variant().unwrap();
        log::trace!("Found variant {:?}", variant.name);
        match variant.data.kind {
            StructKind::Unit => {
                let wip = wip.pop().map_err(|e| ArgsError {
                    kind: ArgsErrorKind::GenericReflect(e),
                })?;
                return Ok(wip);
            }
            // Need to re-work the parsing of args input so that fields present in a enum are grouped
            // Or see if struct enum be iteratively built assuming the input will eventually have all
            // the needed fields
            StructKind::TupleStruct => {
                todo!("Enum TupleStruct Unsupported. Only unit-like enums supported")
            }
            StructKind::Struct => todo!("Enum Struct Unsupported. Only unit-like enums supported"),
            StructKind::Tuple => todo!("Enum Tuple Unsupported. Only unit-like enums supported"),
            _ => todo!("Unknown Enum variant"),
        }
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
pub fn from_slice<'input, 'facet, T>(s: &'facet [&'input str]) -> Result<T, ArgsError>
where
    T: Facet<'facet>,
    'input: 'facet,
{
    let cli_p = CLIParser::new(s);

    log::trace!("Entering from_slice function");
    let mut wip =
        Wip::alloc::<T>().map_err(|e| ArgsError::new(ArgsErrorKind::GenericReflect(e)))?;
    log::trace!("Allocated Poke for type T");
    let Type::User(UserType::Struct(st)) = wip.shape().ty else {
        return Err(ArgsError::new(ArgsErrorKind::GenericArgsError(
            "Expected struct type".to_string(),
        )));
    };

    for c in cli_p {
        match c {
            Ok(arg) => match arg.kind {
                ArgumentKind::Positional { value } => {
                    wip = parse_positional_arg(wip, value.expect("todo"), &st)?;
                }
                ArgumentKind::Named { name, value } => {
                    let field_name = &kebab_to_snake(name);
                    wip = parse_named_arg(wip, field_name, value)?;
                }
                ArgumentKind::ShortName { short_name, value } => {
                    wip = parse_short_arg(wip, short_name, value, &st)?;
                }
            },
            Err(e) => {
                return Err(ArgsError::new(ArgsErrorKind::GenericArgsError(format!(
                    "Unknown error: {:?}",
                    e
                ))));
            }
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
