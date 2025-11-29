//! Code generation for extension attributes.

use facet_macros_parse::{Ident, TokenStream};
use quote::{ToTokens, quote};

/// Emits the code for an `ExtensionAttr` on a field.
///
/// This generates code that calls the extension crate's dispatcher macro:
///
/// For `#[facet(kdl::child)]` on field `server: Server`:
/// ```ignore
/// kdl::__attr!(child { server : Server })
/// ```
///
/// For `#[facet(args::short = 'v')]` on field `verbose: bool`:
/// ```ignore
/// args::__attr!(short { verbose : bool | = 'v' })
/// ```
pub fn emit_extension_attr_for_field(
    ns_ident: &Ident,
    key_ident: &Ident,
    args: &TokenStream,
    field_name: &impl ToTokens,
    field_type: &TokenStream,
) -> TokenStream {
    if args.is_empty() {
        // No args: ns::__attr!(key { field : Type })
        quote! {
            #ns_ident::__attr!(#key_ident { #field_name : #field_type })
        }
    } else {
        // With args: ns::__attr!(key { field : Type | args })
        quote! {
            #ns_ident::__attr!(#key_ident { #field_name : #field_type | #args })
        }
    }
}

/// Emits the code for an `ExtensionAttr` without field context.
///
/// This is used for struct-level, enum-level, or variant-level attributes.
///
/// For `#[facet(ns::attr)]` at container level:
/// ```ignore
/// ns::__attr!(attr { })
/// ```
///
/// For `#[facet(ns::attr = "value")]` at container level:
/// ```ignore
/// ns::__attr!(attr { | = "value" })
/// ```
pub fn emit_extension_attr(ns_ident: &Ident, key_ident: &Ident, args: &TokenStream) -> TokenStream {
    if args.is_empty() {
        // No args: ns::__attr!(key { })
        quote! {
            #ns_ident::__attr!(#key_ident { })
        }
    } else {
        // With args: ns::__attr!(key { | args })
        quote! {
            #ns_ident::__attr!(#key_ident { | #args })
        }
    }
}
