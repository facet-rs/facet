use facet_core::Facet;
use facet_deserialize::DeserError;

/// Deserialize JSON from a given byte slice
pub(crate) fn from_slice<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
    _recursion_depth: usize,
) -> Result<T, DeserError<'input>> {
    crate::iterative::from_slice(input)
}

/// Deserialize JSON from a given string
pub(crate) fn from_str<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
    recursion_depth: usize,
) -> Result<T, DeserError<'input>> {
    let input = input.as_bytes();
    from_slice(input, recursion_depth)
}

/// Deserialize JSON from a given string, converting any dynamic error into a static one.
///
/// This function attempts to deserialize a type `T` implementing `Facet` from the input string slice.
/// If deserialization fails, the error is converted into an owned, static error type to avoid lifetime issues.
pub(crate) fn from_str_static_error<'input: 'facet, 'facet, T: Facet<'facet>>(
    input: &'input str,
    _recursion_depth: usize,
) -> Result<T, DeserError<'input>> {
    crate::iterative::from_str_static_error(input)
}
