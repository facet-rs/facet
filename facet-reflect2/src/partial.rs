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

    /// Apply an operation.
    pub fn apply(&mut self, op: Op) -> Result<(), ReflectError> {
        todo!()
    }

    /// Build the final value, consuming the Partial.
    ///
    /// # Safety
    ///
    /// The caller must ensure the type `T` matches the shape used to create this Partial.
    pub unsafe fn build<T>(self) -> Result<T, ReflectError> {
        todo!()
    }
}
