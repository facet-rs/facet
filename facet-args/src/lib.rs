#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

use facet_core::{Def, Facet, FieldAttribute};
use facet_reflect::{ReflectError, Wip};

fn parse_field<'facet>(wip: Wip<'facet>, value: &'facet str) -> Result<Wip<'facet>, ReflectError> {
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
            return Err(ReflectError::OperationFailed {
                shape,
                operation: "parsing field",
            });
        }
    }?
    .pop()
}

/// Parses command-line arguments
pub fn from_slice<'input, 'facet, T>(s: &[&'input str]) -> T
where
    T: Facet<'facet>,
    'input: 'facet,
{
    log::trace!("Entering from_slice function");
    let mut s = s;
    let mut wip = Wip::alloc::<T>().expect("failed to allocate");
    log::trace!("Allocated Poke for type T");

    while let Some(token) = s.first() {
        log::trace!("Processing token: {}", token);
        s = &s[1..];

        if let Some(key) = token.strip_prefix("--") {
            log::trace!("Found named argument: {}", key);
            let field_index = match wip.field_index(key) {
                Some(index) => index,
                None => panic!("Unknown argument: {}", key),
            };
            let field = wip.field(field_index).unwrap();

            if field.shape().is_type::<bool>() {
                wip = parse_field(field, "true").unwrap();
            } else {
                let value = s.first().expect("expected value after argument");
                log::trace!("Field value: {}", value);
                s = &s[1..];
                wip = parse_field(field, value).unwrap();
            }
        } else if let Some(key) = token.strip_prefix("-") {
            log::trace!("Found short named argument: {}", key);
            let Def::Struct(sd) = wip.shape().def else {
                panic!("Expected struct definition");
            };
            for (field_index, f) in sd.fields.iter().enumerate() {
                if f.attributes
                    .iter()
                    .any(|a| matches!(a, FieldAttribute::Arbitrary(a) if a.contains("short") && a.contains(key))
                   )
                {
                    log::trace!("Found field matching short_code: {} for field {}", key, f.name);
                    let field = wip.field(field_index).unwrap();
                    if field.shape().is_type::<bool>() {
                        wip = parse_field(field, "true").unwrap();
                    } else {
                        let value = s.first().expect("expected value after argument");
                        log::trace!("Field value: {}", value);
                        s = &s[1..];
                        wip = parse_field(field, value).unwrap();
                    }
                    break;
                }
            }
        } else {
            log::trace!("Encountered positional argument: {}", token);
            let Def::Struct(sd) = wip.shape().def else {
                panic!("Expected struct definition");
            };

            for (field_index, f) in sd.fields.iter().enumerate() {
                if f.attributes
                    .iter()
                    .any(|a| matches!(a, FieldAttribute::Arbitrary(a) if a.contains("positional")))
                {
                    if wip.is_field_set(field_index).unwrap() {
                        continue;
                    }
                    let field = wip.field(field_index).unwrap();
                    wip = parse_field(field, token).unwrap();
                    break;
                }
            }
        }
    }
    wip.build().unwrap().materialize().unwrap()
}
