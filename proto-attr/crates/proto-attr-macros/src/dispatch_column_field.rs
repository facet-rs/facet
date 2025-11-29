//! Implementation of `__dispatch_column_field!` proc-macro.
//!
//! Dispatches column field parsing while preserving spans.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote_spanned;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Token, braced};

/// Input format:
/// ```ignore
/// @namespace { proto_ext }
/// @slots { @name { None } @primary_key { false } }
/// @field { name }
/// @rest { = "value", primary_key }
/// ```
struct DispatchColumnFieldInput {
    namespace: Ident,
    slots: TokenStream2,
    field: Ident,
    rest: TokenStream2,
}

impl Parse for DispatchColumnFieldInput {
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

        // @slots { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "slots" {
            return Err(syn::Error::new(label.span(), "expected `slots`"));
        }
        let slots_content;
        braced!(slots_content in input);
        let slots: TokenStream2 = slots_content.parse()?;

        // @field { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "field" {
            return Err(syn::Error::new(label.span(), "expected `field`"));
        }
        let field_content;
        braced!(field_content in input);
        let field: Ident = field_content.parse()?;

        // @rest { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "rest" {
            return Err(syn::Error::new(label.span(), "expected `rest`"));
        }
        let rest_content;
        braced!(rest_content in input);
        let rest: TokenStream2 = rest_content.parse()?;

        Ok(DispatchColumnFieldInput {
            namespace,
            slots,
            field,
            rest,
        })
    }
}

pub fn dispatch_column_field(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DispatchColumnFieldInput);

    let namespace = &input.namespace;
    let field = &input.field;
    let field_str = field.to_string();
    let field_span = field.span();
    let slots = &input.slots;
    let rest = &input.rest;

    // Parse rest to check what follows the field name
    let rest_tokens: Vec<proc_macro2::TokenTree> = rest.clone().into_iter().collect();

    match field_str.as_str() {
        "name" => {
            // Check if next token is `=`
            if let Some(proc_macro2::TokenTree::Punct(p)) = rest_tokens.first() {
                if p.as_char() == '=' {
                    // name = something
                    // Check if there's a literal after =
                    if let Some(proc_macro2::TokenTree::Literal(lit)) = rest_tokens.get(1) {
                        let lit_token = lit.clone();
                        let remaining: TokenStream2 = rest_tokens.into_iter().skip(2).collect();
                        let expanded = quote_spanned! { field_span =>
                            #namespace::__parse_column_fields!{
                                @field #field
                                @slots { #slots }
                                @assign #lit_token
                                @rest { #remaining }
                            }
                        };
                        return expanded.into();
                    }
                }
            }
            // name without proper = literal
            let remaining: TokenStream2 = if !rest_tokens.is_empty()
                && matches!(rest_tokens.first(), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == ',')
            {
                rest_tokens.into_iter().skip(1).collect()
            } else {
                rest.clone()
            };
            let expanded = quote_spanned! { field_span =>
                #namespace::__parse_column_fields!{
                    @field #field
                    @slots { #slots }
                    @error_missing_value
                    @rest { #remaining }
                }
            };
            expanded.into()
        }
        "primary_key" => {
            // Check if next token is `=`
            if let Some(proc_macro2::TokenTree::Punct(p)) = rest_tokens.first() {
                if p.as_char() == '=' {
                    // primary_key = true/false
                    if let Some(proc_macro2::TokenTree::Ident(ident)) = rest_tokens.get(1) {
                        let ident_str = ident.to_string();
                        if ident_str == "true" || ident_str == "false" {
                            let remaining: TokenStream2 = rest_tokens.into_iter().skip(2).collect();
                            if ident_str == "true" {
                                let expanded = quote_spanned! { field_span =>
                                    #namespace::__parse_column_fields!{
                                        @field #field
                                        @slots { #slots }
                                        @assign_bool true
                                        @rest { #remaining }
                                    }
                                };
                                return expanded.into();
                            } else {
                                let expanded = quote_spanned! { field_span =>
                                    #namespace::__parse_column_fields!{
                                        @field #field
                                        @slots { #slots }
                                        @assign_bool false
                                        @rest { #remaining }
                                    }
                                };
                                return expanded.into();
                            }
                        }
                    }
                }
            }
            // primary_key as flag (no value)
            let expanded = quote_spanned! { field_span =>
                #namespace::__parse_column_fields!{
                    @field #field
                    @slots { #slots }
                    @flag
                    @rest { #rest }
                }
            };
            expanded.into()
        }
        _ => {
            // Unknown field - generate error
            let expanded = quote_spanned! { field_span =>
                #namespace::__field_error_bridge!{
                    @struct_name { column }
                    @known_fields { name, primary_key }
                    @got_name { #field }
                    @got_rest { #rest }
                }
            };
            expanded.into()
        }
    }
}
