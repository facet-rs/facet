use super::*;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Allocation, constructors etc.
////////////////////////////////////////////////////////////////////////////////////////////////////
impl<'facet> Partial<'facet> {
    /// Allocates a new [TypedPartial] instance on the heap, with the given shape and type
    pub fn alloc<T>() -> Result<TypedPartial<'facet, T>, ReflectError>
    where
        T: Facet<'facet> + ?Sized,
    {
        Ok(TypedPartial {
            inner: Self::alloc_shape(T::SHAPE)?,
            phantom: PhantomData,
        })
    }

    /// Allocates a new [Partial] instance on the heap, with the given shape.
    pub fn alloc_shape(shape: &'static Shape) -> Result<Self, ReflectError> {
        crate::trace!(
            "alloc_shape({:?}), with layout {:?}",
            shape,
            shape.layout.sized_layout()
        );

        let data = shape.allocate().map_err(|_| ReflectError::Unsized {
            shape,
            operation: "alloc_shape",
        })?;

        // Preallocate a couple of frames. The cost of allocating 4 frames is
        // basically identical to allocating 1 frame, so for every type that
        // has at least 1 level of nesting, this saves at least one guaranteed reallocation.
        let mut frames = Vec::with_capacity(4);
        frames.push(Frame::new(data, shape, FrameOwnership::Owned));

        Ok(Self {
            frames,
            state: PartialState::Active,
            deferred_resolution: None,
            invariant: PhantomData,
        })
    }
}
