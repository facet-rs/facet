//! Implementation of `__spanned_error!` proc-macro.
//!
//! A generic helper for emitting errors with precise spans from macro_rules.

use proc_macro::TokenStream;
#[cfg(not(feature = "nightly"))]
use quote::quote_spanned;
use syn::parse::{Parse, ParseStream};
use syn::{LitStr, Token};

/// Input format:
/// ```ignore
/// { tokens for span } => "error message"
/// ```
struct SpannedErrorInput {
    span_tokens: proc_macro2::TokenStream,
    message: LitStr,
}

impl Parse for SpannedErrorInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse the braced tokens for span
        let content;
        syn::braced!(content in input);
        let span_tokens: proc_macro2::TokenStream = content.parse()?;

        // Parse =>
        input.parse::<Token![=>]>()?;

        // Parse the message string
        let message: LitStr = input.parse()?;

        Ok(SpannedErrorInput {
            span_tokens,
            message,
        })
    }
}

pub fn spanned_error(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as SpannedErrorInput);

    let message = input.message.value();

    // Get span from the first token, or use call_site if empty
    let span = input
        .span_tokens
        .clone()
        .into_iter()
        .next()
        .map(|t| t.span())
        .unwrap_or_else(proc_macro2::Span::call_site);

    #[cfg(feature = "nightly")]
    {
        use proc_macro::{Diagnostic, Level};

        let diag = Diagnostic::spanned(span.unwrap(), Level::Error, &message);
        diag.emit();

        // Return a dummy valid value to satisfy type inference
        // The error is already shown; this just prevents cascading errors
        "proto_ext::Attr::Skip".parse().unwrap()
    }

    #[cfg(not(feature = "nightly"))]
    {
        let expanded = quote_spanned! { span =>
            compile_error!(#message)
        };
        expanded.into()
    }
}
