//! Diff and compare Facet values with detailed structural difference reporting.
//!
//! This crate provides:
//! - Structural diffing of Facet types without requiring `PartialEq`
//! - Pretty assertions via `assert_same!` and `assert_sameish!` macros
//! - Multi-format rendering (Rust, JSON, XML styles)
//! - ANSI colored terminal output
//!
//! # Quick Start
//!
//! ```
//! use facet::Facet;
//! use rediff::assert_same;
//!
//! #[derive(Facet)]
//! struct Point { x: i32, y: i32 }
//!
//! let a = Point { x: 10, y: 20 };
//! let b = Point { x: 10, y: 20 };
//! assert_same!(a, b);
//! ```
//!
//! # Diffing Values
//!
//! ```
//! use facet::Facet;
//! use rediff::{FacetDiff, format_diff_default};
//!
//! #[derive(Facet)]
//! struct Config {
//!     host: String,
//!     port: u16,
//! }
//!
//! let old = Config { host: "localhost".into(), port: 8080 };
//! let new = Config { host: "localhost".into(), port: 9000 };
//!
//! let diff = old.diff(&new);
//! println!("{}", format_diff_default(&diff));
//! ```

#![warn(missing_docs)]
#![forbid(unsafe_code)]

mod tracing_macros;

// Core types module (from facet-diff-core)
mod core_sequences;
mod display;
pub mod layout;
mod path;
mod symbols;
mod theme;
mod types;

// Diff computation (from facet-diff)
mod diff;
mod report;
mod sequences;

// Assertion helpers (from facet-assert)
mod same;

// Re-export core types
pub use core_sequences::{Interspersed, ReplaceGroup, Updates, UpdatesGroup};
pub use path::*;
pub use symbols::*;
pub use theme::*;
pub use types::*;

// Re-export diff computation
pub use diff::{
    DiffFormat, DiffOptions, FacetDiff, LeafChange, LeafChangeKind, collect_leaf_changes,
    diff_new_peek, diff_new_peek_with_options, format_diff, format_diff_compact,
    format_diff_compact_plain, format_diff_default,
};
pub use report::DiffReport;

// Re-export layout types for custom rendering
pub use layout::{
    AnsiBackend, BuildOptions, ColorBackend, DiffFlavor, JsonFlavor, PlainBackend, RenderOptions,
    RustFlavor, XmlFlavor, build_layout, render_to_string,
};

// Re-export assertion helpers
pub use same::{
    SameOptions, SameReport, Sameness, check_same, check_same_report, check_same_with,
    check_same_with_report, check_sameish, check_sameish_report, check_sameish_with,
    check_sameish_with_report,
};

// =============================================================================
// assert_same! - Same-type comparison (the common case)
// =============================================================================

/// Asserts that two values are structurally the same.
///
/// This macro does not require `PartialEq` - it uses Facet reflection to
/// compare values structurally. Both values must have the same type, which
/// enables type inference to flow between arguments.
///
/// For comparing values of different types (e.g., during migrations), use
/// [`assert_sameish!`] instead.
///
/// # Panics
///
/// Panics if the values are not structurally same, displaying a colored diff
/// showing exactly what differs.
///
/// Also panics if either value contains an opaque type that cannot be inspected.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use rediff::assert_same;
///
/// #[derive(Facet)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let a = Person { name: "Alice".into(), age: 30 };
/// let b = Person { name: "Alice".into(), age: 30 };
/// assert_same!(a, b);
/// ```
///
/// Type inference works naturally:
/// ```
/// use rediff::assert_same;
///
/// let x: Option<Option<i32>> = Some(None);
/// assert_same!(x, Some(None)); // Type of Some(None) inferred from x
/// ```
#[macro_export]
macro_rules! assert_same {
    ($left:expr, $right:expr $(,)?) => {
        match $crate::check_same(&$left, &$right) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_same!(left, right)` failed\n\n{diff}\n"
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_same!(left, right)` failed: cannot compare opaque type `{type_name}`"
                );
            }
        }
    };
    ($left:expr, $right:expr, $($arg:tt)+) => {
        match $crate::check_same(&$left, &$right) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_same!(left, right)` failed: {}\n\n{diff}\n",
                    format_args!($($arg)+)
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_same!(left, right)` failed: {}: cannot compare opaque type `{type_name}`",
                    format_args!($($arg)+)
                );
            }
        }
    };
}

/// Asserts that two values are structurally the same with custom options.
///
/// Like [`assert_same!`], but allows configuring comparison behavior via [`SameOptions`].
///
/// # Panics
///
/// Panics if the values are not structurally same, displaying a colored diff.
///
/// # Example
///
/// ```
/// use rediff::{assert_same_with, SameOptions};
///
/// let a = 1.0000001_f64;
/// let b = 1.0000002_f64;
///
/// // This would fail with exact comparison:
/// // assert_same!(a, b);
///
/// // But passes with tolerance:
/// assert_same_with!(a, b, SameOptions::new().float_tolerance(1e-6));
/// ```
#[macro_export]
macro_rules! assert_same_with {
    ($left:expr, $right:expr, $options:expr $(,)?) => {
        match $crate::check_same_with(&$left, &$right, $options) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_same_with!(left, right, options)` failed\n\n{diff}\n"
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_same_with!(left, right, options)` failed: cannot compare opaque type `{type_name}`"
                );
            }
        }
    };
    ($left:expr, $right:expr, $options:expr, $($arg:tt)+) => {
        match $crate::check_same_with(&$left, &$right, $options) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_same_with!(left, right, options)` failed: {}\n\n{diff}\n",
                    format_args!($($arg)+)
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_same_with!(left, right, options)` failed: {}: cannot compare opaque type `{type_name}`",
                    format_args!($($arg)+)
                );
            }
        }
    };
}

/// Asserts that two values are structurally the same (debug builds only).
///
/// Like [`assert_same!`], but only enabled in debug builds.
#[macro_export]
macro_rules! debug_assert_same {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::assert_same!($($arg)*);
        }
    };
}

/// Asserts that two values are structurally the same with custom options (debug builds only).
///
/// Like [`assert_same_with!`], but only enabled in debug builds.
#[macro_export]
macro_rules! debug_assert_same_with {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::assert_same_with!($($arg)*);
        }
    };
}

// =============================================================================
// assert_sameish! - Cross-type comparison (for migrations, etc.)
// =============================================================================

/// Asserts that two values of potentially different types are structurally the same.
///
/// Unlike [`assert_same!`], this allows comparing values of different types.
/// Two values are "sameish" if they have the same structure and values,
/// even if they have different type names.
///
/// **Note:** Because the two arguments can have different types, the compiler
/// cannot infer types from one side to the other. If you get type inference
/// errors, either add type annotations or use [`assert_same!`] instead.
///
/// # Panics
///
/// Panics if the values are not structurally same, displaying a colored diff.
///
/// # Example
///
/// ```
/// use facet::Facet;
/// use rediff::assert_sameish;
///
/// #[derive(Facet)]
/// struct PersonV1 {
///     name: String,
///     age: u32,
/// }
///
/// #[derive(Facet)]
/// struct PersonV2 {
///     name: String,
///     age: u32,
/// }
///
/// let old = PersonV1 { name: "Alice".into(), age: 30 };
/// let new = PersonV2 { name: "Alice".into(), age: 30 };
/// assert_sameish!(old, new); // Different types, same structure
/// ```
#[macro_export]
macro_rules! assert_sameish {
    ($left:expr, $right:expr $(,)?) => {
        match $crate::check_sameish(&$left, &$right) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_sameish!(left, right)` failed\n\n{diff}\n"
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_sameish!(left, right)` failed: cannot compare opaque type `{type_name}`"
                );
            }
        }
    };
    ($left:expr, $right:expr, $($arg:tt)+) => {
        match $crate::check_sameish(&$left, &$right) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_sameish!(left, right)` failed: {}\n\n{diff}\n",
                    format_args!($($arg)+)
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_sameish!(left, right)` failed: {}: cannot compare opaque type `{type_name}`",
                    format_args!($($arg)+)
                );
            }
        }
    };
}

/// Asserts that two values of different types are structurally the same with custom options.
///
/// Like [`assert_sameish!`], but allows configuring comparison behavior via [`SameOptions`].
#[macro_export]
macro_rules! assert_sameish_with {
    ($left:expr, $right:expr, $options:expr $(,)?) => {
        match $crate::check_sameish_with(&$left, &$right, $options) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_sameish_with!(left, right, options)` failed\n\n{diff}\n"
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_sameish_with!(left, right, options)` failed: cannot compare opaque type `{type_name}`"
                );
            }
        }
    };
    ($left:expr, $right:expr, $options:expr, $($arg:tt)+) => {
        match $crate::check_sameish_with(&$left, &$right, $options) {
            $crate::Sameness::Same => {}
            $crate::Sameness::Different(diff) => {
                panic!(
                    "assertion `assert_sameish_with!(left, right, options)` failed: {}\n\n{diff}\n",
                    format_args!($($arg)+)
                );
            }
            $crate::Sameness::Opaque { type_name } => {
                panic!(
                    "assertion `assert_sameish_with!(left, right, options)` failed: {}: cannot compare opaque type `{type_name}`",
                    format_args!($($arg)+)
                );
            }
        }
    };
}

/// Asserts that two values of different types are structurally the same (debug builds only).
///
/// Like [`assert_sameish!`], but only enabled in debug builds.
#[macro_export]
macro_rules! debug_assert_sameish {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::assert_sameish!($($arg)*);
        }
    };
}

/// Asserts that two values of different types are structurally the same with options (debug builds only).
///
/// Like [`assert_sameish_with!`], but only enabled in debug builds.
#[macro_export]
macro_rules! debug_assert_sameish_with {
    ($($arg:tt)*) => {
        if cfg!(debug_assertions) {
            $crate::assert_sameish_with!($($arg)*);
        }
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
