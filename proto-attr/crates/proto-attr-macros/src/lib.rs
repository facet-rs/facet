//! Proc-macros for the proto-attr grammar system.
//!
//! This crate provides:
//! - `__make_parse_attr!` - generates types and parsing macros from a grammar
//! - `__attr_error!` - produces helpful errors for unknown attributes
//! - `__field_error!` - produces helpful errors for unknown fields

use proc_macro::TokenStream;

mod attr_error;
mod field_error;
mod make_parse_attr;

/// Generates attribute types and parsing macros from a grammar definition.
///
/// This is called by `define_attr_grammar!` and should not be used directly.
#[proc_macro]
pub fn __make_parse_attr(input: TokenStream) -> TokenStream {
    make_parse_attr::make_parse_attr(input)
}

/// Produces a compile error for an unknown attribute with suggestions.
///
/// This is called by the generated `__parse_attr!` macro on error paths.
#[proc_macro]
pub fn __attr_error(input: TokenStream) -> TokenStream {
    attr_error::attr_error(input)
}

/// Produces a compile error for an unknown field with suggestions.
///
/// This is called by the generated field-parsing macros on error paths.
#[proc_macro]
pub fn __field_error(input: TokenStream) -> TokenStream {
    field_error::field_error(input)
}
