use crate::deserialize::{DeserErrorKind, Outcome, Raw, Scalar, Span, Spanned, Subspan};
use alloc::borrow::Cow;

/// General purpose wrapper for results
pub(crate) fn wrap_result<'input, T>(
    result: Result<T, DeserErrorKind>,
    success_fn: impl FnOnce(T) -> Outcome<'input>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'input>, Raw>, Spanned<DeserErrorKind, Raw>> {
    match result {
        Ok(value) => Ok(Spanned {
            node: success_fn(value),
            span,
        }),
        Err(err) => Err(Spanned { node: err, span }),
    }
}

/// Convenience wrapper for validation results that map to a single outcome
pub(crate) fn wrap_outcome_result<'input>(
    result: Result<(), DeserErrorKind>,
    success_outcome: Outcome<'input>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'input>, Raw>, Spanned<DeserErrorKind, Raw>> {
    wrap_result(result, |_| success_outcome, span)
}

/// Convenience wrapper for string results that become scalars
pub(crate) fn wrap_string_result<'input>(
    result: Result<Cow<'input, str>, DeserErrorKind>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'input>, Raw>, Spanned<DeserErrorKind, Raw>> {
    wrap_result(result, |s| Outcome::Scalar(Scalar::String(s)), span)
}

/// Convenience wrapper for field name results that become scalars
pub(crate) fn wrap_field_result<'input>(
    result: Result<&'static str, DeserErrorKind>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'input>, Raw>, Spanned<DeserErrorKind, Raw>> {
    wrap_result(
        result.map(Cow::Borrowed),
        |s| Outcome::Scalar(Scalar::String(s)),
        span,
    )
}

/// Convenience wrapper for creating a Resegmented outcome with subspans
pub(crate) fn wrap_resegmented_result<'input>(
    subspans: Vec<Subspan>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'input>, Raw>, Spanned<DeserErrorKind, Raw>> {
    Ok(Spanned {
        node: Outcome::Resegmented(subspans),
        span,
    })
}
