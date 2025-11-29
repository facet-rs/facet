//! Implementation of `__dispatch_attr!` proc-macro.
//!
//! A dispatcher that can inspect token values while preserving spans.
//! This overcomes the macro_rules limitation where pattern matching
//! doesn't capture the matched token.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote_spanned;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Token, braced};

/// Input format:
/// ```ignore
/// @namespace { proto_ext }
/// @known_attrs { skip, rename, column }
/// @name { rename }
/// @rest { ("value") }
/// ```
struct DispatchAttrInput {
    namespace: Ident,
    known_attrs: Vec<Ident>,
    attr_name: Ident,
    rest: TokenStream2,
}

impl Parse for DispatchAttrInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // @namespace { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "namespace" {
            return Err(syn::Error::new(label.span(), "expected `namespace`"));
        }
        let ns_content;
        braced!(ns_content in input);
        let namespace: Ident = ns_content.parse()?;

        // @known_attrs { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "known_attrs" {
            return Err(syn::Error::new(label.span(), "expected `known_attrs`"));
        }
        let known_content;
        braced!(known_content in input);
        let known_attrs = known_content
            .parse_terminated(Ident::parse, Token![,])?
            .into_iter()
            .collect();

        // @name { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "name" {
            return Err(syn::Error::new(label.span(), "expected `name`"));
        }
        let name_content;
        braced!(name_content in input);
        let attr_name: Ident = name_content.parse()?;

        // @rest { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "rest" {
            return Err(syn::Error::new(label.span(), "expected `rest`"));
        }
        let rest_content;
        braced!(rest_content in input);
        let rest: TokenStream2 = rest_content.parse()?;

        Ok(DispatchAttrInput {
            namespace,
            known_attrs,
            attr_name,
            rest,
        })
    }
}

pub fn dispatch_attr(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DispatchAttrInput);

    let namespace = &input.namespace;
    let attr_name = &input.attr_name;
    let attr_name_str = attr_name.to_string();
    let attr_span = attr_name.span();
    let rest = &input.rest;

    // Check if attr_name matches any known attr
    for known in &input.known_attrs {
        if attr_name_str == known.to_string() {
            // Generate call to the specific parser macro
            // e.g., proto_ext::__parse_skip!{ @name skip @rest ... }
            let parser_name = Ident::new(&format!("__parse_{}", attr_name_str), attr_span);

            let expanded = quote_spanned! { attr_span =>
                #namespace::#parser_name!{ @name #attr_name @rest #rest }
            };
            return expanded.into();
        }
    }

    // Unknown attribute - generate error call with the original span
    let known_list: Vec<_> = input.known_attrs.iter().collect();

    let expanded = quote_spanned! { attr_span =>
        #namespace::__attr_error_bridge!{
            @known_attrs { #(#known_list),* }
            @got_name { #attr_name }
            @got_rest { #rest }
        }
    };

    expanded.into()
}
