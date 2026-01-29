use super::*;
use crate::typeplan::{TypePlan, TypePlanCore, TypePlanNode};
use crate::{AllocError, AllocatedShape, partial::arena::Idx};
use ::alloc::collections::BTreeMap;
use ::alloc::sync::Arc;
use ::alloc::vec;
use core::marker::PhantomData;

impl Partial<'static, true> {
    /// Create a new borrowing Partial for the given type.
    ///
    /// This allocates memory for a value of type `T` and returns a `Partial`
    /// that can be used to initialize it incrementally.
    ///
    /// Use `.of::<T>().scope(...)` for closure-based allocation with better ergonomics.
    #[inline]
    pub fn alloc<T: Facet<'static>>() -> Result<Self, AllocError> {
        // This is a convenience method that builds the plan internally.
        // For repeated deserialization, build the plan once and call partial() directly.
        // Note: This leaks the TypePlan, so it's only suitable for one-off usage.
        let plan = Box::leak(Box::new(TypePlan::<T>::build()?));
        plan.partial()
    }
}

impl Partial<'static, false> {
    /// Create a new owned Partial for the given type.
    ///
    /// This allocates memory for a value of type `T` and returns a `Partial`
    /// that can be used to initialize it incrementally.
    ///
    /// Use `.of_owned::<T>().scope(...)` for closure-based allocation with better ergonomics.
    #[inline]
    pub fn alloc_owned<T: Facet<'static>>() -> Result<Self, AllocError> {
        // This is a convenience method that builds the plan internally.
        // For repeated deserialization, build the plan once and call partial_owned() directly.
        // Note: This leaks the TypePlan, so it's only suitable for one-off usage.
        let plan = Box::leak(Box::new(TypePlan::<T>::build()?));
        plan.partial_owned()
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Partial::from_raw - direct initialization from external memory
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Creates a new Partial pointing to caller-provided memory.
    ///
    /// This is a low-level API that lets the caller:
    /// - Control where the value is allocated (stack, existing heap allocation, etc.)
    /// - Avoid the heap allocation that [Partial::alloc] does
    /// - Use MaybeUninit on the stack for the final value
    ///
    /// # Safety
    ///
    /// The caller MUST ensure:
    /// - `data` points to properly aligned, writable memory of at least `shape.layout.size()` bytes
    /// - The memory remains valid for the lifetime of this Partial and any value built from it
    /// - The memory is not aliased by any other mutable references while the Partial exists
    /// - If the Partial is dropped without calling `build()`, the caller handles the uninitialized memory
    ///
    /// # Example: Stack allocation with MaybeUninit
    ///
    /// ```ignore
    /// use std::mem::MaybeUninit;
    ///
    /// // Stack-allocate space for the value
    /// let mut slot = MaybeUninit::<MyStruct>::uninit();
    /// let data = PtrUninit::new(slot.as_mut_ptr().cast());
    ///
    /// // Build the TypePlan (can be reused via Arc)
    /// let plan = TypePlan::<MyStruct>::build()?;
    ///
    /// // Create Partial pointing to our stack memory
    /// let partial = unsafe { Partial::from_raw(data, plan.arc_core(), plan.core().root_id())? };
    ///
    /// // Initialize fields...
    /// let partial = partial.set_field("name", "test")?.set_field("value", 42)?;
    ///
    /// // Build consumes the Partial but does NOT allocate - value is already in `slot`
    /// let heap_value = partial.build()?;
    ///
    /// // SAFETY: We fully initialized the value, so we can assume_init
    /// let value = unsafe { slot.assume_init() };
    /// ```
    ///
    /// # Memory ownership
    ///
    /// The returned Partial has external ownership, which means:
    /// - On successful `build()`: memory ownership transfers to the returned HeapValue
    /// - On drop without `build()`: partially initialized memory is dropped in place,
    ///   but memory is NOT deallocated (caller must handle the memory)
    pub unsafe fn from_raw(
        data: PtrUninit,
        plan: Arc<TypePlanCore>,
        type_plan_id: crate::typeplan::NodeId,
    ) -> Result<Self, AllocError> {
        let shape = plan.node(type_plan_id).shape;
        let layout = shape.layout.sized_layout().map_err(|_| AllocError {
            shape,
            operation: "type is not sized",
        })?;
        let allocated = AllocatedShape::new(shape, layout.size());

        Ok(Self {
            mode: FrameMode::Strict {
                stack: vec![Frame::new(
                    data,
                    allocated,
                    FrameOwnership::External,
                    type_plan_id,
                )],
            },
            state: PartialState::Active,
            root_plan: plan,
            _marker: PhantomData,
        })
    }
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// TypePlan methods for creating Partials
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'a, T: Facet<'a> + ?Sized> TypePlan<T> {
    /// Create a borrowing Partial from this plan.
    ///
    /// The Partial borrows from this TypePlan and can be used to deserialize
    /// values that may borrow from the input.
    #[inline]
    pub fn partial<'facet>(&self) -> Result<Partial<'facet, true>, AllocError> {
        create_partial_internal::<true>(self.core(), self.core().root_id())
    }

    /// Create an owned Partial from this plan.
    ///
    /// The Partial borrows from this TypePlan. The deserialized value will be
    /// fully owned ('static lifetime for borrowed data).
    #[inline]
    pub fn partial_owned(&self) -> Result<Partial<'static, false>, AllocError> {
        create_partial_internal::<false>(self.core(), self.core().root_id())
    }
}

/// Internal helper to create a Partial from plan and node.
fn create_partial_internal<'facet, const BORROW: bool>(
    plan: Arc<TypePlanCore>,
    type_plan_id: Idx<TypePlanNode>,
) -> Result<Partial<'facet, BORROW>, AllocError> {
    let node = plan.node(type_plan_id);
    let shape = node.shape;
    let layout = shape.layout.sized_layout().map_err(|_| AllocError {
        shape,
        operation: "type is not sized",
    })?;

    // Allocate memory for the value
    let data = unsafe { alloc_layout(layout)? };
    let allocated = AllocatedShape::new(shape, layout.size());

    Ok(Partial {
        mode: FrameMode::Strict {
            stack: vec![Frame::new(
                data,
                allocated,
                FrameOwnership::Owned,
                type_plan_id,
            )],
        },
        state: PartialState::Active,
        root_plan: plan,
        _marker: PhantomData,
    })
}

/// Allocate memory with the given layout.
///
/// # Safety
/// The caller must ensure the returned pointer is used correctly.
unsafe fn alloc_layout(layout: core::alloc::Layout) -> Result<PtrUninit, AllocError> {
    use ::alloc::alloc::{alloc, handle_alloc_error};

    if layout.size() == 0 {
        // For ZSTs, use NonNull::dangling() aligned properly
        let ptr: *mut u8 = core::ptr::NonNull::dangling().as_ptr();
        return Ok(PtrUninit::new(ptr));
    }

    // SAFETY: Layout is guaranteed to be valid (non-zero size checked above)
    let ptr = unsafe { alloc(layout) };
    if ptr.is_null() {
        handle_alloc_error(layout);
    }

    Ok(PtrUninit::new(ptr))
}

////////////////////////////////////////////////////////////////////////////////////////////////////
// Deferred mode entry points
////////////////////////////////////////////////////////////////////////////////////////////////////

impl<'facet, const BORROW: bool> Partial<'facet, BORROW> {
    /// Enter deferred mode for the current frame.
    ///
    /// In deferred mode, frames can be stored when popped via `end()` and restored
    /// when re-entered via `begin_field()`. This enables formats like TOML where
    /// keys for the same table may appear non-contiguously.
    ///
    /// # Returns
    ///
    /// A new `Partial` in deferred mode. The original `Partial` is consumed.
    ///
    /// # Deferred mode behavior
    ///
    /// - When `end()` is called on a frame that isn't fully initialized, the frame
    ///   is stored (keyed by its path) instead of being validated.
    /// - When `begin_field()` is called for a stored frame, it's restored instead
    ///   of creating a new one.
    /// - Call `finish_deferred()` when done to validate all stored frames and
    ///   exit deferred mode.
    pub fn enter_deferred(self) -> Self {
        use core::mem::ManuallyDrop;

        let start_depth = self.frames().len();
        // Prevent Drop from running on self, we're taking ownership of its contents
        let mut this = ManuallyDrop::new(self);
        // Take the stack from the mode
        let stack = core::mem::take(this.frames_mut());
        let mut root_plan = Arc::new(TypePlanCore::empty());
        core::mem::swap(&mut root_plan, &mut this.root_plan);

        Self {
            mode: FrameMode::Deferred {
                stack,
                start_depth,
                stored_frames: BTreeMap::new(),
            },
            state: this.state,
            root_plan,
            _marker: PhantomData,
        }
    }
}
