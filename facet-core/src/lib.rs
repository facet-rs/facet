#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(builtin_syntax))]
#![cfg_attr(docsrs, feature(prelude_import))]
#![cfg_attr(docsrs, allow(internal_features))]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(docsrs)]
mod sample_generated_code;

// Opaque pointer utilities
mod opaque;
pub use opaque::*;

// Specialization utilities
pub mod spez;

// Core trait definitions
mod _trait;
pub use _trait::*;

// Const type Id
mod typeid;
pub use typeid::*;

// Type definitions
mod types;
#[allow(unused_imports)] // wtf clippy? we're re-exporting?
pub use types::*;
