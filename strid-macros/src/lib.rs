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

mod attr_grammar;
mod codegen;
mod grammar;

use codegen::{Params, ParamsRef};
use grammar::ItemStruct;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use unsynn::*;

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
    let args_ts: TokenStream2 = args.into();
    let input_ts: TokenStream2 = input.into();

    let mut args_iter = args_ts.to_token_iter();
    let parsed_args = match args_iter.parse::<attr_grammar::AttrArgs>() {
        Ok(args) => args,
        Err(e) => return compile_error(format!("failed to parse braid args: {e}")).into(),
    };
    let args = match Params::from_args(parsed_args) {
        Ok(args) => args,
        Err(e) => return compile_error(format!("failed to process braid args: {e}")).into(),
    };

    let mut input_iter = input_ts.to_token_iter();
    let body = match input_iter.parse::<ItemStruct>() {
        Ok(body) => body,
        Err(e) => return compile_error(format!("failed to parse struct: {e}")).into(),
    };

    args.build(body).map_or_else(
        |e| compile_error(e).into(),
        |codegen| codegen.generate().into(),
    )
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
    let args_ts: TokenStream2 = args.into();
    let input_ts: TokenStream2 = input.into();

    let mut args_iter = args_ts.to_token_iter();
    let parsed_args = match args_iter.parse::<attr_grammar::AttrArgs>() {
        Ok(args) => args,
        Err(e) => return compile_error(format!("failed to parse braid_ref args: {e}")).into(),
    };
    let args = match ParamsRef::from_args(parsed_args) {
        Ok(args) => args,
        Err(e) => return compile_error(format!("failed to process braid_ref args: {e}")).into(),
    };

    let mut input_iter = input_ts.to_token_iter();
    let mut body = match input_iter.parse::<ItemStruct>() {
        Ok(body) => body,
        Err(e) => return compile_error(format!("failed to parse struct: {e}")).into(),
    };

    args.build(&mut body)
        .map_or_else(|e| compile_error(e).into(), |tokens| tokens.into())
}

/// Helper to create a compile error.
fn compile_error(msg: impl std::fmt::Display) -> TokenStream2 {
    let msg = msg.to_string();
    quote::quote! {
        compile_error!(#msg)
    }
}

fn as_validator(validator: &grammar::Type) -> proc_macro2::TokenStream {
    let ty = validator.to_token_stream();
    quote::quote! { <#ty as ::strid::Validator> }
}

fn as_normalizer(normalizer: &grammar::Type) -> proc_macro2::TokenStream {
    let ty = normalizer.to_token_stream();
    quote::quote! { <#ty as ::strid::Normalizer> }
}
