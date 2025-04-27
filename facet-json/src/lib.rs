#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![deny(unsafe_code)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(not(feature = "alloc"))]
compile_error!("feature `alloc` is required");

mod deserialize;
pub use deserialize::*;

#[cfg(feature = "std")]
mod serialize;
#[cfg(feature = "std")]
pub use serialize::*;

#[cfg(feature = "std")]
fn variant_is_transparent(variant: &facet_core::Variant) -> bool {
    variant.data.kind == facet_core::StructKind::Tuple && variant.data.fields.len() == 1
}

#[cfg(feature = "std")]
trait First<T> {
    fn with_first(self) -> impl Iterator<Item = (bool, T)>;
}

#[cfg(feature = "std")]
impl<Iter, T> First<T> for Iter
where
    Iter: Iterator<Item = T>,
{
    fn with_first(self) -> impl Iterator<Item = (bool, T)> {
        self.enumerate().map(|(idx, elem)| (idx == 0, elem))
    }
}
