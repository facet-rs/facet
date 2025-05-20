use alloc::borrow::Cow;
use alloc::string::ToString;
use facet_core::{FieldAttribute, Shape, Type, UserType};
use facet_deserialize::{DeserErrorKind, Outcome, Raw, Scalar, Span, Spanned};
use facet_reflect::Wip;

pub(crate) fn validate_field<'facet, 'shape>(
    field_name: &str,
    shape: &'shape Shape<'shape>,
    wip: &Wip<'facet, 'shape>,
) -> Result<(), DeserErrorKind<'shape>> {
    if let Type::User(UserType::Struct(_)) = &shape.ty {
        if wip.field_index(field_name).is_none() {
            return Err(DeserErrorKind::UnknownField {
                field_name: field_name.to_string(),
                shape,
            });
        }
    }
    Ok(())
}

// Find a positional field
pub(crate) fn find_positional_field<'facet, 'shape>(
    shape: &'shape Shape<'shape>,
    wip: &Wip<'facet, 'shape>,
) -> Result<&'shape str, DeserErrorKind<'shape>> {
    if let Type::User(UserType::Struct(st)) = &shape.ty {
        for (idx, field) in st.fields.iter().enumerate() {
            for attr in field.attributes.iter() {
                if let FieldAttribute::Arbitrary(a) = attr {
                    if a.contains("positional") {
                        // Check if this field is already set
                        let is_set = wip.is_field_set(idx).unwrap_or(false);
                        if !is_set {
                            return Ok(field.name);
                        }
                    }
                }
            }
        }
    }
    Err(DeserErrorKind::UnknownField {
        field_name: "positional argument".to_string(),
        shape,
    })
}

// Find an unset boolean field for implicit false handling
pub(crate) fn find_unset_bool_field<'facet, 'shape>(
    shape: &'shape Shape<'shape>,
    wip: &Wip<'facet, 'shape>,
) -> Option<&'shape str> {
    if let Type::User(UserType::Struct(st)) = &shape.ty {
        for (idx, field) in st.fields.iter().enumerate() {
            if !wip.is_field_set(idx).unwrap_or(false) && field.shape().is_type::<bool>() {
                return Some(field.name);
            }
        }
    }
    None
}

pub(crate) fn find_field_by_short_flag<'shape>(
    key: &str,
    shape: &'shape Shape<'shape>,
) -> Result<&'shape str, DeserErrorKind<'shape>> {
    match &shape.ty {
        Type::User(UserType::Struct(st)) => st
            .fields
            .iter()
            .find(|field| {
                field.attributes.iter().any(|attr| {
                    matches!(attr, FieldAttribute::Arbitrary(a) if a.contains("short") &&
                            (a.contains(key) || (key.len() == 1 && field.name == key)))
                })
            })
            .map(|field| field.name)
            .ok_or_else(|| DeserErrorKind::UnknownField {
                field_name: key.to_string(),
                shape,
            }),
        _ => Err(DeserErrorKind::UnsupportedType {
            got: shape,
            wanted: "struct",
        }),
    }
}

// Create a missing value error
pub(crate) fn create_missing_value_error<'shape>(field: &str) -> DeserErrorKind<'shape> {
    DeserErrorKind::MissingValue {
        expected: "argument value",
        field: field.to_string(),
    }
}

// Handle boolean value parsing
pub(crate) fn handle_bool_value<'shape>(
    args_available: bool,
) -> Result<Scalar<'static>, DeserErrorKind<'shape>> {
    Ok(Scalar::Bool(args_available))
}

// Check if a value is available and valid (not a flag)
pub(crate) fn validate_value_available<'shape, 'input>(
    arg_idx: usize,
    args: &[&'input str],
) -> Result<&'input str, DeserErrorKind<'shape>> {
    if arg_idx >= args.len() {
        return Err(create_missing_value_error(args[arg_idx.saturating_sub(1)]));
    }

    let arg = args[arg_idx];
    if arg.starts_with('-') {
        return Err(create_missing_value_error(args[arg_idx.saturating_sub(1)]));
    }

    Ok(arg)
}

// Check if a list has reached its end
pub(crate) fn is_list_ended(arg_idx: usize, args: &[&str]) -> bool {
    arg_idx >= args.len() || args[arg_idx].starts_with('-')
}

// Validate a struct type and return appropriate error if it's not a struct
pub(crate) fn validate_struct_type<'shape>(
    shape: &'shape Shape<'shape>,
) -> Result<(), DeserErrorKind<'shape>> {
    if !matches!(shape.ty, Type::User(UserType::Struct(_))) {
        Err(DeserErrorKind::UnsupportedType {
            got: shape,
            wanted: "struct",
        })
    } else {
        Ok(())
    }
}

pub(crate) fn create_unknown_field_error<'shape>(
    field_name: &str,
    shape: &'shape Shape<'shape>,
) -> DeserErrorKind<'shape> {
    DeserErrorKind::UnknownField {
        field_name: field_name.to_string(),
        shape,
    }
}

pub(crate) fn handle_unset_bool_field_error<'shape>(
    field_name_opt: Option<&'shape str>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'shape>, Raw>, Spanned<DeserErrorKind<'shape>, Raw>> {
    Ok(Spanned {
        node: match field_name_opt {
            Some(field_name) => Outcome::Scalar(Scalar::String(Cow::Borrowed(field_name))),
            None => Outcome::ObjectEnded,
        },
        span,
    })
}
