use alloc::borrow::Cow;
use facet_deserialize::{DeserErrorKind, Outcome, Raw, Scalar, Span, Spanned};

/// General purpose wrapper for results
pub(crate) fn wrap_result<'input, 'shape, T>(
    result: Result<T, DeserErrorKind<'shape>>,
    success_fn: impl FnOnce(T) -> Outcome<'input>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'input>, Raw>, Spanned<DeserErrorKind<'shape>, Raw>> {
    match result {
        Ok(value) => Ok(Spanned {
            node: success_fn(value),
            span,
        }),
        Err(err) => Err(Spanned { node: err, span }),
    }
}

/// Convenience wrapper for validation results that map to a single outcome
pub(crate) fn wrap_outcome_result<'input, 'shape>(
    result: Result<(), DeserErrorKind<'shape>>,
    success_outcome: Outcome<'input>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'input>, Raw>, Spanned<DeserErrorKind<'shape>, Raw>> {
    wrap_result(result, |_| success_outcome, span)
}

/// Convenience wrapper for string results that become scalars
pub(crate) fn wrap_string_result<'input, 'shape>(
    result: Result<Cow<'input, str>, DeserErrorKind<'shape>>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'input>, Raw>, Spanned<DeserErrorKind<'shape>, Raw>> {
    wrap_result(result, |s| Outcome::Scalar(Scalar::String(s)), span)
}

/// Convenience wrapper for field name results that become scalars
pub(crate) fn wrap_field_result<'shape>(
    result: Result<&'shape str, DeserErrorKind<'shape>>,
    span: Span<Raw>,
) -> Result<Spanned<Outcome<'shape>, Raw>, Spanned<DeserErrorKind<'shape>, Raw>> {
    wrap_result(
        result.map(Cow::Borrowed),
        |s| Outcome::Scalar(Scalar::String(s)),
        span,
    )
}
