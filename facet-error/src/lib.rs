//! # facet-error
//!
//! A `thiserror` replacement powered by facet reflection.
//!
//! ## Usage
//!
//! ```ignore
//! use facet::Facet;
//! use facet_error as error;
//!
//! #[derive(Facet, Debug)]
//! #[facet(derive(Error))]
//! pub enum MyError {
//!     /// data store disconnected
//!     #[facet(error::from)]
//!     Disconnect(std::io::Error),
//!
//!     /// invalid header (expected {expected}, found {found})
//!     InvalidHeader { expected: String, found: String },
//!
//!     /// unknown error
//!     Unknown,
//! }
//! ```
//!
//! This generates:
//! - `impl Display for MyError` using doc comments as format strings
//! - `impl Error for MyError` with proper `source()` implementation
//! - `impl From<std::io::Error> for MyError` for variants with `#[facet(error::from)]`

// ============================================================================
// ATTRIBUTE GRAMMAR
// ============================================================================

// Error extension attributes for use with #[facet(error::attr)] syntax.
//
// After importing `use facet_error as error;`, users can write:
//   #[facet(error::from)]
//   #[facet(error::source)]

facet::define_attr_grammar! {
    ns "error";
    crate_path ::facet_error;

    /// Error attribute types for field configuration.
    pub enum Attr {
        /// Marks a field as the error source and generates a `From` impl.
        ///
        /// Usage: `#[facet(error::from)]`
        ///
        /// This attribute:
        /// - Marks the field as the source in `Error::source()`
        /// - Generates a `From<FieldType> for ErrorType` implementation
        From,

        /// Marks a field as the error source without generating a `From` impl.
        ///
        /// Usage: `#[facet(error::source)]`
        ///
        /// This attribute only marks the field as the source in `Error::source()`.
        Source,
    }
}

// ============================================================================
// PLUGIN TEMPLATE
// ============================================================================

/// Plugin chain entry point.
///
/// Called by `#[derive(Facet)]` when `#[facet(derive(Error))]` is present.
/// Adds the Error plugin template to the chain and forwards to the next plugin or finalize.
#[macro_export]
macro_rules! __facet_invoke {
    (
        @tokens { $($tokens:tt)* }
        @remaining { $($remaining:tt)* }
        @plugins { $($plugins:tt)* }
        @facet_crate { $($facet_crate:tt)* }
    ) => {
        // Forward with our template added to plugins
        $crate::__facet_invoke_internal! {
            @tokens { $($tokens)* }
            @remaining { $($remaining)* }
            @plugins {
                $($plugins)*
                @plugin {
                    @name { "Error" }
                    @template {
                        // Template using @ directives for code generation
                        // This will be evaluated by __facet_finalize!

                        // Display impl - use doc comments as format strings
                        impl ::core::fmt::Display for @Self {
                            #[allow(unused_variables)] // not all fields used in format strings
                            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                                match self {
                                    @for_variant {
                                        Self::@variant_name @variant_pattern => {
                                            write!(f, @format_doc_comment)
                                        }
                                    }
                                }
                            }
                        }

                        // Error impl with source() support
                        impl ::std::error::Error for @Self {
                            fn source(&self) -> Option<&(dyn ::std::error::Error + 'static)> {
                                match self {
                                    @for_variant {
                                        @if_field_attr(error::source) {
                                            Self::@variant_name @variant_pattern_only(error::source) => {
                                                Some(@field_expr)
                                            }
                                        }
                                        @if_field_attr(error::from) {
                                            Self::@variant_name @variant_pattern_only(error::from) => {
                                                Some(@field_expr)
                                            }
                                        }
                                    }
                                    _ => None,
                                }
                            }
                        }

                        // From impls for #[facet(error::from)] fields
                        @for_variant {
                            @if_any_field_attr(error::from) {
                                @for_field {
                                    @if_attr(error::from) {
                                        impl ::core::convert::From<@field_type> for @Self {
                                            fn from(source: @field_type) -> Self {
                                                @if_struct_variant {
                                                    Self::@variant_name {
                                                        @field_name: source,
                                                        @for_field {
                                                            @if_attr(default::value) {
                                                                @field_name: (@attr_args).into(),
                                                            }
                                                        }
                                                    }
                                                }
                                                @if_tuple_variant {
                                                    Self::@variant_name(source)
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            @facet_crate { $($facet_crate)* }
        }
    };
}

/// Internal macro that either chains to next plugin or calls finalize
#[doc(hidden)]
#[macro_export]
macro_rules! __facet_invoke_internal {
    // No more plugins - call finalize
    (
        @tokens { $($tokens:tt)* }
        @remaining { }
        @plugins { $($plugins:tt)* }
        @facet_crate { $($facet_crate:tt)* }
    ) => {
        $($facet_crate)*::__facet_finalize! {
            @tokens { $($tokens)* }
            @plugins { $($plugins)* }
            @facet_crate { $($facet_crate)* }
        }
    };

    // More plugins - chain to next
    (
        @tokens { $($tokens:tt)* }
        @remaining { $next:path $(, $rest:path)* $(,)? }
        @plugins { $($plugins:tt)* }
        @facet_crate { $($facet_crate:tt)* }
    ) => {
        $next! {
            @tokens { $($tokens)* }
            @remaining { $($rest),* }
            @plugins { $($plugins)* }
            @facet_crate { $($facet_crate)* }
        }
    };
}
