//! Operations for partial value construction.

mod builder;

use std::collections::VecDeque;
use std::marker::PhantomData;

use facet_core::{Facet, PtrConst, PtrMut, Shape};
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
    Set { dst: Path, src: Source<'a> },
    /// Push an element onto the current list frame.
    Push { src: Source<'a> },
    /// Insert a key-value pair into the current map frame.
    Insert { key: Imm<'a>, value: Source<'a> },
    /// End the current frame and pop back to parent.
    End,
}

/// How to fill a value.
pub enum Source<'a> {
    /// Move a complete value from ptr into destination.
    Imm(Imm<'a>),
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
/// This is enforced at construction time via [`Imm::from_ref`] (safe) or
/// [`Imm::new`] (unsafe).
pub struct Imm<'a> {
    ptr: PtrMut,
    shape: &'static Shape,
    _marker: PhantomData<&'a ()>,
}

impl<'a> Imm<'a> {
    /// Create an Imm from a mutable reference to a value.
    ///
    /// This is the safe way to create an Imm - the lifetime ensures the
    /// source value remains valid until `apply_batch()` is called.
    #[inline]
    pub fn from_ref<'facet, T: Facet<'facet>>(value: &'a mut T) -> Self {
        Self {
            ptr: PtrMut::new(value),
            shape: T::SHAPE,
            _marker: PhantomData,
        }
    }

    /// Create an Imm from a raw pointer and shape.
    ///
    /// # Safety
    ///
    /// - `ptr` must point to a valid, initialized value whose type matches `shape`
    /// - `ptr` must remain valid for lifetime `'a`
    #[inline]
    pub unsafe fn new(ptr: PtrMut, shape: &'static Shape) -> Self {
        Self {
            ptr,
            shape,
            _marker: PhantomData,
        }
    }

    /// Get the pointer to the source value (as const for reading).
    #[inline]
    pub fn ptr(&self) -> PtrConst {
        self.ptr.as_const()
    }

    /// Get the mutable pointer to the source value.
    #[inline]
    pub fn ptr_mut(&self) -> PtrMut {
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

/// A batch of operations to apply to a [`Partial`](crate::Partial).
///
/// Operations are stored in a `VecDeque` and consumed (popped) from the front
/// as they are processed by [`Partial::apply_batch`](crate::Partial::apply_batch).
///
/// After `apply_batch` returns:
/// - Consumed ops have been removed from the batch (caller must forget their source values)
/// - Remaining ops in the batch were NOT consumed (caller should drop them normally)
pub struct OpBatch<'a> {
    ops: VecDeque<Op<'a>>,
}

impl<'a> OpBatch<'a> {
    /// Create a new empty batch.
    pub fn new() -> Self {
        Self {
            ops: VecDeque::new(),
        }
    }

    /// Create a batch with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            ops: VecDeque::with_capacity(capacity),
        }
    }

    /// Add an operation to the back of the batch.
    pub fn push(&mut self, op: Op<'a>) {
        self.ops.push_back(op);
    }

    /// Pop an operation from the front of the batch.
    pub fn pop(&mut self) -> Option<Op<'a>> {
        self.ops.pop_front()
    }

    /// Push an operation back to the front of the batch.
    pub fn push_front(&mut self, op: Op<'a>) {
        self.ops.push_front(op);
    }

    /// Get the number of operations in the batch.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Returns true if the batch is empty.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

impl Default for OpBatch<'_> {
    fn default() -> Self {
        Self::new()
    }
}
