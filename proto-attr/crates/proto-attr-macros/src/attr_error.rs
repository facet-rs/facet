//! Implementation of `__attr_error!` proc-macro.

use proc_macro::TokenStream;
#[cfg(not(feature = "nightly"))]
use quote::quote_spanned;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Token, braced};

/// Input format:
/// ```ignore
/// @known_attrs { skip, rename, column }
/// @got_name { colum }
/// @got_rest { (...) }
/// ```
struct AttrErrorInput {
    known_attrs: Vec<Ident>,
    got_name: Ident,
    // got_rest is captured but not used for now
}

impl Parse for AttrErrorInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
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

        Ok(AttrErrorInput {
            known_attrs,
            got_name,
        })
    }
}

pub fn attr_error(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as AttrErrorInput);

    let got_name_str = input.got_name.to_string();
    let got_span = input.got_name.span();

    // Find best suggestion using strsim
    let mut best_suggestion: Option<(&Ident, f64)> = None;
    for known in &input.known_attrs {
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

    let known_list: Vec<_> = input.known_attrs.iter().map(|i| i.to_string()).collect();
    let known_str = known_list.join(", ");

    #[cfg(feature = "nightly")]
    {
        use proc_macro::{Diagnostic, Level};

        let error_msg = format!("unknown attribute `{}`", got_name_str);
        let mut diag = Diagnostic::spanned(got_span.unwrap(), Level::Error, error_msg);

        diag = diag.note(format!("expected {}", known_str));

        if let Some((suggestion, _)) = best_suggestion {
            diag = diag.help(format!("did you mean `{}`?", suggestion));
        }

        diag.emit();

        // Return a dummy valid value to satisfy type inference
        // The error is already shown; this just prevents cascading errors
        "proto_ext::Attr::Skip".parse().unwrap()
    }

    #[cfg(not(feature = "nightly"))]
    {
        let message = match best_suggestion {
            Some((suggestion, _)) => {
                format!(
                    "unknown attribute `{}`, did you mean `{}`?\navailable proto-ext attributes: {}",
                    got_name_str, suggestion, known_str
                )
            }
            None => {
                format!(
                    "unknown attribute `{}`\navailable proto-ext attributes: {}",
                    got_name_str, known_str
                )
            }
        };

        let expanded = quote_spanned! { got_span =>
            compile_error!(#message)
        };

        expanded.into()
    }
}
