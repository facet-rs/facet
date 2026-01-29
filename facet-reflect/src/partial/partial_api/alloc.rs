use super::*;
use crate::{
    AllocError, AllocatedShape,
    typeplan::{TypePlan, TypePlanCore, build_core_for_format},
};
use bumpalo::Bump;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Closure-based allocation (manages bump internally)
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'facet> Partial<'facet, 'static, true> {
    /// Execute a closure with a freshly allocated Partial (borrowing mode).
    ///
    /// This is a convenience method that manages the bump allocator internally.
    /// The closure receives a Partial and can return any result that doesn't
    /// borrow from the Partial's internal storage.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let person: Person = Partial::alloc::<Person, _>(|p| {
    ///     p.set_field("name", "Alice")?
    ///      .set_field("age", 30u32)?
    ///      .build()?
    ///      .materialize()
    /// })?;
    /// ```
    pub fn alloc<T, R, F>(f: F) -> Result<R, AllocError>
    where
        T: Facet<'facet>,
        F: for<'plan> FnOnce(Partial<'facet, 'plan, true>) -> Result<R, AllocError>,
    {
        let bump = Bump::new();
        let plan = TypePlan::<T>::build(&bump)?;
        let partial = plan.partial()?;
        f(partial)
    }

    /// Execute a closure with a Partial allocated from a shape (borrowing mode).
    ///
    /// This creates the bump allocator and TypePlan internally, then calls
    /// the closure with the Partial.
    ///
    /// # Safety
    ///
    /// The shape must be valid and match the expected type structure.
    pub unsafe fn with_shape<R, F>(shape: &'static Shape, f: F) -> Result<R, AllocError>
    where
        F: for<'plan> FnOnce(Partial<'facet, 'plan, true>) -> Result<R, AllocError>,
    {
        let bump = Bump::new();
        let plan = build_core_for_format(&bump, shape, None)?;
        let partial = alloc_impl(plan, shape)?;
        f(partial)
    }
}

impl Partial<'static, 'static, false> {
    /// Execute a closure with a freshly allocated Partial (owned mode).
    ///
    /// Same as `alloc` but creates an owned Partial that doesn't borrow from input.
    pub fn alloc_owned<T, R, F>(f: F) -> Result<R, AllocError>
    where
        T: Facet<'static>,
        F: for<'plan> FnOnce(Partial<'static, 'plan, false>) -> Result<R, AllocError>,
    {
        let bump = Bump::new();
        let plan = TypePlan::<T>::build(&bump)?;
        let partial = plan.partial_owned()?;
        f(partial)
    }

    /// Execute a closure with a Partial allocated from a shape (owned mode).
    ///
    /// # Safety
    ///
    /// The shape must be valid and match the expected type structure.
    pub unsafe fn with_shape_owned<R, F>(shape: &'static Shape, f: F) -> Result<R, AllocError>
    where
        F: for<'plan> FnOnce(Partial<'static, 'plan, false>) -> Result<R, AllocError>,
    {
        let bump = Bump::new();
        let plan = build_core_for_format(&bump, shape, None)?;
        let partial = alloc_impl(plan, shape)?;
        f(partial)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Allocation using TypePlan
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'plan, T: ?Sized> TypePlan<'plan, T> {
    /// Allocates a new borrowing [Partial] using this TypePlan.
    ///
    /// This creates a Partial that can hold references with lifetime 'facet.
    /// The Partial will use this TypePlan for deserialization.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use bumpalo::Bump;
    /// use facet_reflect::TypePlan;
    ///
    /// let bump = Bump::new();
    /// let plan = TypePlan::<MyStruct>::build(&bump)?;
    /// let partial = plan.partial()?;
    /// ```
    pub fn partial<'facet>(&self) -> Result<Partial<'facet, 'plan, true>, AllocError>
    where
        T: Facet<'facet>,
    {
        let shape = T::SHAPE;
        let plan = self.core();

        let data = shape.allocate().map_err(|_| AllocError {
            shape,
            operation: "partial: allocation failed",
        })?;

        let allocated_size = shape.layout.sized_layout().expect("must be sized").size();

        let mut stack = Vec::with_capacity(4);
        stack.push(Frame::new(
            data,
            AllocatedShape::new(shape, allocated_size),
            FrameOwnership::Owned,
            plan.root(),
        ));

        Ok(Partial {
            mode: FrameMode::Strict { stack },
            state: PartialState::Active,
            root_plan: plan,
            _marker: PhantomData,
        })
    }

    /// Allocates a new owned [Partial] using this TypePlan.
    ///
    /// This creates a Partial that cannot hold borrowed references.
    /// Use this when the input buffer is temporary and won't outlive the result.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use bumpalo::Bump;
    /// use facet_reflect::TypePlan;
    ///
    /// let bump = Bump::new();
    /// let plan = TypePlan::<MyStruct>::build(&bump)?;
    /// let partial = plan.partial_owned()?;
    /// ```
    pub fn partial_owned(&self) -> Result<Partial<'static, 'plan, false>, AllocError>
    where
        T: Facet<'static>,
    {
        alloc_impl(self.core(), T::SHAPE)
    }

    /// Creates a [Partial] that writes into caller-provided memory.
    ///
    /// # Safety
    ///
    /// The caller must ensure:
    /// - `ptr` points to valid, properly aligned memory for type T
    /// - The memory has at least `T::SHAPE.layout.size()` bytes available
    /// - The memory is not accessed through any other pointer while the Partial exists
    /// - If deserialization fails, the memory may be partially initialized
    pub unsafe fn partial_in_place<'facet>(
        &self,
        ptr: PtrUninit,
    ) -> Result<Partial<'facet, 'plan, true>, AllocError>
    where
        T: Facet<'facet>,
    {
        // SAFETY: Caller upholds requirements
        unsafe { from_raw_impl(self.core(), ptr, T::SHAPE) }
    }

    /// Creates an owned [Partial] that writes into caller-provided memory.
    ///
    /// # Safety
    /// Same requirements as `partial_in_place`.
    pub unsafe fn partial_in_place_owned(
        &self,
        ptr: PtrUninit,
    ) -> Result<Partial<'static, 'plan, false>, AllocError>
    where
        T: Facet<'static>,
    {
        // SAFETY: Caller upholds requirements
        unsafe { from_raw_impl(self.core(), ptr, T::SHAPE) }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Legacy alloc methods for backwards compatibility
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'facet, 'plan> Partial<'facet, 'plan, true> {
    /// Allocates a new [Partial] using a TypePlanCore.
    ///
    /// # Safety
    /// The caller must ensure `shape` matches the plan's type.
    pub fn alloc_shape(
        plan: TypePlanCore<'plan>,
        shape: &'static Shape,
    ) -> Result<Self, AllocError> {
        alloc_impl(plan, shape)
    }
}

impl<'plan> Partial<'static, 'plan, false> {
    /// Allocates an owned [Partial] using a TypePlanCore.
    ///
    /// # Safety
    /// The caller must ensure `shape` matches the plan's type.
    pub fn alloc_shape_owned(
        plan: TypePlanCore<'plan>,
        shape: &'static Shape,
    ) -> Result<Self, AllocError> {
        alloc_impl(plan, shape)
    }
}

impl<'facet, 'plan, const BORROW: bool> Partial<'facet, 'plan, BORROW> {
    /// Creates a [Partial] that writes into caller-provided memory.
    ///
    /// # Safety
    /// - `ptr` must point to valid memory for the shape
    /// - `plan` must have been built from `shape`
    pub unsafe fn from_raw_shape(
        plan: TypePlanCore<'plan>,
        ptr: PtrUninit,
        shape: &'static Shape,
    ) -> Result<Self, AllocError> {
        // SAFETY: Caller upholds requirements
        unsafe { from_raw_impl(plan, ptr, shape) }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Internal implementation
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Internal helper to allocate a Partial from a TypePlanCore and shape.
fn alloc_impl<'facet, 'plan, const BORROW: bool>(
    plan: TypePlanCore<'plan>,
    shape: &'static Shape,
) -> Result<Partial<'facet, 'plan, BORROW>, AllocError> {
    crate::trace!(
        "alloc_impl({:?}), with layout {:?}",
        shape,
        shape.layout.sized_layout()
    );

    let data = shape.allocate().map_err(|_| AllocError {
        shape,
        operation: "alloc_impl: allocation failed",
    })?;

    let allocated_size = shape.layout.sized_layout().expect("must be sized").size();

    let mut stack = Vec::with_capacity(4);
    stack.push(Frame::new(
        data,
        AllocatedShape::new(shape, allocated_size),
        FrameOwnership::Owned,
        plan.root(),
    ));

    Ok(Partial {
        mode: FrameMode::Strict { stack },
        state: PartialState::Active,
        root_plan: plan,
        _marker: PhantomData,
    })
}

/// Internal helper to create a Partial from raw pointer.
///
/// # Safety
/// Same requirements as `Partial::from_raw_shape`.
unsafe fn from_raw_impl<'facet, 'plan, const BORROW: bool>(
    plan: TypePlanCore<'plan>,
    ptr: PtrUninit,
    shape: &'static Shape,
) -> Result<Partial<'facet, 'plan, BORROW>, AllocError> {
    crate::trace!(
        "from_raw_impl({:p}, {:?}), with layout {:?}",
        ptr.as_mut_byte_ptr(),
        shape,
        shape.layout.sized_layout()
    );

    let allocated_size = shape
        .layout
        .sized_layout()
        .map_err(|_| AllocError {
            shape,
            operation: "from_raw_impl: shape is unsized",
        })?
        .size();

    let mut stack = Vec::with_capacity(4);
    stack.push(Frame::new(
        ptr,
        AllocatedShape::new(shape, allocated_size),
        FrameOwnership::External,
        plan.root(),
    ));

    Ok(Partial {
        mode: FrameMode::Strict { stack },
        state: PartialState::Active,
        root_plan: plan,
        _marker: PhantomData,
    })
}
