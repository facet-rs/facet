//! Implementation of `#[derive(Faket)]` proc-macro.
//!
//! This processes `#[faket(...)]` attributes and dispatches them to the
//! appropriate `__parse_attr!` macros based on namespace.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Attribute, Data, DeriveInput, Fields, Ident, Path, Token, parse_macro_input};

/// Parse a faket attribute's content.
///
/// Formats:
/// - `skip` → unprefixed, calls `proto_attr::__parse_attr!(skip)`
/// - `rename = "foo"` → unprefixed
/// - `ns::skip` → namespaced, calls `ns::__parse_attr!(skip)`
/// - `ns::column(name = "id")` → namespaced with args
struct FaketAttrContent {
    /// The namespace path (e.g., `proto_ext`), if any
    namespace: Option<Path>,
    /// The attribute name (e.g., `skip`, `column`)
    attr_name: Ident,
    /// Everything after the attr name (e.g., `(name = "id")` or `= "foo"`)
    rest: TokenStream2,
}

impl Parse for FaketAttrContent {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Try to parse as a path first (could be `ns::attr` or just `attr`)
        let first_ident: Ident = input.parse()?;

        if input.peek(Token![::]) {
            // Namespaced: ns::attr...
            input.parse::<Token![::]>()?;
            let attr_name: Ident = input.parse()?;
            let rest: TokenStream2 = input.parse()?;

            // Build the namespace path from just the first ident
            let namespace = syn::parse_quote!(#first_ident);

            Ok(FaketAttrContent {
                namespace: Some(namespace),
                attr_name,
                rest,
            })
        } else {
            // Unprefixed: attr...
            let rest: TokenStream2 = input.parse()?;

            Ok(FaketAttrContent {
                namespace: None,
                attr_name: first_ident,
                rest,
            })
        }
    }
}

/// Extract faket attributes from a list of attributes
fn extract_faket_attrs(attrs: &[Attribute]) -> Vec<&Attribute> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("faket"))
        .collect()
}

/// Generate the __parse_attr! call for a single attribute
fn generate_parse_call(attr: &Attribute) -> syn::Result<TokenStream2> {
    let content: FaketAttrContent = attr.parse_args()?;
    let attr_name = &content.attr_name;
    let rest = &content.rest;
    let span = attr.span();

    match &content.namespace {
        Some(ns) => {
            // Namespaced: call ns::__parse_attr!(attr_name rest)
            Ok(quote_spanned! { span =>
                #ns::__parse_attr!(#attr_name #rest)
            })
        }
        None => {
            // Unprefixed: for now, just error - in real facet this would call facet's parser
            Ok(quote_spanned! { span =>
                compile_error!("unprefixed attributes not yet supported in prototype; use `ns::attr` syntax")
            })
        }
    }
}

/// Process a struct and generate the Faket impl
fn process_struct(name: &Ident, attrs: &[Attribute], fields: &Fields) -> syn::Result<TokenStream2> {
    // Collect struct-level attributes
    let struct_attrs = extract_faket_attrs(attrs);
    let struct_attr_calls: Vec<TokenStream2> = struct_attrs
        .iter()
        .map(|a| generate_parse_call(a))
        .collect::<syn::Result<_>>()?;

    // Collect field-level attributes
    let mut field_attr_sections = Vec::new();

    match fields {
        Fields::Named(named) => {
            for field in &named.named {
                let field_name = field.ident.as_ref().unwrap();
                let field_name_str = field_name.to_string();
                let field_attrs = extract_faket_attrs(&field.attrs);
                let field_attr_calls: Vec<TokenStream2> = field_attrs
                    .iter()
                    .map(|a| generate_parse_call(a))
                    .collect::<syn::Result<_>>()?;

                if !field_attr_calls.is_empty() {
                    field_attr_sections.push(quote! {
                        (#field_name_str, &[#(#field_attr_calls),*])
                    });
                }
            }
        }
        Fields::Unnamed(unnamed) => {
            for (idx, field) in unnamed.unnamed.iter().enumerate() {
                let field_attrs = extract_faket_attrs(&field.attrs);
                let field_attr_calls: Vec<TokenStream2> = field_attrs
                    .iter()
                    .map(|a| generate_parse_call(a))
                    .collect::<syn::Result<_>>()?;

                if !field_attr_calls.is_empty() {
                    field_attr_sections.push(quote! {
                        (#idx, &[#(#field_attr_calls),*])
                    });
                }
            }
        }
        Fields::Unit => {}
    }

    // Generate a simple trait impl that holds the parsed attributes
    // For the prototype, we just generate code that exercises the parsing
    Ok(quote! {
        impl #name {
            /// Returns the parsed struct-level attributes (prototype)
            #[allow(dead_code)]
            pub const STRUCT_ATTRS: &'static [proto_ext::Attr] = &[
                #(#struct_attr_calls),*
            ];
        }

        // Force evaluation of field attributes at compile time
        const _: () = {
            #(let _ = #field_attr_sections;)*
        };
    })
}

pub fn derive_faket(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let expanded = match &input.data {
        Data::Struct(data) => process_struct(name, &input.attrs, &data.fields),
        Data::Enum(_) => Err(syn::Error::new_spanned(
            &input,
            "Faket derive not yet implemented for enums",
        )),
        Data::Union(_) => Err(syn::Error::new_spanned(
            &input,
            "Faket derive not supported for unions",
        )),
    };

    match expanded {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
