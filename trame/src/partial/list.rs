//! List frame handling for Vec and similar types.
//!
//! # State Machine
//!
//! ```text
//!                          alloc_shape / Set dst=[] src=Stage
//!                                       │
//!                                       ▼
//!                               ┌──────────────┐
//!                               │   PENDING    │
//!                               │              │
//!                               │ • INIT=false │
//!                               │ • Vec not    │
//!                               │   created    │
//!                               └──────┬───────┘
//!                                      │
//!             ┌────────────────────────┼────────────────────────┐
//!             │                        │                        │
//!             │ Set dst=[]             │ Append                 │ Set dst=[]
//!             │ src=Imm/Default        │ (creates Vec)          │ src=Imm/Default
//!             │                        ▼                        │
//!             │                ┌──────────────┐                 │
//!             │                │   BUILDING   │                 │
//!             │                │              │                 │
//!             │                │ • INIT=true  │◄────────┐       │
//!             │                │ • Vec exists │         │       │
//!             │                │ • staged_len │    Append       │
//!             │                └──────┬───────┘         │       │
//!             │                       │                 │       │
//!             │                       │ End             │       │
//!             │                       ▼                 │       │
//!             │                ┌──────────────┐         │       │
//!             └───────────────►│   COMPLETE   │◄────────┴───────┘
//!                              │              │
//!              Set dst=[]      │ • INIT=true  │
//!              src=Stage       │ • Vec exists │
//!             ┌────────────────┤ • staged=0   │
//!             │                └──────┬───────┘
//!             ▼                       │
//!        (to PENDING)                 │
//!                                     │ Append
//!                                     ▼
//!                                   ERROR
//!                         "Cannot append to complete list"
//! ```
//!
//! # Direct-Fill Only
//!
//! This module only supports direct-fill lists (Vec, etc.) where we can:
//! 1. Get a raw pointer to the buffer (`as_mut_ptr_typed`)
//! 2. Reserve capacity (`reserve`)
//! 3. Query capacity (`capacity`)
//! 4. Commit elements by setting length (`set_len`)
//!
//! List types that don't support these operations (e.g., VecDeque) will error
//! when attempting incremental building.

use crate::arena::Idx;
use crate::errors::{ReflectError, ReflectErrorKind};
use crate::frame::{Frame, FrameFlags, FrameKind, ListFrame, ParentLink};
use crate::ops::Source;
use crate::shape_desc::ShapeDesc;
use facet_core::{ListDef, PtrConst, PtrMut, PtrUninit, Shape};

use super::Partial;

// ============================================================================
// State
// ============================================================================

/// The build state of a list frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListState {
    /// No Vec created yet. Waiting for first operation.
    Pending,
    /// Vec exists, actively accepting elements.
    Building,
    /// Vec is finalized (via End or set via Imm/Default).
    Complete,
}

impl ListFrame {
    /// Get the current build state of this list frame.
    pub fn state(&self) -> ListState {
        if !self.initialized {
            ListState::Pending
        } else if self.finalized {
            ListState::Complete
        } else {
            ListState::Building
        }
    }
}

// ============================================================================
// Direct-fill support
// ============================================================================

/// Direct-fill operations for lists.
///
/// Provides direct access to the Vec's buffer for zero-copy element insertion.
pub(crate) struct DirectFill {
    as_mut_ptr_typed: facet_core::ListAsMutPtrTypedFn,
    reserve: facet_core::ListReserveFn,
    capacity: facet_core::ListCapacityFn,
    set_len: facet_core::ListSetLenFn,
    init_in_place: facet_core::ListInitInPlaceWithCapacityFn,
    pub element_shape: &'static Shape,
    pub element_size: usize,
}

impl DirectFill {
    /// Get direct-fill operations from a ListDef.
    ///
    /// Returns None if the list type doesn't support direct-fill.
    pub fn new(def: &ListDef) -> Option<Self> {
        let element_shape = def.t;
        let element_size = element_shape.layout.sized_layout().ok()?.size();

        Some(Self {
            as_mut_ptr_typed: def.as_mut_ptr_typed()?,
            reserve: def.reserve()?,
            capacity: def.capacity()?,
            set_len: def.set_len()?,
            init_in_place: def.init_in_place_with_capacity()?,
            element_shape,
            element_size,
        })
    }

    /// Initialize the Vec with given capacity.
    ///
    /// # Safety
    /// - `data` must point to uninitialized memory for the list
    pub unsafe fn init(&self, data: PtrUninit, capacity: usize) -> PtrMut {
        // SAFETY: caller guarantees data points to uninitialized list memory
        unsafe { (self.init_in_place)(data, capacity) }
    }

    /// Get current capacity.
    ///
    /// # Safety
    /// - `list_ptr` must point to an initialized list
    pub unsafe fn capacity(&self, list_ptr: PtrConst) -> usize {
        // SAFETY: caller guarantees list_ptr points to initialized list
        unsafe { (self.capacity)(list_ptr) }
    }

    /// Get pointer to the slot at index `idx` in the list's buffer.
    ///
    /// # Safety
    /// - `list_ptr` must point to an initialized list
    /// - `idx` must be less than capacity
    pub unsafe fn slot_ptr(&self, list_ptr: PtrMut, idx: usize) -> PtrUninit {
        // SAFETY: caller guarantees list_ptr is initialized and idx < capacity
        unsafe {
            let buffer = (self.as_mut_ptr_typed)(list_ptr);
            let offset = idx * self.element_size;
            PtrUninit::new(buffer.add(offset))
        }
    }

    /// Ensure the list has capacity for at least one more element.
    ///
    /// # Safety
    /// - `list_ptr` must point to an initialized list
    pub unsafe fn ensure_capacity(&self, list_ptr: PtrMut, list_frame: &mut ListFrame) {
        let current_len = list_frame.len + list_frame.staged_len;
        if current_len >= list_frame.cached_capacity {
            // Vec::reserve(n) ensures capacity >= vec.len() + n.
            // But vec.len() only knows about committed elements (list_frame.len),
            // not staged elements. We need to reserve enough for staged + 1.
            let min_reserve = list_frame.staged_len + 1;
            let additional = if list_frame.cached_capacity == 0 {
                min_reserve.max(4)
            } else {
                min_reserve.max(list_frame.cached_capacity)
            };
            // SAFETY: caller guarantees list_ptr points to initialized list
            unsafe {
                (self.reserve)(list_ptr, additional);
                list_frame.cached_capacity = (self.capacity)(list_ptr.as_const());
            }
        }
    }

    /// Commit staged elements by setting the Vec's length.
    ///
    /// # Safety
    /// - `list_ptr` must point to an initialized list
    /// - `new_len` elements must be initialized in the buffer
    pub unsafe fn commit(&self, list_ptr: PtrMut, new_len: usize) {
        // SAFETY: caller guarantees list_ptr is initialized and new_len elements are valid
        unsafe { (self.set_len)(list_ptr, new_len) }
    }
}

// ============================================================================
// Operations
// ============================================================================

impl<'facet> Partial<'facet> {
    /// Get DirectFill for the current list frame, or error if not supported.
    fn list_direct_fill(&self) -> Result<DirectFill, ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::List(list_frame) = &frame.kind else {
            unreachable!()
        };
        DirectFill::new(&list_frame.def).ok_or_else(|| {
            self.error(ReflectErrorKind::ListDoesNotSupportDirectFill { shape: frame.shape })
        })
    }

    /// Initialize a list frame's Vec if not already done.
    ///
    /// Transitions: PENDING → BUILDING
    pub(crate) fn list_ensure_initialized(&mut self, capacity: usize) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::List(list_frame) = &frame.kind else {
            return Ok(());
        };

        if list_frame.initialized {
            return Ok(());
        }

        let df = self.list_direct_fill()?;

        // SAFETY: frame.data points to uninitialized list memory
        let frame = self.arena.get(self.current);
        let list_ptr = unsafe { df.init(frame.data, capacity) };
        let cached_capacity = unsafe { df.capacity(list_ptr.as_const()) };

        // Update frame state
        let frame = self.arena.get_mut(self.current);
        if let FrameKind::List(l) = &mut frame.kind {
            l.initialized = true;
            l.cached_capacity = cached_capacity;
        }
        frame.flags |= FrameFlags::INIT;

        Ok(())
    }

    /// Commit staged elements by calling set_len on the Vec.
    ///
    /// Transitions: BUILDING → COMPLETE
    pub(crate) fn list_finalize(&mut self) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::List(ref list_frame) = frame.kind else {
            return Ok(());
        };

        // Commit staged elements if any
        if list_frame.staged_len > 0 {
            let df = self.list_direct_fill()?;
            let list_ptr = unsafe { frame.data.assume_init() };
            let new_len = list_frame.len + list_frame.staged_len;

            // SAFETY: list_ptr is initialized, new_len elements are initialized
            unsafe { df.commit(list_ptr, new_len) };

            let frame = self.arena.get_mut(self.current);
            if let FrameKind::List(ref mut l) = frame.kind {
                l.len = new_len;
                l.staged_len = 0;
                l.finalized = true;
            }
        } else {
            // No staged elements, just mark as finalized
            let frame = self.arena.get_mut(self.current);
            if let FrameKind::List(ref mut l) = frame.kind {
                l.finalized = true;
            }
        }

        Ok(())
    }

    /// Handle Append operation on a list.
    ///
    /// Transitions: PENDING → BUILDING, BUILDING → BUILDING
    /// Error from: COMPLETE
    pub(crate) fn list_append(&mut self, source: &Source<'_>) -> Result<(), ReflectError> {
        let frame = self.arena.get(self.current);
        let FrameKind::List(list_frame) = &frame.kind else {
            unreachable!()
        };

        // Check state - cannot append to complete list
        let state = list_frame.state();
        if state == ListState::Complete {
            return Err(self.error(ReflectErrorKind::CannotAppendToCompleteCollection));
        }

        // Initialize if pending (PENDING → BUILDING transition)
        if state == ListState::Pending {
            self.list_ensure_initialized(0)?;
        }

        // Get direct-fill operations and list pointer (frame may have changed)
        let df = self.list_direct_fill()?;
        let frame = self.arena.get(self.current);
        let list_ptr = unsafe { frame.data.assume_init() };

        match source {
            Source::Imm(mov) => {
                if !df.element_shape.is_shape(mov.shape()) {
                    return Err(self.error(ReflectErrorKind::ShapeMismatch {
                        expected: ShapeDesc::Static(df.element_shape),
                        actual: ShapeDesc::Static(mov.shape()),
                    }));
                }
                self.list_append_imm(list_ptr, &df, mov.ptr())?;
            }
            Source::Stage(_capacity) => {
                self.list_append_stage(list_ptr, &df)?;
            }
            Source::Default => {
                self.list_append_default(list_ptr, &df)?;
            }
        }

        Ok(())
    }

    /// Handle completion of a ListElement child frame.
    pub(crate) fn list_element_end(&mut self, parent_idx: Idx<Frame>) -> Result<(), ReflectError> {
        // Element is in Vec's buffer (direct-fill), just increment staged_len
        let _ = self.arena.free(self.current);

        let parent = self.arena.get_mut(parent_idx);
        if let FrameKind::List(ref mut l) = parent.kind {
            l.staged_len += 1;
        }

        self.current = parent_idx;
        Ok(())
    }

    /// Update ListFrame state after Set at empty path with Imm/Default.
    pub(crate) fn list_sync_after_set(&mut self) {
        let frame = self.arena.get_mut(self.current);
        if let FrameKind::List(ref mut l) = frame.kind {
            let list_ptr = unsafe { frame.data.assume_init() };
            l.len = unsafe { (l.def.vtable.len)(list_ptr.as_const()) };
            if let Some(cap_fn) = l.def.capacity() {
                l.cached_capacity = unsafe { cap_fn(list_ptr.as_const()) };
            }
            l.initialized = true;
            l.finalized = true; // Whole value set, no more appends allowed
            l.staged_len = 0;
        }
    }

    // ========================================================================
    // Append helpers
    // ========================================================================

    fn list_append_imm(
        &mut self,
        list_ptr: PtrMut,
        df: &DirectFill,
        src: PtrConst,
    ) -> Result<(), ReflectError> {
        let frame = self.arena.get_mut(self.current);
        let FrameKind::List(ref mut list_frame) = frame.kind else {
            unreachable!()
        };

        unsafe { df.ensure_capacity(list_ptr, list_frame) };

        let idx = list_frame.len + list_frame.staged_len;
        let slot = unsafe { df.slot_ptr(list_ptr, idx) };

        unsafe {
            std::ptr::copy_nonoverlapping(
                src.as_byte_ptr(),
                slot.as_mut_byte_ptr(),
                df.element_size,
            );
        }

        list_frame.staged_len += 1;
        Ok(())
    }

    fn list_append_stage(&mut self, list_ptr: PtrMut, df: &DirectFill) -> Result<(), ReflectError> {
        {
            let frame = self.arena.get_mut(self.current);
            let FrameKind::List(ref mut list_frame) = frame.kind else {
                unreachable!()
            };
            unsafe { df.ensure_capacity(list_ptr, list_frame) };
        }

        let frame = self.arena.get(self.current);
        let FrameKind::List(ref list_frame) = frame.kind else {
            unreachable!()
        };
        let idx = list_frame.len + list_frame.staged_len;
        let slot = unsafe { df.slot_ptr(list_ptr, idx) };

        let mut element_frame = Self::create_frame_for_shape(slot, df.element_shape);
        element_frame.parent_link = ParentLink::ListElement {
            parent: self.current,
        };

        let element_idx = self.arena.alloc(element_frame);
        self.current = element_idx;
        Ok(())
    }

    fn list_append_default(
        &mut self,
        list_ptr: PtrMut,
        df: &DirectFill,
    ) -> Result<(), ReflectError> {
        let frame = self.arena.get_mut(self.current);
        let FrameKind::List(ref mut list_frame) = frame.kind else {
            unreachable!()
        };

        unsafe { df.ensure_capacity(list_ptr, list_frame) };

        let idx = list_frame.len + list_frame.staged_len;
        let slot = unsafe { df.slot_ptr(list_ptr, idx) };

        let ok = unsafe { df.element_shape.call_default_in_place(slot) };
        if ok.is_none() {
            return Err(self.error(ReflectErrorKind::NoDefault {
                shape: ShapeDesc::Static(df.element_shape),
            }));
        }

        list_frame.staged_len += 1;
        Ok(())
    }
}
