//! Code generation for extension attributes.

use facet_macros_parse::{TokenStream, TokenTree};
use quote::{format_ident, quote};

/// Emits the code for an `ExtensionAttr`.
///
/// This generates code that:
/// 1. Validates the attribute exists using the shadowing trick with `#[diagnostic::on_unimplemented]`
/// 2. Calls the attribute function from the extension crate with the tokens
/// 3. Returns an `ExtensionAttr` struct
///
/// For `#[facet(kdl::child)]`, this generates:
/// ```ignore
/// {
///     struct child { _private: () }  // Fallback with user's span
///     {
///         use kdl::attrs::*;  // Shadows fallback if valid
///         __check_attr::<child>();  // Triggers on_unimplemented if invalid
///     }
///     fn __ext_get() -> ::facet::AnyStaticRef {
///         kdl::attrs::child(&[])
///     }
///     ::facet::ExtensionAttr { ns: "kdl", key: "child", get: __ext_get }
/// }
/// ```
pub fn emit_extension_attr(ns: &str, key: &str, args: &TokenStream) -> TokenStream {
    // Convert the args TokenStream into a static slice of facet::Token
    let args_tokens = emit_token_trees(args);

    // Create identifiers for the namespace and key
    let ns_ident = format_ident!("{}", ns);
    let key_ident = format_ident!("{}", key);

    quote! {
        {
            // Fallback struct - if the attribute doesn't exist in the namespace,
            // this won't be shadowed and the trait bound check will fail with
            // a nice error message from #[diagnostic::on_unimplemented]
            #[allow(non_camel_case_types)]
            struct #key_ident { _private: () }

            {
                // Glob import from the attrs module - this shadows the fallback
                // if a valid attribute with this name exists
                #[allow(unused_imports)]
                use #ns_ident::attrs::*;

                // This triggers the trait bound check - if the attribute doesn't
                // exist, ValidAttr<key> won't implement IsValidAttr and we get
                // the nice on_unimplemented error
                __check_attr::<#key_ident>();
            }

            // Getter that calls the attribute function with the tokens
            // Note: Extension functions handle their own caching via Box::leak
            fn __ext_get() -> ::facet::AnyStaticRef {
                #ns_ident::attrs::#key_ident(&[#args_tokens])
            }

            ::facet::ExtensionAttr {
                ns: #ns,
                key: #key,
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
