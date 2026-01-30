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

/// Indexed children for structs and arrays.
/// Each slot is either NOT_STARTED, COMPLETE, or a valid frame index.
pub struct IndexedChildren(Vec<Idx<Frame>>);

impl IndexedChildren {
    /// Create indexed children with the given count, all NOT_STARTED.
    pub fn new(count: usize) -> Self {
        Self(vec![Idx::NOT_STARTED; count])
    }

    /// Mark a child as complete.
    pub fn mark_complete(&mut self, idx: usize) {
        self.0[idx] = Idx::COMPLETE;
    }

    /// Check if all children are complete.
    pub fn all_complete(&self) -> bool {
        self.0.iter().all(|id| id.is_complete())
    }
}

/// Children structure varies by container type.
pub enum Children {
    /// Structs, arrays: indexed by field/element index
    Indexed(IndexedChildren),

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
            children: Children::Indexed(IndexedChildren::new(field_count)),
        }
    }

    /// Assert that the given shape matches this frame's shape.
    pub fn assert_shape(&self, actual: &'static Shape, path: &Path) -> Result<(), ReflectError> {
        if self.shape.is_shape(actual) {
            Ok(())
        } else {
            Err(ReflectError {
                location: ErrorLocation {
                    shape: self.shape,
                    path: path.clone(),
                },
                kind: ReflectErrorKind::ShapeMismatch {
                    expected: self.shape,
                    actual,
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
