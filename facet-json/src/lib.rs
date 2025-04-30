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
use core::iter::Peekable;

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
trait First<T>: Iterator + Sized {
    fn with_first(self) -> WithFirstIter<Self>;
}

#[cfg(feature = "std")]
impl<Iter: Iterator<Item = T>, T> First<T> for Iter {
    fn with_first(self) -> WithFirstIter<Iter> {
        WithFirstIter {
            iter: self.peekable(),
            first: true,
        }
    }
}

struct WithFirstIter<Iter: Iterator> {
    iter: Peekable<Iter>,
    first: bool,
}

impl<Iter> Iterator for WithFirstIter<Iter>
where
    Iter: Iterator,
{
    type Item = (bool, Iter::Item);

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|item| {
            let result = (self.first, item);
            self.first = false;
            result
        })
    }
}

impl<Iter> DoubleEndedIterator for WithFirstIter<Iter>
where
    Iter: DoubleEndedIterator,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back().map(|item| {
            if self.first {
                if self.iter.peek().is_none() {
                    self.first = false;
                    (true, item)
                } else {
                    (false, item)
                }
            } else {
                (false, item)
            }
        })
    }
}
