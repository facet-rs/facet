use super::*;
use crate::{
    AllocError, AllocatedShape,
    typeplan::{TypePlan, TypePlanCore, build_core_for_format},
};
use bumpalo::Bump;
use core::marker::PhantomData;

////////////////////////////////////////////////////////////////////////////////////////////////////
// Builder for closure-based allocation with nice ergonomics
////////////////////////////////////////////////////////////////////////////////////////////////////

/// Builder for creating a borrowing Partial with closure-based allocation.
///
/// Created via [`Partial::of::<T>()`]. This captures the type parameter,
/// allowing the `scope` method to infer all other type parameters.
///
/// # Example
///
/// ```ignore
/// let result = Partial::of::<Person>().scope(|p| {
///     p.set_field("name", "Alice")?
///      .set_field("age", 30u32)?
///      .build()?
///      .materialize()
/// })?;
/// ```
pub struct PartialBuilder<'facet, T: ?Sized> {
    _marker: PhantomData<(&'facet (), fn() -> T)>,
}

impl<'facet, T: Facet<'facet>> PartialBuilder<'facet, T> {
    /// Execute a closure with a freshly allocated Partial.
    ///
    /// The bump allocator is managed internally. The closure can return
    /// any error type that `AllocError` can convert into.
    pub fn scope<R, E, F>(self, f: F) -> Result<R, E>
    where
        E: From<AllocError>,
        F: for<'plan> FnOnce(Partial<'facet, 'plan, true>) -> Result<R, E>,
    {
        let bump = Bump::new();
        let plan = TypePlan::<T>::build(&bump)?;
        let partial = plan.partial()?;
        f(partial)
    }
}

/// Builder for creating an owned Partial with closure-based allocation.
///
/// Created via [`Partial::of_owned::<T>()`]. This captures the type parameter,
/// allowing the `scope` method to infer all other type parameters.
///
/// # Example
///
/// ```ignore
/// let result = Partial::of_owned::<Person>().scope(|p| {
///     p.set_field("name", "Alice".to_string())?
///      .set_field("age", 30u32)?
///      .build()?
///      .materialize()
/// })?;
/// ```
pub struct PartialBuilderOwned<T: ?Sized> {
    _marker: PhantomData<fn() -> T>,
}

impl<T: Facet<'static>> PartialBuilderOwned<T> {
    /// Execute a closure with a freshly allocated owned Partial.
    ///
    /// The bump allocator is managed internally. The closure can return
    /// any error type that `AllocError` can convert into.
    pub fn scope<R, E, F>(self, f: F) -> Result<R, E>
    where
        E: From<AllocError>,
        F: for<'plan> FnOnce(Partial<'static, 'plan, false>) -> Result<R, E>,
    {
        let bump = Bump::new();
        let plan = TypePlan::<T>::build(&bump)?;
        let partial = plan.partial_owned()?;
        f(partial)
    }
}

impl<'facet, 'plan> Partial<'facet, 'plan, true> {
    /// Create a builder for closure-based Partial allocation (borrowing).
    ///
    /// This captures the type parameter `T`, allowing the `scope` method
    /// to infer all other type parameters automatically.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let person = Partial::of::<Person>().scope(|p| {
    ///     p.set_field("name", "Alice")?
    ///      .set_field("age", 30u32)?
    ///      .build()?
    ///      .materialize()
    /// })?;
    /// ```
    pub fn of<T: ?Sized>() -> PartialBuilder<'facet, T> {
        PartialBuilder {
            _marker: PhantomData,
        }
    }
}

impl<'plan> Partial<'static, 'plan, false> {
    /// Create a builder for closure-based Partial allocation (owned).
    ///
    /// This captures the type parameter `T`, allowing the `scope` method
    /// to infer all other type parameters automatically.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let person = Partial::of_owned::<Person>().scope(|p| {
    ///     p.set_field("name", "Alice".to_string())?
    ///      .set_field("age", 30u32)?
    ///      .build()?
    ///      .materialize()
    /// })?;
    /// ```
    pub fn of_owned<T: ?Sized>() -> PartialBuilderOwned<T> {
        PartialBuilderOwned {
            _marker: PhantomData,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Direct allocation with external bump (alloc)
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'facet, 'plan> Partial<'facet, 'plan, true> {
    /// Allocate a Partial for type T using the provided bump allocator.
    ///
    /// This is the primary allocation method for production code. The caller
    /// manages the bump allocator lifetime.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let bump = Bump::new();
    /// let person = Partial::alloc::<Person>(&bump)?
    ///     .set_field("name", "Alice")?
    ///     .set_field("age", 30u32)?
    ///     .build()?
    ///     .materialize::<Person>()?;
    /// ```
    pub fn alloc<T: Facet<'facet>>(bump: &'plan Bump) -> Result<Self, AllocError> {
        let plan = TypePlan::<T>::build(bump)?;
        plan.partial()
    }

    /// Allocate a Partial from a shape using the provided bump allocator.
    ///
    /// # Safety
    ///
    /// The shape must be valid and match the expected type structure.
    pub unsafe fn alloc_shape(
        bump: &'plan Bump,
        shape: &'static Shape,
    ) -> Result<Self, AllocError> {
        let plan = build_core_for_format(bump, shape, None)?;
        alloc_impl(plan, shape)
    }
}

impl<'plan> Partial<'static, 'plan, false> {
    /// Allocate an owned Partial for type T.
    ///
    /// This creates a Partial that cannot hold borrowed references.
    /// Use this when the input buffer is temporary and won't outlive the result.
    pub fn alloc_owned<T: Facet<'static>>(bump: &'plan Bump) -> Result<Self, AllocError> {
        let plan = TypePlan::<T>::build(bump)?;
        plan.partial_owned()
    }

    /// Allocate an owned Partial from a shape.
    ///
    /// # Safety
    ///
    /// The shape must be valid and match the expected type structure.
    pub unsafe fn alloc_shape_owned(
        bump: &'plan Bump,
        shape: &'static Shape,
    ) -> Result<Self, AllocError> {
        let plan = build_core_for_format(bump, shape, None)?;
        alloc_impl(plan, shape)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Closure-based allocation (scope) - manages bump internally
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'facet> Partial<'facet, 'static, true> {
    /// Execute a closure with a freshly allocated Partial.
    ///
    /// This is a convenience method that manages the bump allocator internally.
    /// Great for tests and one-off usage when you don't want to manage the bump.
    /// Named "scope" because the bump lives for the scope of the closure.
    ///
    /// The closure can return any error type that `AllocError` can convert into,
    /// making this work seamlessly with `ReflectError`, `DeserializeError`, or
    /// test error types like `IPanic`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let person: Person = Partial::scope::<Person, _, _>(|p| {
    ///     p.set_field("name", "Alice")?
    ///      .set_field("age", 30u32)?
    ///      .build()?
    ///      .materialize()
    /// })?;
    /// ```
    pub fn scope<T, R, E, F>(f: F) -> Result<R, E>
    where
        T: Facet<'facet>,
        E: From<AllocError>,
        F: for<'plan> FnOnce(Partial<'facet, 'plan, true>) -> Result<R, E>,
    {
        let bump = Bump::new();
        let plan = TypePlan::<T>::build(&bump)?;
        let partial = plan.partial()?;
        f(partial)
    }

    /// Execute a closure with a Partial allocated from a shape.
    ///
    /// # Safety
    ///
    /// The shape must be valid and match the expected type structure.
    pub unsafe fn scope_shape<R, E, F>(shape: &'static Shape, f: F) -> Result<R, E>
    where
        E: From<AllocError>,
        F: for<'plan> FnOnce(Partial<'facet, 'plan, true>) -> Result<R, E>,
    {
        let bump = Bump::new();
        let plan = build_core_for_format(&bump, shape, None)?;
        let partial = alloc_impl(plan, shape)?;
        f(partial)
    }
}

impl Partial<'static, 'static, false> {
    /// Execute a closure with a freshly allocated owned Partial.
    ///
    /// Same as `scope` but creates an owned Partial that doesn't borrow from input.
    pub fn scope_owned<T, R, E, F>(f: F) -> Result<R, E>
    where
        T: Facet<'static>,
        E: From<AllocError>,
        F: for<'plan> FnOnce(Partial<'static, 'plan, false>) -> Result<R, E>,
    {
        let bump = Bump::new();
        let plan = TypePlan::<T>::build(&bump)?;
        let partial = plan.partial_owned()?;
        f(partial)
    }

    /// Execute a closure with an owned Partial allocated from a shape.
    ///
    /// # Safety
    ///
    /// The shape must be valid and match the expected type structure.
    pub unsafe fn scope_shape_owned<R, E, F>(shape: &'static Shape, f: F) -> Result<R, E>
    where
        E: From<AllocError>,
        F: for<'plan> FnOnce(Partial<'static, 'plan, false>) -> Result<R, E>,
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
// TypePlan closure-based API (scope)
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<T: Facet<'static> + ?Sized> TypePlan<'static, T> {
    /// Execute a closure with a freshly built TypePlan.
    ///
    /// Bump allocator is managed internally. Useful when you need access to
    /// the TypePlan (e.g., to create multiple Partials from the same plan).
    /// Named "scope" because the bump lives for the scope of the closure.
    ///
    /// The closure can return any error type that `AllocError` can convert into.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let result = TypePlan::<MyStruct>::scope(|plan| {
    ///     let p1 = plan.partial()?;
    ///     // ... use p1
    ///     let p2 = plan.partial()?;
    ///     // ... use p2
    ///     Ok(something)
    /// })?;
    /// ```
    pub fn scope<R, E, F>(f: F) -> Result<R, E>
    where
        E: From<AllocError>,
        F: for<'plan> FnOnce(&TypePlan<'plan, T>) -> Result<R, E>,
    {
        let bump = Bump::new();
        let plan = TypePlan::<T>::build(&bump)?;
        f(&plan)
    }

    /// Execute a closure with format-specific proxy resolution.
    pub fn scope_format<R, E, F>(format_namespace: Option<&'static str>, f: F) -> Result<R, E>
    where
        E: From<AllocError>,
        F: for<'plan> FnOnce(&TypePlan<'plan, T>) -> Result<R, E>,
    {
        let bump = Bump::new();
        let plan = TypePlan::<T>::build_for_format(&bump, format_namespace)?;
        f(&plan)
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Legacy methods for backwards compatibility
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'facet, 'plan> Partial<'facet, 'plan, true> {
    /// Allocates a new [Partial] using a TypePlanCore.
    ///
    /// This is a lower-level method. Prefer `alloc` or `with` for most use cases.
    pub fn alloc_from_core(
        plan: TypePlanCore<'plan>,
        shape: &'static Shape,
    ) -> Result<Self, AllocError> {
        alloc_impl(plan, shape)
    }
}

impl<'plan> Partial<'static, 'plan, false> {
    /// Allocates an owned [Partial] using a TypePlanCore.
    ///
    /// This is a lower-level method. Prefer `alloc_owned` or `with_owned` for most use cases.
    pub fn alloc_from_core_owned(
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
