#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(builtin_syntax))]
#![cfg_attr(docsrs, feature(prelude_import))]
#![cfg_attr(docsrs, allow(internal_features))]

#[cfg(docsrs)]
pub mod sample_generated_code;

pub use facet_core::*;

pub use facet_derive::*;

#[cfg(feature = "reflect")]
pub use facet_reflect::*;

pub mod hacking;

pub use static_assertions;
