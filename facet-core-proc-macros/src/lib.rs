//! Proc macros for facet-core.

use proc_macro::TokenStream;
use quote::quote;
use unsynn::*;

unsynn! {
    /// Input for the `define_extension_attrs!` macro.
    struct DefineExtensionAttrsInput {
        /// The crate name for error messages (e.g., "KDL")
        crate_name: LiteralString,
        /// Semicolon separator
        _semi: Semicolon,
        /// The comma-delimited list of attribute names
        attrs: CommaDelimitedVec<Ident>,
    }
}

/// Defines extension attributes for use with `#[facet(namespace::attr)]` syntax.
///
/// This proc macro generates an `attrs` module containing:
/// - Marker structs for each attribute
/// - Getter functions that return `AnyStaticRef`
/// - Validation machinery using `#[diagnostic::on_unimplemented]` for compile-time error messages
///
/// # Example
///
/// ```ignore
/// facet_core_proc_macros::define_extension_attrs! {
///     "KDL";
///     child,
///     children,
///     property,
/// }
/// ```
#[proc_macro]
pub fn define_extension_attrs(input: TokenStream) -> TokenStream {
    let input = unsynn::TokenStream::from(input);
    let mut iter = input.to_token_iter();
    let parsed: DefineExtensionAttrsInput = iter.parse().expect("failed to parse input");

    // LiteralString::value() includes quotes, so strip them
    let crate_name_raw = parsed.crate_name.value();
    let crate_name = crate_name_raw.trim_matches('"');
    let attrs: Vec<_> = parsed.attrs.iter().map(|d| d.value.clone()).collect();

    // Build the message string: "`{A}` is not a recognized KDL attribute"
    let message = format!("`{{A}}` is not a recognized {} attribute", crate_name);

    // Build the note string: "valid attributes are: `child`, `children`, ..."
    let attr_list: Vec<String> = attrs.iter().map(|a| format!("`{}`", a)).collect();
    let note = format!("valid attributes are: {}", attr_list.join(", "));

    // Generate the marker structs and functions
    let attr_defs = attrs.iter().map(|name| {
        quote! {
            #[doc(hidden)]
            #[allow(non_camel_case_types)]
            pub struct #name { _private: () }

            #[doc(hidden)]
            pub fn #name(_args: &[::facet_core::Token]) -> ::facet_core::AnyStaticRef {
                static __UNIT: () = ();
                &__UNIT
            }
        }
    });

    // Generate the trait impls: impl IsValidAttr<child> for () {}
    // Use do_not_recommend to suppress "the following types implement trait" noise
    let trait_impls = attrs.iter().map(|name| {
        quote! {
            #[diagnostic::do_not_recommend]
            impl IsValidAttr<#name> for () {}
        }
    });

    let output = quote! {
        /// Extension attributes for this crate.
        pub mod attrs {
            #(#attr_defs)*

            #[doc(hidden)]
            #[diagnostic::on_unimplemented(
                message = #message,
                label = "unknown attribute",
                note = #note
            )]
            pub trait IsValidAttr<A> {}

            #[doc(hidden)]
            pub const fn __check_attr<A>() where (): IsValidAttr<A> {}

            #(#trait_impls)*
        }
    };

    output.into()
}
