//! Runtime ownership helpers for typed-memory interpreters.
//!
//! This module stays deliberately below any format, schema, or reflection
//! model. It owns raw temporary storage and adoption state; callers decide what
//! values live there and which thunks move those values into final handles.

use std::alloc::{self, Layout};
use std::error::Error;
use std::fmt;

/// Allocation/layout failures for raw typed-memory runtime buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawAllocError {
    /// `size` and `align` do not form a valid Rust allocation layout.
    InvalidLayout { size: usize, align: usize },
    /// `count * stride` overflowed while sizing an array buffer.
    SizeOverflow { count: usize, stride: usize },
}

impl fmt::Display for RawAllocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::InvalidLayout { size, align } => {
                write!(f, "invalid raw memory layout: size={size}, align={align}")
            }
            Self::SizeOverflow { count, stride } => {
                write!(
                    f,
                    "raw array buffer size overflow: count={count}, stride={stride}"
                )
            }
        }
    }
}

impl Error for RawAllocError {}

/// Engine-owned raw scratch storage for one value.
///
/// Dropping a `RawScratch` frees the allocation without dropping the value. This
/// is the correct behavior after an interpreter moves the initialized value into
/// a caller handle through an init/push/insert thunk.
#[derive(Debug)]
pub struct RawScratch {
    ptr: *mut u8,
    layout: Option<Layout>,
}

impl RawScratch {
    /// Allocate scratch storage for a value with `size` and `align`.
    pub fn new(size: usize, align: usize) -> Result<Self, RawAllocError> {
        let layout = Layout::from_size_align(size, align)
            .map_err(|_| RawAllocError::InvalidLayout { size, align })?;
        Ok(Self::from_layout(layout))
    }

    /// Allocate scratch storage for a value with an already-validated layout.
    #[must_use]
    pub fn from_layout(layout: Layout) -> Self {
        let ptr = allocate_or_dangling(layout);
        let layout = (layout.size() != 0).then_some(layout);
        Self { ptr, layout }
    }

    /// The raw storage pointer.
    #[must_use]
    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Free the scratch storage without dropping any value that may have lived
    /// in it.
    pub fn dealloc_uninit(&mut self) {
        if let Some(layout) = self.layout.take() {
            unsafe { alloc::dealloc(self.ptr, layout) };
        }
    }
}

impl Drop for RawScratch {
    fn drop(&mut self) {
        self.dealloc_uninit();
    }
}

/// Engine-owned raw contiguous storage for a sequence of elements.
///
/// Dropping frees only the raw allocation. Call [`adopt`](Self::adopt) after a
/// sequence handle takes ownership of the buffer.
#[derive(Debug)]
pub struct RawArrayBuffer {
    ptr: *mut u8,
    cap: usize,
    stride: usize,
    layout: Option<Layout>,
}

impl RawArrayBuffer {
    /// Allocate storage for `count` elements with `stride` bytes and
    /// `elem_align` alignment.
    pub fn new(count: usize, stride: usize, elem_align: usize) -> Result<Self, RawAllocError> {
        let layout = if count == 0 || stride == 0 {
            Layout::from_size_align(0, elem_align).map_err(|_| RawAllocError::InvalidLayout {
                size: 0,
                align: elem_align,
            })?
        } else {
            let size = count
                .checked_mul(stride)
                .ok_or(RawAllocError::SizeOverflow { count, stride })?;
            Layout::from_size_align(size, elem_align).map_err(|_| RawAllocError::InvalidLayout {
                size,
                align: elem_align,
            })?
        };

        let ptr = allocate_or_dangling(layout);
        let layout = (layout.size() != 0).then_some(layout);
        Ok(Self {
            ptr,
            cap: count,
            stride,
            layout,
        })
    }

    /// The start of the raw element storage.
    #[must_use]
    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// The number of elements this buffer can hold.
    #[must_use]
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// The byte stride between elements.
    #[must_use]
    pub fn stride(&self) -> usize {
        self.stride
    }

    /// Return the element slot for `index` when it is within capacity.
    #[must_use]
    pub fn slot(&self, index: usize) -> Option<*mut u8> {
        if index >= self.cap {
            return None;
        }
        Some(unsafe { self.slot_unchecked(index) })
    }

    /// Return the element slot for `index` without bounds checks.
    ///
    /// # Safety
    ///
    /// `index` must be less than [`cap`](Self::cap). If `stride` is non-zero,
    /// the allocation created by [`new`](Self::new) guarantees the offset cannot
    /// overflow for in-bounds indices.
    pub unsafe fn slot_unchecked(&self, index: usize) -> *mut u8 {
        unsafe { self.ptr.add(index * self.stride) }
    }

    /// Mark the allocation as adopted by the caller.
    pub fn adopt(&mut self) {
        self.layout = None;
    }
}

impl Drop for RawArrayBuffer {
    fn drop(&mut self) {
        if let Some(layout) = self.layout.take() {
            unsafe { alloc::dealloc(self.ptr, layout) };
        }
    }
}

/// Caller-supplied drop hook for an initialized handle/value.
pub type DropThunk = unsafe fn(ctx: *const (), ptr: *mut u8);

/// Guard for an initialized handle that must be dropped unless disarmed.
#[derive(Debug)]
pub struct HandleGuard {
    ptr: *mut u8,
    ctx: *const (),
    drop: DropThunk,
    active: bool,
}

impl HandleGuard {
    /// Track `ptr` as initialized and owned by this guard.
    #[must_use]
    pub fn new(ptr: *mut u8, ctx: *const (), drop: DropThunk) -> Self {
        Self {
            ptr,
            ctx,
            drop,
            active: true,
        }
    }

    /// The initialized handle/value pointer.
    #[must_use]
    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Prevent this guard from dropping the handle/value.
    pub fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for HandleGuard {
    fn drop(&mut self) {
        if self.active {
            unsafe { (self.drop)(self.ctx, self.ptr) };
        }
    }
}

/// Bookkeeping for initialized child slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializedLedger<Mark> {
    marks: Vec<Option<Mark>>,
}

impl<Mark> InitializedLedger<Mark> {
    /// Create a ledger with `len` uninitialized slots.
    #[must_use]
    pub fn new(len: usize) -> Self {
        Self {
            marks: (0..len).map(|_| None).collect(),
        }
    }

    /// Return whether `index` is initialized.
    #[must_use]
    pub fn is_initialized(&self, index: usize) -> bool {
        self.marks[index].is_some()
    }

    /// Return the mark for `index` when initialized.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Mark> {
        self.marks[index].as_ref()
    }

    /// Mark `index` initialized.
    pub fn mark(&mut self, index: usize, mark: Mark) {
        self.marks[index] = Some(mark);
    }

    /// Initialized slots in reverse index order.
    pub fn iter_initialized_rev(&self) -> impl Iterator<Item = (usize, &Mark)> {
        self.marks
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(index, mark)| mark.as_ref().map(|mark| (index, mark)))
    }
}

/// Neutral "initialize handle from scratch value" target.
#[derive(Clone, Copy, Debug)]
pub struct InitTarget {
    /// Opaque per-type context.
    pub ctx: *const (),
    /// Destination handle to initialize.
    pub handle: *mut u8,
    /// Move `*value` into `handle`.
    pub init: unsafe extern "C" fn(ctx: *const (), handle: *mut u8, value: *mut u8),
}

impl InitTarget {
    /// Initialize the destination handle by moving from `value`.
    ///
    /// # Safety
    ///
    /// `value` must point to an initialized value of the type expected by this
    /// target, and `handle` must point to uninitialized storage for the handle.
    pub unsafe fn initialize(self, value: *mut u8) {
        unsafe { (self.init)(self.ctx, self.handle, value) };
    }
}

fn allocate_or_dangling(layout: Layout) -> *mut u8 {
    if layout.size() == 0 {
        layout.align() as *mut u8
    } else {
        let ptr = unsafe { alloc::alloc(layout) };
        if ptr.is_null() {
            alloc::handle_alloc_error(layout);
        }
        ptr
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    unsafe fn count_drop(ctx: *const (), _ptr: *mut u8) {
        let drops = unsafe { &*(ctx as *const AtomicUsize) };
        drops.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn raw_scratch_handles_zero_sized_storage() {
        let mut scratch = RawScratch::new(0, 8).unwrap();
        assert_eq!(scratch.ptr() as usize, 8);
        scratch.dealloc_uninit();
        scratch.dealloc_uninit();
    }

    #[test]
    fn raw_array_buffer_checks_slots() {
        let buffer = RawArrayBuffer::new(3, 4, 4).unwrap();
        assert_eq!(buffer.cap(), 3);
        assert_eq!(buffer.stride(), 4);
        assert!(buffer.slot(3).is_none());
        let base = buffer.ptr() as usize;
        assert_eq!(buffer.slot(2).unwrap() as usize, base + 8);
    }

    #[test]
    fn handle_guard_drops_unless_disarmed() {
        let drops = AtomicUsize::new(0);
        let ctx = &drops as *const AtomicUsize as *const ();

        {
            let value = 0u8;
            let _guard = HandleGuard::new(&value as *const u8 as *mut u8, ctx, count_drop);
        }
        assert_eq!(drops.load(Ordering::SeqCst), 1);

        {
            let value = 0u8;
            let mut guard = HandleGuard::new(&value as *const u8 as *mut u8, ctx, count_drop);
            guard.disarm();
        }
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn initialized_ledger_reports_reverse_order() {
        let mut ledger = InitializedLedger::new(4);
        assert!(!ledger.is_initialized(2));
        ledger.mark(1, "one");
        ledger.mark(3, "three");

        assert_eq!(ledger.get(1), Some(&"one"));
        let initialized = ledger
            .iter_initialized_rev()
            .map(|(index, mark)| (index, *mark))
            .collect::<Vec<_>>();
        assert_eq!(initialized, vec![(3, "three"), (1, "one")]);
    }
}
