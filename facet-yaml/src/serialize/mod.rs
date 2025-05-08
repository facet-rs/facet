//! Create and/or write YAML strings from Rust values.

/// Serialize any `Facet` type to a YAML string.
#[cfg(feature = "alloc")]
pub fn to_string<'a, T: facet_core::Facet<'a>>(
    _value: &'a T,
) -> Result<alloc::string::String, core::convert::Infallible> {
    todo!()
}
