//! Implementation of `__spanned_error!` proc-macro.
//!
//! A generic helper for emitting errors with precise spans from macro_rules.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
#[cfg(not(feature = "nightly"))]
use quote::quote_spanned;
use unsynn::*;

operator! {
    Arrow = "=>";
}

unsynn! {
    /// Input format:
    /// ```ignore
    /// { tokens for span } => "error message"
    /// ```
    struct SpannedErrorInput {
        span_tokens: BraceGroup,
        _arrow: Arrow,
        message: LiteralString,
    }
}

pub fn spanned_error(input: TokenStream) -> TokenStream {
    let input2 = TokenStream2::from(input);
    let mut iter = input2.to_token_iter();

    let parsed: SpannedErrorInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote::quote! { compile_error!(#msg); }.into();
        }
    };

    let message = parsed.message.value();

    // Get span from the first token, or use call_site if empty
    let span = parsed
        .span_tokens
        .0
        .stream()
        .into_iter()
        .next()
        .map(|t| t.span())
        .unwrap_or_else(proc_macro2::Span::call_site);

    #[cfg(feature = "nightly")]
    {
        use proc_macro::{Diagnostic, Level};

        let diag = Diagnostic::spanned(span.unwrap(), Level::Error, message);
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
