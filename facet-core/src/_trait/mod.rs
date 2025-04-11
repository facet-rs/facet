#![warn(missing_docs)]
#![doc = include_str!("../../README.md")]

pub use crate::types::*;

mod impls;

mod macros;
pub use macros::*;

/// Allows querying the [`Shape`] of a type, which in turn lets us inspect any fields, build a value of
/// this type progressively, etc.
///
/// # Safety
///
/// If you implement this wrong, all the safe abstractions in `facet-reflect`,
/// all the serializers, deserializers, the entire ecosystem is unsafe.
///
/// You're responsible for describing the type layout properly, and annotating all the invariants.
pub unsafe trait Facet: Sized {
    /// The shape of this type
    const SHAPE: &'static Shape;

    /// Returns true if the type of `self` is equal to the type of `other`
    fn type_eq<Other: Facet>() -> bool {
        Self::SHAPE == Other::SHAPE
    }
}
