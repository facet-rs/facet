#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(builtin_syntax))]
#![cfg_attr(docsrs, feature(prelude_import))]
#![cfg_attr(docsrs, allow(internal_features))]

pub use facet_core::*;

#[doc = include_str!("derive_facet.md")]
pub use facet_macros::*;

#[cfg(feature = "reflect")]
pub use facet_reflect::*;

pub mod hacking;

pub use static_assertions;
