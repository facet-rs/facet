#![doc = include_str!("../README.md")]

#[proc_macro_derive(Facet, attributes(facet))]
pub fn facet_macros(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::facet_macros(input.into()).into()
}

/// Internal proc macro for extension attribute resolution.
///
/// This is called by the `Facet` derive macro to forward extension attributes
/// to their respective crate's dispatcher macro while preserving spans for
/// better error messages.
#[doc(hidden)]
#[proc_macro]
pub fn __ext(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::ext_attr(input.into()).into()
}

// ============================================================================
// ATTRIBUTE GRAMMAR PROC-MACROS
// ============================================================================

/// Internal proc macro for compiling attribute grammars.
///
/// This is called by `define_attr_grammar!` to generate type definitions
/// and dispatcher macros from a grammar DSL.
#[doc(hidden)]
#[proc_macro]
pub fn __make_parse_attr(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::attr_grammar::make_parse_attr(input.into()).into()
}

/// Internal proc macro for unified attribute dispatch.
///
/// Routes parsed attribute names to the appropriate variant handlers
/// (unit, newtype, or struct).
#[doc(hidden)]
#[proc_macro]
pub fn __dispatch_attr(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::attr_grammar::dispatch_attr(input.into()).into()
}

/// Internal proc macro for building struct field values.
///
/// Parses struct field assignments with type validation and helpful
/// error messages.
#[doc(hidden)]
#[proc_macro]
pub fn __build_struct_fields(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::attr_grammar::build_struct_fields(input.into()).into()
}

/// Internal proc macro for attribute error messages.
///
/// Generates compile_error! with typo suggestions for unknown attributes.
#[doc(hidden)]
#[proc_macro]
pub fn __attr_error(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::attr_grammar::attr_error(input.into()).into()
}

/// Internal proc macro for field error messages.
///
/// Generates compile_error! with typo suggestions for unknown fields.
#[doc(hidden)]
#[proc_macro]
pub fn __field_error(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::attr_grammar::field_error(input.into()).into()
}

/// Internal proc macro for spanned error messages.
///
/// A generic helper for emitting errors with precise spans.
#[doc(hidden)]
#[proc_macro]
pub fn __spanned_error(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::attr_grammar::spanned_error(input.into()).into()
}

#[cfg(feature = "function")]
#[proc_macro_attribute]
pub fn facet_fn(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    facet_macros_impl::function::facet_fn(attr.into(), item.into()).into()
}

#[cfg(feature = "function")]
#[proc_macro]
pub fn fn_shape(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::function::fn_shape(input.into()).into()
}

/// Internal proc macro for unknown extension attribute errors.
///
/// This generates a compile_error! with the span pointing to the unknown identifier.
#[doc(hidden)]
#[proc_macro]
pub fn __unknown_attr(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::unknown_attr(input.into()).into()
}

// ============================================================================
// PLUGIN SYSTEM PROC-MACROS
// ============================================================================

/// Internal proc macro for plugin chain finalization.
///
/// This is called at the end of a plugin chain to:
/// 1. Parse the type definition ONCE
/// 2. Generate the base Facet impl
/// 3. Call each registered plugin's code generator
///
/// Input format:
/// ```ignore
/// __facet_finalize! {
///     @tokens { struct Foo { ... } }
///     @plugins { "Error", }
///     @facet_crate { ::facet }
/// }
/// ```
#[doc(hidden)]
#[proc_macro]
pub fn __facet_finalize(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::facet_finalize(input.into()).into()
}

/// Internal proc macro for "does not accept arguments" errors.
///
/// Input: `"ns::attr", token`
/// Generates compile_error! with span on the token.
#[doc(hidden)]
#[proc_macro]
pub fn __no_args(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::no_args(input.into()).into()
}

/// Attribute macro that runs cleanup code when a method returns an error.
///
/// # Usage
///
/// ```ignore
/// #[on_error(self.cleanup())]
/// pub fn do_something(&mut self) -> Result<(), Error> {
///     self.inner_work()?;
///     Ok(())
/// }
/// ```
///
/// For methods returning `Result<&mut Self, E>`, the macro properly handles
/// the borrow by discarding the returned reference and returning a fresh `Ok(self)`:
///
/// ```ignore
/// #[on_error(self.poison_and_cleanup())]
/// pub fn begin_some(&mut self) -> Result<&mut Self, ReflectError> {
///     self.require_active()?;
///     // ... implementation
///     Ok(self)
/// }
/// ```
#[proc_macro_attribute]
pub fn on_error(
    attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    facet_macros_impl::on_error(attr.into(), item.into()).into()
}
