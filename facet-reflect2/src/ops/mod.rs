//! Operations for partial value construction.

mod builder;

use std::collections::VecDeque;
use std::marker::PhantomData;

use facet_core::{Facet, PtrConst, PtrMut, Shape};
use smallvec::{SmallVec, smallvec};

/// A segment in a path through a nested structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathSegment {
    /// Struct field, tuple element, array index, enum variant.
    Field(u32),
    /// New list/set element or map entry.
    Append,
    /// Jump to root (only valid as first segment).
    Root,
}

/// A path into a nested structure.
#[derive(Clone, Debug, Default)]
pub struct Path(SmallVec<PathSegment, 4>);

impl Path {
    /// Create an empty path.
    pub fn empty() -> Self {
        Self(SmallVec::new())
    }

    /// Create a path with a single field segment.
    pub fn field(n: u32) -> Self {
        Self(smallvec![PathSegment::Field(n)])
    }

    /// Create a path with a single append segment.
    pub fn append() -> Self {
        Self(smallvec![PathSegment::Append])
    }

    /// Create a path starting from root.
    pub fn root() -> Self {
        Self(smallvec![PathSegment::Root])
    }

    /// Create a path from a slice of segments.
    pub fn from_segments(segments: &[PathSegment]) -> Self {
        Self(segments.iter().cloned().collect())
    }

    /// Add a segment to the path.
    pub fn then(mut self, seg: PathSegment) -> Self {
        self.0.push(seg);
        self
    }

    /// Add a field segment to the path.
    pub fn then_field(self, n: u32) -> Self {
        self.then(PathSegment::Field(n))
    }

    /// Add an append segment to the path.
    pub fn then_append(self) -> Self {
        self.then(PathSegment::Append)
    }

    /// Returns the path segments as a slice.
    pub fn segments(&self) -> &[PathSegment] {
        &self.0
    }

    /// Returns true if the path is empty.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the number of segments in the path.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    // --- Deprecated methods for migration ---

    /// Push a field index onto the path (deprecated: use then_field).
    #[deprecated(note = "Use Path::field(n) or path.then_field(n) instead")]
    pub fn push(&mut self, index: u32) {
        self.0.push(PathSegment::Field(index));
    }

    /// Returns the path as field indices.
    /// Panics if any segment is not a Field.
    /// (deprecated: use segments() instead)
    #[deprecated(note = "Use segments() instead")]
    pub fn as_slice(&self) -> Vec<u32> {
        self.0
            .iter()
            .map(|seg| match seg {
                PathSegment::Field(n) => *n,
                _ => panic!("as_slice() requires all segments to be Field"),
            })
            .collect()
    }

    /// Returns the first field index, if the path starts with a Field.
    /// Panics if path is empty or starts with non-Field.
    /// (deprecated: use segments() instead)
    pub fn first_field_index(&self) -> Option<u32> {
        match self.0.first() {
            Some(PathSegment::Field(n)) => Some(*n),
            _ => None,
        }
    }
}

/// An operation on a Partial.
pub enum Op<'a> {
    /// Set a value at a path relative to the current frame.
    Set { dst: Path, src: Source<'a> },
    /// End the current frame and pop back to parent.
    End,
}

/// How to fill a value.
pub enum Source<'a> {
    /// Move a complete value from ptr into destination.
    Imm(Imm<'a>),
    /// Stage for incremental construction - pushes a frame.
    /// Optional capacity hint for collections.
    Stage(Option<usize>),
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
