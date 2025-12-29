//! # facet-default
//!
//! Derive [`Default`] for your types using facet's plugin system with custom field defaults.
//!
//! ## Usage
//!
//! ```ignore
//! use facet::Facet;
//! use facet_default as default;
//!
//! #[derive(Facet, Debug)]
//! #[facet(derive(Default))]
//! pub struct Config {
//!     #[facet(default::value = "localhost")]
//!     host: String,
//!     #[facet(default::value = 8080u16)]
//!     port: u16,
//!     #[facet(default::func = "default_timeout")]
//!     timeout: std::time::Duration,
//!     // No attribute = uses Default::default()
//!     debug: bool,
//! }
//!
//! fn default_timeout() -> std::time::Duration {
//!     std::time::Duration::from_secs(30)
//! }
//! ```
//!
//! ## Attributes
//!
//! ### Field Level
//!
//! - `#[facet(default::value = literal)]` - Use a literal value (converted via `.into()`)
//! - `#[facet(default::func = "path")]` - Call a function to get the default value (path as string)
//!
//! Fields without attributes use `Default::default()`.
//!
//! **Note:** For numeric literals, use type suffixes to ensure correct types (e.g., `8080u16`
//! instead of `8080` for a `u16` field). String literals are automatically converted via `.into()`.
//!
//! ## Enums
//!
//! For enums, mark the default variant:
//!
//! ```ignore
//! #[derive(Facet, Debug)]
//! #[facet(derive(Default))]
//! #[repr(u8)]
//! pub enum Status {
//!     #[facet(default::variant)]
//!     Pending,
//!     Active,
//!     Done,
//! }
//! ```

// ============================================================================
// ATTRIBUTE GRAMMAR
// ============================================================================

facet::define_attr_grammar! {
    ns "default";
    crate_path ::facet_default;

    /// Default attribute types for configuring Default implementation.
    pub enum Attr {
        /// Use a literal value for the field default (converted via `.into()`).
        ///
        /// Usage: `#[facet(default::value = "hello")]`
        /// Usage: `#[facet(default::value = 42)]`
        ///
        /// Note: The type here is nominally `&'static str` but the plugin template
        /// uses `@attr_args` which captures the raw tokens, so any value works.
        Value(&'static str),

        /// Call a function to get the default value.
        ///
        /// Usage: `#[facet(default::func = my_default_fn)]`
        ///
        /// Note: The type here is nominally `&'static str` but the plugin template
        /// uses `@attr_args` which captures the raw tokens, so any path works.
        Func(&'static str),

        /// Mark an enum variant as the default.
        ///
        /// Usage: `#[facet(default::variant)]`
        Variant,
    }
}

// ============================================================================
// PLUGIN TEMPLATE
// ============================================================================

/// Plugin chain entry point.
///
/// Called by `#[derive(Facet)]` when `#[facet(derive(Default))]` is present.
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
                    @name { "Default" }
                    @template {
                        impl ::core::default::Default for @Self {
                            fn default() -> Self {
                                @if_struct {
                                    Self {
                                        @for_field {
                                            @field_name: @field_default_expr,
                                        }
                                    }
                                }
                                @if_enum {
                                    @for_variant {
                                        @if_attr(default::variant) {
                                            Self::@variant_name @variant_default_construction
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
