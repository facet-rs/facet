#![doc = include_str!("../README.md")]

#[proc_macro_derive(Facet, attributes(facet))]
pub fn facet_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_derive_emit::facet_derive(input.into()).into()
}

#[cfg(feature = "function")]
#[proc_macro_attribute]
pub fn facet_fn(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    facet_derive_emit::function::facet_fn(attr.into(), item.into()).into()
}

#[cfg(feature = "function")]
#[proc_macro]
pub fn fn_shape(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_derive_emit::function::fn_shape(input.into()).into()
}
