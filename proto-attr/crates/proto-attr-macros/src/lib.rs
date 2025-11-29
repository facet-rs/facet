//! Proc-macros for the proto-attr grammar system.
//!
//! This crate provides:
//! - `#[derive(Faket)]` - derive macro that processes `#[faket(...)]` attributes
//! - `__make_parse_attr!` - generates types and parsing macros from a grammar
//! - `__attr_error!` - produces helpful errors for unknown attributes
//! - `__field_error!` - produces helpful errors for unknown fields
//! - `__spanned_error!` - generic helper for emitting spanned errors from macro_rules

#![cfg_attr(feature = "nightly", feature(proc_macro_diagnostic))]

use proc_macro::TokenStream;

mod attr_error;
mod derive_faket;
mod dispatch_attr;
mod dispatch_column_field;
mod field_error;
mod make_parse_attr;
mod spanned_error;

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

/// Produces a compile error with precise span from macro_rules.
///
/// Usage: `__spanned_error!{ { tokens } => "message" }`
///
/// The error will be spanned to the first token in the braces.
#[proc_macro]
pub fn __spanned_error(input: TokenStream) -> TokenStream {
    spanned_error::spanned_error(input)
}

/// Dispatches to attribute parsers while preserving spans.
///
/// This overcomes the macro_rules limitation where pattern matching
/// doesn't capture the matched token for span preservation.
#[proc_macro]
pub fn __dispatch_attr(input: TokenStream) -> TokenStream {
    dispatch_attr::dispatch_attr(input)
}

/// Dispatches column field parsing while preserving spans.
#[proc_macro]
pub fn __dispatch_column_field(input: TokenStream) -> TokenStream {
    dispatch_column_field::dispatch_column_field(input)
}
