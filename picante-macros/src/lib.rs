#![warn(missing_docs)]

//! Proc macros for Picante (`#[picante::input]`, `#[picante::tracked]`, etc.).

use proc_macro::TokenStream;

mod db;
mod input;
mod interned;
mod struct_item;
mod tracked;
mod util;

/// Generate a memoized derived query from an async function.
#[proc_macro_attribute]
pub fn tracked(_attr: TokenStream, item: TokenStream) -> TokenStream {
    tracked::expand(item)
}

/// Generate an interned-key input "entity" from a struct definition.
#[proc_macro_attribute]
pub fn input(_attr: TokenStream, item: TokenStream) -> TokenStream {
    input::expand(item)
}

/// Generate an interned "entity" from a struct definition.
#[proc_macro_attribute]
pub fn interned(_attr: TokenStream, item: TokenStream) -> TokenStream {
    interned::expand(item)
}

/// Generate a database struct wiring together ingredients and runtime.
#[proc_macro_attribute]
pub fn db(attr: TokenStream, item: TokenStream) -> TokenStream {
    db::expand(attr, item)
}
