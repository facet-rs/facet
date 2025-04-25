#![cfg_attr(not(feature = "std"), no_std)]
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

#[cfg(feature = "camino")]
mod impls_camino;

#[cfg(feature = "ordered-float")]
mod impls_ordered_float;

#[cfg(feature = "uuid")]
mod impls_uuid;

#[cfg(feature = "ulid")]
mod impls_ulid;

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
/// # Safety
///
/// If you implement this wrong, all the safe abstractions in `facet-reflect`,
/// all the serializers, deserializers, the entire ecosystem is unsafe.
///
/// You're responsible for describing the type layout properly, and annotating all the invariants.
pub unsafe trait Facet<'a>: 'a {
    /// The shape of this type
    const SHAPE: &'static Shape;

    /// Returns true if the type of `self` is equal to the type of `other`
    fn type_eq<Other: Facet<'a>>() -> bool {
        Self::SHAPE == Other::SHAPE
    }
}
