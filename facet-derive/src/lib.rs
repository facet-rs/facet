#![doc = include_str!("../README.md")]

#[proc_macro_derive(Facet, attributes(facet))]
pub fn facet_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_derive_emit::facet_derive(input.into()).into()
}
