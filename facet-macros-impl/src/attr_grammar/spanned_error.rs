//! Implementation of `__spanned_error!` proc-macro.
//!
//! A generic helper for emitting errors with precise spans from macro_rules.

use proc_macro2::TokenStream as TokenStream2;
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

/// Emits compile errors with precise source spans from macro_rules, preserving error location information.
pub fn spanned_error(input: TokenStream2) -> TokenStream2 {
    let mut iter = input.to_token_iter();

    let parsed: SpannedErrorInput = match iter.parse() {
        Ok(i) => i,
        Err(e) => {
            let msg = e.to_string();
            return quote::quote! { compile_error!(#msg); };
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

    quote_spanned! { span =>
        compile_error!(#message)
    }
}
