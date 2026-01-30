//! Operations for partial value construction.

mod builder;

use std::marker::PhantomData;

use facet_core::{Facet, PtrConst, Shape};
use smallvec::SmallVec;

/// A path into a nested structure.
#[derive(Clone, Debug, Default)]
pub struct Path(SmallVec<u32, 2>);

impl Path {
    /// Push an index onto the path.
    pub fn push(&mut self, index: u32) {
        self.0.push(index);
    }

    /// Returns true if the path is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the path indices as a slice.
    pub fn as_slice(&self) -> &[u32] {
        &self.0
    }

    /// Returns the number of elements in the path.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// An operation on a Partial.
pub enum Op<'a> {
    /// Set a value at a path relative to the current frame.
    Set { path: Path, source: Source<'a> },
    /// End the current frame and pop back to parent.
    End,
}

/// How to fill a value.
pub enum Source<'a> {
    /// Move a complete value from ptr into destination.
    Move(Move<'a>),
    /// Build incrementally - pushes a frame.
    Build(Build),
    /// Use the type's default value.
    Default,
}

/// A value to move into the destination.
///
/// The lifetime `'a` ensures the source pointer remains valid until the
/// operation is applied.
///
/// # Safety invariant
///
/// `ptr` must point to a valid, initialized value whose type matches `shape`.
/// This is enforced at construction time via [`Move::from_ref`] (safe) or
/// [`Move::new`] (unsafe).
pub struct Move<'a> {
    ptr: PtrConst,
    shape: &'static Shape,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Move<'a> {
    /// Create a Move from a reference to a value.
    ///
    /// This is the safe way to create a Move - the lifetime ensures the
    /// source value remains valid.
    ///
    /// After `apply()` returns successfully, the value's bytes have been copied
    /// and the caller must not drop the source value (use `mem::forget`).
    #[inline]
    pub fn from_ref<'facet, T: Facet<'facet>>(value: &'a T) -> Self {
        Self {
            ptr: PtrConst::new(value),
            shape: T::SHAPE,
            _marker: PhantomData,
        }
    }

    /// Create a Move from a raw pointer and shape.
    ///
    /// # Safety
    ///
    /// - `ptr` must point to a valid, initialized value whose type matches `shape`
    /// - `ptr` must remain valid for lifetime `'a`
    /// - After `apply()` returns successfully, the value has been moved and the
    ///   caller must not drop or use the source value (e.g., use `mem::forget`)
    #[inline]
    pub unsafe fn new(ptr: PtrConst, shape: &'static Shape) -> Self {
        Self {
            ptr,
            shape,
            _marker: PhantomData,
        }
    }

    /// Get the pointer to the source value.
    #[inline]
    pub fn ptr(&self) -> PtrConst {
        self.ptr
    }

    /// Get the shape of the source value.
    #[inline]
    pub fn shape(&self) -> &'static Shape {
        self.shape
    }
}

/// Build a value incrementally.
pub struct Build {
    pub len_hint: Option<usize>,
}
