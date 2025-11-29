//! Implementation of `__make_parse_attr!` proc-macro.

use proc_macro::TokenStream;

pub fn make_parse_attr(_input: TokenStream) -> TokenStream {
    // TODO: Phase 3-4 implementation
    // For now, we'll hand-write the output in proto-ext
    TokenStream::new()
}
