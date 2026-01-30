//! Frame structure for tracking partial value construction.
//!
//! A frame represents a value being constructed. Frames form a tree mirroring
//! the structure of the target type. Each frame tracks:
//! - The memory location being written to
//! - The shape (type metadata) of the value
//! - Whether this frame owns the allocation
//! - Whether the value is initialized
//! - Child frames for composite types

use crate::arena::FrameId;
use facet_core::{PtrUninit, Shape};
use hashbrown::HashMap;

bitflags::bitflags! {
    /// Flags tracking frame state.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct FrameFlags: u8 {
        /// This frame owns its memory allocation (must dealloc on drop)
        const OWNS_ALLOCATION = 1 << 0;

        /// The value at `data` is initialized (must drop on cleanup)
        const IS_INIT = 1 << 1;
    }
}

/// Type-erased key for map lookups.
///
/// Wraps a pointer to a key value along with its shape, enabling
/// `Hash` and `Eq` implementations via the shape's vtable.
///
/// # Safety
///
/// The pointed-to value must remain valid and unchanged for the lifetime
/// of this key. The shape must match the actual type of the value.
pub struct DynKey {
    /// Pointer to the key value
    pub ptr: PtrUninit,

    /// Shape of the key type (provides hash/eq vtable)
    pub shape: &'static Shape,
}

impl std::hash::Hash for DynKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Safety: caller guarantees ptr is valid and initialized
        unsafe {
            if let Some(hasher) = self.shape.vtable.hash {
                hasher(self.ptr.assume_init(), state);
            } else {
                // No hash function - this shouldn't happen for valid map keys
                panic!("DynKey shape has no hash vtable");
            }
        }
    }
}

impl PartialEq for DynKey {
    fn eq(&self, other: &Self) -> bool {
        // Different shapes can't be equal
        if !std::ptr::eq(self.shape, other.shape) {
            return false;
        }

        // Safety: caller guarantees ptrs are valid and initialized
        unsafe {
            if let Some(eq_fn) = self.shape.vtable.eq {
                eq_fn(self.ptr.assume_init(), other.ptr.assume_init())
            } else {
                // No eq function - this shouldn't happen for valid map keys
                panic!("DynKey shape has no eq vtable");
            }
        }
    }
}

impl Eq for DynKey {}

/// Children structure varies by container type.
///
/// Different composite types need different child tracking strategies:
/// - Structs/arrays use indexed access by field/element position
/// - Enums have at most one variant active
/// - Lists can grow dynamically
/// - Maps need key-based lookup for re-entry
/// - Smart pointers have a single inner value
/// - Scalars have no children
#[derive(Debug)]
pub enum Children {
    /// Structs, arrays: indexed by field/element index.
    ///
    /// Each slot holds:
    /// - `NOT_STARTED`: field not started
    /// - `COMPLETE`: field complete
    /// - Valid index: field in progress
    Indexed(Vec<FrameId>),

    /// Enums: at most one variant active at a time.
    ///
    /// - `None`: no variant selected
    /// - `Some((variant_idx, COMPLETE))`: variant complete
    /// - `Some((variant_idx, frame_id))`: variant in progress
    Variant(Option<(u32, FrameId)>),

    /// Lists: can grow dynamically via Push.
    ///
    /// Elements may be sparse in deferred mode (NOT_STARTED slots).
    List(Vec<FrameId>),

    /// Maps: keyed by actual values for O(1) re-entry.
    ///
    /// The DynKey owns the key allocation. On cleanup we must drop
    /// the key value. On successful insert, we move the key out.
    Map(HashMap<DynKey, FrameId>),

    /// Option inner, smart pointer inner: single child.
    ///
    /// - `NOT_STARTED`: inner not started
    /// - `COMPLETE`: inner complete
    /// - Valid index: inner in progress
    Single(FrameId),

    /// Scalars, sets: no children.
    ///
    /// Sets can't be re-entered so we don't track elements.
    None,
}

impl Default for Children {
    fn default() -> Self {
        Children::None
    }
}

/// A frame tracking construction of a single value.
///
/// Frames form a tree structure where each frame knows its parent
/// and children. The root frame has no parent.
pub struct Frame {
    /// Parent frame, if any. Root frames have `None`.
    pub parent: Option<FrameId>,

    /// Child frames for composite types.
    pub children: Children,

    /// Pointer to the memory being written.
    pub data: PtrUninit,

    /// Shape (type metadata) of the value being constructed.
    pub shape: &'static Shape,

    /// State flags.
    pub flags: FrameFlags,
}

impl Frame {
    /// Create a new frame.
    pub fn new(
        parent: Option<FrameId>,
        data: PtrUninit,
        shape: &'static Shape,
        children: Children,
    ) -> Self {
        Frame {
            parent,
            children,
            data,
            shape,
            flags: FrameFlags::empty(),
        }
    }

    /// Returns true if this frame owns its memory allocation.
    #[inline]
    pub fn owns_allocation(&self) -> bool {
        self.flags.contains(FrameFlags::OWNS_ALLOCATION)
    }

    /// Returns true if the value is initialized.
    #[inline]
    pub fn is_init(&self) -> bool {
        self.flags.contains(FrameFlags::IS_INIT)
    }

    /// Mark the value as initialized.
    #[inline]
    pub fn set_init(&mut self) {
        self.flags.insert(FrameFlags::IS_INIT);
    }

    /// Mark that this frame owns the allocation.
    #[inline]
    pub fn set_owns_allocation(&mut self) {
        self.flags.insert(FrameFlags::OWNS_ALLOCATION);
    }
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("parent", &self.parent)
            .field("children", &self.children)
            .field("shape", &self.shape.id)
            .field("flags", &self.flags)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_core::Facet;

    #[test]
    fn frame_flags() {
        let mut flags = FrameFlags::empty();
        assert!(!flags.contains(FrameFlags::OWNS_ALLOCATION));
        assert!(!flags.contains(FrameFlags::IS_INIT));

        flags.insert(FrameFlags::OWNS_ALLOCATION);
        assert!(flags.contains(FrameFlags::OWNS_ALLOCATION));
        assert!(!flags.contains(FrameFlags::IS_INIT));

        flags.insert(FrameFlags::IS_INIT);
        assert!(flags.contains(FrameFlags::OWNS_ALLOCATION));
        assert!(flags.contains(FrameFlags::IS_INIT));
    }

    #[test]
    fn frame_creation() {
        let data = PtrUninit::dangling::<u32>();
        let shape = <u32 as Facet>::SHAPE;

        let frame = Frame::new(None, data, shape, Children::None);

        assert!(frame.parent.is_none());
        assert!(!frame.owns_allocation());
        assert!(!frame.is_init());
    }

    #[test]
    fn children_variants() {
        // Indexed for structs
        let indexed = Children::Indexed(vec![FrameId::NOT_STARTED; 3]);
        assert!(matches!(indexed, Children::Indexed(ref v) if v.len() == 3));

        // Variant for enums
        let variant = Children::Variant(Some((1, FrameId::COMPLETE)));
        assert!(matches!(variant, Children::Variant(Some((1, id))) if id.is_complete()));

        // Single for Option/Box
        let single = Children::Single(FrameId::NOT_STARTED);
        assert!(matches!(single, Children::Single(id) if id.is_not_started()));
    }
}
