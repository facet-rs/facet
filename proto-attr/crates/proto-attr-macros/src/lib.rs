//! Proc-macros for the proto-attr grammar system.
//!
//! This crate provides:
//! - `#[derive(Faket)]` - derive macro that processes `#[faket(...)]` attributes
//! - `__make_parse_attr!` - generates types and parsing macros from a grammar
//! - `__attr_error!` - produces helpful errors for unknown attributes
//! - `__field_error!` - produces helpful errors for unknown fields

use proc_macro::TokenStream;

mod attr_error;
mod derive_faket;
mod field_error;
mod make_parse_attr;

/// Derive macro that processes `#[faket(...)]` attributes.
///
/// Supports namespaced attributes like `#[faket(ns::attr(...))]` which
/// are dispatched to `ns::__parse_attr!(attr(...))`.
///
/// # Example
///
/// ```ignore
/// #[derive(Faket)]
/// #[faket(proto_ext::skip)]
/// struct Foo {
///     #[faket(proto_ext::column(name = "id", primary_key))]
///     id: i64,
/// }
/// ```
#[proc_macro_derive(Faket, attributes(faket))]
pub fn derive_faket(input: TokenStream) -> TokenStream {
    derive_faket::derive_faket(input)
}

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
