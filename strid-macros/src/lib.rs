//! You probably want the [`strid`] crate, which
//! has the documentation this crate lacks.
//!
//!   [`strid`]: https://docs.rs/strid/*/strid/

#![warn(
    missing_docs,
    unused_import_braces,
    unused_imports,
    unused_qualifications
)]
#![deny(
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unused_must_use
)]
#![forbid(unsafe_code)]

extern crate proc_macro;

mod codegen;

use codegen::{Params, ParamsRef};
use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Constructs a braid
///
/// Any attributes assigned to the the struct will be applied to both the owned
/// and borrowed types, except for doc-comments, with will only be applied to the
/// owned form.
///
/// Available options:
/// * `ref_name = "RefName"`
///   * Sets the name of the borrowed type
/// * `ref_doc = "Alternate doc comment"`
///   * Overrides the default doc comment for the borrowed type
/// * `ref_attr = "#[derive(...)]"`
///   * Provides an attribute to be placed only on the borrowed type
/// * `owned_attr = "#[derive(...)]"`
///   * Provides an attribute to be placed only on the owned type
/// * either `validator [ = "Type" ]` or `normalizer [ = "Type" ]`
///   * Indicates the type is validated or normalized. If not specified, it is assumed that the
///     braid implements the relevant trait itself.
/// * `clone = "impl|omit"` (default: `impl`)
///   * Changes the automatic derivation of a `Clone` implementation on the owned type.
/// * `debug = "impl|owned|omit"` (default `impl`)
///   * Changes how automatic implementations of the `Debug` trait are provided. If `owned`, then
///     the owned type will generate a `Debug` implementation that will just delegate to the
///     borrowed implementation. If `omit`, then no implementations of `Debug` will be provided.
/// * `display = "impl|owned|omit"` (default `impl`)
///   * Changes how automatic implementations of the `Display` trait are provided. If `owned`, then
///     the owned type will generate a `Display` implementation that will just delegate to the
///     borrowed implementation. If `omit`, then no implementations of `Display` will be provided.
/// * `ord = "impl|owned|omit"` (default `impl`)
///   * Changes how automatic implementations of the `PartialOrd` and `Ord` traits are provided. If
///     `owned`, then the owned type will generate implementations that will just delegate to the
///     borrowed implementations. If `omit`, then no implementations will be provided.
/// * `serde = "impl|omit"` (default `omit`)
///   * Adds serialize and deserialize implementations
/// * `no_expose`
///   * Functions that expose the internal field type will not be exposed publicly.
/// * `no_std`
///   * Generates `no_std`-compatible braid (still requires `alloc`)
#[proc_macro_attribute]
pub fn braid(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as Params);
    let body = parse_macro_input!(input as syn::ItemStruct);

    args.build(body)
        .map_or_else(syn::Error::into_compile_error, |codegen| codegen.generate())
        .into()
}

/// Constructs a ref-only braid
///
/// Available options:
/// * either `validator [ = "Type" ]`
///   * Indicates the type is validated. If not specified, it is assumed that the braid implements
///     the relevant trait itself.
/// * `debug = "impl|omit"` (default `impl`)
///   * Changes how automatic implementations of the `Debug` trait are provided. If `omit`, then no
///     implementations of `Debug` will be provided.
/// * `display = "impl|omit"` (default `impl`)
///   * Changes how automatic implementations of the `Display` trait are provided. If `omit`, then
///     no implementations of `Display` will be provided.
/// * `ord = "impl|omit"` (default `impl`)
///   * Changes how automatic implementations of the `PartialOrd` and `Ord` traits are provided. If
///     `omit`, then no implementations will be provided.
/// * `serde = "impl|omit"` (default `omit`)
///   * Adds serialize and deserialize implementations
/// * `no_std`
///   * Generates a `no_std`-compatible braid that doesn't require `alloc`
#[proc_macro_attribute]
pub fn braid_ref(args: TokenStream, input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as ParamsRef);
    let mut body = parse_macro_input!(input as syn::ItemStruct);

    args.build(&mut body)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn as_validator(validator: &syn::Type) -> proc_macro2::TokenStream {
    quote::quote! { <#validator as ::strid::Validator> }
}

fn as_normalizer(normalizer: &syn::Type) -> proc_macro2::TokenStream {
    quote::quote! { <#normalizer as ::strid::Normalizer> }
}
