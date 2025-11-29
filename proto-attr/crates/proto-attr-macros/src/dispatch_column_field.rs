//! Implementation of `__dispatch_column_field!` proc-macro.
//!
//! Dispatches column field parsing while preserving spans.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote_spanned;
use unsynn::*;

keyword! {
    KNamespace = "namespace";
    KSlots = "slots";
    KField = "field";
    KRest = "rest";
}

operator! {
    At = "@";
}

unsynn! {
    /// Input format:
    /// ```ignore
    /// @namespace { proto_ext }
    /// @slots { @name { None } @primary_key { false } }
    /// @field { name }
    /// @rest { = "value", primary_key }
    /// ```
    struct DispatchColumnFieldInput {
        namespace_section: NamespaceSection,
        slots_section: SlotsSection,
        field_section: FieldSection,
        rest_section: RestSection,
    }

    struct NamespaceSection {
        _at: At,
        _kw: KNamespace,
        content: BraceGroupContaining<Ident>,
    }

    struct SlotsSection {
        _at: At,
        _kw: KSlots,
        content: BraceGroup,
    }

    struct FieldSection {
        _at: At,
        _kw: KField,
        content: BraceGroupContaining<Ident>,
    }

    struct RestSection {
        _at: At,
        _kw: KRest,
        content: BraceGroup,
    }
}

pub fn dispatch_column_field(input: TokenStream) -> TokenStream {
    let input2 = TokenStream2::from(input);
    let mut iter = input2.to_token_iter();

    let parsed: DispatchColumnFieldInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote::quote! { compile_error!(#msg); }.into();
        }
    };

    let namespace = &parsed.namespace_section.content.content;
    let field = &parsed.field_section.content.content;
    let field_str = field.to_string();
    let field_span = field.span();
    let slots = parsed.slots_section.content.0.stream();
    let rest = parsed.rest_section.content.0.stream();

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
