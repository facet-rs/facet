//! Frame for tracking partial value construction.

use crate::arena::{Arena, Idx};
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

    /// Parent frame (if any) and our index within it.
    pub parent: Option<(Idx<Frame>, u32)>,
}

/// Build the absolute path from root to the given frame by walking up the parent chain.
pub fn absolute_path(arena: &Arena<Frame>, mut idx: Idx<Frame>) -> Path {
    let mut indices = Vec::new();
    while idx.is_valid() {
        let frame = arena.get(idx);
        if let Some((parent_idx, field_idx)) = frame.parent {
            indices.push(field_idx);
            idx = parent_idx;
        } else {
            break;
        }
    }
    indices.reverse();
    let mut path = Path::default();
    for i in indices {
        path.push(i);
    }
    path
}

impl Frame {
    pub fn new(data: PtrUninit, shape: &'static Shape) -> Self {
        Frame {
            data,
            shape,
            flags: FrameFlags::empty(),
            children: Children::None,
            parent: None,
        }
    }

    /// Create a frame for a struct with the given number of fields.
    pub fn new_struct(data: PtrUninit, shape: &'static Shape, field_count: usize) -> Self {
        Frame {
            data,
            shape,
            flags: FrameFlags::empty(),
            children: Children::Indexed(IndexedChildren::new(field_count)),
            parent: None,
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
    /// Returns an error if the frame is already initialized.
    /// Call [`uninit()`](Self::uninit) first to clear it.
    ///
    /// # Safety
    ///
    /// - `src` must point to a valid, initialized value matching `shape`
    /// - `shape` must match `self.shape`
    pub unsafe fn copy_from(
        &mut self,
        src: PtrConst,
        shape: &'static Shape,
    ) -> Result<(), ReflectErrorKind> {
        if self.flags.contains(FrameFlags::INIT) {
            return Err(ReflectErrorKind::AlreadyInitialized);
        }
        debug_assert!(self.shape.is_shape(shape), "shape mismatch");

        // SAFETY: caller guarantees src points to valid data matching shape,
        // and shape matches self.shape (debug_assert above)
        unsafe {
            self.data.copy_from(src, self.shape).unwrap();
        }
        self.flags |= FrameFlags::INIT;
        Ok(())
    }

    /// Deallocate the frame's memory if it owns the allocation.
    ///
    /// This should be called after the value has been moved out or dropped.
    pub fn dealloc_if_owned(self) {
        if self.flags.contains(FrameFlags::OWNS_ALLOC) {
            let layout = self.shape.layout.sized_layout().unwrap();
            if layout.size() > 0 {
                // SAFETY: we allocated this memory with this layout
                unsafe {
                    std::alloc::dealloc(self.data.as_mut_byte_ptr(), layout);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_core::Facet;
    use std::ptr::NonNull;

    fn dummy_frame() -> Frame {
        Frame::new(
            PtrUninit::new(NonNull::<u8>::dangling().as_ptr()),
            u32::SHAPE,
        )
    }

    fn dummy_frame_with_parent(parent: Idx<Frame>, index: u32) -> Frame {
        let mut frame = dummy_frame();
        frame.parent = Some((parent, index));
        frame
    }

    #[test]
    fn absolute_path_root_frame() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());

        let path = absolute_path(&arena, root);
        assert!(path.is_empty());
    }

    #[test]
    fn absolute_path_one_level() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let child = arena.alloc(dummy_frame_with_parent(root, 3));

        let path = absolute_path(&arena, child);
        assert_eq!(path.as_slice(), &[3]);
    }

    #[test]
    fn absolute_path_two_levels() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let child = arena.alloc(dummy_frame_with_parent(root, 1));
        let grandchild = arena.alloc(dummy_frame_with_parent(child, 2));

        let path = absolute_path(&arena, grandchild);
        assert_eq!(path.as_slice(), &[1, 2]);
    }

    #[test]
    fn absolute_path_three_levels() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let a = arena.alloc(dummy_frame_with_parent(root, 0));
        let b = arena.alloc(dummy_frame_with_parent(a, 5));
        let c = arena.alloc(dummy_frame_with_parent(b, 10));

        let path = absolute_path(&arena, c);
        assert_eq!(path.as_slice(), &[0, 5, 10]);
    }

    #[test]
    fn absolute_path_sibling_frames() {
        let mut arena = Arena::new();
        let root = arena.alloc(dummy_frame());
        let child0 = arena.alloc(dummy_frame_with_parent(root, 0));
        let child1 = arena.alloc(dummy_frame_with_parent(root, 1));
        let child2 = arena.alloc(dummy_frame_with_parent(root, 2));

        assert_eq!(absolute_path(&arena, child0).as_slice(), &[0]);
        assert_eq!(absolute_path(&arena, child1).as_slice(), &[1]);
        assert_eq!(absolute_path(&arena, child2).as_slice(), &[2]);
    }

    #[test]
    fn absolute_path_deep_nesting() {
        let mut arena = Arena::new();
        let mut current = arena.alloc(dummy_frame());

        for i in 0..10 {
            current = arena.alloc(dummy_frame_with_parent(current, i));
        }

        let path = absolute_path(&arena, current);
        assert_eq!(path.as_slice(), &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
