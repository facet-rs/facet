//! Runtime ownership helpers for typed-memory interpreters.
//!
//! This module stays deliberately below any format, schema, or reflection
//! model. It owns raw temporary storage and adoption state; callers decide what
//! values live there and which thunks move those values into final handles.

use std::alloc::{self, Layout};
use std::array;
use std::error::Error;
use std::fmt;
use std::mem::MaybeUninit;

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

/// Reusable scratch storage for one interpreter run.
///
/// A session keeps raw buffers around after a moved-out child value is released,
/// so repeated list elements or option/pointer payloads can reuse memory instead
/// of allocating for every child decode. Dropping the session frees all buffers
/// without dropping values.
#[derive(Debug, Default)]
pub struct ScratchSession {
    buffers: Vec<ScratchBuffer>,
}

#[derive(Debug)]
struct ScratchBuffer {
    ptr: *mut u8,
    layout: Layout,
    active: bool,
}

/// A checked-out scratch slot from [`ScratchSession`].
#[derive(Debug)]
pub struct ScratchSlot {
    index: usize,
    ptr: *mut u8,
}

impl ScratchSession {
    /// Create an empty scratch session.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserve scratch storage for `layout`.
    pub fn reserve(&mut self, layout: Layout) -> ScratchSlot {
        if let Some((index, buffer)) = self
            .buffers
            .iter_mut()
            .enumerate()
            .find(|(_, buffer)| !buffer.active && layout_fits(buffer.layout, layout))
        {
            buffer.active = true;
            return ScratchSlot {
                index,
                ptr: buffer.ptr,
            };
        }

        let ptr = allocate_or_dangling(layout);
        let index = self.buffers.len();
        self.buffers.push(ScratchBuffer {
            ptr,
            layout,
            active: true,
        });
        ScratchSlot { index, ptr }
    }

    /// Release a moved-out scratch slot for reuse.
    pub fn release(&mut self, slot: ScratchSlot) {
        let buffer = &mut self.buffers[slot.index];
        debug_assert!(buffer.active, "scratch slot released twice");
        debug_assert_eq!(buffer.ptr, slot.ptr, "scratch slot pointer mismatch");
        buffer.active = false;
    }
}

impl ScratchSlot {
    /// The raw storage pointer for this slot.
    #[must_use]
    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }
}

impl Drop for ScratchSession {
    fn drop(&mut self) {
        for buffer in self.buffers.drain(..) {
            if buffer.layout.size() != 0 {
                unsafe { alloc::dealloc(buffer.ptr, buffer.layout) };
            }
        }
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

/// Growable engine-owned raw storage for directly filling a sequence.
///
/// Callers reserve the next slot, initialize it, then mark it initialized. If
/// decoding fails before adoption, dropping the builder calls the supplied drop
/// thunk for initialized elements and frees the raw allocation. After a list
/// handle adopts the buffer through a `from_raw_parts`-style thunk, call
/// [`adopt`](Self::adopt) so the builder does not touch the values or buffer.
#[derive(Debug)]
pub struct RawArrayBuilder {
    ptr: *mut u8,
    len: usize,
    cap: usize,
    elem_layout: Layout,
    allocation: Option<Layout>,
    drop_ctx: *const (),
    drop: DropThunk,
}

impl RawArrayBuilder {
    /// Create an empty builder for elements with `elem_layout`.
    #[must_use]
    pub fn new(elem_layout: Layout, drop_ctx: *const (), drop: DropThunk) -> Self {
        let empty = empty_array_layout(elem_layout.align());
        Self {
            ptr: allocate_or_dangling(empty),
            len: 0,
            cap: 0,
            elem_layout,
            allocation: None,
            drop_ctx,
            drop,
        }
    }

    /// The start of the raw element storage.
    #[must_use]
    pub fn ptr(&self) -> *mut u8 {
        self.ptr
    }

    /// Number of initialized elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether there are no initialized elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Number of element slots allocated.
    #[must_use]
    pub fn cap(&self) -> usize {
        self.cap
    }

    /// Byte stride between elements.
    #[must_use]
    pub fn stride(&self) -> usize {
        self.elem_layout.size()
    }

    /// Reserve and return the next uninitialized element slot.
    pub fn next_uninit_slot(&mut self) -> Result<*mut u8, RawAllocError> {
        let required = self.len.checked_add(1).ok_or(RawAllocError::SizeOverflow {
            count: self.len,
            stride: self.elem_layout.size(),
        })?;
        self.ensure_capacity(required)?;
        Ok(unsafe { self.slot_unchecked(self.len) })
    }

    /// Mark the current next slot as initialized after the caller wrote it.
    ///
    /// # Safety
    ///
    /// The caller must have just initialized the slot returned by
    /// [`next_uninit_slot`](Self::next_uninit_slot), and must call this at most
    /// once per reserved slot.
    pub unsafe fn mark_initialized(&mut self) {
        debug_assert!(self.len < self.cap, "marking beyond reserved capacity");
        self.len += 1;
    }

    /// Mark the allocation and initialized values as adopted by the caller.
    pub fn adopt(&mut self) {
        self.len = 0;
        self.cap = 0;
        self.allocation = None;
    }

    unsafe fn slot_unchecked(&self, index: usize) -> *mut u8 {
        unsafe { self.ptr.add(index * self.elem_layout.size()) }
    }

    fn ensure_capacity(&mut self, required: usize) -> Result<(), RawAllocError> {
        if required <= self.cap {
            return Ok(());
        }

        let doubled = self.cap.checked_mul(2).ok_or(RawAllocError::SizeOverflow {
            count: self.cap,
            stride: self.elem_layout.size(),
        })?;
        let next_cap = required.max(doubled).max(4);
        let new_layout = array_layout(next_cap, self.elem_layout)?;

        if self.elem_layout.size() == 0 {
            self.cap = next_cap;
            return Ok(());
        }

        let new_ptr = if let Some(old_layout) = self.allocation {
            unsafe { alloc::realloc(self.ptr, old_layout, new_layout.size()) }
        } else {
            unsafe { alloc::alloc(new_layout) }
        };
        if new_ptr.is_null() {
            alloc::handle_alloc_error(new_layout);
        }

        self.ptr = new_ptr;
        self.cap = next_cap;
        self.allocation = Some(new_layout);
        Ok(())
    }
}

impl Drop for RawArrayBuilder {
    fn drop(&mut self) {
        for index in (0..self.len).rev() {
            let slot = unsafe { self.slot_unchecked(index) };
            unsafe { (self.drop)(self.drop_ctx, slot) };
        }

        if let Some(layout) = self.allocation.take() {
            unsafe { alloc::dealloc(self.ptr, layout) };
        }
    }
}

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
pub struct InitializedLedger<Mark, const INLINE: usize = 8> {
    storage: LedgerStorage<Mark, INLINE>,
}

enum LedgerStorage<Mark, const INLINE: usize> {
    Inline {
        len: usize,
        initialized: u64,
        marks: [MaybeUninit<Mark>; INLINE],
    },
    Heap(Vec<Option<Mark>>),
}

impl<Mark, const INLINE: usize> InitializedLedger<Mark, INLINE> {
    /// Create a ledger with `len` uninitialized slots.
    #[must_use]
    pub fn new(len: usize) -> Self {
        let storage = if len <= INLINE && len <= u64::BITS as usize {
            LedgerStorage::Inline {
                len,
                initialized: 0,
                marks: array::from_fn(|_| MaybeUninit::uninit()),
            }
        } else {
            LedgerStorage::Heap((0..len).map(|_| None).collect())
        };
        Self { storage }
    }

    fn len(&self) -> usize {
        match &self.storage {
            LedgerStorage::Inline { len, .. } => *len,
            LedgerStorage::Heap(marks) => marks.len(),
        }
    }

    /// Return whether `index` is initialized.
    #[must_use]
    pub fn is_initialized(&self, index: usize) -> bool {
        match &self.storage {
            LedgerStorage::Inline {
                len, initialized, ..
            } => {
                assert!(index < *len, "ledger index out of bounds");
                (*initialized & initialized_bit(index)) != 0
            }
            LedgerStorage::Heap(marks) => marks[index].is_some(),
        }
    }

    /// Return the mark for `index` when initialized.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Mark> {
        match &self.storage {
            LedgerStorage::Inline {
                len,
                initialized,
                marks,
            } => {
                assert!(index < *len, "ledger index out of bounds");
                ((*initialized & initialized_bit(index)) != 0)
                    .then(|| unsafe { marks[index].assume_init_ref() })
            }
            LedgerStorage::Heap(marks) => marks[index].as_ref(),
        }
    }

    /// Mark `index` initialized.
    pub fn mark(&mut self, index: usize, mark: Mark) {
        match &mut self.storage {
            LedgerStorage::Inline {
                len,
                initialized,
                marks,
            } => {
                assert!(index < *len, "ledger index out of bounds");
                let bit = initialized_bit(index);
                if (*initialized & bit) != 0 {
                    unsafe {
                        marks[index].assume_init_drop();
                    }
                }
                marks[index].write(mark);
                *initialized |= bit;
            }
            LedgerStorage::Heap(marks) => {
                marks[index] = Some(mark);
            }
        }
    }

    /// Initialized slots in reverse index order.
    pub fn iter_initialized_rev(&self) -> impl Iterator<Item = (usize, &Mark)> {
        (0..self.len())
            .rev()
            .filter_map(|index| self.get(index).map(|mark| (index, mark)))
    }
}

impl<Mark: Clone, const INLINE: usize> Clone for InitializedLedger<Mark, INLINE> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
        }
    }
}

impl<Mark: fmt::Debug, const INLINE: usize> fmt::Debug for InitializedLedger<Mark, INLINE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter_initialized_rev()).finish()
    }
}

impl<Mark: PartialEq, const INLINE: usize> PartialEq for InitializedLedger<Mark, INLINE> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len()
            && (0..self.len()).all(|index| self.get(index) == other.get(index))
    }
}

impl<Mark: Eq, const INLINE: usize> Eq for InitializedLedger<Mark, INLINE> {}

impl<Mark: Clone, const INLINE: usize> Clone for LedgerStorage<Mark, INLINE> {
    fn clone(&self) -> Self {
        match self {
            Self::Inline {
                len,
                initialized,
                marks,
            } => {
                let mut cloned = InitializedLedger::<Mark, INLINE>::new(*len);
                for (index, mark) in marks.iter().enumerate().take(*len) {
                    if (*initialized & initialized_bit(index)) != 0 {
                        cloned.mark(index, unsafe { mark.assume_init_ref() }.clone());
                    }
                }
                cloned.storage
            }
            Self::Heap(marks) => Self::Heap(marks.clone()),
        }
    }
}

impl<Mark, const INLINE: usize> Drop for LedgerStorage<Mark, INLINE> {
    fn drop(&mut self) {
        if let Self::Inline {
            len,
            initialized,
            marks,
        } = self
        {
            for (index, mark) in marks.iter_mut().enumerate().take(*len) {
                if (*initialized & initialized_bit(index)) != 0 {
                    unsafe {
                        mark.assume_init_drop();
                    }
                }
            }
        }
    }
}

fn initialized_bit(index: usize) -> u64 {
    1u64 << index
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

fn empty_array_layout(align: usize) -> Layout {
    Layout::from_size_align(0, align).expect("alignment came from a valid element layout")
}

fn array_layout(count: usize, element: Layout) -> Result<Layout, RawAllocError> {
    if count == 0 || element.size() == 0 {
        return Layout::from_size_align(0, element.align()).map_err(|_| {
            RawAllocError::InvalidLayout {
                size: 0,
                align: element.align(),
            }
        });
    }

    let size = count
        .checked_mul(element.size())
        .ok_or(RawAllocError::SizeOverflow {
            count,
            stride: element.size(),
        })?;
    Layout::from_size_align(size, element.align()).map_err(|_| RawAllocError::InvalidLayout {
        size,
        align: element.align(),
    })
}

fn layout_fits(buffer: Layout, requested: Layout) -> bool {
    buffer.size() >= requested.size() && buffer.align() >= requested.align()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountedDrop<'a>(&'a AtomicUsize);

    impl Drop for CountedDrop<'_> {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    unsafe fn count_drop(ctx: *const (), _ptr: *mut u8) {
        let drops = unsafe { &*(ctx as *const AtomicUsize) };
        drops.fetch_add(1, Ordering::SeqCst);
    }

    unsafe fn counted_drop_in_place(ctx: *const (), ptr: *mut u8) {
        let _drops = unsafe { &*(ctx as *const AtomicUsize) };
        unsafe {
            ptr::drop_in_place(ptr.cast::<CountedDrop<'_>>());
        }
    }

    #[test]
    fn raw_scratch_handles_zero_sized_storage() {
        let mut scratch = RawScratch::new(0, 8).unwrap();
        assert_eq!(scratch.ptr() as usize, 8);
        scratch.dealloc_uninit();
        scratch.dealloc_uninit();
    }

    #[test]
    fn scratch_session_reuses_released_slots() {
        let mut session = ScratchSession::new();
        let first = session.reserve(Layout::from_size_align(16, 8).unwrap());
        let first_ptr = first.ptr();
        session.release(first);

        let second = session.reserve(Layout::from_size_align(8, 4).unwrap());
        assert_eq!(second.ptr(), first_ptr);
        session.release(second);
    }

    #[test]
    fn scratch_session_keeps_active_slots_distinct() {
        let mut session = ScratchSession::new();
        let first = session.reserve(Layout::from_size_align(16, 8).unwrap());
        let second = session.reserve(Layout::from_size_align(16, 8).unwrap());
        assert_ne!(second.ptr(), first.ptr());
        session.release(second);
        session.release(first);
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
    fn raw_array_builder_grows_and_drops_initialized_elements() {
        let drops = AtomicUsize::new(0);
        let ctx = &drops as *const AtomicUsize as *const ();
        {
            let mut builder =
                RawArrayBuilder::new(Layout::new::<CountedDrop<'_>>(), ctx, counted_drop_in_place);
            for _ in 0..5 {
                let slot = builder.next_uninit_slot().unwrap();
                unsafe {
                    slot.cast::<CountedDrop<'_>>().write(CountedDrop(&drops));
                    builder.mark_initialized();
                }
            }
            assert_eq!(builder.len(), 5);
            assert!(builder.cap() >= 5);
        }
        assert_eq!(drops.load(Ordering::SeqCst), 5);
    }

    #[test]
    fn raw_array_builder_adopt_disarms_values_and_allocation() {
        let drops = AtomicUsize::new(0);
        let ctx = &drops as *const AtomicUsize as *const ();
        let element = Layout::new::<u64>();
        let mut builder = RawArrayBuilder::new(element, ctx, count_drop);
        let slot = builder.next_uninit_slot().unwrap();
        unsafe {
            slot.cast::<u64>().write(42);
            builder.mark_initialized();
        }
        assert_eq!(builder.len(), 1);
        let ptr = builder.ptr();
        let cap = builder.cap();

        builder.adopt();
        drop(builder);

        assert_eq!(drops.load(Ordering::SeqCst), 0);
        unsafe {
            alloc::dealloc(ptr, array_layout(cap, element).unwrap());
        }
    }

    #[test]
    fn raw_array_builder_handles_zero_sized_elements() {
        let drops = AtomicUsize::new(0);
        let ctx = &drops as *const AtomicUsize as *const ();
        {
            let mut builder = RawArrayBuilder::new(Layout::new::<()>(), ctx, count_drop);

            let first = builder.next_uninit_slot().unwrap();
            unsafe {
                first.cast::<()>().write(());
                builder.mark_initialized();
            }
            let second = builder.next_uninit_slot().unwrap();
            unsafe {
                second.cast::<()>().write(());
                builder.mark_initialized();
            }

            assert_eq!(builder.len(), 2);
            assert_eq!(builder.stride(), 0);
            assert_eq!(first, second);
        }
        assert_eq!(drops.load(Ordering::SeqCst), 2);
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
        let mut ledger: InitializedLedger<&str> = InitializedLedger::new(4);
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

    #[test]
    fn initialized_ledger_drops_replaced_and_live_inline_marks() {
        let drops = AtomicUsize::new(0);
        {
            let mut ledger: InitializedLedger<CountedDrop<'_>> = InitializedLedger::new(1);
            ledger.mark(0, CountedDrop(&drops));
            assert_eq!(drops.load(Ordering::SeqCst), 0);

            ledger.mark(0, CountedDrop(&drops));
            assert_eq!(drops.load(Ordering::SeqCst), 1);
        }
        assert_eq!(drops.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn initialized_ledger_reports_reverse_order_from_heap_storage() {
        let mut ledger: InitializedLedger<&str, 2> = InitializedLedger::new(4);
        ledger.mark(0, "zero");
        ledger.mark(2, "two");

        assert_eq!(ledger.get(2), Some(&"two"));
        let initialized = ledger
            .iter_initialized_rev()
            .map(|(index, mark)| (index, *mark))
            .collect::<Vec<_>>();
        assert_eq!(initialized, vec![(2, "two"), (0, "zero")]);
    }
}
