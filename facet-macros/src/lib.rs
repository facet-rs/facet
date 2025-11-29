#![doc = include_str!("../README.md")]

#[proc_macro_derive(Facet, attributes(facet))]
pub fn facet_macros(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_emit::facet_macros(input.into()).into()
}

/// Internal proc macro for extension attribute resolution.
///
/// This is called by the `Facet` derive macro to forward extension attributes
/// to their respective crate's dispatcher macro while preserving spans for
/// better error messages.
#[doc(hidden)]
#[proc_macro]
pub fn __ext(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_emit::ext_attr(input.into()).into()
}

#[cfg(feature = "function")]
#[proc_macro_attribute]
pub fn facet_fn(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    facet_macros_emit::function::facet_fn(attr.into(), item.into()).into()
}

#[cfg(feature = "function")]
#[proc_macro]
pub fn fn_shape(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_emit::function::fn_shape(input.into()).into()
}

/// Internal proc macro for unknown extension attribute errors.
///
/// This generates a compile_error! with the span pointing to the unknown identifier.
#[doc(hidden)]
#[proc_macro]
pub fn __unknown_attr(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_emit::unknown_attr(input.into()).into()
}
