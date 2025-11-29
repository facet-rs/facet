//! Implementation of `__dispatch_struct_field!` proc-macro.
//!
//! A generic dispatcher for struct field parsing that preserves spans.
//! Unlike `__dispatch_column_field`, this takes field metadata as parameters.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote_spanned;
use unsynn::*;

keyword! {
    KNamespace = "namespace";
    KStructName = "struct_name";
    KCallbackMacro = "callback_macro";
    KKnownFields = "known_fields";
    KSlots = "slots";
    KField = "field";
    KRest = "rest";
    KString = "string";
    KBool = "bool";
}

operator! {
    At = "@";
    Col = ":";
}

unsynn! {
    /// Input format:
    /// ```ignore
    /// @namespace { $crate }
    /// @struct_name { column }
    /// @callback_macro { __parse_column_fields }
    /// @known_fields { name: string, primary_key: bool }
    /// @slots { @name { None } @primary_key { false } }
    /// @field { name }
    /// @rest { = "value", primary_key }
    /// ```
    struct DispatchStructFieldInput {
        namespace_section: NamespaceSection,
        struct_name_section: StructNameSection,
        callback_macro_section: CallbackMacroSection,
        known_fields_section: KnownFieldsSection,
        slots_section: SlotsSection,
        field_section: FieldSection,
        rest_section: RestSection,
    }

    struct NamespaceSection {
        _at: At,
        _kw: KNamespace,
        content: BraceGroup,
    }

    struct StructNameSection {
        _at: At,
        _kw: KStructName,
        content: BraceGroupContaining<Ident>,
    }

    struct CallbackMacroSection {
        _at: At,
        _kw: KCallbackMacro,
        content: BraceGroupContaining<Ident>,
    }

    struct KnownFieldsSection {
        _at: At,
        _kw: KKnownFields,
        content: BraceGroupContaining<CommaDelimitedVec<FieldDef>>,
    }

    struct FieldDef {
        name: Ident,
        _colon: Col,
        kind: Ident,
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

#[derive(Clone, Copy, PartialEq)]
enum FieldKind {
    String,
    Bool,
}

pub fn dispatch_struct_field(input: TokenStream) -> TokenStream {
    let input2 = TokenStream2::from(input);
    let mut iter = input2.to_token_iter();

    let parsed: DispatchStructFieldInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote::quote! { compile_error!(#msg); }.into();
        }
    };

    let namespace = parsed.namespace_section.content.0.stream();
    let struct_name = &parsed.struct_name_section.content.content;
    let callback_macro = &parsed.callback_macro_section.content.content;
    let field = &parsed.field_section.content.content;
    let field_str = field.to_string();
    let field_span = field.span();
    let slots = parsed.slots_section.content.0.stream();
    let rest = parsed.rest_section.content.0.stream();

    // Parse known_fields
    let known_fields: Vec<(Ident, FieldKind)> = parsed
        .known_fields_section
        .content
        .content
        .iter()
        .filter_map(|d| {
            let name = d.value.name.clone();
            let kind_str = d.value.kind.to_string();
            let kind = match kind_str.as_str() {
                "string" => FieldKind::String,
                "bool" => FieldKind::Bool,
                _ => return None,
            };
            Some((name, kind))
        })
        .collect();

    // Parse rest to check what follows the field name
    let rest_tokens: Vec<proc_macro2::TokenTree> = rest.clone().into_iter().collect();

    // Find if this field is known
    if let Some((_, field_kind)) = known_fields
        .iter()
        .find(|(name, _)| name.to_string() == field_str)
    {
        match field_kind {
            FieldKind::String => {
                // Check if next token is `=`
                if let Some(proc_macro2::TokenTree::Punct(p)) = rest_tokens.first() {
                    if p.as_char() == '=' {
                        // field = something
                        // Check if there's a literal after =
                        if let Some(proc_macro2::TokenTree::Literal(lit)) = rest_tokens.get(1) {
                            let lit_token = lit.clone();
                            let remaining: TokenStream2 = rest_tokens.into_iter().skip(2).collect();
                            let expanded = quote_spanned! { field_span =>
                                #namespace::#callback_macro!{
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
                // field without proper = literal
                let remaining: TokenStream2 = if !rest_tokens.is_empty()
                    && matches!(rest_tokens.first(), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == ',')
                {
                    rest_tokens.into_iter().skip(1).collect()
                } else {
                    rest.clone()
                };
                let expanded = quote_spanned! { field_span =>
                    #namespace::#callback_macro!{
                        @field #field
                        @slots { #slots }
                        @error_missing_value
                        @rest { #remaining }
                    }
                };
                expanded.into()
            }
            FieldKind::Bool => {
                // Check if next token is `=`
                if let Some(proc_macro2::TokenTree::Punct(p)) = rest_tokens.first() {
                    if p.as_char() == '=' {
                        // field = true/false
                        if let Some(proc_macro2::TokenTree::Ident(ident)) = rest_tokens.get(1) {
                            let ident_str = ident.to_string();
                            if ident_str == "true" || ident_str == "false" {
                                let remaining: TokenStream2 =
                                    rest_tokens.into_iter().skip(2).collect();
                                if ident_str == "true" {
                                    let expanded = quote_spanned! { field_span =>
                                        #namespace::#callback_macro!{
                                            @field #field
                                            @slots { #slots }
                                            @assign_bool true
                                            @rest { #remaining }
                                        }
                                    };
                                    return expanded.into();
                                } else {
                                    let expanded = quote_spanned! { field_span =>
                                        #namespace::#callback_macro!{
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
                // bool field as flag (no value)
                let remaining: TokenStream2 = if !rest_tokens.is_empty()
                    && matches!(rest_tokens.first(), Some(proc_macro2::TokenTree::Punct(p)) if p.as_char() == ',')
                {
                    rest_tokens.into_iter().skip(1).collect()
                } else {
                    rest.clone()
                };
                let expanded = quote_spanned! { field_span =>
                    #namespace::#callback_macro!{
                        @field #field
                        @slots { #slots }
                        @flag
                        @rest { #remaining }
                    }
                };
                expanded.into()
            }
        }
    } else {
        // Unknown field - generate error
        let known_names: Vec<_> = known_fields.iter().map(|(n, _)| n).collect();
        let expanded = quote_spanned! { field_span =>
            #namespace::__field_error_bridge!{
                @struct_name { #struct_name }
                @known_fields { #(#known_names),* }
                @got_name { #field }
                @got_rest { #rest }
            }
        };
        expanded.into()
    }
}
