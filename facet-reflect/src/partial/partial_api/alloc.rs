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
        // SAFETY: T::SHAPE comes from the Facet implementation for T,
        // which is an unsafe trait requiring accurate shape descriptions.
        unsafe { Self::alloc_shape(T::SHAPE) }
    }

    /// Allocates a new [Partial] instance on the heap, with the given shape.
    ///
    /// This creates a borrowing Partial that can hold references with lifetime 'facet.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `shape` accurately describes the memory layout
    /// and invariants of any type `T` that will be materialized from this `Partial`.
    ///
    /// In particular:
    /// - `shape.id` must match `TypeId::of::<T>()`
    /// - `shape.layout` must match `Layout::of::<T>()`
    /// - `shape.ty` and `shape.def` must accurately describe T's structure
    /// - All vtable operations must be valid for type T
    ///
    /// Violating these requirements may cause undefined behavior when accessing
    /// fields, materializing values, or calling vtable methods.
    ///
    /// **Safe alternative**: Use [`Partial::alloc::<T>()`](Self::alloc) which gets the shape
    /// from `T::SHAPE` (guaranteed safe by `unsafe impl Facet for T`).
    pub unsafe fn alloc_shape(shape: &'static Shape) -> Result<Self, ReflectError> {
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
        // SAFETY: T::SHAPE comes from the Facet implementation for T,
        // which is an unsafe trait requiring accurate shape descriptions.
        unsafe { Self::alloc_shape_owned(T::SHAPE) }
    }

    /// Allocates a new [Partial] instance on the heap, with the given shape.
    ///
    /// This creates an owned Partial that cannot hold borrowed references.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `shape` accurately describes the memory layout
    /// and invariants of any type `T` that will be materialized from this `Partial`.
    ///
    /// In particular:
    /// - `shape.id` must match `TypeId::of::<T>()`
    /// - `shape.layout` must match `Layout::of::<T>()`
    /// - `shape.ty` and `shape.def` must accurately describe T's structure
    /// - All vtable operations must be valid for type T
    ///
    /// Violating these requirements may cause undefined behavior when accessing
    /// fields, materializing values, or calling vtable methods.
    ///
    /// **Safe alternative**: Use [`Partial::alloc_owned::<T>()`](Self::alloc_owned) which gets the shape
    /// from `T::SHAPE` (guaranteed safe by `unsafe impl Facet for T`).
    pub unsafe fn alloc_shape_owned(shape: &'static Shape) -> Result<Self, ReflectError> {
        alloc_shape_inner(shape)
    }
}

/// Create a Partial that writes into externally-owned memory (e.g., caller's stack).
/// This enables stack-friendly deserialization without heap allocation.
impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Creates a [Partial] that writes into caller-provided memory.
    ///
    /// This is useful for stack-friendly deserialization where you want to avoid
    /// heap allocation. The caller provides a `MaybeUninit<T>` and this Partial
    /// will deserialize directly into that memory.
    ///
    /// After successful deserialization, call [`finish_in_place`](Self::finish_in_place)
    /// to validate the value is fully initialized. On success, the caller can safely
    /// call `MaybeUninit::assume_init()`.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// - `ptr` points to valid, properly aligned memory for the type described by `shape`
    /// - The memory has at least `shape.layout.size()` bytes available
    /// - The memory is not accessed (read or written) through any other pointer while
    ///   the Partial exists, except through this Partial's methods
    /// - If deserialization fails (returns Err) or panics, the memory must not be
    ///   assumed to contain a valid value - it may be partially initialized
    /// - The memory must remain valid for the lifetime of the Partial
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::mem::MaybeUninit;
    /// use facet_core::{Facet, PtrUninit};
    /// use facet_reflect::Partial;
    ///
    /// let mut slot = MaybeUninit::<MyStruct>::uninit();
    /// let ptr = PtrUninit::new(slot.as_mut_ptr().cast());
    ///
    /// let partial = unsafe { Partial::from_raw(ptr, MyStruct::SHAPE)? };
    /// // ... deserialize into partial ...
    /// partial.finish_in_place()?;
    ///
    /// // Now safe to assume initialized
    /// let value = unsafe { slot.assume_init() };
    /// ```
    pub unsafe fn from_raw(ptr: PtrUninit, shape: &'static Shape) -> Result<Self, ReflectError> {
        crate::trace!(
            "from_raw({:p}, {:?}), with layout {:?}",
            ptr.as_mut_byte_ptr(),
            shape,
            shape.layout.sized_layout()
        );

        // Verify the shape is sized
        let allocated_size = shape
            .layout
            .sized_layout()
            .map_err(|_| {
                ReflectError::without_path(ReflectErrorKind::Unsized {
                    shape,
                    operation: "from_raw",
                })
            })?
            .size();

        // Preallocate a couple of frames for nested structures
        let mut stack = Vec::with_capacity(4);
        stack.push(Frame::new(
            ptr,
            AllocatedShape::new(shape, allocated_size),
            FrameOwnership::External,
        ));

        Ok(Partial {
            mode: FrameMode::Strict { stack },
            state: PartialState::Active,
            invariant: PhantomData,
        })
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

    let data = shape.allocate().map_err(|_| {
        ReflectError::without_path(ReflectErrorKind::Unsized {
            shape,
            operation: "alloc_shape",
        })
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
