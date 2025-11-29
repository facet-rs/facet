//! Implementation of `__field_error!` proc-macro.

use proc_macro::TokenStream;
use quote::quote_spanned;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Token, braced};

/// Input format:
/// ```ignore
/// @struct_name { Column }
/// @known_fields { name, primary_key }
/// @got_name { nam }
/// @got_rest { = "..." }
/// ```
struct FieldErrorInput {
    struct_name: Ident,
    known_fields: Vec<Ident>,
    got_name: Ident,
    // got_rest is captured but not used for now
}

impl Parse for FieldErrorInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // @struct_name { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "struct_name" {
            return Err(syn::Error::new(label.span(), "expected `struct_name`"));
        }
        let struct_name_content;
        braced!(struct_name_content in input);
        let struct_name: Ident = struct_name_content.parse()?;

        // @known_fields { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "known_fields" {
            return Err(syn::Error::new(label.span(), "expected `known_fields`"));
        }
        let known_content;
        braced!(known_content in input);
        let known_fields = known_content
            .parse_terminated(Ident::parse, Token![,])?
            .into_iter()
            .collect();

        // @got_name { ... }
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "got_name" {
            return Err(syn::Error::new(label.span(), "expected `got_name`"));
        }
        let got_name_content;
        braced!(got_name_content in input);
        let got_name: Ident = got_name_content.parse()?;

        // @got_rest { ... } - consume but ignore
        input.parse::<Token![@]>()?;
        let label: Ident = input.parse()?;
        if label != "got_rest" {
            return Err(syn::Error::new(label.span(), "expected `got_rest`"));
        }
        let _got_rest_content;
        braced!(_got_rest_content in input);
        // Consume all tokens in got_rest
        let _: proc_macro2::TokenStream = _got_rest_content.parse()?;

        Ok(FieldErrorInput {
            struct_name,
            known_fields,
            got_name,
        })
    }
}

pub fn field_error(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as FieldErrorInput);

    let struct_name_str = input.struct_name.to_string();
    let got_name_str = input.got_name.to_string();
    let got_span = input.got_name.span();

    // Find best suggestion using strsim
    let mut best_suggestion: Option<(&Ident, f64)> = None;
    for known in &input.known_fields {
        let score = strsim::jaro_winkler(&got_name_str, &known.to_string());
        if score > 0.7 {
            match &best_suggestion {
                None => best_suggestion = Some((known, score)),
                Some((_, best_score)) if score > *best_score => {
                    best_suggestion = Some((known, score))
                }
                _ => {}
            }
        }
    }

    let known_list: Vec<_> = input.known_fields.iter().map(|i| i.to_string()).collect();
    let known_str = known_list.join(", ");

    let message = match best_suggestion {
        Some((suggestion, _)) => {
            format!(
                "unknown field `{}` in `{}`, did you mean `{}`?\n\navailable fields: {}",
                got_name_str, struct_name_str, suggestion, known_str
            )
        }
        None => {
            format!(
                "unknown field `{}` in `{}`\n\navailable fields: {}",
                got_name_str, struct_name_str, known_str
            )
        }
    };

    let expanded = quote_spanned! { got_span =>
        compile_error!(#message)
    };

    expanded.into()
}
