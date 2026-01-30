//! Frame for tracking partial value construction.

use crate::arena::Idx;
use crate::errors::{ErrorLocation, ReflectError, ReflectErrorKind};
use crate::ops::Path;
use facet_core::{PtrConst, PtrUninit, Shape};

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct FrameFlags: u8 {
        /// The value is initialized (for scalars)
        const INIT = 1 << 0;
        /// This frame owns its allocation
        const OWNS_ALLOC = 1 << 1;
    }
}

/// Children structure varies by container type.
/// Each child slot is either NOT_STARTED, COMPLETE, or a valid frame index.
pub enum Children {
    /// Structs, arrays: indexed by field/element index
    Indexed(Vec<Idx<Frame>>),

    /// Scalars: no children
    None,
}

/// A frame tracking construction of a single value.
pub struct Frame {
    /// Pointer to the memory being written.
    pub data: PtrUninit,

    /// Shape (type metadata) of the value.
    pub shape: &'static Shape,

    /// State flags.
    pub flags: FrameFlags,

    /// Children tracking (for compound types).
    pub children: Children,
}

impl Frame {
    pub fn new(data: PtrUninit, shape: &'static Shape) -> Self {
        Frame {
            data,
            shape,
            flags: FrameFlags::empty(),
            children: Children::None,
        }
    }

    /// Create a frame for a struct with the given number of fields.
    pub fn new_struct(data: PtrUninit, shape: &'static Shape, field_count: usize) -> Self {
        Frame {
            data,
            shape,
            flags: FrameFlags::empty(),
            children: Children::Indexed(vec![Idx::NOT_STARTED; field_count]),
        }
    }

    /// Mark a child as complete.
    pub fn mark_child_complete(&mut self, idx: usize) {
        match &mut self.children {
            Children::Indexed(slots) => {
                slots[idx] = Idx::COMPLETE;
            }
            Children::None => panic!("cannot mark child on scalar"),
        }
    }

    /// Check if all children are complete.
    pub fn all_children_complete(&self) -> bool {
        match &self.children {
            Children::Indexed(slots) => slots.iter().all(|id| id.is_complete()),
            Children::None => true,
        }
    }

    /// Assert that the given shape matches this frame's shape.
    ///
    /// Returns an error with shape mismatch details if they don't match.
    pub fn assert_shape(&self, shape: &'static Shape, path: &Path) -> Result<(), ReflectError> {
        if self.shape.is_shape(shape) {
            Ok(())
        } else {
            Err(ReflectError {
                location: ErrorLocation {
                    shape: self.shape,
                    path: path.clone(),
                },
                kind: ReflectErrorKind::ShapeMismatch {
                    expected: self.shape,
                    actual: shape,
                },
            })
        }
    }

    /// Drop any initialized value, returning frame to uninitialized state.
    ///
    /// This is idempotent - calling on an uninitialized frame is a no-op.
    pub fn uninit(&mut self) {
        if self.flags.contains(FrameFlags::INIT) {
            // SAFETY: INIT flag means the value is fully initialized
            unsafe {
                self.shape.call_drop_in_place(self.data.assume_init());
            }
            self.flags.remove(FrameFlags::INIT);
        }
    }

    /// Copy a value into this frame, marking it as initialized.
    ///
    /// # Panics
    ///
    /// Panics if the frame is already initialized. Call [`uninit()`](Self::uninit) first.
    ///
    /// # Safety
    ///
    /// - `src` must point to a valid, initialized value matching `shape`
    /// - `shape` must match `self.shape`
    pub unsafe fn copy_from(&mut self, src: PtrConst, shape: &'static Shape) {
        assert!(
            !self.flags.contains(FrameFlags::INIT),
            "frame already initialized - call uninit() first"
        );
        debug_assert!(self.shape.is_shape(shape), "shape mismatch");

        // SAFETY: caller guarantees src points to valid data matching shape,
        // and shape matches self.shape (debug_assert above)
        unsafe {
            self.data.copy_from(src, self.shape).unwrap();
        }
        self.flags |= FrameFlags::INIT;
    }
}
