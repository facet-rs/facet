use super::*;
use crate::{
    AllocError, AllocatedShape,
    typeplan::{TypePlan, TypePlanCore},
};

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
