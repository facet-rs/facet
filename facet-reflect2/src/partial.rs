//! Partial value construction.

use crate::arena::{Arena, Idx};
use crate::errors::ReflectError;
use crate::frame::Frame;
use crate::ops::Op;
use facet_core::Shape;

/// Manages incremental construction of a value.
pub struct Partial {
    arena: Arena<Frame>,
    root: Idx<Frame>,
    current: Idx<Frame>,
}

impl Partial {
    /// Create a new Partial for constructing a value of the given shape.
    pub fn alloc(shape: &'static Shape) -> Self {
        todo!()
    }

    /// Apply a sequence of operations.
    pub fn apply(&mut self, ops: &[Op]) -> Result<(), ReflectError> {
        todo!()
    }

    /// Build the final value, consuming the Partial.
    ///
    /// # Panics
    ///
    /// Panics if `T::SHAPE` does not match the shape passed to `alloc`.
    pub fn build<T>(self) -> Result<T, ReflectError> {
        todo!()
    }
}
