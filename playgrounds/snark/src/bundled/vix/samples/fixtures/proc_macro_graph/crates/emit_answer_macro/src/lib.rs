extern crate proc_macro;

use proc_macro::TokenStream;

#[proc_macro_derive(EmitAnswer)]
pub fn emit_answer_derive(_input: TokenStream) -> TokenStream {
    r#"impl MacroAnswer {
        pub const PROC_MACRO_MESSAGE: &'static str = "proc macro says hello";
    }"#
    .parse()
    .unwrap()
}
