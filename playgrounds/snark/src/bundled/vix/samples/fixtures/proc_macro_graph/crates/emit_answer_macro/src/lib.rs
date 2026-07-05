extern crate proc_macro;

use proc_macro::TokenStream;

#[proc_macro]
pub fn emit_answer(_input: TokenStream) -> TokenStream {
    "\"proc macro says hello\"".parse().unwrap()
}
