//! Code generation for extension attributes.

use crate::{Delimiter, Group, Ident, PFacetAttr, Punct, Spacing, TokenStream, TokenTree};
use quote::{ToTokens, quote, quote_spanned};

/// Emits the code for an `ExtensionAttr` on a field.
///
/// This generates code that calls our `__ext!` proc macro, which then
/// forwards to the extension crate's dispatcher macro with proper spans.
///
/// For `#[facet(xml::element)]` on field `server: Server`:
/// ```ignore
/// ::facet::__ext!(xml::element { server : Server })
/// ```
///
/// For `#[facet(args::short = 'v')]` on field `verbose: bool`:
/// ```ignore
/// ::facet::__ext!(args::short { verbose : bool | = 'v' })
/// ```
pub fn emit_extension_attr_for_field(
    ns_ident: &Ident,
    key_ident: &Ident,
    args: &TokenStream,
    field_name: &impl ToTokens,
    field_type: &TokenStream,
    facet_crate: &TokenStream,
) -> TokenStream {
    if args.is_empty() {
        // No args: ::facet::__ext!(ns::key { field : Type })
        quote! {
            #facet_crate::__ext!(#ns_ident::#key_ident { #field_name : #field_type })
        }
    } else {
        // With args: ::facet::__ext!(ns::key { field : Type | args })
        quote! {
            #facet_crate::__ext!(#ns_ident::#key_ident { #field_name : #field_type | #args })
        }
    }
}

/// Emits the code for an `ExtensionAttr` without field context.
///
/// This is used for struct-level, enum-level, or variant-level attributes.
///
/// For `#[facet(ns::attr)]` at container level:
/// ```ignore
/// ::facet::__ext!(ns::attr { })
/// ```
///
/// For `#[facet(ns::attr = "value")]` at container level:
/// ```ignore
/// ::facet::__ext!(ns::attr { | = "value" })
/// ```
pub fn emit_extension_attr(
    ns_ident: &Ident,
    key_ident: &Ident,
    args: &TokenStream,
    facet_crate: &TokenStream,
) -> TokenStream {
    if args.is_empty() {
        // No args: ::facet::__ext!(ns::key { })
        quote! {
            #facet_crate::__ext!(#ns_ident::#key_ident { })
        }
    } else {
        // With args: ::facet::__ext!(ns::key { | args })
        quote! {
            #facet_crate::__ext!(#ns_ident::#key_ident { | #args })
        }
    }
}

/// Emits an attribute through grammar dispatch.
///
/// - Builtin attrs (no namespace) → `::facet::__attr!(...)`
/// - Namespaced attrs → `::facet::__ext!(ns::key ...)`
pub fn emit_attr(attr: &PFacetAttr, facet_crate: &TokenStream) -> TokenStream {
    let key = &attr.key;
    let args = &attr.args;

    match &attr.ns {
        Some(ns) => {
            // Namespaced: use __ext! which routes to ns::__attr!
            emit_extension_attr(ns, key, args, facet_crate)
        }
        None => {
            // Builtin: route directly to ::facet::__attr! (macro_export puts it at crate root)
            if args.is_empty() {
                quote! {
                    #facet_crate::__attr!(@ns { #facet_crate::builtin } #key { })
                }
            } else {
                quote! {
                    #facet_crate::__attr!(@ns { #facet_crate::builtin } #key { | #args })
                }
            }
        }
    }
}

/// Emits an attribute through grammar dispatch, with field context.
///
/// - Builtin attrs (no namespace) → `::facet::__attr!(...)`
/// - Namespaced attrs → `::facet::__ext!(ns::key ...)`
pub fn emit_attr_for_field(
    attr: &PFacetAttr,
    field_name: &impl ToTokens,
    field_type: &TokenStream,
    facet_crate: &TokenStream,
) -> TokenStream {
    let key = &attr.key;
    let args = &attr.args;

    match &attr.ns {
        Some(ns) => {
            // Namespaced: use existing helper
            emit_extension_attr_for_field(ns, key, args, field_name, field_type, facet_crate)
        }
        None => {
            // Builtin: route directly to ::facet::__attr! (macro_export puts it at crate root)
            if args.is_empty() {
                quote! {
                    #facet_crate::__attr!(@ns { #facet_crate::builtin } #key { #field_name : #field_type })
                }
            } else {
                quote! {
                    #facet_crate::__attr!(@ns { #facet_crate::builtin } #key { #field_name : #field_type | #args })
                }
            }
        }
    }
}

/// Implementation of the `__ext!` proc macro.
///
/// This proc macro receives extension attribute invocations and forwards them
/// to the extension crate's dispatcher macro while preserving spans for better
/// error messages.
///
/// Input format: `ns::attr_name { field : Type }` or `ns::attr_name { field : Type | args }`
/// Output: `ns::__attr!(attr_name { field : Type })` or `ns::__attr!(attr_name { field : Type | args })`
pub fn ext_attr(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter().peekable();

    // Parse: ns :: attr_name { ... }
    let ns_ident = match tokens.next() {
        Some(TokenTree::Ident(ident)) => ident,
        _ => {
            return quote! {
                ::core::compile_error!("__ext!: expected namespace identifier")
            };
        }
    };

    // Expect ::
    match (tokens.next(), tokens.next()) {
        (Some(TokenTree::Punct(p1)), Some(TokenTree::Punct(p2)))
            if p1.as_char() == ':' && p2.as_char() == ':' => {}
        _ => {
            return quote! {
                ::core::compile_error!("__ext!: expected '::'")
            };
        }
    }

    // Get the attribute name (this has the span we want to preserve!)
    let attr_ident = match tokens.next() {
        Some(TokenTree::Ident(ident)) => ident,
        _ => {
            return quote! {
                ::core::compile_error!("__ext!: expected attribute name")
            };
        }
    };

    // Get the braced content { ... }
    let body = match tokens.next() {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => g,
        _ => {
            return quote! {
                ::core::compile_error!("__ext!: expected braced body")
            };
        }
    };

    // Build the output: ns::__attr!(@ns { ns } attr_name { ... })
    // The attr_ident preserves its original span!
    // We pass the namespace so __attr! can do `use $ns::Attr as __ExtAttr;`
    let __attr = Ident::new("__attr", attr_ident.span());
    let at = Punct::new('@', Spacing::Alone);
    let ns_keyword = Ident::new("ns", attr_ident.span());

    let colon1 = Punct::new(':', Spacing::Joint);
    let colon2 = Punct::new(':', Spacing::Alone);
    let bang = Punct::new('!', Spacing::Alone);

    // Build the macro invocation tokens manually to preserve spans
    let mut output = TokenStream::new();
    output.extend([TokenTree::Ident(ns_ident.clone())]);
    output.extend([TokenTree::Punct(colon1.clone())]);
    output.extend([TokenTree::Punct(colon2.clone())]);
    output.extend([TokenTree::Ident(__attr)]);
    output.extend([TokenTree::Punct(bang)]);

    // Build the macro arguments: (@ns { ns_ident } attr_name { ... })
    let mut macro_args = TokenStream::new();
    // @ns { ns_ident }
    macro_args.extend([TokenTree::Punct(at)]);
    macro_args.extend([TokenTree::Ident(ns_keyword)]);
    let mut ns_group_content = TokenStream::new();
    ns_group_content.extend([TokenTree::Ident(ns_ident)]);
    macro_args.extend([TokenTree::Group(Group::new(
        Delimiter::Brace,
        ns_group_content,
    ))]);
    // attr_name { ... }
    macro_args.extend([TokenTree::Ident(attr_ident)]);
    macro_args.extend([TokenTree::Group(body)]);

    let args_group = Group::new(Delimiter::Parenthesis, macro_args);
    output.extend([TokenTree::Group(args_group)]);

    output
}

/// Implementation of the `__unknown_attr!` proc macro.
///
/// This generates a compile_error! with the span pointing to the unknown identifier.
///
/// Input: `unknown_ident`
/// Output: `compile_error!("unknown extension attribute `unknown_ident`")` with span on the ident
pub fn unknown_attr(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();

    // Get the unknown attribute identifier
    let ident = match tokens.next() {
        Some(TokenTree::Ident(ident)) => ident,
        _ => {
            return quote! {
                ::core::compile_error!("__unknown_attr!: expected identifier")
            };
        }
    };

    let span = ident.span();
    let message = format!("unknown extension attribute `{ident}`");

    quote_spanned! { span =>
        ::core::compile_error!(#message)
    }
}

/// Implementation of the `__no_args!` proc macro.
///
/// Generates a "does not accept arguments" error with the span pointing to the arguments.
///
/// Input: `"ns::attr", token`
/// Output: `compile_error!("ns::attr does not accept arguments")` with span on token
pub fn no_args(input: TokenStream) -> TokenStream {
    let mut tokens = input.into_iter();

    // Get the message string literal
    let msg = match tokens.next() {
        Some(TokenTree::Literal(lit)) => {
            let s = lit.to_string();
            s.trim_matches('"').to_string()
        }
        _ => {
            return quote! {
                ::core::compile_error!("__no_args!: expected string literal")
            };
        }
    };

    // Skip comma
    tokens.next();

    // Get token for span
    let span = match tokens.next() {
        Some(tt) => tt.span(),
        None => {
            return quote! {
                ::core::compile_error!("__no_args!: expected token for span")
            };
        }
    };

    let message = format!("{msg} does not accept arguments");

    quote_spanned! { span =>
        ::core::compile_error!(#message)
    }
}
