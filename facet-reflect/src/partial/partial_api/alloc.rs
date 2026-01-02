use super::*;
use crate::AllocatedShape;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Allocation, constructors etc.
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Allocate a Partial that can borrow from input with lifetime 'facet.
/// This is the default mode - use this when deserializing from a buffer that outlives the result.
impl<'facet> Partial<'facet, true> {
    /// Allocates a new [Partial] instance on the heap, with the given shape and type.
    ///
    /// This creates a borrowing Partial that can hold references with lifetime 'facet.
    pub fn alloc<T>() -> Result<Self, ReflectError>
    where
        T: Facet<'facet> + ?Sized,
    {
        Self::alloc_shape(T::SHAPE)
    }

    /// Allocates a new [Partial] instance on the heap, with the given shape.
    ///
    /// This creates a borrowing Partial that can hold references with lifetime 'facet.
    pub fn alloc_shape(shape: &'static Shape) -> Result<Self, ReflectError> {
        alloc_shape_inner(shape)
    }
}

/// Allocate a Partial that cannot borrow - all data must be owned.
/// Use this when deserializing from a temporary buffer (e.g., HTTP request body).
impl Partial<'static, false> {
    /// Allocates a new [Partial] instance on the heap, with the given shape and type.
    ///
    /// This creates an owned Partial that cannot hold borrowed references.
    /// Use this when the input buffer is temporary and won't outlive the result.
    pub fn alloc_owned<T>() -> Result<Self, ReflectError>
    where
        T: Facet<'static> + ?Sized,
    {
        Self::alloc_shape_owned(T::SHAPE)
    }

    /// Allocates a new [Partial] instance on the heap, with the given shape.
    ///
    /// This creates an owned Partial that cannot hold borrowed references.
    pub fn alloc_shape_owned(shape: &'static Shape) -> Result<Self, ReflectError> {
        alloc_shape_inner(shape)
    }
}

fn alloc_shape_inner<'facet, const BORROW: bool>(
    shape: &'static Shape,
) -> Result<Partial<'facet, BORROW>, ReflectError> {
    crate::trace!(
        "alloc_shape({:?}), with layout {:?}",
        shape,
        shape.layout.sized_layout()
    );

    let data = shape.allocate().map_err(|_| ReflectError::Unsized {
        shape,
        operation: "alloc_shape",
    })?;

    // Get the actual allocated size
    let allocated_size = shape.layout.sized_layout().expect("must be sized").size();

    // Preallocate a couple of frames. The cost of allocating 4 frames is
    // basically identical to allocating 1 frame, so for every type that
    // has at least 1 level of nesting, this saves at least one guaranteed reallocation.
    let mut stack = Vec::with_capacity(4);
    stack.push(Frame::new(
        data,
        AllocatedShape::new(shape, allocated_size),
        FrameOwnership::Owned,
    ));

    Ok(Partial {
        mode: FrameMode::Strict { stack },
        state: PartialState::Active,
        invariant: PhantomData,
    })
}
