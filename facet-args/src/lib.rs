#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

mod format;

pub(crate) mod arg;
pub(crate) mod error;
pub(crate) mod span;

pub use format::from_slice;
pub use format::from_std_args;

/// Extension attribute namespace for argument parsing-specific field markers.
///
/// Import this module and use attributes like `#[facet(args::positional)]`,
/// `#[facet(args::short)]`, etc. to control how fields are parsed from command
/// line arguments.
///
/// # Example
///
/// ```ignore
/// use facet::Facet;
/// use facet_args::args;
///
/// #[derive(Facet)]
/// struct Args {
///     /// Input file (positional argument)
///     #[facet(args::positional)]
///     input: String,
///
///     /// Verbose flag with short option -v
///     #[facet(args::short)]
///     verbose: bool,
///
///     /// Jobs count with short option -j
///     #[facet(args::short = "j")]
///     jobs: u32,
/// }
/// ```
pub mod args {
    facet_core::facet_ext_attr!(positional);
    facet_core::facet_ext_attr!(short);
}
