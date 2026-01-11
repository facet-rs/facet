//! Re-exports from roam-macros-parse with additional proc-macro specific utilities.

pub use roam_macros_parse::*;

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote_spanned;

/// Error type for validation/codegen errors in proc macros.
#[derive(Debug, Clone)]
pub struct Error {
    pub span: Span,
    pub message: String,
}

impl Error {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    pub fn to_compile_error(&self) -> TokenStream2 {
        let msg = &self.message;
        let span = self.span;
        quote_spanned! {span=> compile_error!(#msg); }
    }
}

impl From<ParseError> for Error {
    fn from(err: ParseError) -> Self {
        Self::new(Span::call_site(), err.to_string())
    }
}

/// Parse a trait definition from a token stream, returning a proc-macro friendly error.
pub fn parse(tokens: &TokenStream2) -> Result<ServiceTrait, Error> {
    parse_trait(tokens).map_err(Error::from)
}
