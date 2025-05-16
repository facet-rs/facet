use facet_core::Facet;
use facet_deserialize::DeserError;

/// Deserialize JSON from a given byte slice
pub(crate) fn from_slice<'input, 'facet, 'shape, T: Facet<'facet>>(
    input: &'input [u8],
    _recursion_depth: usize,
) -> Result<T, DeserError<'input, 'shape>>
where
    'input: 'facet,
{
    crate::iterative::from_slice(input)
}

/// Deserialize JSON from a given string
pub(crate) fn from_str<'input, 'facet, 'shape, T: Facet<'facet>>(
    input: &'input str,
    recursion_depth: usize,
) -> Result<T, DeserError<'input, 'shape>>
where
    'input: 'facet,
{
    let input = input.as_bytes();
    from_slice(input, recursion_depth)
}
