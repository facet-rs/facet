//! Tasks, frames, and the typed calling convention — tooth 2 of the
//! substrate, per the ruled ABI (vixen repo, docs/design/
//! tooth-2-frames-abi.md).
//!
//! - **A frame is a declared record.** Its layout (args, locals, spill
//!   slots) is computed by the same machinery as any record
//!   (mem::declared); code addresses it by statically known byte
//!   offsets. The debugger reads frames the way it reads values.
//! - **Frames live in a per-task arena**, never on the C stack.
//!   Parking a task costs nothing: the live frame chain already sits
//!   in the arena — stop running and the state is simply still there.
//! - **Arguments travel frame-direct**: the caller writes each
//!   argument into the callee's frame at its known offset — typed
//!   stores, no marshalling. Composite returns go through a
//!   caller-designated return slot (sret shape).
//! - **The await-spill rule**: at an await point every live value is
//!   in a frame. In THIS lane it holds by construction — the
//!   instruction set is three-address over frame offsets, values are
//!   always frame-resident. Stencil lanes may cache registers between
//!   awaits; the rule constrains them at await sites.
//! - **Sync vs async sites are distinct in the ABI** (Amos's
//!   refinement): only [`Op::Await`] sites carry await-point
//!   machinery; synchronous host calls will be a separate op with no
//!   park path, no numbering, no spill obligations.
//! - **Typed instructions over untagged operands** (constitution A6):
//!   the arena is raw bytes; ops imply types; nothing is
//!   self-describing at runtime.
//!
//! Trace events are first-class (frame-granular, per the ruling); in
//! this slice they are recorded directly — the strippable
//! IR-instrumentation form arrives with the trace-vocabulary slice.

use core::future::Future;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};

use crate::exec::{CompareSide, TaskFault, fault_site};
use crate::mem::Layout;
use crate::{CallSiteFacts, RegionId, VerifiedProgram};

/// One immutable value payload made visible to task code for native reads.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct RawValueMemory {
    ptr: *const u8,
    len: usize,
}

/// One immutable value payload made visible to task code for native reads.
#[derive(Clone, Copy, Debug)]
pub struct ValueMemory<'a> {
    raw: RawValueMemory,
    _borrow: PhantomData<&'a [u8]>,
}

impl<'a> ValueMemory<'a> {
    #[must_use]
    pub fn from_slice(bytes: &'a [u8]) -> Self {
        Self {
            raw: RawValueMemory {
                ptr: bytes.as_ptr(),
                len: bytes.len(),
            },
            _borrow: PhantomData,
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self {
            raw: RawValueMemory {
                ptr: core::ptr::null(),
                len: 0,
            },
            _borrow: PhantomData,
        }
    }

    /// Returns whether this memory entry has a resident payload.
    #[must_use]
    pub fn is_resident(&self) -> bool {
        !self.raw.ptr.is_null()
    }

    fn as_slice(&self) -> Result<&'a [u8], ArrayOpStatus> {
        // SAFETY: the raw pointer/len were captured from a `&'a [u8]` in
        // `from_slice`, or are the null sentinel from `empty()` (rejected
        // below before any pointer use).
        unsafe { self.raw.as_slice() }
    }

    pub(crate) fn raw(&self) -> RawValueMemory {
        self.raw
    }
}

impl RawValueMemory {
    /// # Safety
    ///
    /// The caller must ensure `'a` does not outlive the borrow the
    /// pointer/length pair was captured from (or that `self` is the null
    /// sentinel, in which case no pointer is dereferenced).
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    unsafe fn as_slice<'a>(&self) -> Result<&'a [u8], ArrayOpStatus> {
        if self.ptr.is_null() {
            return Err(ArrayOpStatus::InvalidHandle);
        }
        Ok(unsafe { core::slice::from_raw_parts(self.ptr, self.len) })
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ValueMemories<'a> {
    pub store: &'a [ValueMemory<'a>],
    /// Molten payloads lent by an external owner, read-only for the task.
    ///
    /// The task's own private molten arena and externally lent molten table
    /// occupy disjoint handle namespaces. A task-local allocation can never
    /// shadow a lent payload.
    pub molten: &'a [ValueMemory<'a>],
}

impl ValueMemories<'_> {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            store: &[],
            molten: &[],
        }
    }
}

#[derive(Clone, Copy)]
struct RawValueMemories<'a> {
    store: &'a [RawValueMemory],
    molten: &'a [RawValueMemory],
}

#[derive(Clone, Copy)]
enum MemoryView<'a> {
    Borrowed(ValueMemories<'a>),
    Raw(RawValueMemories<'a>),
}

impl<'a> From<ValueMemories<'a>> for MemoryView<'a> {
    fn from(value: ValueMemories<'a>) -> Self {
        Self::Borrowed(value)
    }
}

impl<'a> MemoryView<'a> {
    fn store(self, index: usize) -> Result<&'a [u8], ArrayOpStatus> {
        match self {
            MemoryView::Borrowed(memories) => memories
                .store
                .get(index)
                .ok_or(ArrayOpStatus::InvalidHandle)?
                .as_slice(),
            MemoryView::Raw(memories) => {
                let raw = memories
                    .store
                    .get(index)
                    .ok_or(ArrayOpStatus::InvalidHandle)?;
                // SAFETY: raw entries come from the caller-supplied
                // `RawValueMemories` table, valid for `'a` per its contract.
                unsafe { raw.as_slice() }
            }
        }
    }

    fn molten(self, index: usize) -> Result<&'a [u8], ArrayOpStatus> {
        match self {
            MemoryView::Borrowed(memories) => memories
                .molten
                .get(index)
                .ok_or(ArrayOpStatus::InvalidHandle)?
                .as_slice(),
            MemoryView::Raw(memories) => {
                let raw = memories
                    .molten
                    .get(index)
                    .ok_or(ArrayOpStatus::InvalidHandle)?;
                // SAFETY: raw entries come from the caller-supplied
                // `RawValueMemories` table, valid for `'a` per its contract.
                unsafe { raw.as_slice() }
            }
        }
    }
}

/// Checked status for authoritative array construction, region load, and region
/// store operations.
#[repr(i64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrayOpStatus {
    /// Operation completed and all requested bytes were copied or initialized.
    Ok = 1,
    /// The handle did not name a resident value in its declared namespace.
    InvalidHandle = 2,
    /// A resident payload was not a well-formed Weavy array payload.
    MalformedPayload = 3,
    /// The static element width did not match the payload's element width.
    WidthMismatch = 4,
    /// The static schema witness did not match the payload's schema witness.
    SchemaMismatch = 5,
    /// The element index was outside a well-formed payload's element range.
    OutOfRange = 6,
    /// Checked size arithmetic or handle-space arithmetic overflowed.
    Overflow = 7,
    /// The allocator reported exhaustion for an otherwise valid request.
    AllocationFailed = 8,
    /// The requested task-local molten array bytes have not all been written.
    Uninitialized = 9,
}

/// A task-private molten arena: mutable, in-flight, not interned.
///
/// It is owned by exactly one [`Task`] and dies with it, so a discarded task
/// drops its molten state wholesale. Nothing in here has a public identity: no
/// content hash is computed, no handle is store-assigned, and no value here can
/// cross an island boundary. Crossing an edge requires freeze/publish, which
/// this type deliberately does not provide.
///
/// Task-local molten handles occupy the low negative quarter of the i64 handle
/// space. Externally lent molten handles retain the legacy `-1 - index` shape
/// in the high negative range, outside the task-local namespace. Nonnegative
/// handles remain store handles.
#[derive(Clone, Debug, Default)]
pub(crate) struct MoltenArena {
    buffers: Vec<MoltenBuffer>,
}

#[derive(Clone, Debug)]
struct MoltenBuffer {
    bytes: Vec<u8>,
    initialized: Vec<bool>,
}

impl MoltenArena {
    /// Reserve a zeroed array-elements payload and return its task-local molten
    /// handle. Elements are written afterwards through checked array-store ops;
    /// the payload is well-formed from allocation on.
    pub(crate) fn alloc_array(
        &mut self,
        count: i64,
        elem_width: usize,
        elem_schema_ref: i64,
    ) -> Result<i64, ArrayOpStatus> {
        let count = usize::try_from(count).map_err(|_| ArrayOpStatus::Overflow)?;
        if elem_width == 0 {
            return Err(ArrayOpStatus::WidthMismatch);
        }
        let data_len = count
            .checked_mul(elem_width)
            .ok_or(ArrayOpStatus::Overflow)?;
        let total = ARRAY_ELEMENTS_HEADER_SIZE
            .checked_add(data_len)
            .ok_or(ArrayOpStatus::Overflow)?;
        if total > isize::MAX as usize {
            return Err(ArrayOpStatus::Overflow);
        }
        let handle = task_molten_handle(self.buffers.len()).ok_or(ArrayOpStatus::Overflow)?;

        self.buffers
            .try_reserve_exact(1)
            .map_err(|_| ArrayOpStatus::AllocationFailed)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total)
            .map_err(|_| ArrayOpStatus::AllocationFailed)?;
        bytes.extend_from_slice(&ARRAY_ELEMENTS_TAG.to_le_bytes());
        bytes.extend_from_slice(&elem_schema_ref.to_le_bytes());
        bytes.extend_from_slice(&count_i64(count)?.to_le_bytes());
        bytes.extend_from_slice(&count_i64(elem_width)?.to_le_bytes());
        bytes.resize(total, 0);
        // Whole-element writes make initialization a per-element property: one
        // flag per slot, set the moment its complete element is stored.
        let mut initialized = Vec::new();
        initialized
            .try_reserve_exact(count)
            .map_err(|_| ArrayOpStatus::AllocationFailed)?;
        initialized.resize(count, false);

        self.buffers.push(MoltenBuffer { bytes, initialized });
        Ok(handle)
    }

    #[must_use]
    fn bytes(&self, handle: i64) -> Option<&[u8]> {
        self.buffers
            .get(task_molten_index(handle)?)
            .map(|buffer| buffer.bytes.as_slice())
    }

    fn buffer_mut(&mut self, handle: i64) -> Option<&mut MoltenBuffer> {
        let index = task_molten_index(handle)?;
        self.buffers.get_mut(index)
    }

    fn buffer(&self, handle: i64) -> Option<&MoltenBuffer> {
        let index = task_molten_index(handle)?;
        self.buffers.get(index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandleKind {
    Store(usize),
    TaskMolten(usize),
    LentMolten(usize),
}

const TASK_MOLTEN_BASE: i64 = i64::MIN;
pub const ARRAY_POISON_HANDLE: i64 = TASK_MOLTEN_BASE;
const TASK_MOLTEN_FIRST: i64 = TASK_MOLTEN_BASE + 1;
const LENT_MOLTEN_MIN: i64 = i64::MIN / 2;

const ARRAY_WORDS_TAG: i64 = 0;
const ARRAY_ELEMENTS_TAG: i64 = 1;
const ARRAY_WORDS_HEADER_SIZE: usize = 24;
const ARRAY_ELEMENTS_HEADER_SIZE: usize = 32;

fn task_molten_handle(index: usize) -> Option<i64> {
    let index = i64::try_from(index).ok()?;
    let handle = TASK_MOLTEN_FIRST.checked_add(index)?;
    if handle >= LENT_MOLTEN_MIN {
        return None;
    }
    Some(handle)
}

fn task_molten_index(handle: i64) -> Option<usize> {
    if (TASK_MOLTEN_FIRST..LENT_MOLTEN_MIN).contains(&handle) {
        usize::try_from(handle.checked_sub(TASK_MOLTEN_FIRST)?).ok()
    } else {
        None
    }
}

fn lent_molten_index(handle: i64) -> Option<usize> {
    if (LENT_MOLTEN_MIN..0).contains(&handle) {
        usize::try_from((-1i64).checked_sub(handle)?).ok()
    } else {
        None
    }
}

fn classify_handle(handle: i64) -> Option<HandleKind> {
    if handle >= 0 {
        return Some(HandleKind::Store(usize::try_from(handle).ok()?));
    }
    if let Some(index) = task_molten_index(handle) {
        return Some(HandleKind::TaskMolten(index));
    }
    if let Some(index) = lent_molten_index(handle) {
        return Some(HandleKind::LentMolten(index));
    }
    None
}

fn count_i64(value: usize) -> Result<i64, ArrayOpStatus> {
    i64::try_from(value).map_err(|_| ArrayOpStatus::Overflow)
}

/// # Safety
/// `arena` must point to a live [`MoltenArena`] for the duration of the call,
/// and no mutable reference may access that arena while the returned pointer is
/// used.
/// `out_len` must be non-null and writable for one `usize`. The returned pointer
/// is valid only until the next mutation of the arena and must never be written
/// through.
pub(crate) unsafe extern "C" fn molten_bytes_abi(
    arena: *const core::ffi::c_void,
    handle: i64,
    out_len: *mut usize,
) -> *const u8 {
    if arena.is_null() || out_len.is_null() {
        return core::ptr::null();
    }
    let arena = unsafe { &*arena.cast::<MoltenArena>() };
    match arena.buffer(handle) {
        Some(buffer) => {
            unsafe { *out_len = buffer.bytes.len() };
            buffer.bytes.as_ptr()
        }
        None => {
            unsafe { *out_len = 0 };
            core::ptr::null()
        }
    }
}

/// # Safety
/// `arena` must point to a live [`MoltenArena`] for the duration of the call,
/// and no other mutable or shared reference may concurrently access that arena.
/// `out_handle` must be non-null and writable for one `i64`; it must not alias
/// memory inside `arena`. This function writes [`ARRAY_POISON_HANDLE`] before it
/// attempts allocation, then overwrites it only after a successful allocation.
pub(crate) unsafe extern "C" fn array_new_abi(
    arena: *mut core::ffi::c_void,
    count: i64,
    elem_width: usize,
    elem_schema_ref: i64,
    out_handle: *mut i64,
) -> i64 {
    if out_handle.is_null() {
        return ArrayOpStatus::InvalidHandle as i64;
    }
    unsafe { *out_handle = ARRAY_POISON_HANDLE };
    if arena.is_null() {
        return ArrayOpStatus::InvalidHandle as i64;
    }
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.alloc_array(count, elem_width, elem_schema_ref) {
        Ok(handle) => {
            unsafe { *out_handle = handle };
            ArrayOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

/// # Safety
/// `arena` must point to a live [`MoltenArena`] for the duration of the call,
/// and no other mutable or shared reference may concurrently access that arena.
/// `src` must be non-null and readable for `elem_width` bytes when
/// `elem_width > 0`; it must not alias the target molten allocation. Pointer
/// precondition failures return [`ArrayOpStatus::InvalidHandle`] and do not
/// dereference `src`.
pub(crate) unsafe extern "C" fn array_store_abi(
    arena: *mut core::ffi::c_void,
    array: i64,
    index: i64,
    src: *const u8,
    elem_width: usize,
    elem_schema_ref: i64,
) -> i64 {
    if arena.is_null() || (src.is_null() && elem_width != 0) {
        return ArrayOpStatus::InvalidHandle as i64;
    }
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    let src = if elem_width == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(src, elem_width) }
    };
    store_array_region(
        arena,
        ArrayRegion {
            array,
            index,
            elem_width,
            elem_schema_ref,
        },
        src,
    ) as i64
}

/// # Safety
/// `store_value_memories` and `lent_molten_value_memories` must each be null
/// only when their count is zero; otherwise they must point to arrays of
/// [`RawValueMemory`] entries valid for the duration of the call. Every raw
/// value-memory entry selected by `array` must point to bytes that remain
/// readable for the duration of the call. `arena` must point to a live
/// [`MoltenArena`] and must not be mutably aliased. `dst` must be non-null and
/// writable for `elem_width` bytes when `elem_width > 0`, and must not overlap
/// the source payload. Pointer precondition failures return
/// [`ArrayOpStatus::InvalidHandle`] without promising destination zeroing;
/// semantic failures after those preconditions zero the destination region.
pub(crate) unsafe extern "C" fn array_load_abi(
    store_value_memories: *const RawValueMemory,
    store_value_memory_count: usize,
    lent_molten_value_memories: *const RawValueMemory,
    lent_molten_value_memory_count: usize,
    arena: *mut core::ffi::c_void,
    array: i64,
    index: i64,
    dst: *mut u8,
    elem_width: usize,
    elem_schema_ref: i64,
) -> i64 {
    if arena.is_null()
        || (dst.is_null() && elem_width != 0)
        || (store_value_memories.is_null() && store_value_memory_count != 0)
        || (lent_molten_value_memories.is_null() && lent_molten_value_memory_count != 0)
    {
        return ArrayOpStatus::InvalidHandle as i64;
    }
    let store = if store_value_memory_count == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(store_value_memories, store_value_memory_count) }
    };
    let molten = if lent_molten_value_memory_count == 0 {
        &[]
    } else {
        unsafe {
            core::slice::from_raw_parts(lent_molten_value_memories, lent_molten_value_memory_count)
        }
    };
    let memories = MemoryView::Raw(RawValueMemories { store, molten });
    let arena = unsafe { &*arena.cast::<MoltenArena>() };
    let dst = if elem_width == 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(dst, elem_width) }
    };
    load_array_region(
        memories,
        arena,
        ArrayRegion {
            array,
            index,
            elem_width,
            elem_schema_ref,
        },
        dst,
    ) as i64
}

/// # Safety
/// `store_value_memories` and `lent_molten_value_memories` must each be null
/// only when their count is zero; otherwise they must point to arrays of
/// [`RawValueMemory`] entries valid for the duration of the call. Every raw
/// value-memory entry selected by `array` must point to bytes that remain
/// readable for the duration of the call. `arena` must point to a live
/// [`MoltenArena`] and must not be mutably aliased. `out_count` must be
/// non-null, writable for one `i64`, and must not alias `arena`.
pub(crate) unsafe extern "C" fn array_len_abi(
    store_value_memories: *const RawValueMemory,
    store_value_memory_count: usize,
    lent_molten_value_memories: *const RawValueMemory,
    lent_molten_value_memory_count: usize,
    arena: *mut core::ffi::c_void,
    array: i64,
    elem_schema_ref: i64,
    out_count: *mut i64,
) -> i64 {
    if arena.is_null()
        || out_count.is_null()
        || (store_value_memories.is_null() && store_value_memory_count != 0)
        || (lent_molten_value_memories.is_null() && lent_molten_value_memory_count != 0)
    {
        return ArrayOpStatus::InvalidHandle as i64;
    }
    let store = if store_value_memory_count == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(store_value_memories, store_value_memory_count) }
    };
    let molten = if lent_molten_value_memory_count == 0 {
        &[]
    } else {
        unsafe {
            core::slice::from_raw_parts(lent_molten_value_memories, lent_molten_value_memory_count)
        }
    };
    let memories = MemoryView::Raw(RawValueMemories { store, molten });
    let arena = unsafe { &*arena.cast::<MoltenArena>() };
    let (status, count) = load_array_len(memories, arena, array, elem_schema_ref);
    unsafe { *out_count = count };
    status as i64
}

/// Identifies a function in a [`Program`]'s function table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FnId(pub u32);

/// A function: its frame's layout (a declared record of args, locals,
/// and spill slots — offsets are the callers' and body's shared
/// knowledge) and its code.
#[derive(Clone, Debug)]
pub struct Fn {
    pub frame: Layout,
    pub code: Vec<Op>,
}

/// A program: functions addressed by [`FnId`].
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub fns: Vec<Fn>,
}

/// One argument copy of a frame-direct call: `size` bytes from the
/// caller's frame at `src` into the callee's frame at `dst`. Emitted
/// by a lowering that statically knows both layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgCopy {
    pub src: u32,
    pub dst: u32,
    pub size: u32,
}

/// One declared field source for a complete structural construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructuralFieldSource {
    pub field: u32,
    pub source: RegionId,
}

/// Typed, three-address instructions over frame offsets. The
/// vocabulary grows per kind (AddF64, loads/stores of declared
/// fields, sync host calls) — per the ruled stencil order, frame/call/
/// return machinery lands before arithmetic variety.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// Construct one complete product from exactly one source per declared field.
    ProductConstruct {
        dst: RegionId,
        fields: Vec<StructuralFieldSource>,
    },
    /// Project one declared product field into its exact typed destination.
    ProductProject {
        dst: RegionId,
        product: RegionId,
        field: u32,
    },
    /// Copy one complete structural value between exact equal value shapes.
    CopyValue { dst: RegionId, src: RegionId },
    /// Construct one complete compact enum from one statically selected variant.
    EnumConstruct {
        dst: RegionId,
        variant: u32,
        fields: Vec<StructuralFieldSource>,
    },
    /// Validate a compact enum selector and test it against one declared variant.
    EnumIsVariant {
        dst: RegionId,
        value: RegionId,
        variant: u32,
    },
    /// Validate the active variant, then project one exact declared field.
    EnumProjectChecked {
        dst: RegionId,
        value: RegionId,
        variant: u32,
        field: u32,
    },
    /// `frame[dst] = value` (i64).
    ConstI64 { dst: u32, value: i64 },
    /// `frame[dst] = frame[a] + frame[b]` (i64, wrapping).
    AddI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = frame[a] - frame[b]` (i64, wrapping).
    SubI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = frame[a] * frame[b]` (i64, wrapping).
    MulI64 { dst: u32, a: u32, b: u32 },
    /// Total wrapping division: zero maps to zero and `MIN / -1` maps to `MIN`.
    DivI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = frame[src]` (one 64-bit word).
    CopyI64 { dst: u32, src: u32 },
    /// `frame[dst] = (frame[a] == frame[b]) as i64`.
    EqI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] != frame[b]) as i64`.
    NeI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] < frame[b]) as i64`.
    LtI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] <= frame[b]) as i64`.
    LeI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] > frame[b]) as i64`.
    GtI64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = (frame[a] >= frame[b]) as i64`.
    GeI64 { dst: u32, a: u32, b: u32 },
    /// Continue at absolute instruction index `target` in the current function.
    Jump { target: u32 },
    /// Continue at `target` when `frame[value] == 0`, otherwise fall through.
    JumpIfZero { value: u32, target: u32 },
    /// Frame-direct call: allocate the callee's frame in the task
    /// arena, copy `args`, enter. The callee's `Ret` writes `size`
    /// bytes into THIS frame at `ret`.
    Call {
        callee: FnId,
        args: Vec<ArgCopy>,
        ret: u32,
    },
    /// Frame-direct call through a closure's local function-id word.
    CallIndirect {
        callee: u32,
        args: Vec<ArgCopy>,
        ret: u32,
    },
    /// Return `size` bytes at `src` to the caller's designated return
    /// slot (or to the task result if this is the root frame), then
    /// pop the frame.
    Ret { src: u32, size: u32 },
    /// ASYNC await point (numbered in task order of first arrival):
    /// if `input` is ready, consume that ready token, write its value
    /// (i64 in this slice) to `frame[dst]`, and continue; otherwise PARK the task. Sync host
    /// calls are deliberately NOT this op.
    Await { dst: u32, input: u32 },
    /// `frame[dst] = frame[base + frame[index]*stride]` — dynamic
    /// indexing into an INLINE composite (an array living in the
    /// frame, unboxed). Bounds are the checker's obligation: the
    /// count is static in the array's declared layout; a lowering
    /// that emits an out-of-range index has a compiler bug, and a
    /// validation pass may reject programs statically — never a
    /// runtime tag or check here (constitution A6).
    LoadIndexedI64 {
        dst: u32,
        base: u32,
        index: u32,
        stride: u32,
    },
    /// `frame[base + frame[index]*stride] = frame[src]` — the store
    /// twin of [`Op::LoadIndexedI64`], same obligations.
    StoreIndexedI64 {
        base: u32,
        index: u32,
        stride: u32,
        src: u32,
    },
    /// Legacy checked read from an `Array<T>` one-word payload.
    ///
    /// `frame[array]` is a store handle (nonnegative) or a molten handle
    /// (negative). Its payload must be an array-words run with matching
    /// `elem_schema_ref`, or an authoritative array-elements run with width 8.
    /// In-bounds reads write the element to `dst` and `1` to `present`; misses
    /// preserve the legacy shape and write zeroes to both. New lowering should
    /// use [`Op::LoadArray`] to get a checked status.
    LoadArrayWord {
        dst: u32,
        present: u32,
        array: u32,
        index: u32,
        elem_schema_ref: i64,
    },
    /// Reserve a task-private molten array and write its handle to `frame[dst]`.
    ///
    /// `frame[count_slot]` supplies the runtime element count; `elem_width` and
    /// `elem_schema_ref` are static witnesses for later checked region
    /// load/store operations. `frame[status]` receives [`ArrayOpStatus`]. On
    /// failure `frame[dst]` receives [`ARRAY_POISON_HANDLE`].
    ArrayNew {
        dst: u32,
        status: u32,
        count_slot: u32,
        elem_width: u32,
        elem_schema_ref: i64,
    },
    /// Legacy one-word store into a molten array under construction.
    ///
    /// This uses the same checked substrate as [`Op::ArrayStore`] with a static
    /// width of 8. `frame[status]` receives [`ArrayOpStatus`].
    ArrayStoreWord {
        status: u32,
        array: u32,
        index: u32,
        src: u32,
        elem_schema_ref: i64,
    },
    /// Checked region copy into a molten array element.
    ///
    /// The array must be task-local molten storage. This is a WHOLE-ELEMENT
    /// operation: the complete `elem_width`-byte element is copied from
    /// `frame[src..]` into element `frame[index]` when the handle, payload,
    /// exact element width, schema, and index all validate. Records and nested
    /// values are addressed by ordinary static projection on the frame side.
    /// `frame[status]` receives [`ArrayOpStatus`].
    ArrayStore {
        status: u32,
        array: u32,
        index: u32,
        src: u32,
        elem_width: u32,
        elem_schema_ref: i64,
    },
    /// Checked whole-element copy out of a store-backed, lent molten, or
    /// task-local molten array element.
    ///
    /// The complete `elem_width`-byte element is copied to `frame[dst..]` when
    /// the handle, payload, exact element width, schema, and index all validate.
    /// On failure those destination bytes are zeroed and `frame[status]`
    /// receives the precise [`ArrayOpStatus`].
    LoadArray {
        dst: u32,
        status: u32,
        array: u32,
        index: u32,
        elem_width: u32,
        elem_schema_ref: i64,
    },
    /// Element count of an `Array<T>` word payload.
    ///
    /// The twin of [`Op::LoadArrayWord`] over the same payload header:
    /// `frame[array]` is a store or molten handle whose payload must be an
    /// array payload with matching `elem_schema_ref`. A well-formed payload
    /// writes its count to `dst` and [`ArrayOpStatus::Ok`] to `status`; a
    /// malformed or absent one writes zero to `dst` and a precise status.
    /// Length is a property of the value, never of the frame layout.
    LoadArrayLen {
        dst: u32,
        status: u32,
        array: u32,
        elem_schema_ref: i64,
    },
    /// Lexicographically compare two resident value-memory byte runs.
    ///
    /// `frame[a]` and `frame[b]` are value handles. The result is the closed
    /// three-way ordinal `0 = less`, `1 = equal`, `2 = greater`. Task admission
    /// must have made every handle it compares resident in the value-memory
    /// table; even equal handle integers fault if the shared handle is not
    /// resident.
    CompareValueBytes { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = f64::from_bits(bits)` — the immediate carries the
    /// BIT PATTERN (keeps `Op: Eq`; the machine is type-blind about a
    /// 64-bit store anyway — the op exists so lowerings and readers
    /// see intent). Total-order/NaN canonicalization is the LANGUAGE's
    /// value-level concern (vix's TotalF64), not the machine's:
    /// arithmetic here is plain IEEE, identical across lanes.
    ConstF64 { dst: u32, bits: u64 },
    /// `frame[dst] = frame[a] + frame[b]` (f64, IEEE).
    AddF64 { dst: u32, a: u32, b: u32 },
    /// `frame[dst] = frame[a] * frame[b]` (f64, IEEE).
    MulF64 { dst: u32, a: u32, b: u32 },
    /// INSTRUMENTATION (the unified-trace ruling): lowerings emit
    /// trace marks freely; the MODE decides their cost. Innards mode
    /// records [`TaskEvent::Mark`]; Production mode strips them — in
    /// the interpreter a skip, in the JIT the op is simply NOT
    /// COMPILED (zero instructions in the chain). Static ids map back
    /// to source constructs in the lowering's tables.
    Trace { id: u32 },
    /// SYNC host call (Amos's refinement, ruled): a host operation
    /// that ALWAYS completes — no await-point numbering, no park
    /// machinery, no spill obligations beyond frame residency (which
    /// three-address gives anyway). The host function receives the
    /// current frame's bytes and reads/writes at offsets its contract
    /// (known to the lowering) declares — the frame-direct convention
    /// extended to the host boundary. `host` indexes the table passed
    /// to [`Task::run_hosted`].
    HostCall { host: u32 },
    /// Sync host call that yields to the outer driver after completion.
    ///
    /// Use this when host effects change native value-memory provenance and
    /// the next machine op may read through that provenance.
    HostCallYield { host: u32 },
}

/// A synchronous host operation over the current frame's bytes.
pub type HostFn<'h> = &'h mut dyn FnMut(&mut [u8]);

/// An owned sync host operation, as [`TaskExec`] carries them.
pub type BoxedHostFn<'h> = Box<dyn FnMut(&mut [u8]) + 'h>;

/// Frame-granular trace events (the ruled vocabulary, recorded
/// directly in this slice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskEvent {
    FrameEntered(FnId),
    FrameExited(FnId),
    Parked {
        input: u32,
    },
    Resumed,
    /// An [`Op::Trace`] instrumentation mark (Innards mode only).
    Mark(u32),
}

/// How much instrumentation a program instance carries. Tests trace
/// innards; production keeps only the structural events (frames,
/// parks) needed for observability.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TraceMode {
    /// Record every instrumentation mark.
    #[default]
    Innards,
    /// Strip instrumentation marks entirely.
    Production,
}

/// The result of driving a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStep {
    /// The root frame returned; the result is in [`Task::result`].
    Done,
    /// A sync host call completed and the task can be re-entered immediately.
    Yielded,
    /// Parked on an unready input — started-and-blocked, the only
    /// kind of suspension that exists.
    Parked { input: u32 },
}

#[derive(Clone, Debug)]
struct FrameRecord {
    fn_id: FnId,
    /// Arena offset of this frame's first byte.
    base: usize,
    pc: usize,
    /// Absolute arena offset in the CALLER's frame where our `Ret`
    /// writes; `None` for the root frame (writes to the task result).
    ret_to: Option<usize>,
}

/// A task: a frame arena, the live frame chain, and the trace. This
/// struct IS the suspended state — parking is returning.
#[derive(Clone, Debug)]
pub struct Task {
    arena: Vec<u8>,
    molten: MoltenArena,
    frames: Vec<FrameRecord>,
    /// Root return bytes once [`TaskStep::Done`].
    pub result: Vec<u8>,
    pub trace: Vec<TaskEvent>,
    parked_on: Option<u32>,
    mode: TraceMode,
}

impl Task {
    /// Spawn with the entry function's frame allocated. Callers of the
    /// task write entry arguments through [`Task::write_i64`] before
    /// the first [`Task::run`] — the frame-direct convention applies
    /// at the boundary too.
    #[must_use]
    pub fn spawn(program: &Program, entry: FnId) -> Self {
        Self::spawn_with_mode(program, entry, TraceMode::Innards)
    }

    #[must_use]
    pub fn spawn_with_mode(program: &Program, entry: FnId, mode: TraceMode) -> Self {
        let mut task = Task {
            arena: Vec::new(),
            molten: MoltenArena::default(),
            frames: Vec::new(),
            result: Vec::new(),
            trace: Vec::new(),
            parked_on: None,
            mode,
        };
        let base = task.alloc_frame(program.fns[entry.0 as usize].frame);
        task.frames.push(FrameRecord {
            fn_id: entry,
            base,
            pc: 0,
            ret_to: None,
        });
        task.trace.push(TaskEvent::FrameEntered(entry));
        task
    }

    /// Live frame count (the chain that survives parking).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Write an i64 into the CURRENT frame at `offset` — used for
    /// entry arguments and by tests to poke frames.
    pub fn write_i64(&mut self, offset: u32, value: i64) {
        let base = self.frames.last().expect("live frame").base;
        write_i64_at(&mut self.arena, base + offset as usize, value);
    }

    /// Read an i64 from the task result (root return bytes).
    #[must_use]
    pub fn result_i64(&self) -> i64 {
        i64::from_le_bytes(self.result[..8].try_into().expect("8-byte result"))
    }

    fn alloc_frame(&mut self, layout: Layout) -> usize {
        let align = layout.align.max(1);
        let base = self.arena.len().div_ceil(align) * align;
        self.arena.resize(base + layout.size, 0);
        base
    }

    /// Drive until the root returns or the task parks. `ready` and
    /// `awaited` are indexed by await input, exactly as in the proven
    /// suspend protocol. A ready slot is consumed when its await reads
    /// it. Programs containing [`Op::HostCall`] must use [`Task::run_hosted`].
    pub fn run(&mut self, program: &Program, ready: &mut [bool], awaited: &[i64]) -> TaskStep {
        self.run_hosted(program, ready, awaited, &mut [])
    }

    /// [`Task::run`] with a host table for sync host calls.
    pub fn run_hosted(
        &mut self,
        program: &Program,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
    ) -> TaskStep {
        self.run_hosted_with_value_memories(program, ready, awaited, hosts, ValueMemories::empty())
    }

    pub fn run_hosted_with_value_memories(
        &mut self,
        program: &Program,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> TaskStep {
        self.run_hosted_with_value_memories_inner(
            None,
            program,
            ready,
            awaited,
            hosts,
            value_memories,
        )
        .unwrap_or_else(|fault| panic!("legacy raw task fault: {fault:?}"))
    }

    pub(crate) fn run_verified_with_value_memories(
        &mut self,
        verified: &VerifiedProgram,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> Result<TaskStep, TaskFault> {
        self.run_hosted_with_value_memories_inner(
            Some(verified),
            verified.program(),
            ready,
            awaited,
            hosts,
            value_memories,
        )
    }

    fn run_hosted_with_value_memories_inner(
        &mut self,
        verified: Option<&VerifiedProgram>,
        program: &Program,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> Result<TaskStep, TaskFault> {
        loop {
            let frame = self.frames.last().expect("running task has a frame");
            let base = frame.base;
            let fn_id = frame.fn_id;
            let pc = frame.pc;
            let code = &program.fns[frame.fn_id.0 as usize].code;
            if frame.pc >= code.len() {
                panic!("function {:?} fell off its code without Ret", fn_id);
            }
            match code[frame.pc].clone() {
                Op::ConstI64 { dst, value } => {
                    write_i64_at(&mut self.arena, base + dst as usize, value);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::AddI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, va.wrapping_add(vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::MulI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, va.wrapping_mul(vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::DivI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    let value = if vb == 0 { 0 } else { va.wrapping_div(vb) };
                    write_i64_at(&mut self.arena, base + dst as usize, value);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::SubI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, va.wrapping_sub(vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::CopyI64 { dst, src } => {
                    let v = read_i64_at(&self.arena, base + src as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, v);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::EqI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va == vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::NeI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va != vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::LtI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va < vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::LeI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va <= vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::GtI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va > vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::GeI64 { dst, a, b } => {
                    let va = read_i64_at(&self.arena, base + a as usize);
                    let vb = read_i64_at(&self.arena, base + b as usize);
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(va >= vb));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::Jump { target } => {
                    self.frames.last_mut().expect("frame").pc = target as usize;
                }
                Op::JumpIfZero { value, target } => {
                    let v = read_i64_at(&self.arena, base + value as usize);
                    let frame = self.frames.last_mut().expect("frame");
                    if v == 0 {
                        frame.pc = target as usize;
                    } else {
                        frame.pc += 1;
                    }
                }
                Op::Call { callee, args, ret } => {
                    // Advance the caller past the call BEFORE entering:
                    // resumption re-enters the callee, and the caller
                    // continues after the callee's Ret.
                    self.frames.last_mut().expect("frame").pc += 1;
                    let callee_frame = self.alloc_frame(program.fns[callee.0 as usize].frame);
                    for copy in &args {
                        // Frame-direct: caller bytes land at the
                        // callee's statically known offsets.
                        let src = base + copy.src as usize;
                        let dst = callee_frame + copy.dst as usize;
                        self.arena.copy_within(src..src + copy.size as usize, dst);
                    }
                    self.frames.push(FrameRecord {
                        fn_id: callee,
                        base: callee_frame,
                        pc: 0,
                        ret_to: Some(base + ret as usize),
                    });
                    self.trace.push(TaskEvent::FrameEntered(callee));
                }
                Op::CallIndirect { callee, args, ret } => {
                    let raw = read_i64_at(&self.arena, base + callee as usize);
                    let callee = if raw < 0 {
                        let Some(verified) = verified else {
                            panic!("indirect callee is a non-negative local function id");
                        };
                        return Err(TaskFault::IndirectCalleeNegative {
                            site: fault_site(verified, fn_id, pc)?,
                            value: raw,
                        });
                    } else {
                        match u32::try_from(raw) {
                            Ok(callee) => FnId(callee),
                            Err(_) => {
                                let Some(verified) = verified else {
                                    panic!("indirect callee fits a local function id");
                                };
                                let site = fault_site(verified, fn_id, pc)?;
                                let function_count = site
                                    .call
                                    .and_then(|call| match call {
                                        CallSiteFacts::Indirect { obligation, .. } => {
                                            Some(obligation.function_count)
                                        }
                                        CallSiteFacts::Direct { .. } => None,
                                    })
                                    .unwrap_or_else(|| verified.program().fns.len());
                                return Err(TaskFault::IndirectCalleeOutOfRange {
                                    site,
                                    callee: raw,
                                    function_count,
                                });
                            }
                        }
                    };
                    if let Some(verified) = verified {
                        check_indirect_callee_contract(verified, fn_id, pc, callee)?;
                    }
                    self.frames.last_mut().expect("frame").pc += 1;
                    let callee_frame = self.alloc_frame(program.fns[callee.0 as usize].frame);
                    for copy in &args {
                        let src = base + copy.src as usize;
                        let dst = callee_frame + copy.dst as usize;
                        self.arena.copy_within(src..src + copy.size as usize, dst);
                    }
                    self.frames.push(FrameRecord {
                        fn_id: callee,
                        base: callee_frame,
                        pc: 0,
                        ret_to: Some(base + ret as usize),
                    });
                    self.trace.push(TaskEvent::FrameEntered(callee));
                }
                Op::Ret { src, size } => {
                    let popped = self.frames.pop().expect("frame to return from");
                    self.trace.push(TaskEvent::FrameExited(popped.fn_id));
                    let start = popped.base + src as usize;
                    match popped.ret_to {
                        Some(ret_to) => {
                            self.arena.copy_within(start..start + size as usize, ret_to);
                        }
                        None => {
                            self.result = self.arena[start..start + size as usize].to_vec();
                            return Ok(TaskStep::Done);
                        }
                    }
                }
                Op::LoadIndexedI64 {
                    dst,
                    base: arr,
                    index,
                    stride,
                } => {
                    let ix = read_i64_at(&self.arena, base + index as usize);
                    let at = base + arr as usize + ix as usize * stride as usize;
                    let v = read_i64_at(&self.arena, at);
                    write_i64_at(&mut self.arena, base + dst as usize, v);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::StoreIndexedI64 {
                    base: arr,
                    index,
                    stride,
                    src,
                } => {
                    let ix = read_i64_at(&self.arena, base + index as usize);
                    let v = read_i64_at(&self.arena, base + src as usize);
                    let at = base + arr as usize + ix as usize * stride as usize;
                    write_i64_at(&mut self.arena, at, v);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::ArrayNew {
                    dst,
                    status,
                    count_slot,
                    elem_width,
                    elem_schema_ref,
                } => {
                    let count = read_i64_at(&self.arena, base + count_slot as usize);
                    let mut handle = ARRAY_POISON_HANDLE;
                    write_i64_at(&mut self.arena, base + dst as usize, handle);
                    let op_status = self
                        .molten
                        .alloc_array(count, elem_width as usize, elem_schema_ref)
                        .map(|allocated| {
                            handle = allocated;
                            ArrayOpStatus::Ok
                        })
                        .unwrap_or_else(|err| err);
                    write_i64_at(&mut self.arena, base + dst as usize, handle);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::ArrayStoreWord {
                    status,
                    array,
                    index,
                    src,
                    elem_schema_ref,
                } => {
                    let array = read_i64_at(&self.arena, base + array as usize);
                    let index = read_i64_at(&self.arena, base + index as usize);
                    let status_value = store_array_region(
                        &mut self.molten,
                        ArrayRegion {
                            array,
                            index,
                            elem_width: 8,
                            elem_schema_ref,
                        },
                        &self.arena[base + src as usize..],
                    );
                    write_i64_at(&mut self.arena, base + status as usize, status_value as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::ArrayStore {
                    status,
                    array,
                    index,
                    src,
                    elem_width,
                    elem_schema_ref,
                } => {
                    let array = read_i64_at(&self.arena, base + array as usize);
                    let index = read_i64_at(&self.arena, base + index as usize);
                    let status_value = store_array_region(
                        &mut self.molten,
                        ArrayRegion {
                            array,
                            index,
                            elem_width: elem_width as usize,
                            elem_schema_ref,
                        },
                        &self.arena[base + src as usize..],
                    );
                    write_i64_at(&mut self.arena, base + status as usize, status_value as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::LoadArrayWord {
                    dst,
                    present,
                    array,
                    index,
                    elem_schema_ref,
                } => {
                    let array = read_i64_at(&self.arena, base + array as usize);
                    let index = read_i64_at(&self.arena, base + index as usize);
                    let (ok, value) = load_array_word(
                        value_memories,
                        &self.molten,
                        array,
                        index,
                        elem_schema_ref,
                    );
                    write_i64_at(&mut self.arena, base + dst as usize, value);
                    write_i64_at(&mut self.arena, base + present as usize, i64::from(ok));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::LoadArray {
                    dst,
                    status,
                    array,
                    index,
                    elem_width,
                    elem_schema_ref,
                } => {
                    let array = read_i64_at(&self.arena, base + array as usize);
                    let index = read_i64_at(&self.arena, base + index as usize);
                    let dst_at = base + dst as usize;
                    let status_value = {
                        let dst = &mut self.arena[dst_at..];
                        load_array_region(
                            value_memories.into(),
                            &self.molten,
                            ArrayRegion {
                                array,
                                index,
                                elem_width: elem_width as usize,
                                elem_schema_ref,
                            },
                            dst,
                        )
                    };
                    write_i64_at(&mut self.arena, base + status as usize, status_value as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::LoadArrayLen {
                    dst,
                    status,
                    array,
                    elem_schema_ref,
                } => {
                    let array = read_i64_at(&self.arena, base + array as usize);
                    let (status_value, value) =
                        load_array_len(value_memories.into(), &self.molten, array, elem_schema_ref);
                    write_i64_at(&mut self.arena, base + dst as usize, value);
                    write_i64_at(&mut self.arena, base + status as usize, status_value as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::CompareValueBytes { dst, a, b } => {
                    let a = read_i64_at(&self.arena, base + a as usize);
                    let b = read_i64_at(&self.arena, base + b as usize);
                    let ordering = match compare_value_bytes(value_memories, &self.molten, a, b) {
                        Ok(ordering) => ordering,
                        Err((side, handle)) => {
                            let Some(verified) = verified else {
                                panic!("legacy raw CompareValueBytes operand is not resident");
                            };
                            return Err(TaskFault::UnresidentCompareValueBytes {
                                site: fault_site(verified, fn_id, pc)?,
                                side,
                                handle,
                            });
                        }
                    };
                    write_i64_at(&mut self.arena, base + dst as usize, ordering);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::Trace { id } => {
                    if self.mode == TraceMode::Innards {
                        self.trace.push(TaskEvent::Mark(id));
                    }
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::ConstF64 { dst, bits } => {
                    write_i64_at(&mut self.arena, base + dst as usize, bits as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::AddF64 { dst, a, b } => {
                    let va = f64::from_bits(read_i64_at(&self.arena, base + a as usize) as u64);
                    let vb = f64::from_bits(read_i64_at(&self.arena, base + b as usize) as u64);
                    write_i64_at(
                        &mut self.arena,
                        base + dst as usize,
                        (va + vb).to_bits() as i64,
                    );
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::MulF64 { dst, a, b } => {
                    let va = f64::from_bits(read_i64_at(&self.arena, base + a as usize) as u64);
                    let vb = f64::from_bits(read_i64_at(&self.arena, base + b as usize) as u64);
                    write_i64_at(
                        &mut self.arena,
                        base + dst as usize,
                        (va * vb).to_bits() as i64,
                    );
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::HostCall { host } => {
                    let frame_layout = program.fns[fn_id.0 as usize].frame;
                    let end = base + frame_layout.size;
                    hosts[host as usize](&mut self.arena[base..end]);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::HostCallYield { host } => {
                    let frame_layout = program.fns[fn_id.0 as usize].frame;
                    let end = base + frame_layout.size;
                    hosts[host as usize](&mut self.arena[base..end]);
                    self.frames.last_mut().expect("frame").pc += 1;
                    return Ok(TaskStep::Yielded);
                }
                Op::Await { dst, input } => {
                    let idx = input as usize;
                    if let Some(is_ready) = ready.get_mut(idx)
                        && *is_ready
                    {
                        *is_ready = false;
                        if self.parked_on == Some(input) {
                            self.parked_on = None;
                            self.trace.push(TaskEvent::Resumed);
                        }
                        write_i64_at(&mut self.arena, base + dst as usize, awaited[idx]);
                        self.frames.last_mut().expect("frame").pc += 1;
                    } else {
                        // Started-and-blocked: the arena and frame
                        // chain ARE the suspended state; leave pc AT
                        // the await so resume re-checks it.
                        if self.parked_on != Some(input) {
                            self.parked_on = Some(input);
                            self.trace.push(TaskEvent::Parked { input });
                        }
                        return Ok(TaskStep::Parked { input });
                    }
                }
            }
        }
    }
}

/// One burst of task progress — implemented by both lanes so the
/// executor driver below (and vix's demand driver later) can hold
/// either without caring which.
pub trait Advance {
    fn advance(
        &mut self,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> TaskStep;
    fn result_bytes(&self) -> &[u8];
}

/// The interpreter lane bundled with its program.
pub struct Running<'p> {
    pub program: &'p Program,
    pub task: Task,
}

impl Advance for Running<'_> {
    fn advance(
        &mut self,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> TaskStep {
        self.task.run_hosted_with_value_memories(
            self.program,
            ready,
            awaited,
            hosts,
            value_memories,
        )
    }

    fn result_bytes(&self) -> &[u8] {
        &self.task.result
    }
}

/// TOOTH 3 — the async host boundary: a task driven as a real Rust
/// [`Future`], one input future per await index (an "async host call"
/// IS an await whose input is fed by the host's future — the ruled
/// sync/async distinction from the other side). Depends only on
/// `core::future`; bring any executor. The waker-precision rule from
/// the proven async lane carries over: while parked, a wakeup that
/// didn't ready the BLOCKING input never re-enters the lane.
pub struct TaskExec<'h, A: Advance> {
    lane: A,
    inners: Vec<Pin<Box<dyn Future<Output = i64> + 'h>>>,
    hosts: Vec<BoxedHostFn<'h>>,
    resolved: Vec<bool>,
    ready: Vec<bool>,
    awaited: Vec<i64>,
    parked_on: Option<u32>,
}

impl<'h, A: Advance> TaskExec<'h, A> {
    pub fn new(
        lane: A,
        inners: Vec<Pin<Box<dyn Future<Output = i64> + 'h>>>,
        hosts: Vec<BoxedHostFn<'h>>,
    ) -> Self {
        let n = inners.len();
        TaskExec {
            lane,
            inners,
            hosts,
            resolved: vec![false; n],
            ready: vec![false; n],
            awaited: vec![0; n],
            parked_on: None,
        }
    }

    pub fn lane(&self) -> &A {
        &self.lane
    }
}

impl<A: Advance + Unpin> Future for TaskExec<'_, A> {
    type Output = Vec<u8>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Vec<u8>> {
        let this = &mut *self;

        // Drive EVERY unresolved input; independent awaits make
        // progress concurrently.
        for i in 0..this.inners.len() {
            if !this.resolved[i]
                && let Poll::Ready(value) = this.inners[i].as_mut().poll(cx)
            {
                this.awaited[i] = value;
                this.ready[i] = true;
                this.resolved[i] = true;
            }
        }

        // Parked and the blocking input still isn't ready: don't
        // re-enter the lane.
        if let Some(i) = this.parked_on
            && !this.ready[i as usize]
        {
            return Poll::Pending;
        }

        let mut host_refs: Vec<HostFn<'_>> = this
            .hosts
            .iter_mut()
            .map(|h| h.as_mut() as HostFn<'_>)
            .collect();
        loop {
            match this.lane.advance(
                &mut this.ready,
                &this.awaited,
                &mut host_refs,
                ValueMemories::empty(),
            ) {
                TaskStep::Done => return Poll::Ready(this.lane.result_bytes().to_vec()),
                TaskStep::Yielded => {}
                TaskStep::Parked { input } => {
                    this.parked_on = Some(input);
                    return Poll::Pending;
                }
            }
        }
    }
}

fn read_i64_at(arena: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(arena[at..at + 8].try_into().expect("aligned i64 slot"))
}

fn write_i64_at(arena: &mut [u8], at: usize, value: i64) {
    arena[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

/// Resolve a handle to its payload. Negative handles name molten values: the
/// task-local and externally lent namespaces are disjoint. Nonnegative handles
/// index the lent store table.
fn handle_bytes<'a>(
    value_memories: MemoryView<'a>,
    molten: &'a MoltenArena,
    handle: i64,
) -> Result<&'a [u8], ArrayOpStatus> {
    match classify_handle(handle).ok_or(ArrayOpStatus::InvalidHandle)? {
        HandleKind::TaskMolten(_) => molten.bytes(handle).ok_or(ArrayOpStatus::InvalidHandle),
        HandleKind::LentMolten(index) => value_memories.molten(index),
        HandleKind::Store(index) => value_memories.store(index),
    }
}

struct ResidentPayload<'a> {
    bytes: &'a [u8],
    initialized: Option<&'a [bool]>,
}

fn handle_payload<'a>(
    value_memories: MemoryView<'a>,
    molten: &'a MoltenArena,
    handle: i64,
) -> Result<ResidentPayload<'a>, ArrayOpStatus> {
    match classify_handle(handle).ok_or(ArrayOpStatus::InvalidHandle)? {
        HandleKind::TaskMolten(_) => {
            let buffer = molten.buffer(handle).ok_or(ArrayOpStatus::InvalidHandle)?;
            Ok(ResidentPayload {
                bytes: &buffer.bytes,
                initialized: Some(&buffer.initialized),
            })
        }
        HandleKind::LentMolten(index) => Ok(ResidentPayload {
            bytes: value_memories.molten(index)?,
            initialized: None,
        }),
        HandleKind::Store(index) => Ok(ResidentPayload {
            bytes: value_memories.store(index)?,
            initialized: None,
        }),
    }
}

struct ArrayPayload<'a> {
    bytes: &'a [u8],
    count: usize,
    elem_width: usize,
    body_offset: usize,
}

#[derive(Clone, Copy)]
struct ArrayRegion {
    array: i64,
    index: i64,
    elem_width: usize,
    elem_schema_ref: i64,
}

fn parse_array_payload<'a>(
    bytes: &'a [u8],
    elem_schema_ref: i64,
    expected_elem_width: Option<usize>,
) -> Result<ArrayPayload<'a>, ArrayOpStatus> {
    // Structural validation FIRST, before any schema comparison: a minimum
    // header, a recognized tag, a positive element width, and a checked exact
    // total length. Only bytes that pass this gate are a structurally valid
    // array; malformed external bytes are classified `MalformedPayload` even
    // when their schema word happens to differ from what a caller expects.
    if bytes.len() < ARRAY_WORDS_HEADER_SIZE {
        return Err(ArrayOpStatus::MalformedPayload);
    }
    let tag = read_i64_at(bytes, 0);
    let (elem_width, body_offset) = match tag {
        // Live compatibility path: existing store-backed `LoadArrayWord`
        // callers and `weavy/examples/array_get_bench.rs` still provide the
        // original one-word payload format.
        ARRAY_WORDS_TAG => (8usize, ARRAY_WORDS_HEADER_SIZE),
        ARRAY_ELEMENTS_TAG => {
            if bytes.len() < ARRAY_ELEMENTS_HEADER_SIZE {
                return Err(ArrayOpStatus::MalformedPayload);
            }
            let elem_width = usize::try_from(read_i64_at(bytes, 24))
                .map_err(|_| ArrayOpStatus::MalformedPayload)?;
            if elem_width == 0 {
                return Err(ArrayOpStatus::MalformedPayload);
            }
            (elem_width, ARRAY_ELEMENTS_HEADER_SIZE)
        }
        _ => return Err(ArrayOpStatus::MalformedPayload),
    };
    let count =
        usize::try_from(read_i64_at(bytes, 16)).map_err(|_| ArrayOpStatus::MalformedPayload)?;
    let expected_len = count
        .checked_mul(elem_width)
        .and_then(|n| body_offset.checked_add(n))
        .ok_or(ArrayOpStatus::MalformedPayload)?;
    if bytes.len() != expected_len {
        return Err(ArrayOpStatus::MalformedPayload);
    }
    // Structurally valid: now the schema, then the expected element width.
    // A valid array of another schema is `SchemaMismatch`; a valid array of the
    // matching schema but a different width is `WidthMismatch`.
    let schema = read_i64_at(bytes, 8);
    if schema != elem_schema_ref {
        return Err(ArrayOpStatus::SchemaMismatch);
    }
    if let Some(expected) = expected_elem_width
        && elem_width != expected
    {
        return Err(ArrayOpStatus::WidthMismatch);
    }
    Ok(ArrayPayload {
        bytes,
        count,
        elem_width,
        body_offset,
    })
}

fn load_array_word(
    value_memories: ValueMemories<'_>,
    molten: &MoltenArena,
    array: i64,
    index: i64,
    elem_schema_ref: i64,
) -> (bool, i64) {
    let mut value = [0u8; 8];
    let status = load_array_region(
        value_memories.into(),
        molten,
        ArrayRegion {
            array,
            index,
            elem_width: 8,
            elem_schema_ref,
        },
        &mut value,
    );
    (status == ArrayOpStatus::Ok, i64::from_le_bytes(value))
}

fn load_array_len(
    value_memories: MemoryView<'_>,
    molten: &MoltenArena,
    array: i64,
    elem_schema_ref: i64,
) -> (ArrayOpStatus, i64) {
    match handle_bytes(value_memories, molten, array)
        .and_then(|bytes| parse_array_payload(bytes, elem_schema_ref, None))
    {
        Ok(payload) => match count_i64(payload.count) {
            Ok(count) => (ArrayOpStatus::Ok, count),
            Err(status) => (status, 0),
        },
        Err(status) => (status, 0),
    }
}

fn load_array_region(
    value_memories: MemoryView<'_>,
    molten: &MoltenArena,
    region: ArrayRegion,
    dst: &mut [u8],
) -> ArrayOpStatus {
    if region.elem_width == 0 {
        return ArrayOpStatus::WidthMismatch;
    }
    let copy_len = region.elem_width.min(dst.len());
    dst[..copy_len].fill(0);
    let resident = match handle_payload(value_memories, molten, region.array) {
        Ok(resident) => resident,
        Err(status) => return status,
    };
    let payload = match parse_array_payload(
        resident.bytes,
        region.elem_schema_ref,
        Some(region.elem_width),
    ) {
        Ok(payload) => payload,
        Err(status) => return status,
    };
    if dst.len() < region.elem_width {
        return ArrayOpStatus::Overflow;
    }
    let (offset, elem_index) = match payload_element_offset(&payload, region.index) {
        Ok(located) => located,
        Err(status) => return status,
    };
    if let Some(initialized) = resident.initialized
        && !initialized[elem_index]
    {
        return ArrayOpStatus::Uninitialized;
    }
    dst[..region.elem_width].copy_from_slice(&payload.bytes[offset..offset + region.elem_width]);
    ArrayOpStatus::Ok
}

fn store_array_region(molten: &mut MoltenArena, region: ArrayRegion, src: &[u8]) -> ArrayOpStatus {
    if region.elem_width == 0 {
        return ArrayOpStatus::WidthMismatch;
    }
    let Some(buffer) = molten.buffer_mut(region.array) else {
        return ArrayOpStatus::InvalidHandle;
    };
    let (offset, elem_index) = {
        let payload = match parse_array_payload(
            &buffer.bytes,
            region.elem_schema_ref,
            Some(region.elem_width),
        ) {
            Ok(payload) => payload,
            Err(status) => return status,
        };
        if src.len() < region.elem_width {
            return ArrayOpStatus::Overflow;
        }
        match payload_element_offset(&payload, region.index) {
            Ok(located) => located,
            Err(status) => return status,
        }
    };
    buffer.bytes[offset..offset + region.elem_width].copy_from_slice(&src[..region.elem_width]);
    buffer.initialized[elem_index] = true;
    ArrayOpStatus::Ok
}

/// Locate a whole element within a structurally valid payload. Returns the
/// element's byte offset and its element index. The element width is not
/// re-checked here: `parse_array_payload` was called with `Some(elem_width)`,
/// so `payload.elem_width` already equals the operation's expected width.
fn payload_element_offset(
    payload: &ArrayPayload<'_>,
    index: i64,
) -> Result<(usize, usize), ArrayOpStatus> {
    let index = usize::try_from(index).map_err(|_| ArrayOpStatus::OutOfRange)?;
    if index >= payload.count {
        return Err(ArrayOpStatus::OutOfRange);
    }
    let offset = payload
        .body_offset
        .checked_add(
            index
                .checked_mul(payload.elem_width)
                .ok_or(ArrayOpStatus::Overflow)?,
        )
        .ok_or(ArrayOpStatus::Overflow)?;
    Ok((offset, index))
}

fn check_indirect_callee_contract(
    verified: &VerifiedProgram,
    function: FnId,
    pc: usize,
    callee: FnId,
) -> Result<(), TaskFault> {
    let site = fault_site(verified, function, pc)?;
    let Some(CallSiteFacts::Indirect { obligation, .. }) = site.call else {
        return Err(TaskFault::MissingIndirectCallFacts { site });
    };
    let callee_index = callee.0 as usize;
    if callee_index >= obligation.function_count {
        return Err(TaskFault::IndirectCalleeOutOfRange {
            site,
            callee: i64::from(callee.0),
            function_count: obligation.function_count,
        });
    }
    let actual = verified
        .facts()
        .function(callee)
        .and_then(|function| function.call_contract());
    if actual != Some(obligation.contract) {
        return Err(TaskFault::IndirectCalleeContractMismatch {
            site,
            callee,
            expected: obligation.contract,
            actual,
        });
    }
    Ok(())
}

fn compare_value_bytes(
    value_memories: ValueMemories<'_>,
    molten: &MoltenArena,
    a: i64,
    b: i64,
) -> Result<i64, (CompareSide, i64)> {
    let memories = MemoryView::from(value_memories);
    let a_bytes = handle_bytes(memories, molten, a).map_err(|_| (CompareSide::Left, a))?;
    if a == b {
        return Ok(1);
    }
    let b_bytes = handle_bytes(memories, molten, b).map_err(|_| (CompareSide::Right, b))?;
    Ok(match a_bytes.cmp(b_bytes) {
        core::cmp::Ordering::Less => 0,
        core::cmp::Ordering::Equal => 1,
        core::cmp::Ordering::Greater => 2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::Access;
    use crate::mem::declared::{declared_struct, i64_};

    fn frame_of_i64s(n: usize) -> Layout {
        Layout {
            size: n * 8,
            align: 8,
        }
    }

    #[test]
    fn task_molten_handle_namespace_reserves_poison_and_lent_space() {
        assert_eq!(task_molten_handle(0), Some(ARRAY_POISON_HANDLE + 1));
        assert_eq!(task_molten_index(ARRAY_POISON_HANDLE), None);
        assert_eq!(classify_handle(ARRAY_POISON_HANDLE), None);
        assert_eq!(task_molten_index(ARRAY_POISON_HANDLE + 1), Some(0));
        assert_eq!(lent_molten_index(ARRAY_POISON_HANDLE), None);
        assert_eq!(lent_molten_index(ARRAY_POISON_HANDLE + 1), None);
        assert_eq!(lent_molten_index(LENT_MOLTEN_MIN - 1), None);
        let old_truncating_u32_index = ((-1i128 - i128::from(LENT_MOLTEN_MIN - 1)) as u32) as usize;
        assert_eq!(old_truncating_u32_index, 0);
        assert_eq!(classify_handle(-1), Some(HandleKind::LentMolten(0)));
        assert_eq!(lent_molten_index(-1), Some(0));

        let max_index_i64 = LENT_MOLTEN_MIN - TASK_MOLTEN_FIRST - 1;
        if let Ok(max_index) = usize::try_from(max_index_i64) {
            let last = task_molten_handle(max_index).expect("last encodable handle");
            assert_eq!(last, LENT_MOLTEN_MIN - 1);
            assert_eq!(task_molten_index(last), Some(max_index));
            assert_eq!(task_molten_handle(max_index.saturating_add(1)), None);
        } else {
            let max_index = usize::MAX;
            let last = task_molten_handle(max_index).expect("usize::MAX still fits on this target");
            assert!(last < LENT_MOLTEN_MIN);
            assert_eq!(task_molten_index(last), Some(max_index));
        }

        let first_lent_index = (-1i64).checked_sub(LENT_MOLTEN_MIN).unwrap();
        assert_eq!(
            lent_molten_index(LENT_MOLTEN_MIN),
            usize::try_from(first_lent_index).ok()
        );
    }

    /// callee(x, y) at offsets 0,8 -> returns (x*y)+x from slot 16.
    fn mul_plus_x() -> Fn {
        Fn {
            frame: frame_of_i64s(3),
            code: vec![
                Op::MulI64 {
                    dst: 16,
                    a: 0,
                    b: 8,
                },
                Op::AddI64 {
                    dst: 16,
                    a: 16,
                    b: 0,
                },
                Op::Ret { src: 16, size: 8 },
            ],
        }
    }

    #[test]
    fn frame_direct_calls_compute_and_trace_frames() {
        // outer: locals a@0=6, b@8=7; calls callee(a,b) -> ret@16;
        // returns ret+a.
        let program = Program {
            fns: vec![
                Fn {
                    frame: frame_of_i64s(3),
                    code: vec![
                        Op::ConstI64 { dst: 0, value: 6 },
                        Op::ConstI64 { dst: 8, value: 7 },
                        Op::Call {
                            callee: FnId(1),
                            args: vec![
                                ArgCopy {
                                    src: 0,
                                    dst: 0,
                                    size: 8,
                                },
                                ArgCopy {
                                    src: 8,
                                    dst: 8,
                                    size: 8,
                                },
                            ],
                            ret: 16,
                        },
                        Op::AddI64 {
                            dst: 16,
                            a: 16,
                            b: 0,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                },
                mul_plus_x(),
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        // (6*7)+6 = 48, +6 again in the caller = 54.
        assert_eq!(task.result_i64(), 54);
        assert_eq!(
            task.trace,
            vec![
                TaskEvent::FrameEntered(FnId(0)),
                TaskEvent::FrameEntered(FnId(1)),
                TaskEvent::FrameExited(FnId(1)),
                TaskEvent::FrameExited(FnId(0)),
            ]
        );
    }

    #[test]
    fn parking_preserves_the_live_frame_chain() {
        // outer: local salt@0=100; calls callee -> ret@8; returns
        // ret+salt. callee: awaits input#0 into 0, doubles it, returns.
        // The park happens two frames deep; the caller's local must
        // survive in the arena across the suspension.
        let program = Program {
            fns: vec![
                Fn {
                    frame: frame_of_i64s(2),
                    code: vec![
                        Op::ConstI64 { dst: 0, value: 100 },
                        Op::Call {
                            callee: FnId(1),
                            args: vec![],
                            ret: 8,
                        },
                        Op::AddI64 { dst: 8, a: 8, b: 0 },
                        Op::Ret { src: 8, size: 8 },
                    ],
                },
                Fn {
                    frame: frame_of_i64s(1),
                    code: vec![
                        Op::Await { dst: 0, input: 0 },
                        Op::AddI64 { dst: 0, a: 0, b: 0 },
                        Op::Ret { src: 0, size: 8 },
                    ],
                },
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        let mut ready = [false];

        assert_eq!(
            task.run(&program, &mut ready, &[0]),
            TaskStep::Parked { input: 0 }
        );
        assert_eq!(task.depth(), 2, "both frames live while parked");
        assert!(task.trace.contains(&TaskEvent::Parked { input: 0 }));

        // The task struct IS the suspended state; nothing else exists.
        ready[0] = true;
        assert_eq!(task.run(&program, &mut ready, &[21]), TaskStep::Done);
        assert_eq!(task.result_i64(), 21 * 2 + 100);
        assert!(task.trace.contains(&TaskEvent::Resumed));
        let exits: Vec<_> = task
            .trace
            .iter()
            .filter(|e| matches!(e, TaskEvent::FrameExited(_)))
            .collect();
        assert_eq!(exits.len(), 2);
    }

    #[test]
    fn ready_awaits_never_park() {
        let program = Program {
            fns: vec![Fn {
                frame: frame_of_i64s(1),
                code: vec![Op::Await { dst: 0, input: 0 }, Op::Ret { src: 0, size: 8 }],
            }],
        };
        let mut task = Task::spawn(&program, FnId(0));
        let mut ready = [true];
        assert_eq!(task.run(&program, &mut ready, &[42]), TaskStep::Done);
        assert_eq!(task.result_i64(), 42);
        assert!(
            !task
                .trace
                .iter()
                .any(|e| matches!(e, TaskEvent::Parked { .. }))
        );
    }

    #[test]
    fn frame_layouts_come_from_declared_records() {
        // A callee frame declared as a record of (x: i64, y: i64,
        // out: i64): the lowering-side story — ArgCopy dst offsets ARE
        // the declared record's field offsets.
        let frame_desc = declared_struct((), vec![i64_(()), i64_(()), i64_(())]);
        let Access::Record(record) = &frame_desc.access else {
            panic!("record expected");
        };
        let x = u32::try_from(record.fields[0].offset).unwrap();
        let y = u32::try_from(record.fields[1].offset).unwrap();
        let out = u32::try_from(record.fields[2].offset).unwrap();

        let program = Program {
            fns: vec![
                Fn {
                    frame: frame_of_i64s(3),
                    code: vec![
                        Op::ConstI64 { dst: 0, value: 6 },
                        Op::ConstI64 { dst: 8, value: 9 },
                        Op::Call {
                            callee: FnId(1),
                            args: vec![
                                ArgCopy {
                                    src: 0,
                                    dst: x,
                                    size: 8,
                                },
                                ArgCopy {
                                    src: 8,
                                    dst: y,
                                    size: 8,
                                },
                            ],
                            ret: 16,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                },
                Fn {
                    frame: frame_desc.layout,
                    code: vec![
                        Op::MulI64 {
                            dst: out,
                            a: x,
                            b: y,
                        },
                        Op::Ret { src: out, size: 8 },
                    ],
                },
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(task.result_i64(), 54);
    }

    #[test]
    fn inline_composites_pass_by_value_and_survive_parking() {
        // Amos's stress: a 48-byte inline array of six i64s living IN
        // frames (the "stack" that happens to be arena-heap), never
        // boxed. The whole array crosses a call BY VALUE in one
        // ArgCopy; the callee parks on an await with the composite
        // live in BOTH frames; the callee mutates ITS copy; the
        // caller's copy is untouched (value semantics).
        use crate::mem::Access;
        use crate::mem::declared::{array_of, declared_struct, i64_};

        // Caller frame: (header, arr[6], out, idx, val).
        let caller_desc = declared_struct(
            (),
            vec![
                i64_(()),
                array_of((), i64_(()), 6),
                i64_(()),
                i64_(()),
                i64_(()),
            ],
        );
        let Access::Record(caller_rec) = &caller_desc.access else {
            panic!("record");
        };
        let off = |i: usize| u32::try_from(caller_rec.fields[i].offset).unwrap();
        let (header, arr, out, idx, val) = (off(0), off(1), off(2), off(3), off(4));

        // Callee frame: (arr[6], ix, a, b, sum).
        let callee_desc = declared_struct(
            (),
            vec![
                array_of((), i64_(()), 6),
                i64_(()),
                i64_(()),
                i64_(()),
                i64_(()),
            ],
        );
        let Access::Record(callee_rec) = &callee_desc.access else {
            panic!("record");
        };
        let coff = |i: usize| u32::try_from(callee_rec.fields[i].offset).unwrap();
        let (c_arr, c_ix, c_a, c_b, c_sum) = (coff(0), coff(1), coff(2), coff(3), coff(4));
        assert_eq!(
            callee_rec.fields[0].descriptor.layout.size, 48,
            "inline, unboxed"
        );

        let mut caller_code = vec![Op::ConstI64 {
            dst: header,
            value: 7,
        }];
        // Fill arr[k] = 10*(k+1) through the dynamic-index op.
        for k in 0..6i64 {
            caller_code.push(Op::ConstI64 { dst: idx, value: k });
            caller_code.push(Op::ConstI64 {
                dst: val,
                value: 10 * (k + 1),
            });
            caller_code.push(Op::StoreIndexedI64 {
                base: arr,
                index: idx,
                stride: 8,
                src: val,
            });
        }
        caller_code.push(Op::Call {
            callee: FnId(1),
            // ONE copy moves the whole 48-byte composite by value.
            args: vec![ArgCopy {
                src: arr,
                dst: c_arr,
                size: 48,
            }],
            ret: out,
        });
        // Prove the caller's copy survived the callee's mutation:
        // reload own arr[2] (callee overwrites its own arr[2] with 999).
        caller_code.push(Op::ConstI64 { dst: idx, value: 2 });
        caller_code.push(Op::LoadIndexedI64 {
            dst: val,
            base: arr,
            index: idx,
            stride: 8,
        });
        caller_code.push(Op::AddI64 {
            dst: out,
            a: out,
            b: val,
        });
        caller_code.push(Op::Ret { src: out, size: 8 });

        let callee_code = vec![
            // Park FIRST — the 48-byte composite is live in both
            // frames across the suspension.
            Op::Await {
                dst: c_ix,
                input: 0,
            },
            Op::LoadIndexedI64 {
                dst: c_a,
                base: c_arr,
                index: c_ix,
                stride: 8,
            },
            Op::ConstI64 {
                dst: c_sum,
                value: 1,
            },
            Op::AddI64 {
                dst: c_ix,
                a: c_ix,
                b: c_sum,
            },
            Op::LoadIndexedI64 {
                dst: c_b,
                base: c_arr,
                index: c_ix,
                stride: 8,
            },
            Op::AddI64 {
                dst: c_sum,
                a: c_a,
                b: c_b,
            },
            // Mutate OUR copy: arr[ix] = 999 (value semantics check).
            Op::ConstI64 {
                dst: c_a,
                value: 999,
            },
            Op::StoreIndexedI64 {
                base: c_arr,
                index: c_ix,
                stride: 8,
                src: c_a,
            },
            Op::Ret {
                src: c_sum,
                size: 8,
            },
        ];

        let program = Program {
            fns: vec![
                Fn {
                    frame: caller_desc.layout,
                    code: caller_code,
                },
                Fn {
                    frame: callee_desc.layout,
                    code: callee_code,
                },
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        let mut ready = [false];
        assert_eq!(
            task.run(&program, &mut ready, &[0]),
            TaskStep::Parked { input: 0 }
        );
        assert_eq!(
            task.depth(),
            2,
            "parked with 48-byte composites live in both frames"
        );

        // ix=2: a=arr[2]=30, b=arr[3]=40, sum=70; caller adds its own
        // UNMUTATED arr[2]=30 → 100. (If by-value copying were shared,
        // the callee's 999 would bleed through and this would be 1069.)
        ready[0] = true;
        assert_eq!(task.run(&program, &mut ready, &[2]), TaskStep::Done);
        assert_eq!(task.result_i64(), 100);
    }

    #[test]
    fn composite_returns_flow_through_ret_slots() {
        // A callee builds a 24-byte inline array and returns the WHOLE
        // composite through the caller's designated slot (sret shape);
        // the caller indexes into the returned bytes in place.
        let program = Program {
            fns: vec![
                Fn {
                    // (ret_arr[3] @0, idx @24, out @32)
                    frame: Layout { size: 40, align: 8 },
                    code: vec![
                        Op::Call {
                            callee: FnId(1),
                            args: vec![],
                            ret: 0,
                        },
                        Op::ConstI64 { dst: 24, value: 1 },
                        Op::LoadIndexedI64 {
                            dst: 32,
                            base: 0,
                            index: 24,
                            stride: 8,
                        },
                        Op::Ret { src: 32, size: 8 },
                    ],
                },
                Fn {
                    // (arr[3] @0, idx @24, val @32)
                    frame: Layout { size: 40, align: 8 },
                    code: vec![
                        Op::ConstI64 { dst: 24, value: 0 },
                        Op::ConstI64 { dst: 32, value: 5 },
                        Op::StoreIndexedI64 {
                            base: 0,
                            index: 24,
                            stride: 8,
                            src: 32,
                        },
                        Op::ConstI64 { dst: 24, value: 1 },
                        Op::ConstI64 { dst: 32, value: 6 },
                        Op::StoreIndexedI64 {
                            base: 0,
                            index: 24,
                            stride: 8,
                            src: 32,
                        },
                        Op::ConstI64 { dst: 24, value: 2 },
                        Op::ConstI64 { dst: 32, value: 7 },
                        Op::StoreIndexedI64 {
                            base: 0,
                            index: 24,
                            stride: 8,
                            src: 32,
                        },
                        Op::Ret { src: 0, size: 24 },
                    ],
                },
            ],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(
            task.result_i64(),
            6,
            "indexed into the 24-byte returned composite"
        );
    }

    fn later(value: i64, ms: u64) -> Pin<Box<dyn Future<Output = i64>>> {
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            value
        })
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tasks_await_real_futures_across_call_frames() {
        // outer calls callee; callee awaits TWO real futures (late #0,
        // early #1) and combines them with a frame local. The demand
        // driver shape vix will use, in miniature.
        let program = Program {
            fns: vec![
                Fn {
                    frame: frame_of_i64s(2),
                    code: vec![
                        Op::ConstI64 {
                            dst: 0,
                            value: 1000,
                        },
                        Op::Call {
                            callee: FnId(1),
                            args: vec![],
                            ret: 8,
                        },
                        Op::AddI64 { dst: 8, a: 8, b: 0 },
                        Op::Ret { src: 8, size: 8 },
                    ],
                },
                Fn {
                    frame: frame_of_i64s(3),
                    code: vec![
                        Op::Await { dst: 0, input: 0 },
                        Op::Await { dst: 8, input: 1 },
                        Op::MulI64 {
                            dst: 16,
                            a: 0,
                            b: 8,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                },
            ],
        };
        let running = Running {
            program: &program,
            task: Task::spawn(&program, FnId(0)),
        };
        let exec = TaskExec::new(running, vec![later(6, 60), later(7, 20)], vec![]);
        let result = exec.await;
        assert_eq!(
            i64::from_le_bytes(result[..8].try_into().unwrap()),
            6 * 7 + 1000
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn external_wakes_resume_parked_tasks() {
        // The async-host shape: a oneshot fed by another tokio task —
        // an external event wakes the parked task through the ordinary
        // waker path, and sync host calls coexist in the same run.
        let program = Program {
            fns: vec![Fn {
                frame: frame_of_i64s(2),
                code: vec![
                    Op::Await { dst: 0, input: 0 },
                    Op::HostCall { host: 0 },
                    Op::Ret { src: 8, size: 8 },
                ],
            }],
        };
        let (tx, rx) = tokio::sync::oneshot::channel::<i64>();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            tx.send(21).unwrap();
        });
        let input: Pin<Box<dyn Future<Output = i64>>> =
            Box::pin(async move { rx.await.expect("sender lives") });
        let host: BoxedHostFn = Box::new(|frame: &mut [u8]| {
            let v = i64::from_le_bytes(frame[0..8].try_into().unwrap());
            frame[8..16].copy_from_slice(&(v * 2).to_le_bytes());
        });
        let running = Running {
            program: &program,
            task: Task::spawn(&program, FnId(0)),
        };
        let result = TaskExec::new(running, vec![input], vec![host]).await;
        assert_eq!(i64::from_le_bytes(result[..8].try_into().unwrap()), 42);
    }

    #[test]
    fn three_deep_calls_return_through_designated_slots() {
        // f0 -> f1 -> f2; each adds its own constant.
        let leaf = Fn {
            frame: frame_of_i64s(2),
            code: vec![
                Op::ConstI64 { dst: 8, value: 1 },
                Op::AddI64 { dst: 0, a: 0, b: 8 },
                Op::Ret { src: 0, size: 8 },
            ],
        };
        let mid = Fn {
            frame: frame_of_i64s(2),
            code: vec![
                Op::Call {
                    callee: FnId(2),
                    args: vec![ArgCopy {
                        src: 0,
                        dst: 0,
                        size: 8,
                    }],
                    ret: 8,
                },
                Op::AddI64 { dst: 8, a: 8, b: 0 },
                Op::Ret { src: 8, size: 8 },
            ],
        };
        let root = Fn {
            frame: frame_of_i64s(2),
            code: vec![
                Op::ConstI64 { dst: 0, value: 10 },
                Op::Call {
                    callee: FnId(1),
                    args: vec![ArgCopy {
                        src: 0,
                        dst: 0,
                        size: 8,
                    }],
                    ret: 8,
                },
                Op::Ret { src: 8, size: 8 },
            ],
        };
        let program = Program {
            fns: vec![root, mid, leaf],
        };
        let mut task = Task::spawn(&program, FnId(0));
        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        // leaf: 10+1=11; mid: 11+10=21.
        assert_eq!(task.result_i64(), 21);
        assert_eq!(task.depth(), 0);
    }

    #[test]
    fn direct_recursion_uses_task_frames_not_the_rust_stack() {
        let countdown = Fn {
            frame: frame_of_i64s(6),
            code: vec![
                Op::ConstI64 { dst: 8, value: 0 },
                Op::EqI64 {
                    dst: 24,
                    a: 0,
                    b: 8,
                },
                Op::JumpIfZero {
                    value: 24,
                    target: 4,
                },
                Op::Ret { src: 8, size: 8 },
                Op::ConstI64 { dst: 16, value: 1 },
                Op::SubI64 {
                    dst: 32,
                    a: 0,
                    b: 16,
                },
                Op::Call {
                    callee: FnId(0),
                    args: vec![ArgCopy {
                        src: 32,
                        dst: 0,
                        size: 8,
                    }],
                    ret: 40,
                },
                Op::Ret { src: 40, size: 8 },
            ],
        };
        let program = Program {
            fns: vec![countdown],
        };
        let mut task = Task::spawn_with_mode(&program, FnId(0), TraceMode::Production);
        task.write_i64(0, 100_000);

        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(task.result_i64(), 0);
        assert_eq!(task.depth(), 0);
    }

    #[test]
    fn mutual_recursion_calls_through_recorded_fn_ids() {
        let even = Fn {
            frame: frame_of_i64s(6),
            code: vec![
                Op::ConstI64 { dst: 8, value: 0 },
                Op::EqI64 {
                    dst: 24,
                    a: 0,
                    b: 8,
                },
                Op::JumpIfZero {
                    value: 24,
                    target: 5,
                },
                Op::ConstI64 { dst: 40, value: 1 },
                Op::Ret { src: 40, size: 8 },
                Op::ConstI64 { dst: 16, value: 1 },
                Op::SubI64 {
                    dst: 32,
                    a: 0,
                    b: 16,
                },
                Op::Call {
                    callee: FnId(1),
                    args: vec![ArgCopy {
                        src: 32,
                        dst: 0,
                        size: 8,
                    }],
                    ret: 40,
                },
                Op::Ret { src: 40, size: 8 },
            ],
        };
        let odd = Fn {
            frame: frame_of_i64s(6),
            code: vec![
                Op::ConstI64 { dst: 8, value: 0 },
                Op::EqI64 {
                    dst: 24,
                    a: 0,
                    b: 8,
                },
                Op::JumpIfZero {
                    value: 24,
                    target: 5,
                },
                Op::ConstI64 { dst: 40, value: 0 },
                Op::Ret { src: 40, size: 8 },
                Op::ConstI64 { dst: 16, value: 1 },
                Op::SubI64 {
                    dst: 32,
                    a: 0,
                    b: 16,
                },
                Op::Call {
                    callee: FnId(0),
                    args: vec![ArgCopy {
                        src: 32,
                        dst: 0,
                        size: 8,
                    }],
                    ret: 40,
                },
                Op::Ret { src: 40, size: 8 },
            ],
        };
        let program = Program {
            fns: vec![even, odd],
        };
        let mut task = Task::spawn(&program, FnId(0));
        task.write_i64(0, 101);

        assert_eq!(task.run(&program, &mut [], &[]), TaskStep::Done);
        assert_eq!(task.result_i64(), 0);
    }

    #[test]
    fn nonresident_sentinel_reads_as_invalid_handle_not_empty_payload() {
        // `ValueMemory::empty()` is the nonresident/evicted sentinel
        // (null ptr, len 0). It must never be treated as a valid
        // zero-length resident payload.
        let store = [ValueMemory::empty()];
        let memories = ValueMemories {
            store: &store,
            molten: &[],
        };
        let molten = MoltenArena::default();
        let mut dst = [0u8; 8];
        let status = load_array_region(
            MemoryView::from(memories),
            &molten,
            ArrayRegion {
                array: 0,
                index: 0,
                elem_width: 8,
                elem_schema_ref: 0,
            },
            &mut dst,
        );
        assert_eq!(status, ArrayOpStatus::InvalidHandle);
        assert_eq!(dst, [0u8; 8]);
    }

    #[test]
    fn resident_empty_slice_is_not_classified_as_nonresident() {
        // A resident value built from an empty slice (nonnull ptr, len 0)
        // is a real, present payload — just not a well-formed array (too
        // short to hold the array header). It must fail as
        // MalformedPayload, not as InvalidHandle: the checked-array path
        // must distinguish "resident but malformed" from "absent".
        let store = [ValueMemory::from_slice(&[])];
        let memories = ValueMemories {
            store: &store,
            molten: &[],
        };
        let molten = MoltenArena::default();
        let mut dst = [0u8; 8];
        let status = load_array_region(
            MemoryView::from(memories),
            &molten,
            ArrayRegion {
                array: 0,
                index: 0,
                elem_width: 8,
                elem_schema_ref: 0,
            },
            &mut dst,
        );
        assert_eq!(status, ArrayOpStatus::MalformedPayload);
        assert_eq!(dst, [0u8; 8]);
    }
}
