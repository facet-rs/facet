//! Partial value construction.

use std::marker::PhantomData;

use crate::arena::{Arena, Idx};
use crate::errors::ReflectError;
use crate::frame::Frame;
use crate::ops::Op;
use facet_core::{Facet, Shape};

/// Manages incremental construction of a value.
pub struct Partial<'facet> {
    arena: Arena<Frame>,
    root: Idx<Frame>,
    current: Idx<Frame>,
    _marker: PhantomData<&'facet ()>,
}

impl<'facet> Partial<'facet> {
    /// Allocate for a known type.
    pub fn alloc<T: Facet<'facet>>() -> Self {
        Self::alloc_shape(T::SHAPE)
    }

    /// Allocate for a dynamic shape.
    pub fn alloc_shape(shape: &'static Shape) -> Self {
        let _ = shape;
        todo!()
    }

    /// Apply a sequence of operations.
    pub fn apply(&mut self, ops: &[Op]) -> Result<(), ReflectError> {
        let _ = ops;
        todo!()
    }

    /// Build the final value, consuming the Partial.
    ///
    /// # Panics
    ///
    /// Panics if `T::SHAPE` does not match the shape passed to `alloc`.
    pub fn build<T: Facet<'facet>>(self) -> Result<T, ReflectError> {
        todo!()
    }
}
