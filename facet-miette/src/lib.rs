//! # facet-miette
//!
//! Derive [`miette::Diagnostic`] for your error types using facet's plugin system.
//!
//! ## Usage
//!
//! Since the crate is named `facet-miette` but the derive is `Diagnostic`, you need
//! to use the explicit path syntax:
//!
//! ```ignore
//! use facet::Facet;
//! use facet_miette as diagnostic; // for attribute namespace
//! use miette::SourceSpan;
//!
//! #[derive(Facet, Debug)]
//! #[facet(derive(Error, facet_miette::Diagnostic))]
//! pub enum ParseError {
//!     /// Unexpected token in input
//!     #[facet(diagnostic::code = "parse::unexpected_token")]
//!     #[facet(diagnostic::help = "Check for typos or missing delimiters")]
//!     UnexpectedToken {
//!         #[facet(diagnostic::source_code)]
//!         src: String,
//!         #[facet(diagnostic::label = "this token was unexpected")]
//!         span: SourceSpan,
//!     },
//!
//!     /// End of file reached unexpectedly
//!     #[facet(diagnostic::code = "parse::unexpected_eof")]
//!     UnexpectedEof,
//! }
//! ```
//!
//! ## Attributes
//!
//! ### Container/Variant Level
//!
//! - `#[facet(diagnostic::code = "my_lib::error_code")]` - Error code for this diagnostic
//! - `#[facet(diagnostic::help = "Helpful message")]` - Help text shown to user
//! - `#[facet(diagnostic::url = "https://...")]` - URL for more information
//! - `#[facet(diagnostic::severity = "warning")]` - Severity: "error", "warning", or "advice"
//!
//! ### Field Level
//!
//! - `#[facet(diagnostic::source_code)]` - Field containing the source text (impl `SourceCode`)
//! - `#[facet(diagnostic::label = "description")]` - Field is a span to highlight with label
//! - `#[facet(diagnostic::related)]` - Field contains related diagnostics (iterator)
//!
//! ## Integration with facet-error
//!
//! You'll typically use both `Error` and `Diagnostic` together:
//!
//! ```ignore
//! #[derive(Facet, Debug)]
//! #[facet(derive(Error, facet_miette::Diagnostic))]
//! pub enum MyError {
//!     /// Something went wrong
//!     #[facet(diagnostic::code = "my_error")]
//!     SomeError,
//! }
//! ```

// Re-export miette types for convenience
pub use miette::{Diagnostic, LabeledSpan, Severity, SourceCode, SourceSpan};

// ============================================================================
// ATTRIBUTE GRAMMAR
// ============================================================================

facet::define_attr_grammar! {
    ns "diagnostic";
    crate_path ::facet_miette;

    /// Diagnostic attribute types for configuring miette::Diagnostic implementation.
    pub enum Attr {
        /// Error code for this diagnostic.
        ///
        /// Usage: `#[facet(diagnostic::code = "my_lib::error")]`
        Code(&'static str),

        /// Help message for this diagnostic.
        ///
        /// Usage: `#[facet(diagnostic::help = "Try doing X instead")]`
        Help(&'static str),

        /// URL for more information.
        ///
        /// Usage: `#[facet(diagnostic::url = "https://example.com/errors/E001")]`
        Url(&'static str),

        /// Severity level: "error", "warning", or "advice".
        ///
        /// Usage: `#[facet(diagnostic::severity = "warning")]`
        Severity(&'static str),

        /// Marks a field as containing the source code to display.
        ///
        /// Usage: `#[facet(diagnostic::source_code)]`
        SourceCode,

        /// Marks a field as a span to highlight with an optional label.
        ///
        /// Usage: `#[facet(diagnostic::label = "this is the problem")]`
        Label(&'static str),

        /// Marks a field as containing related diagnostics.
        ///
        /// Usage: `#[facet(diagnostic::related)]`
        Related,
    }
}

// ============================================================================
// PLUGIN TEMPLATE
// ============================================================================

/// Plugin chain entry point.
///
/// Called by `#[derive(Facet)]` when `#[facet(derive(Diagnostic))]` is present.
#[macro_export]
macro_rules! __facet_invoke {
    (
        @tokens { $($tokens:tt)* }
        @remaining { $($remaining:tt)* }
        @plugins { $($plugins:tt)* }
        @facet_crate { $($facet_crate:tt)* }
    ) => {
        $crate::__facet_invoke_internal! {
            @tokens { $($tokens)* }
            @remaining { $($remaining)* }
            @plugins {
                $($plugins)*
                @plugin {
                    @name { "Diagnostic" }
                    @template {
                        impl ::miette::Diagnostic for @Self {
                            fn code<'__facet_a>(&'__facet_a self) -> ::core::option::Option<::std::boxed::Box<dyn ::core::fmt::Display + '__facet_a>> {
                                match self {
                                    @for_variant {
                                        @if_attr(diagnostic::code) {
                                            Self::@variant_name { .. } => ::core::option::Option::Some(::std::boxed::Box::new(@attr_args)),
                                        }
                                    }
                                    _ => ::core::option::Option::None,
                                }
                            }

                            fn severity(&self) -> ::core::option::Option<::miette::Severity> {
                                match self {
                                    @for_variant {
                                        @if_attr(diagnostic::severity) {
                                            Self::@variant_name { .. } => {
                                                let s: &str = @attr_args;
                                                ::core::option::Option::Some(match s {
                                                    "error" => ::miette::Severity::Error,
                                                    "warning" => ::miette::Severity::Warning,
                                                    "advice" => ::miette::Severity::Advice,
                                                    _ => ::miette::Severity::Error,
                                                })
                                            }
                                        }
                                    }
                                    _ => ::core::option::Option::None,
                                }
                            }

                            fn help<'__facet_a>(&'__facet_a self) -> ::core::option::Option<::std::boxed::Box<dyn ::core::fmt::Display + '__facet_a>> {
                                match self {
                                    @for_variant {
                                        @if_attr(diagnostic::help) {
                                            Self::@variant_name { .. } => ::core::option::Option::Some(::std::boxed::Box::new(@attr_args)),
                                        }
                                    }
                                    _ => ::core::option::Option::None,
                                }
                            }

                            fn url<'__facet_a>(&'__facet_a self) -> ::core::option::Option<::std::boxed::Box<dyn ::core::fmt::Display + '__facet_a>> {
                                match self {
                                    @for_variant {
                                        @if_attr(diagnostic::url) {
                                            Self::@variant_name { .. } => ::core::option::Option::Some(::std::boxed::Box::new(@attr_args)),
                                        }
                                    }
                                    _ => ::core::option::Option::None,
                                }
                            }

                            fn source_code(&self) -> ::core::option::Option<&dyn ::miette::SourceCode> {
                                match self {
                                    @for_variant {
                                        @if_field_attr(diagnostic::source_code) {
                                            Self::@variant_name { @field_name, .. } => ::core::option::Option::Some(@field_name),
                                        }
                                    }
                                    _ => ::core::option::Option::None,
                                }
                            }

                            fn labels(&self) -> ::core::option::Option<::std::boxed::Box<dyn ::core::iter::Iterator<Item = ::miette::LabeledSpan> + '_>> {
                                match self {
                                    @for_variant {
                                        @if_any_field_attr(diagnostic::label) {
                                            Self::@variant_name @variant_pattern => {
                                                let mut __facet_labels = ::std::vec::Vec::new();
                                                @for_field {
                                                    @if_attr(diagnostic::label) {
                                                        __facet_labels.push(::miette::LabeledSpan::at(
                                                            @field_name.clone(),
                                                            @attr_args
                                                        ));
                                                    }
                                                }
                                                ::core::option::Option::Some(::std::boxed::Box::new(__facet_labels.into_iter()))
                                            }
                                        }
                                    }
                                    _ => ::core::option::Option::None,
                                }
                            }

                            fn related<'__facet_a>(&'__facet_a self) -> ::core::option::Option<::std::boxed::Box<dyn ::core::iter::Iterator<Item = &'__facet_a dyn ::miette::Diagnostic> + '__facet_a>> {
                                match self {
                                    @for_variant {
                                        @if_field_attr(diagnostic::related) {
                                            Self::@variant_name { @field_name, .. } => {
                                                ::core::option::Option::Some(::std::boxed::Box::new(
                                                    @field_name.iter().map(|__facet_e| __facet_e as &dyn ::miette::Diagnostic)
                                                ))
                                            }
                                        }
                                    }
                                    _ => ::core::option::Option::None,
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
