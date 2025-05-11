use crate::error::{ArgsError, ArgsErrorKind};
use facet_core::FieldAttribute;
use facet_reflect::Wip;

/// Process a named argument (--name value)
pub fn parse_named_arg<'input, 'facet>(
    wip: Wip<'facet>,
    key: &str,
    args: &mut &[&'input str],
) -> Result<Wip<'facet>, ArgsError>
where
    'input: 'facet,
{
    // Extract the named argument parsing logic from from_slice
    let field_index = match wip.field_index(key) {
        Some(index) => index,
        None => {
            return Err(ArgsError::new(ArgsErrorKind::GenericArgsError(format!(
                "Unknown argument `{key}`",
            ))));
        }
    };

    let field = wip
        .field(field_index)
        .expect("field_index should be a valid field bound");

    if field.shape().is_type::<bool>() {
        crate::parse_field(field, "true")
    } else {
        let value = args
            .first()
            .ok_or(ArgsError::new(ArgsErrorKind::GenericArgsError(format!(
                "expected value after argument `{key}`"
            ))))?;
        *args = &args[1..]; // Consume the value token
        crate::parse_field(field, value)
    }
}

/// Process a short argument (-n value)
pub fn parse_short_arg<'input, 'facet>(
    wip: Wip<'facet>,
    key: &str,
    args: &mut &[&'input str],
    st: &facet_core::StructType,
) -> Result<Wip<'facet>, ArgsError>
where
    'input: 'facet,
{
    // Extract the short argument parsing logic from from_slice
    for (field_index, f) in st.fields.iter().enumerate() {
        if f.attributes.iter().any(
            |a| matches!(a, FieldAttribute::Arbitrary(a) if a.contains("short") && a.contains(key)),
        ) {
            let field = wip.field(field_index).expect("field_index is in bounds");
            if field.shape().is_type::<bool>() {
                return crate::parse_field(field, "true");
            } else {
                let value = args
                    .first()
                    .ok_or(ArgsError::new(ArgsErrorKind::GenericArgsError(format!(
                        "expected value after argument `{key}`"
                    ))))?;
                *args = &args[1..]; // Consume the value token
                return crate::parse_field(field, value);
            }
        }
    }
    // No matching field found
    Err(ArgsError::new(ArgsErrorKind::GenericArgsError(format!(
        "Unknown short argument `-{}`",
        key
    ))))
}

/// Process a positional argument
pub fn parse_positional_arg<'input, 'facet>(
    wip: Wip<'facet>,
    token: &'input str,
    st: &facet_core::StructType,
) -> Result<Wip<'facet>, ArgsError>
where
    'input: 'facet,
{
    // Extract the positional argument parsing logic from from_slice
    for (field_index, f) in st.fields.iter().enumerate() {
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
            return crate::parse_field(field, token);
        }
    }
    // No matching field found
    Err(ArgsError::new(ArgsErrorKind::GenericArgsError(format!(
        "No positional argument field available for token `{}`",
        token
    ))))
}
