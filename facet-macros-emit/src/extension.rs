//! Code generation for extension attributes.

use facet_macros_parse::{TokenStream, TokenTree};
use quote::quote;

/// Emits the code for an `ExtensionAttr`.
///
/// This generates an inline `ExtensionAttr` struct literal with:
/// - `ns`: the namespace string
/// - `key`: the key string
/// - `args`: a static slice of `facet::Token`
/// - `get`: a function pointer that returns a no-op unit value (for marker attributes)
///
/// For simple marker attributes like `#[facet(kdl::child)]`, this is sufficient.
/// The attribute can be detected via `has_extension_attr("kdl", "child")` and
/// any arguments are available in the `args` field.
pub fn emit_extension_attr(ns: &str, key: &str, args: &TokenStream) -> TokenStream {
    // Convert the args TokenStream into a static slice of facet::Token
    let args_tokens = emit_token_trees(args);

    quote! {
        {
            // No-op getter for marker attributes - returns a reference to ()
            fn __ext_get() -> &'static (dyn ::core::any::Any + ::core::marker::Send + ::core::marker::Sync) {
                static __UNIT: () = ();
                &__UNIT
            }

            ::facet::ExtensionAttr {
                ns: #ns,
                key: #key,
                args: &[#args_tokens],
                get: __ext_get,
            }
        }
    }
}

/// Converts a TokenStream into code that constructs a static slice of `facet::Token`.
fn emit_token_trees(tokens: &TokenStream) -> TokenStream {
    let mut items = Vec::new();

    for tt in tokens.clone() {
        items.push(emit_token_tree(&tt));
    }

    quote! { #(#items),* }
}

/// Emits code to construct a single `facet::Token`.
fn emit_token_tree(tt: &facet_macros_parse::TokenTree) -> TokenStream {
    // We use DUMMY span for now since proc_macro2 spans aren't easily convertible
    // to our static TokenSpan type at macro time
    let span = quote! { ::facet::TokenSpan::DUMMY };

    match tt {
        TokenTree::Ident(ident) => {
            let s = ident.to_string();
            quote! {
                ::facet::Token::Ident {
                    name: #s,
                    span: #span,
                }
            }
        }
        TokenTree::Punct(punct) => {
            let c = punct.as_char();
            let joint = punct.spacing() == facet_macros_parse::Spacing::Joint;
            quote! {
                ::facet::Token::Punct {
                    ch: #c,
                    joint: #joint,
                    span: #span,
                }
            }
        }
        TokenTree::Literal(lit) => {
            // For literals, we need to preserve the exact representation and determine the kind
            let s = lit.to_string();
            let kind = if s.starts_with("b\"") {
                quote! { ::facet::LiteralKind::ByteString }
            } else if s.starts_with("b'") {
                quote! { ::facet::LiteralKind::Byte }
            } else if s.starts_with('"') || s.starts_with("r#") || s.starts_with("r\"") {
                quote! { ::facet::LiteralKind::String }
            } else if s.starts_with('\'') {
                quote! { ::facet::LiteralKind::Char }
            } else if s.contains('.') || s.contains('e') || s.contains('E') {
                // Could be float if it has decimal point or exponent
                // But check it's not a suffix like "123u32"
                if s.chars().any(|c| c == '.') || (s.contains('e') && !s.ends_with("usize")) {
                    quote! { ::facet::LiteralKind::Float }
                } else {
                    quote! { ::facet::LiteralKind::Integer }
                }
            } else {
                quote! { ::facet::LiteralKind::Integer }
            };

            quote! {
                ::facet::Token::Literal {
                    kind: #kind,
                    text: #s,
                    span: #span,
                }
            }
        }
        TokenTree::Group(group) => {
            let delimiter = match group.delimiter() {
                facet_macros_parse::Delimiter::Parenthesis => {
                    quote! { ::facet::Delimiter::Parenthesis }
                }
                facet_macros_parse::Delimiter::Brace => quote! { ::facet::Delimiter::Brace },
                facet_macros_parse::Delimiter::Bracket => quote! { ::facet::Delimiter::Bracket },
                facet_macros_parse::Delimiter::None => quote! { ::facet::Delimiter::None },
            };
            let inner = emit_token_trees(&group.stream());
            quote! {
                ::facet::Token::Group {
                    delimiter: #delimiter,
                    tokens: &[#inner],
                    span: #span,
                }
            }
        }
    }
}
