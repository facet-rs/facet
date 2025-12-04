#![cfg_attr(not(feature = "std"), no_std)]
// Enable portable_simd when available (detected via autocfg in build.rs)
#![cfg_attr(has_portable_simd, feature(portable_simd))]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "alloc")]
extern crate alloc;

mod macros;
pub use macros::*;

// Opaque pointer utilities
mod ptr;
pub use ptr::*;

// Opaque wrapper utility
mod opaque;
pub use opaque::*;

// Specialization utilities
pub mod spez;

// Definition for `core::` types
mod impls_core;

// Definition for `alloc::` types
#[cfg(feature = "alloc")]
mod impls_alloc;

// Definition for `std::` types (that aren't in `alloc` or `core)
#[cfg(feature = "std")]
mod impls_std;

#[cfg(feature = "bytes")]
mod impls_bytes;

#[cfg(feature = "camino")]
mod impls_camino;

#[cfg(feature = "ordered-float")]
mod impls_ordered_float;

#[cfg(feature = "uuid")]
mod impls_uuid;

#[cfg(feature = "ulid")]
mod impls_ulid;

#[cfg(feature = "time")]
mod impls_time;

#[cfg(feature = "chrono")]
mod impls_chrono;

#[cfg(feature = "url")]
mod impls_url;

#[cfg(feature = "jiff02")]
mod impls_jiff;

#[cfg(feature = "num-complex")]
mod impls_num_complex;

#[cfg(feature = "ruint")]
mod impls_ruint;

#[cfg(feature = "indexmap")]
mod impls_indexmap;

// Const type Id
mod typeid;
pub use typeid::*;

// Type definitions
mod types;
#[allow(unused_imports)] // wtf clippy? we're re-exporting?
pub use types::*;

/// Allows querying the [`Shape`] of a type, which in turn lets us inspect any fields, build a value of
/// this type progressively, etc.
///
/// The `'facet` lifetime allows `Facet` to be derived for types that borrow from something else.
///
/// # Safety
///
/// If you implement this wrong, all the safe abstractions in `facet-reflect`,
/// all the serializers, deserializers, the entire ecosystem is unsafe.
///
/// You're responsible for describing the type layout properly, and annotating all the invariants.
pub unsafe trait Facet<'facet>: 'facet {
    /// The shape of this type
    ///
    /// Shape embeds all other constants of this trait.
    const SHAPE: &'static Shape;
}

mod shape_util;

// Write trait for serializers
mod write;
pub use write::Write;

/// Re-export paste for use in macros
#[doc(hidden)]
pub use paste;
