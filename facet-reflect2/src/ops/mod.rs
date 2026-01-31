//! Operations for partial value construction.

mod builder;

use std::cell::Cell;
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
/// This holds a `PtrMut` because dropping the value (if unconsumed) requires
/// mutable access - `Drop::drop` takes `&mut self`.
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
    /// source value remains valid until `apply()` is called.
    ///
    /// # Ownership
    ///
    /// After `apply()` is called (whether it succeeds or fails), the value's bytes
    /// may have been copied and the caller must not drop the source value. Use
    /// `mem::forget` on the source immediately after calling `apply()`.
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
    /// - After `apply()` returns successfully, the value has been moved and the
    ///   caller must not drop or use the source value (e.g., use `mem::forget`)
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

/// A batch of operations with ownership tracking.
///
/// When you create an `OpBatch`, ownership of all `Imm` values transfers to the batch.
/// The caller must `mem::forget` all source values immediately after adding them.
///
/// When `apply()` processes ops, it marks each one as consumed. On drop, the batch
/// drops any unconsumed `Imm` values (those that were never copied into the partial).
pub struct OpBatch<'a> {
    ops: Vec<Op<'a>>,
    /// Index of the first unconsumed op. Everything before this was consumed.
    /// Use Cell for interior mutability so apply() can update it.
    consumed_up_to: Cell<usize>,
}

impl<'a> OpBatch<'a> {
    /// Create a new empty batch.
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            consumed_up_to: Cell::new(0),
        }
    }

    /// Create a batch with the given capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            ops: Vec::with_capacity(capacity),
            consumed_up_to: Cell::new(0),
        }
    }

    /// Add an operation to the batch.
    pub fn push(&mut self, op: Op<'a>) {
        self.ops.push(op);
    }

    /// Get the operations as a slice.
    pub fn ops(&self) -> &[Op<'a>] {
        &self.ops
    }

    /// Mark that ops up to (but not including) `index` have been consumed.
    /// Called by `Partial::apply()` as it processes each op.
    pub fn mark_consumed_up_to(&self, index: usize) {
        self.consumed_up_to.set(index);
    }

    /// Get the current consumed index.
    pub fn consumed_up_to(&self) -> usize {
        self.consumed_up_to.get()
    }
}

impl Default for OpBatch<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for OpBatch<'_> {
    fn drop(&mut self) {
        // Drop any Imm values that were NOT consumed (from consumed_up_to onwards)
        let start = self.consumed_up_to.get();
        for op in &self.ops[start..] {
            // Drop Imm values in this op
            match op {
                Op::Set {
                    src: Source::Imm(imm),
                    ..
                } => {
                    // SAFETY: This Imm was never copied, so we own it and must drop it
                    unsafe {
                        imm.shape.call_drop_in_place(imm.ptr);
                    }
                }
                Op::Push {
                    src: Source::Imm(imm),
                } => unsafe {
                    imm.shape.call_drop_in_place(imm.ptr);
                },
                Op::Insert { key, value } => {
                    // Always drop the key
                    unsafe {
                        key.shape.call_drop_in_place(key.ptr);
                    }
                    // Drop value if it's an Imm
                    if let Source::Imm(imm) = value {
                        unsafe {
                            imm.shape.call_drop_in_place(imm.ptr);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
