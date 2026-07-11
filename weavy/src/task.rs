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
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use crate::exec::{CompareSide, FaultSite, TaskFault, fault_site};
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

impl ArrayOpStatus {
    #[must_use]
    pub const fn from_word(word: i64) -> Option<Self> {
        match word {
            1 => Some(Self::Ok),
            2 => Some(Self::InvalidHandle),
            3 => Some(Self::MalformedPayload),
            4 => Some(Self::WidthMismatch),
            5 => Some(Self::SchemaMismatch),
            6 => Some(Self::OutOfRange),
            7 => Some(Self::Overflow),
            8 => Some(Self::AllocationFailed),
            9 => Some(Self::Uninitialized),
            _ => None,
        }
    }
}

/// Why a [`Op::StringConcat`] could not produce a resident result value.
///
/// Residency of both operands is a task-admission obligation; the two unresident
/// variants name the offending side. Allocation exhaustion is a runtime resource
/// condition. Every variant maps to a typed [`crate::exec::TaskFault`], never to
/// a fabricated handle or silent empty string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StringConcatFault {
    /// The left operand handle did not name a resident value.
    LeftUnresident(i64),
    /// The right operand handle did not name a resident value.
    RightUnresident(i64),
    /// Checked length arithmetic or the arena allocation could not be satisfied.
    AllocationFailed,
}

/// Closed status vocabulary for canonical string operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i64)]
pub enum StringOpStatus {
    Ok = 0,
    MissingDelimiter = 1,
    InvalidInteger = 2,
    IntegerOverflow = 3,
}

impl StringOpStatus {
    #[must_use]
    pub const fn from_word(word: i64) -> Option<Self> {
        match word {
            0 => Some(Self::Ok),
            1 => Some(Self::MissingDelimiter),
            2 => Some(Self::InvalidInteger),
            3 => Some(Self::IntegerOverflow),
            _ => None,
        }
    }
}

impl StringConcatFault {
    /// The closed i64 status the JIT ABI helper reports to its stencil.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    const OK_STATUS: i64 = 0;
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    const LEFT_STATUS: i64 = 1;
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    const RIGHT_STATUS: i64 = 2;
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    const ALLOCATION_STATUS: i64 = 3;

    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    fn status(self) -> i64 {
        match self {
            StringConcatFault::LeftUnresident(_) => Self::LEFT_STATUS,
            StringConcatFault::RightUnresident(_) => Self::RIGHT_STATUS,
            StringConcatFault::AllocationFailed => Self::ALLOCATION_STATUS,
        }
    }
}

/// Why a [`Op::Publish`] could not append its descriptor.
///
/// Copying the record bytes is otherwise infallible: task admission bounds the
/// record region inside the frame, so the only failure is a log allocation the
/// system cannot satisfy. It maps to a typed [`crate::exec::TaskFault`], never to
/// a dropped, truncated, or partially written descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PublicationFault {
    /// The append-only log could not reserve room for the descriptor.
    AllocationFailed,
}

impl PublicationFault {
    /// The closed i64 status the JIT ABI helper reports to its stencil.
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    const OK_STATUS: i64 = 0;
    #[cfg_attr(not(feature = "jit"), allow(dead_code))]
    const ALLOCATION_STATUS: i64 = 1;
}

/// One published descriptor's provenance, record type witness, and byte span
/// into the owning [`PublicationLog`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PublishedRecord {
    /// Opaque, front-end-assigned provenance key. Never interpreted here.
    pub(crate) site: u64,
    /// The `record_schema_ref` witness copied verbatim from the op. Task
    /// admission has already proven it names a valid publication-record schema;
    /// the read-only reader resolves it to type the bytes.
    pub(crate) schema_ref: i64,
    start: usize,
    len: usize,
}

/// A task-owned, append-only log of published descriptors.
///
/// Descriptors accumulate in publication order across every taken path and are
/// never mutated or removed once written. The log is owned by exactly one task
/// and dies with it; its bytes are copies, so nothing here aliases the frame,
/// the molten arena, or any lent value memory. It is exposed read-only through
/// the task's completion surface after the root returns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PublicationLog {
    records: Vec<PublishedRecord>,
    bytes: Vec<u8>,
}

impl PublicationLog {
    /// Copy `record` into the log under `site`/`schema_ref`, appending one
    /// descriptor. Fails only when the log cannot grow to hold the copy; on
    /// failure nothing is appended.
    fn publish(
        &mut self,
        site: u64,
        schema_ref: i64,
        record: &[u8],
    ) -> Result<(), PublicationFault> {
        self.records
            .try_reserve(1)
            .map_err(|_| PublicationFault::AllocationFailed)?;
        self.bytes
            .try_reserve(record.len())
            .map_err(|_| PublicationFault::AllocationFailed)?;
        let start = self.bytes.len();
        self.bytes.extend_from_slice(record);
        self.records.push(PublishedRecord {
            site,
            schema_ref,
            start,
            len: record.len(),
        });
        Ok(())
    }

    /// Number of descriptors published so far, in publication order.
    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.records.len()
    }

    /// The descriptor at `index` in publication order: its provenance key,
    /// record schema witness, and exact copied bytes.
    #[must_use]
    pub(crate) fn get(&self, index: usize) -> Option<(&PublishedRecord, &[u8])> {
        let record = self.records.get(index)?;
        let bytes = &self.bytes[record.start..record.start + record.len];
        Some((record, bytes))
    }
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
#[derive(Clone, Debug)]
pub(crate) struct MoltenArena {
    buffers: Vec<MoltenBuffer>,
    ordered_nodes: Vec<OrderedNode>,
    ordered_cursors: Vec<OrderedCursor>,
    task_generation: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct OrderedNode {
    schema: i64,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    left: Option<usize>,
    right: Option<usize>,
    height: u8,
    len: usize,
}

struct OrderedNodeParts {
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    left: Option<usize>,
    right: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OrderedCursorOperation {
    Probe,
    Insert,
    Iterate,
}

#[derive(Clone, Debug)]
pub(crate) struct OrderedCursor {
    task_generation: u64,
    schema: i64,
    state: OrderedCursorState,
    consumed: bool,
}

#[derive(Clone, Debug)]
enum OrderedCursorState {
    Probe {
        root: Option<usize>,
    },
    Insert {
        root: Option<usize>,
        current: Option<usize>,
        path: Vec<OrderedPathStep>,
        phase: OrderedInsertPhase,
    },
    Iterate {
        stack: Vec<usize>,
        done: bool,
    },
}

impl OrderedCursorState {
    fn operation(&self) -> OrderedCursorOperation {
        match self {
            Self::Probe { .. } => OrderedCursorOperation::Probe,
            Self::Insert { .. } => OrderedCursorOperation::Insert,
            Self::Iterate { .. } => OrderedCursorOperation::Iterate,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct OrderedPathStep {
    node: usize,
    direction: OrderedDirection,
}

#[derive(Clone, Copy, Debug)]
enum OrderedDirection {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
enum OrderedInsertPhase {
    Inspect,
    Advance(usize),
    Ready(Option<usize>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OrderedCursorError {
    Invalid,
    Stale,
    SchemaMismatch,
    OperationMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OrderedCursorToken {
    index: usize,
    task_generation: u64,
}

impl OrderedCursorToken {
    /// Flatten the token into the two opaque frame words that carry it: the
    /// arena cursor index and the task generation it was minted under.
    pub(crate) fn into_words(self) -> (i64, i64) {
        (self.index as i64, self.task_generation as i64)
    }

    /// Reconstruct a token from its two opaque frame words. A negative index
    /// (e.g. the poison sentinel written to a failed begin) never names a
    /// cursor, so it yields `None` and the operation reports `InvalidHandle`.
    /// A fabricated generation is caught by the arena's live generation check.
    pub(crate) fn from_words(index: i64, generation: i64) -> Option<Self> {
        Some(Self {
            index: usize::try_from(index).ok()?,
            task_generation: generation as u64,
        })
    }
}

/// One resolved probe step: whether the cursor named a node, that node's key
/// bytes, and the left/right child collection handles for the bytecode to
/// descend into after a structural comparison.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrderedProbeStep {
    pub present: bool,
    pub key: Vec<u8>,
    pub left: i64,
    pub right: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrderedIterateStep {
    pub present: bool,
    pub row: Vec<u8>,
}

/// Map a cursor consumption error onto the closed [`OrderedOpStatus`] ABI.
fn ordered_consume_status(err: OrderedCursorError) -> OrderedOpStatus {
    match err {
        OrderedCursorError::Invalid => OrderedOpStatus::InvalidHandle,
        OrderedCursorError::Stale => OrderedOpStatus::Stale,
        OrderedCursorError::SchemaMismatch => OrderedOpStatus::SchemaMismatch,
        OrderedCursorError::OperationMismatch => OrderedOpStatus::OperationMismatch,
    }
}

/// The canonical empty ordered-collection root handle: it names no arena node.
/// Any `n >= 1` names arena node `n - 1`.
pub(crate) const ORDERED_EMPTY_HANDLE: i64 = 0;

/// Poison written to a cursor's index word when a begin operation fails, so a
/// failed cursor never aliases a live arena cursor index.
pub(crate) const ORDERED_CURSOR_POISON: i64 = -1;

/// Checked status for ordered-collection substrate operations. Closed set,
/// stable i64 ABI shared by the interpreter and JIT lanes; Vix lowering maps
/// these to language-level `MissingKey`/`DuplicateKey` at the source site.
#[repr(i64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderedOpStatus {
    /// The operation completed and any cursor/handle output is valid.
    Ok = 1,
    /// The collection root handle did not name a resident arena node.
    InvalidHandle = 2,
    /// A cursor or handle schema did not match the operation's declared schema.
    SchemaMismatch = 3,
    /// A cursor was consumed under a different operation than it was begun for.
    OperationMismatch = 4,
    /// A cursor was forged, cross-task, or already consumed.
    Stale = 5,
    /// The arena reported exhaustion for an otherwise valid request.
    AllocationFailed = 6,
    /// A known-new insertion encountered a structurally equal key.
    DuplicateKey = 7,
    /// An advance operation received a word outside the closed ordering ABI.
    InvalidOrdering = 8,
}

impl OrderedOpStatus {
    #[must_use]
    pub const fn from_word(word: i64) -> Option<Self> {
        match word {
            1 => Some(Self::Ok),
            2 => Some(Self::InvalidHandle),
            3 => Some(Self::SchemaMismatch),
            4 => Some(Self::OperationMismatch),
            5 => Some(Self::Stale),
            6 => Some(Self::AllocationFailed),
            7 => Some(Self::DuplicateKey),
            8 => Some(Self::InvalidOrdering),
            _ => None,
        }
    }
}

static NEXT_MOLTEN_TASK_GENERATION: AtomicU64 = AtomicU64::new(1);

impl Default for MoltenArena {
    fn default() -> Self {
        Self {
            buffers: Vec::new(),
            ordered_nodes: Vec::new(),
            ordered_cursors: Vec::new(),
            task_generation: NEXT_MOLTEN_TASK_GENERATION.fetch_add(1, AtomicOrdering::Relaxed),
        }
    }
}

#[derive(Clone, Debug)]
struct MoltenBuffer {
    bytes: Vec<u8>,
    initialized: Vec<bool>,
}

impl MoltenArena {
    pub(crate) fn alloc_ordered_node(
        &mut self,
        schema: i64,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
        left: Option<usize>,
        right: Option<usize>,
    ) -> Result<usize, OrderedCursorError> {
        for child in [left, right].into_iter().flatten() {
            if self
                .ordered_nodes
                .get(child)
                .is_none_or(|node| node.schema != schema)
            {
                return Err(OrderedCursorError::SchemaMismatch);
            }
        }
        let height = 1u8
            .checked_add(self.ordered_height(left).max(self.ordered_height(right)))
            .ok_or(OrderedCursorError::Invalid)?;
        let len = 1usize
            .checked_add(self.ordered_len_at(left))
            .and_then(|len| len.checked_add(self.ordered_len_at(right)))
            .ok_or(OrderedCursorError::Invalid)?;
        self.ordered_nodes
            .try_reserve(1)
            .map_err(|_| OrderedCursorError::Invalid)?;
        let index = self.ordered_nodes.len();
        self.ordered_nodes.push(OrderedNode {
            schema,
            key,
            value,
            left,
            right,
            height,
            len,
        });
        Ok(index)
    }

    fn ordered_height(&self, node: Option<usize>) -> u8 {
        node.and_then(|index| self.ordered_nodes.get(index))
            .map_or(0, |node| node.height)
    }

    fn ordered_len_at(&self, node: Option<usize>) -> usize {
        node.and_then(|index| self.ordered_nodes.get(index))
            .map_or(0, |node| node.len)
    }

    fn ordered_node_parts(&self, index: usize) -> Result<OrderedNodeParts, OrderedCursorError> {
        let node = self
            .ordered_nodes
            .get(index)
            .ok_or(OrderedCursorError::Invalid)?;
        Ok(OrderedNodeParts {
            key: node.key.clone(),
            value: node.value.clone(),
            left: node.left,
            right: node.right,
        })
    }

    fn alloc_balanced_ordered_node(
        &mut self,
        schema: i64,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
        left: Option<usize>,
        right: Option<usize>,
    ) -> Result<usize, OrderedCursorError> {
        let skew = i16::from(self.ordered_height(left)) - i16::from(self.ordered_height(right));
        if skew > 1 {
            let left_index = left.ok_or(OrderedCursorError::Invalid)?;
            let OrderedNodeParts {
                key: left_key,
                value: left_value,
                left: left_left,
                right: left_right,
            } = self.ordered_node_parts(left_index)?;
            if self.ordered_height(left_right) > self.ordered_height(left_left) {
                let pivot = left_right.ok_or(OrderedCursorError::Invalid)?;
                let OrderedNodeParts {
                    key: pivot_key,
                    value: pivot_value,
                    left: pivot_left,
                    right: pivot_right,
                } = self.ordered_node_parts(pivot)?;
                let new_left =
                    self.alloc_ordered_node(schema, left_key, left_value, left_left, pivot_left)?;
                let new_right = self.alloc_ordered_node(schema, key, value, pivot_right, right)?;
                return self.alloc_ordered_node(
                    schema,
                    pivot_key,
                    pivot_value,
                    Some(new_left),
                    Some(new_right),
                );
            }
            let new_right = self.alloc_ordered_node(schema, key, value, left_right, right)?;
            return self.alloc_ordered_node(
                schema,
                left_key,
                left_value,
                left_left,
                Some(new_right),
            );
        }
        if skew < -1 {
            let right_index = right.ok_or(OrderedCursorError::Invalid)?;
            let OrderedNodeParts {
                key: right_key,
                value: right_value,
                left: right_left,
                right: right_right,
            } = self.ordered_node_parts(right_index)?;
            if self.ordered_height(right_left) > self.ordered_height(right_right) {
                let pivot = right_left.ok_or(OrderedCursorError::Invalid)?;
                let OrderedNodeParts {
                    key: pivot_key,
                    value: pivot_value,
                    left: pivot_left,
                    right: pivot_right,
                } = self.ordered_node_parts(pivot)?;
                let new_left = self.alloc_ordered_node(schema, key, value, left, pivot_left)?;
                let new_right = self.alloc_ordered_node(
                    schema,
                    right_key,
                    right_value,
                    pivot_right,
                    right_right,
                )?;
                return self.alloc_ordered_node(
                    schema,
                    pivot_key,
                    pivot_value,
                    Some(new_left),
                    Some(new_right),
                );
            }
            let new_left = self.alloc_ordered_node(schema, key, value, left, right_left)?;
            return self.alloc_ordered_node(
                schema,
                right_key,
                right_value,
                Some(new_left),
                right_right,
            );
        }
        self.alloc_ordered_node(schema, key, value, left, right)
    }

    /// Decode an ordered-collection root handle into an arena node index.
    /// [`ORDERED_EMPTY_HANDLE`] is the canonical empty root (no node); any
    /// `n >= 1` names node `n - 1`, bounds-checked against the node arena.
    fn ordered_root(&self, collection: i64) -> Result<Option<usize>, OrderedOpStatus> {
        if collection == ORDERED_EMPTY_HANDLE {
            return Ok(None);
        }
        let index = usize::try_from(collection - 1).map_err(|_| OrderedOpStatus::InvalidHandle)?;
        if index >= self.ordered_nodes.len() {
            return Err(OrderedOpStatus::InvalidHandle);
        }
        Ok(Some(index))
    }

    /// Begin a single-use Probe cursor over the collection named by `collection`
    /// under the declared `schema`. The returned token is confined to this
    /// arena's current task generation and to the Probe operation.
    pub(crate) fn begin_ordered_probe(
        &mut self,
        collection: i64,
        schema: i64,
    ) -> Result<OrderedCursorToken, OrderedOpStatus> {
        let root = self.ordered_root(collection)?;
        self.begin_ordered_cursor(schema, OrderedCursorOperation::Probe, root)
            .map_err(|err| match err {
                OrderedCursorError::SchemaMismatch => OrderedOpStatus::SchemaMismatch,
                OrderedCursorError::OperationMismatch => OrderedOpStatus::OperationMismatch,
                OrderedCursorError::Stale => OrderedOpStatus::Stale,
                OrderedCursorError::Invalid => OrderedOpStatus::AllocationFailed,
            })
    }

    pub(crate) fn begin_ordered_insert(
        &mut self,
        collection: i64,
        schema: i64,
    ) -> Result<OrderedCursorToken, OrderedOpStatus> {
        let root = self.ordered_root(collection)?;
        self.begin_ordered_cursor(schema, OrderedCursorOperation::Insert, root)
            .map_err(|err| match err {
                OrderedCursorError::SchemaMismatch => OrderedOpStatus::SchemaMismatch,
                OrderedCursorError::OperationMismatch => OrderedOpStatus::OperationMismatch,
                OrderedCursorError::Stale => OrderedOpStatus::Stale,
                OrderedCursorError::Invalid => OrderedOpStatus::AllocationFailed,
            })
    }

    pub(crate) fn begin_ordered_iterate(
        &mut self,
        collection: i64,
        schema: i64,
    ) -> Result<OrderedCursorToken, OrderedOpStatus> {
        let root = self.ordered_root(collection)?;
        self.begin_ordered_cursor(schema, OrderedCursorOperation::Iterate, root)
            .map_err(|err| match err {
                OrderedCursorError::SchemaMismatch => OrderedOpStatus::SchemaMismatch,
                OrderedCursorError::OperationMismatch => OrderedOpStatus::OperationMismatch,
                OrderedCursorError::Stale => OrderedOpStatus::Stale,
                OrderedCursorError::Invalid => OrderedOpStatus::AllocationFailed,
            })
    }

    pub(crate) fn ordered_collection_len(
        &self,
        collection: i64,
        schema: i64,
    ) -> Result<i64, OrderedOpStatus> {
        let root = self.ordered_root(collection)?;
        if root.is_some_and(|index| self.ordered_nodes[index].schema != schema) {
            return Err(OrderedOpStatus::SchemaMismatch);
        }
        i64::try_from(self.ordered_len_at(root)).map_err(|_| OrderedOpStatus::AllocationFailed)
    }

    /// Encode an arena node child into the collection handle the bytecode
    /// descends into: absent children are the canonical empty collection.
    fn ordered_child_handle(child: Option<usize>) -> i64 {
        child.map_or(ORDERED_EMPTY_HANDLE, |index| index as i64 + 1)
    }

    /// Consume a single-use Probe cursor and resolve one probe step: whether it
    /// named a node, and if so that node's key bytes and its left/right child
    /// collection handles. The cursor is spent whether or not a node was named.
    pub(crate) fn probe_ordered_key(
        &mut self,
        token: OrderedCursorToken,
        schema: i64,
    ) -> Result<OrderedProbeStep, OrderedOpStatus> {
        let root = self
            .consume_ordered_cursor(token, schema, OrderedCursorOperation::Probe)
            .map_err(ordered_consume_status)?;
        let Some(index) = root else {
            return Ok(OrderedProbeStep {
                present: false,
                key: Vec::new(),
                left: ORDERED_EMPTY_HANDLE,
                right: ORDERED_EMPTY_HANDLE,
            });
        };
        let node = &self.ordered_nodes[index];
        Ok(OrderedProbeStep {
            present: true,
            key: node.key.clone(),
            left: Self::ordered_child_handle(node.left),
            right: Self::ordered_child_handle(node.right),
        })
    }

    /// Consume a single-use Probe cursor and expose the current node's value.
    /// This is the `get` terminal, taken only after a structural comparison has
    /// found an equal key; `has` never calls it, so membership never projects a
    /// value. Returns `(present, value bytes)`; an empty position is a miss.
    pub(crate) fn probe_ordered_value(
        &mut self,
        token: OrderedCursorToken,
        schema: i64,
    ) -> Result<(bool, Vec<u8>), OrderedOpStatus> {
        let root = self
            .consume_ordered_cursor(token, schema, OrderedCursorOperation::Probe)
            .map_err(ordered_consume_status)?;
        let Some(index) = root else {
            return Ok((false, Vec::new()));
        };
        let value = self.ordered_nodes[index].value.clone().unwrap_or_default();
        Ok((true, value))
    }

    pub(crate) fn inspect_ordered_insert(
        &mut self,
        token: OrderedCursorToken,
        schema: i64,
    ) -> Result<OrderedProbeStep, OrderedOpStatus> {
        let cursor_index = self
            .ordered_cursor_index(token, schema, OrderedCursorOperation::Insert)
            .map_err(ordered_consume_status)?;
        let current = match &mut self.ordered_cursors[cursor_index].state {
            OrderedCursorState::Insert { current, phase, .. }
                if matches!(phase, OrderedInsertPhase::Inspect) =>
            {
                let current = *current;
                *phase =
                    current.map_or(OrderedInsertPhase::Ready(None), OrderedInsertPhase::Advance);
                current
            }
            OrderedCursorState::Insert { .. } => {
                return Err(OrderedOpStatus::OperationMismatch);
            }
            _ => unreachable!(),
        };
        let Some(index) = current else {
            return Ok(OrderedProbeStep {
                present: false,
                key: Vec::new(),
                left: ORDERED_EMPTY_HANDLE,
                right: ORDERED_EMPTY_HANDLE,
            });
        };
        let node = &self.ordered_nodes[index];
        Ok(OrderedProbeStep {
            present: true,
            key: node.key.clone(),
            left: Self::ordered_child_handle(node.left),
            right: Self::ordered_child_handle(node.right),
        })
    }

    pub(crate) fn advance_ordered_insert(
        &mut self,
        token: OrderedCursorToken,
        schema: i64,
        ordering: i64,
    ) -> Result<bool, OrderedOpStatus> {
        let direction = match ordering {
            0 => Some(OrderedDirection::Left),
            1 => None,
            2 => Some(OrderedDirection::Right),
            _ => return Err(OrderedOpStatus::InvalidOrdering),
        };
        let cursor_index = self
            .ordered_cursor_index(token, schema, OrderedCursorOperation::Insert)
            .map_err(ordered_consume_status)?;
        let current = match &self.ordered_cursors[cursor_index].state {
            OrderedCursorState::Insert {
                phase: OrderedInsertPhase::Advance(current),
                ..
            } => *current,
            OrderedCursorState::Insert { .. } => {
                return Err(OrderedOpStatus::OperationMismatch);
            }
            _ => unreachable!(),
        };
        if direction.is_none() {
            let OrderedCursorState::Insert { phase, .. } =
                &mut self.ordered_cursors[cursor_index].state
            else {
                unreachable!()
            };
            *phase = OrderedInsertPhase::Ready(Some(current));
            return Ok(true);
        }
        let direction = direction.unwrap();
        let child = match direction {
            OrderedDirection::Left => self.ordered_nodes[current].left,
            OrderedDirection::Right => self.ordered_nodes[current].right,
        };
        let OrderedCursorState::Insert {
            current: cursor_current,
            path,
            phase,
            ..
        } = &mut self.ordered_cursors[cursor_index].state
        else {
            unreachable!()
        };
        path.try_reserve(1)
            .map_err(|_| OrderedOpStatus::AllocationFailed)?;
        path.push(OrderedPathStep {
            node: current,
            direction,
        });
        *cursor_current = child;
        *phase = OrderedInsertPhase::Inspect;
        Ok(false)
    }

    pub(crate) fn commit_ordered_insert(
        &mut self,
        token: OrderedCursorToken,
        schema: i64,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
        replace: bool,
    ) -> Result<i64, OrderedOpStatus> {
        let cursor_index = self
            .ordered_cursor_index(token, schema, OrderedCursorOperation::Insert)
            .map_err(ordered_consume_status)?;
        let (root, path, existing) = match &self.ordered_cursors[cursor_index].state {
            OrderedCursorState::Insert {
                root,
                path,
                phase: OrderedInsertPhase::Ready(existing),
                ..
            } => (*root, path.clone(), *existing),
            OrderedCursorState::Insert { .. } => {
                return Err(OrderedOpStatus::OperationMismatch);
            }
            _ => unreachable!(),
        };
        self.ordered_cursors[cursor_index].consumed = true;
        if existing.is_some() && !replace {
            return Err(OrderedOpStatus::DuplicateKey);
        }
        let mut rebuilt = if let Some(existing) = existing {
            let OrderedNodeParts { left, right, .. } = self
                .ordered_node_parts(existing)
                .map_err(ordered_consume_status)?;
            self.alloc_ordered_node(schema, key, value, left, right)
                .map_err(|_| OrderedOpStatus::AllocationFailed)?
        } else {
            self.alloc_ordered_node(schema, key, value, None, None)
                .map_err(|_| OrderedOpStatus::AllocationFailed)?
        };
        for step in path.into_iter().rev() {
            let OrderedNodeParts {
                key: parent_key,
                value: parent_value,
                mut left,
                mut right,
            } = self
                .ordered_node_parts(step.node)
                .map_err(ordered_consume_status)?;
            match step.direction {
                OrderedDirection::Left => left = Some(rebuilt),
                OrderedDirection::Right => right = Some(rebuilt),
            }
            rebuilt = self
                .alloc_balanced_ordered_node(schema, parent_key, parent_value, left, right)
                .map_err(|_| OrderedOpStatus::AllocationFailed)?;
        }
        let _ = root;
        Ok(Self::ordered_child_handle(Some(rebuilt)))
    }

    pub(crate) fn iterate_ordered_row(
        &mut self,
        token: OrderedCursorToken,
        schema: i64,
    ) -> Result<OrderedIterateStep, OrderedOpStatus> {
        let cursor_index = self
            .ordered_cursor_index(token, schema, OrderedCursorOperation::Iterate)
            .map_err(ordered_consume_status)?;
        let next = match &mut self.ordered_cursors[cursor_index].state {
            OrderedCursorState::Iterate { stack, done } => {
                let Some(next) = stack.pop() else {
                    *done = true;
                    self.ordered_cursors[cursor_index].consumed = true;
                    return Ok(OrderedIterateStep {
                        present: false,
                        row: Vec::new(),
                    });
                };
                next
            }
            _ => unreachable!(),
        };
        let OrderedNodeParts {
            key, value, right, ..
        } = self
            .ordered_node_parts(next)
            .map_err(ordered_consume_status)?;
        let mut right_spine = Vec::new();
        self.ordered_left_spine(right, &mut right_spine)
            .map_err(ordered_consume_status)?;
        let OrderedCursorState::Iterate { stack, .. } =
            &mut self.ordered_cursors[cursor_index].state
        else {
            unreachable!()
        };
        stack
            .try_reserve(right_spine.len())
            .map_err(|_| OrderedOpStatus::AllocationFailed)?;
        stack.extend(right_spine);
        let mut row = key;
        if let Some(value) = value {
            row.try_reserve(value.len())
                .map_err(|_| OrderedOpStatus::AllocationFailed)?;
            row.extend(value);
        }
        Ok(OrderedIterateStep { present: true, row })
    }

    pub(crate) fn begin_ordered_cursor(
        &mut self,
        schema: i64,
        operation: OrderedCursorOperation,
        root: Option<usize>,
    ) -> Result<OrderedCursorToken, OrderedCursorError> {
        if root.is_some_and(|root| {
            self.ordered_nodes
                .get(root)
                .is_none_or(|node| node.schema != schema)
        }) {
            return Err(OrderedCursorError::SchemaMismatch);
        }
        self.ordered_cursors
            .try_reserve(1)
            .map_err(|_| OrderedCursorError::Invalid)?;
        let state = match operation {
            OrderedCursorOperation::Probe => OrderedCursorState::Probe { root },
            OrderedCursorOperation::Insert => OrderedCursorState::Insert {
                root,
                current: root,
                path: Vec::new(),
                phase: OrderedInsertPhase::Inspect,
            },
            OrderedCursorOperation::Iterate => {
                let mut stack = Vec::new();
                self.ordered_left_spine(root, &mut stack)?;
                OrderedCursorState::Iterate { stack, done: false }
            }
        };
        let index = self.ordered_cursors.len();
        self.ordered_cursors.push(OrderedCursor {
            task_generation: self.task_generation,
            schema,
            state,
            consumed: false,
        });
        Ok(OrderedCursorToken {
            index,
            task_generation: self.task_generation,
        })
    }

    pub(crate) fn consume_ordered_cursor(
        &mut self,
        token: OrderedCursorToken,
        schema: i64,
        operation: OrderedCursorOperation,
    ) -> Result<Option<usize>, OrderedCursorError> {
        let index = self.ordered_cursor_index(token, schema, operation)?;
        let cursor = &mut self.ordered_cursors[index];
        cursor.consumed = true;
        match cursor.state {
            OrderedCursorState::Probe { root } => Ok(root),
            OrderedCursorState::Insert { root, .. } => Ok(root),
            OrderedCursorState::Iterate { .. } => Ok(None),
        }
    }

    fn ordered_cursor_index(
        &self,
        token: OrderedCursorToken,
        schema: i64,
        operation: OrderedCursorOperation,
    ) -> Result<usize, OrderedCursorError> {
        if token.task_generation != self.task_generation {
            return Err(OrderedCursorError::Invalid);
        }
        let cursor = self
            .ordered_cursors
            .get(token.index)
            .ok_or(OrderedCursorError::Invalid)?;
        if cursor.task_generation != self.task_generation || cursor.consumed {
            return Err(OrderedCursorError::Stale);
        }
        if cursor.schema != schema {
            return Err(OrderedCursorError::SchemaMismatch);
        }
        if cursor.state.operation() != operation {
            return Err(OrderedCursorError::OperationMismatch);
        }
        Ok(token.index)
    }

    fn ordered_left_spine(
        &self,
        mut node: Option<usize>,
        out: &mut Vec<usize>,
    ) -> Result<(), OrderedCursorError> {
        while let Some(index) = node {
            let current = self
                .ordered_nodes
                .get(index)
                .ok_or(OrderedCursorError::Invalid)?;
            out.try_reserve(1)
                .map_err(|_| OrderedCursorError::Invalid)?;
            out.push(index);
            node = current.left;
        }
        Ok(())
    }
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

    /// Join two resident value-memory byte runs into one fresh task-local value.
    ///
    /// Both operands are resolved through the shared [`handle_bytes`] contract:
    /// store handles, lent molten handles, and this task's own molten handles all
    /// contribute their exact payload bytes. The joined bytes are copied into a
    /// new molten buffer whose handle occupies the same task-local namespace as
    /// [`Op::ArrayNew`] results, so it can never collide with a store or lent
    /// handle. The buffer carries no per-element initialization vector: a string
    /// value is one opaque byte run, addressed only by the byte-run contract.
    fn concat_value_bytes(
        &mut self,
        memories: MemoryView<'_>,
        a: i64,
        b: i64,
    ) -> Result<i64, StringConcatFault> {
        let left = handle_bytes(memories, self, a)
            .map_err(|_| StringConcatFault::LeftUnresident(a))?
            .to_vec();
        let right = handle_bytes(memories, self, b)
            .map_err(|_| StringConcatFault::RightUnresident(b))?
            .to_vec();
        let total = left
            .len()
            .checked_add(right.len())
            .ok_or(StringConcatFault::AllocationFailed)?;
        if total > isize::MAX as usize {
            return Err(StringConcatFault::AllocationFailed);
        }
        let handle =
            task_molten_handle(self.buffers.len()).ok_or(StringConcatFault::AllocationFailed)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(total)
            .map_err(|_| StringConcatFault::AllocationFailed)?;
        bytes.extend_from_slice(&left);
        bytes.extend_from_slice(&right);
        self.buffers
            .try_reserve_exact(1)
            .map_err(|_| StringConcatFault::AllocationFailed)?;
        self.buffers.push(MoltenBuffer {
            bytes,
            initialized: Vec::new(),
        });
        Ok(handle)
    }

    fn split_once_value_bytes(
        &mut self,
        memories: MemoryView<'_>,
        text: i64,
        delimiter: i64,
    ) -> Result<(StringOpStatus, Option<i64>, Option<i64>), StringConcatFault> {
        let text = handle_bytes(memories, self, text)
            .map_err(|_| StringConcatFault::LeftUnresident(text))?
            .to_vec();
        let delimiter = handle_bytes(memories, self, delimiter)
            .map_err(|_| StringConcatFault::RightUnresident(delimiter))?
            .to_vec();
        let Some(index) = find_subslice(&text, &delimiter) else {
            return Ok((StringOpStatus::MissingDelimiter, None, None));
        };
        let split = index
            .checked_add(delimiter.len())
            .ok_or(StringConcatFault::AllocationFailed)?;
        let left = self.alloc_string_bytes(&text[..index])?;
        let right = self.alloc_string_bytes(&text[split..])?;
        Ok((StringOpStatus::Ok, Some(left), Some(right)))
    }

    fn alloc_string_bytes(&mut self, bytes: &[u8]) -> Result<i64, StringConcatFault> {
        let handle =
            task_molten_handle(self.buffers.len()).ok_or(StringConcatFault::AllocationFailed)?;
        let mut owned = Vec::new();
        owned
            .try_reserve_exact(bytes.len())
            .map_err(|_| StringConcatFault::AllocationFailed)?;
        owned.extend_from_slice(bytes);
        self.buffers
            .try_reserve_exact(1)
            .map_err(|_| StringConcatFault::AllocationFailed)?;
        self.buffers.push(MoltenBuffer {
            bytes: owned,
            initialized: Vec::new(),
        });
        Ok(handle)
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
/// `out_index` and `out_generation` must each be non-null and writable for one
/// `i64`, and must not alias memory inside `arena`. This function writes
/// [`ORDERED_CURSOR_POISON`]/`0` to the outputs before it attempts the
/// operation, overwriting them only on success.
pub(crate) unsafe extern "C" fn ordered_begin_probe_abi(
    arena: *mut core::ffi::c_void,
    collection: i64,
    schema: i64,
    out_index: *mut i64,
    out_generation: *mut i64,
) -> i64 {
    if out_index.is_null() || out_generation.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe {
        *out_index = ORDERED_CURSOR_POISON;
        *out_generation = 0;
    }
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.begin_ordered_probe(collection, schema) {
        Ok(token) => {
            let (index, generation) = token.into_words();
            unsafe {
                *out_index = index;
                *out_generation = generation;
            }
            OrderedOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

pub(crate) unsafe extern "C" fn ordered_begin_insert_abi(
    arena: *mut core::ffi::c_void,
    collection: i64,
    schema: i64,
    out_index: *mut i64,
    out_generation: *mut i64,
) -> i64 {
    if out_index.is_null() || out_generation.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe {
        *out_index = ORDERED_CURSOR_POISON;
        *out_generation = 0;
    }
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.begin_ordered_insert(collection, schema) {
        Ok(token) => {
            let (index, generation) = token.into_words();
            unsafe {
                *out_index = index;
                *out_generation = generation;
            }
            OrderedOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

pub(crate) unsafe extern "C" fn ordered_begin_iterate_abi(
    arena: *mut core::ffi::c_void,
    collection: i64,
    schema: i64,
    out_index: *mut i64,
    out_generation: *mut i64,
) -> i64 {
    if out_index.is_null() || out_generation.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe {
        *out_index = ORDERED_CURSOR_POISON;
        *out_generation = 0;
    }
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.begin_ordered_iterate(collection, schema) {
        Ok(token) => {
            let (index, generation) = token.into_words();
            unsafe {
                *out_index = index;
                *out_generation = generation;
            }
            OrderedOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

/// # Safety
/// `arena` must point to a live [`MoltenArena`] for the duration of the call and
/// must not be mutably aliased elsewhere. `out_present`, `out_left`, and
/// `out_right` must each be non-null and writable for one `i64`. `out_key` must
/// be non-null and writable for `key_width` bytes when `key_width > 0`. None of
/// the outputs may alias each other or memory inside `arena`. Every output is
/// cleared before the operation, so a failure never leaves stale key bytes.
#[allow(clippy::too_many_arguments)]
pub(crate) unsafe extern "C" fn ordered_probe_key_abi(
    arena: *mut core::ffi::c_void,
    index: i64,
    generation: i64,
    schema: i64,
    key_width: usize,
    out_present: *mut i64,
    out_left: *mut i64,
    out_right: *mut i64,
    out_key: *mut u8,
) -> i64 {
    if out_present.is_null()
        || out_left.is_null()
        || out_right.is_null()
        || (out_key.is_null() && key_width != 0)
    {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe {
        *out_present = 0;
        *out_left = ORDERED_EMPTY_HANDLE;
        *out_right = ORDERED_EMPTY_HANDLE;
    }
    let out_key = if key_width == 0 {
        &mut [][..]
    } else {
        unsafe { core::slice::from_raw_parts_mut(out_key, key_width) }
    };
    out_key.fill(0);
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let Some(token) = OrderedCursorToken::from_words(index, generation) else {
        return OrderedOpStatus::InvalidHandle as i64;
    };
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.probe_ordered_key(token, schema) {
        Ok(step) => {
            if step.present && step.key.len() != out_key.len() {
                return OrderedOpStatus::SchemaMismatch as i64;
            }
            unsafe {
                *out_present = i64::from(step.present);
                *out_left = step.left;
                *out_right = step.right;
            }
            out_key[..step.key.len()].copy_from_slice(&step.key);
            OrderedOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

/// # Safety
/// `arena` must point to a live [`MoltenArena`] for the duration of the call and
/// must not be mutably aliased elsewhere. `out_present` must be non-null and
/// writable for one `i64`. `out_value` must be non-null and writable for
/// `value_width` bytes when `value_width > 0`. No output may alias another or
/// memory inside `arena`. Both outputs are cleared before the operation.
pub(crate) unsafe extern "C" fn ordered_probe_value_abi(
    arena: *mut core::ffi::c_void,
    index: i64,
    generation: i64,
    schema: i64,
    value_width: usize,
    out_present: *mut i64,
    out_value: *mut u8,
) -> i64 {
    if out_present.is_null() || (out_value.is_null() && value_width != 0) {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe {
        *out_present = 0;
    }
    let out_value = if value_width == 0 {
        &mut [][..]
    } else {
        unsafe { core::slice::from_raw_parts_mut(out_value, value_width) }
    };
    out_value.fill(0);
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let Some(token) = OrderedCursorToken::from_words(index, generation) else {
        return OrderedOpStatus::InvalidHandle as i64;
    };
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.probe_ordered_value(token, schema) {
        Ok((present, value)) => {
            if present && value.len() != out_value.len() {
                return OrderedOpStatus::SchemaMismatch as i64;
            }
            unsafe {
                *out_present = i64::from(present);
            }
            out_value[..value.len()].copy_from_slice(&value);
            OrderedOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) unsafe extern "C" fn ordered_insert_inspect_abi(
    arena: *mut core::ffi::c_void,
    index: i64,
    generation: i64,
    schema: i64,
    key_width: usize,
    out_present: *mut i64,
    out_key: *mut u8,
) -> i64 {
    if out_present.is_null() || (out_key.is_null() && key_width != 0) {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe { *out_present = 0 };
    let out_key = if key_width == 0 {
        &mut [][..]
    } else {
        unsafe { core::slice::from_raw_parts_mut(out_key, key_width) }
    };
    out_key.fill(0);
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let Some(token) = OrderedCursorToken::from_words(index, generation) else {
        return OrderedOpStatus::InvalidHandle as i64;
    };
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.inspect_ordered_insert(token, schema) {
        Ok(step) => {
            if step.present && step.key.len() != out_key.len() {
                return OrderedOpStatus::SchemaMismatch as i64;
            }
            unsafe { *out_present = i64::from(step.present) };
            out_key[..step.key.len()].copy_from_slice(&step.key);
            OrderedOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

pub(crate) unsafe extern "C" fn ordered_insert_advance_abi(
    arena: *mut core::ffi::c_void,
    index: i64,
    generation: i64,
    schema: i64,
    ordering: i64,
    out_ready: *mut i64,
) -> i64 {
    if out_ready.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe { *out_ready = 0 };
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let Some(token) = OrderedCursorToken::from_words(index, generation) else {
        return OrderedOpStatus::InvalidHandle as i64;
    };
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.advance_ordered_insert(token, schema, ordering) {
        Ok(ready) => {
            unsafe { *out_ready = i64::from(ready) };
            OrderedOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) unsafe extern "C" fn ordered_insert_commit_abi(
    arena: *mut core::ffi::c_void,
    index: i64,
    generation: i64,
    schema: i64,
    key: *const u8,
    key_width: usize,
    value: *const u8,
    value_width: usize,
    has_value: i64,
    replace: i64,
    out_collection: *mut i64,
) -> i64 {
    if out_collection.is_null()
        || (key.is_null() && key_width != 0)
        || (has_value != 0 && value.is_null() && value_width != 0)
    {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe { *out_collection = ORDERED_CURSOR_POISON };
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let Some(token) = OrderedCursorToken::from_words(index, generation) else {
        return OrderedOpStatus::InvalidHandle as i64;
    };
    let key = if key_width == 0 {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(key, key_width) }.to_vec()
    };
    let value = (has_value != 0).then(|| {
        if value_width == 0 {
            Vec::new()
        } else {
            unsafe { core::slice::from_raw_parts(value, value_width) }.to_vec()
        }
    });
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.commit_ordered_insert(token, schema, key, value, replace != 0) {
        Ok(collection) => {
            unsafe { *out_collection = collection };
            OrderedOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) unsafe extern "C" fn ordered_iterate_row_abi(
    arena: *mut core::ffi::c_void,
    index: i64,
    generation: i64,
    schema: i64,
    row_width: usize,
    out_present: *mut i64,
    out_row: *mut u8,
) -> i64 {
    if out_present.is_null() || (out_row.is_null() && row_width != 0) {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe { *out_present = 0 };
    let out_row = if row_width == 0 {
        &mut [][..]
    } else {
        unsafe { core::slice::from_raw_parts_mut(out_row, row_width) }
    };
    out_row.fill(0);
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let Some(token) = OrderedCursorToken::from_words(index, generation) else {
        return OrderedOpStatus::InvalidHandle as i64;
    };
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.iterate_ordered_row(token, schema) {
        Ok(step) => {
            if step.present && step.row.len() != out_row.len() {
                return OrderedOpStatus::SchemaMismatch as i64;
            }
            unsafe { *out_present = i64::from(step.present) };
            out_row[..step.row.len()].copy_from_slice(&step.row);
            OrderedOpStatus::Ok as i64
        }
        Err(status) => status as i64,
    }
}

pub(crate) unsafe extern "C" fn ordered_len_abi(
    arena: *mut core::ffi::c_void,
    collection: i64,
    schema: i64,
    out_len: *mut i64,
) -> i64 {
    if out_len.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    unsafe { *out_len = 0 };
    if arena.is_null() {
        return OrderedOpStatus::InvalidHandle as i64;
    }
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.ordered_collection_len(collection, schema) {
        Ok(len) => {
            unsafe { *out_len = len };
            OrderedOpStatus::Ok as i64
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

/// # Safety
/// `store_value_memories` and `lent_molten_value_memories` must each be null only
/// when their count is zero; otherwise they must point to arrays of
/// [`RawValueMemory`] entries valid for the duration of the call. Every raw entry
/// selected by `a` or `b` must point to bytes readable for the call. `arena` must
/// point to a live [`MoltenArena`] and must not be otherwise aliased. `out_handle`
/// must be non-null, writable for one `i64`, and must not alias `arena`. This
/// function writes [`ARRAY_POISON_HANDLE`] before it attempts allocation and
/// overwrites it only on success.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) unsafe extern "C" fn string_concat_abi(
    store_value_memories: *const RawValueMemory,
    store_value_memory_count: usize,
    lent_molten_value_memories: *const RawValueMemory,
    lent_molten_value_memory_count: usize,
    arena: *mut core::ffi::c_void,
    a: i64,
    b: i64,
    out_handle: *mut i64,
) -> i64 {
    if out_handle.is_null() {
        return StringConcatFault::AllocationFailed.status();
    }
    unsafe { *out_handle = ARRAY_POISON_HANDLE };
    if arena.is_null()
        || (store_value_memories.is_null() && store_value_memory_count != 0)
        || (lent_molten_value_memories.is_null() && lent_molten_value_memory_count != 0)
    {
        return StringConcatFault::AllocationFailed.status();
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
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.concat_value_bytes(memories, a, b) {
        Ok(handle) => {
            unsafe { *out_handle = handle };
            StringConcatFault::OK_STATUS
        }
        Err(fault) => fault.status(),
    }
}

#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) unsafe extern "C" fn string_contains_abi(
    store: *const RawValueMemory,
    store_len: usize,
    lent: *const RawValueMemory,
    lent_len: usize,
    arena: *mut core::ffi::c_void,
    text: i64,
    needle: i64,
    out: *mut i64,
) -> i64 {
    if out.is_null()
        || arena.is_null()
        || (store.is_null() && store_len != 0)
        || (lent.is_null() && lent_len != 0)
    {
        return 2;
    }
    let store = if store_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(store, store_len) }
    };
    let lent = if lent_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(lent, lent_len) }
    };
    let memories = MemoryView::Raw(RawValueMemories {
        store,
        molten: lent,
    });
    let arena = unsafe { &*arena.cast::<MoltenArena>() };
    match string_contains_value_bytes(memories, arena, text, needle) {
        Ok(found) => {
            unsafe { *out = i64::from(found) };
            0
        }
        Err(_) => 2,
    }
}

#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) unsafe extern "C" fn string_split_once_abi(
    store: *const RawValueMemory,
    store_len: usize,
    lent: *const RawValueMemory,
    lent_len: usize,
    arena: *mut core::ffi::c_void,
    text: i64,
    delimiter: i64,
    left: *mut i64,
    right: *mut i64,
) -> i64 {
    if left.is_null()
        || right.is_null()
        || arena.is_null()
        || (store.is_null() && store_len != 0)
        || (lent.is_null() && lent_len != 0)
    {
        return StringOpStatus::InvalidInteger as i64;
    }
    let store = if store_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(store, store_len) }
    };
    let lent = if lent_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(lent, lent_len) }
    };
    let memories = MemoryView::Raw(RawValueMemories {
        store,
        molten: lent,
    });
    let arena = unsafe { &mut *arena.cast::<MoltenArena>() };
    match arena.split_once_value_bytes(memories, text, delimiter) {
        Ok((status, Some(a), Some(b))) => {
            unsafe {
                *left = a;
                *right = b
            };
            status as i64
        }
        Ok((status, _, _)) => status as i64,
        Err(_) => StringOpStatus::InvalidInteger as i64,
    }
}

#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) unsafe extern "C" fn string_parse_int_abi(
    store: *const RawValueMemory,
    store_len: usize,
    lent: *const RawValueMemory,
    lent_len: usize,
    arena: *mut core::ffi::c_void,
    text: i64,
    out: *mut i64,
) -> i64 {
    if out.is_null()
        || arena.is_null()
        || (store.is_null() && store_len != 0)
        || (lent.is_null() && lent_len != 0)
    {
        return StringOpStatus::InvalidInteger as i64;
    }
    let store = if store_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(store, store_len) }
    };
    let lent = if lent_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(lent, lent_len) }
    };
    let memories = MemoryView::Raw(RawValueMemories {
        store,
        molten: lent,
    });
    let arena = unsafe { &*arena.cast::<MoltenArena>() };
    match string_parse_int_value_bytes(memories, arena, text) {
        Ok((status, value)) => {
            unsafe { *out = value };
            status as i64
        }
        Err(_) => StringOpStatus::InvalidInteger as i64,
    }
}

/// # Safety
/// `log` must point to a live [`PublicationLog`] and must not be otherwise
/// aliased for the duration of the call. `record` must point to `record_len`
/// bytes readable for the call (the task frame's record region); it may be null
/// only when `record_len` is zero. The bytes are copied before return, so the
/// pointer need not stay valid afterward. Returns [`PublicationFault::OK_STATUS`]
/// on a successful append and [`PublicationFault::ALLOCATION_STATUS`] when the
/// log could not grow — in which case nothing is appended.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
pub(crate) unsafe extern "C" fn publish_abi(
    log: *mut core::ffi::c_void,
    site: u64,
    schema_ref: i64,
    record: *const u8,
    record_len: usize,
) -> i64 {
    if log.is_null() || (record.is_null() && record_len != 0) {
        return PublicationFault::ALLOCATION_STATUS;
    }
    let log = unsafe { &mut *log.cast::<PublicationLog>() };
    let bytes = if record_len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(record, record_len) }
    };
    match log.publish(site, schema_ref, bytes) {
        Ok(()) => PublicationFault::OK_STATUS,
        Err(PublicationFault::AllocationFailed) => PublicationFault::ALLOCATION_STATUS,
    }
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
    /// Validate a checked array status and compare it with one closed status.
    ///
    /// A malformed status word faults; it is never silently false.
    ArrayStatusIs {
        dst: u32,
        status: u32,
        expected: ArrayOpStatus,
    },
    /// Lexicographically compare two resident value-memory byte runs.
    ///
    /// `frame[a]` and `frame[b]` are value handles. The result is the closed
    /// three-way ordinal `0 = less`, `1 = equal`, `2 = greater`. Task admission
    /// must have made every handle it compares resident in the value-memory
    /// table; even equal handle integers fault if the shared handle is not
    /// resident.
    CompareValueBytes { dst: u32, a: u32, b: u32 },
    /// Concatenate two resident value-memory byte runs into one fresh string.
    ///
    /// `frame[a]` and `frame[b]` are string value handles. Their exact byte runs
    /// are joined, in operand order, into a new task-local molten value whose
    /// handle is written to `frame[dst]`. The result is itself a resident value
    /// handle: it feeds further [`Op::StringConcat`] and [`Op::CompareValueBytes`]
    /// with identical byte semantics to an interned literal. Task admission must
    /// have made every operand handle resident; a non-resident operand faults
    /// with the precise side, and an allocation the arena cannot satisfy faults
    /// rather than fabricating a handle.
    StringConcat { dst: u32, a: u32, b: u32 },
    /// Search two resident string byte runs without exposing their handles.
    StringContains { dst: u32, text: u32, needle: u32 },
    /// Split a resident string at its first delimiter occurrence.
    StringSplitOnce {
        left: u32,
        right: u32,
        status: u32,
        text: u32,
        delimiter: u32,
    },
    /// Parse a resident string as a signed decimal integer.
    StringParseInt { dst: u32, status: u32, text: u32 },
    /// Compare a string-operation status with one closed status word.
    StringStatusIs {
        dst: u32,
        status: u32,
        expected: StringOpStatus,
    },
    /// Append one descriptor to the task's verified append-only publication log.
    ///
    /// The complete `record_width`-byte value at `frame[record..]` is copied by
    /// value into task-owned storage, tagged with the opaque provenance key
    /// `site` and the record schema witnessed by `record_schema_ref`. The schema
    /// must be a [`crate::PayloadKind::PublicationRecord`] whose inline shape is a
    /// scalar run — the log therefore holds only owned scalar bytes and can never
    /// carry a store or molten handle out of the task (no handle leakage). `site`
    /// is never interpreted by the machine: it is stable front-end provenance
    /// identity, not a check outcome. Publishing is a pure control-flow effect on
    /// a taken path with no host observer and no callback; the completed ordered
    /// log is exposed read-only after the task is done. An allocation the log
    /// cannot satisfy faults rather than dropping or truncating a descriptor.
    Publish {
        site: u64,
        record: u32,
        record_width: u32,
        record_schema_ref: i64,
    },
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
    /// Begin a single-use Probe cursor over an ordered collection.
    ///
    /// `frame[collection]` holds the collection root handle: `0` is the
    /// canonical empty collection, and any `n >= 1` names arena node `n - 1`.
    /// On success the two-word opaque region at `cursor` receives the cursor
    /// token (arena index, task generation) and `frame[status]` receives
    /// [`OrderedOpStatus::Ok`]; on failure the cursor index word receives
    /// [`ORDERED_CURSOR_POISON`], its generation word `0`, and `frame[status]`
    /// the precise [`OrderedOpStatus`]. The cursor word is internal-only: the
    /// verifier forbids it at entries, results, calls, publication, copy, and
    /// scalar interpretation.
    OrderedBeginProbe {
        cursor: u32,
        status: u32,
        collection: u32,
        collection_schema_ref: i64,
    },
    /// Consume a Probe cursor and expose one probe step of the closed handshake.
    ///
    /// `frame[cursor]` is the two-word opaque cursor token. On a live cursor,
    /// `frame[present]` receives `1` when the cursor named a node (and `key`
    /// receives that node's `key_width` key bytes, `left`/`right` its child
    /// collection handles) or `0` at an empty position (a probe miss). The
    /// bytecode then compares its search key against `key` and descends into
    /// `left` or `right`. `frame[status]` receives [`OrderedOpStatus`]; a
    /// forged, stale, cross-task, cross-schema, or cross-operation cursor
    /// yields the precise status with `present = 0` and `key` zeroed.
    OrderedProbeKey {
        cursor: u32,
        present: u32,
        key: u32,
        left: u32,
        right: u32,
        status: u32,
        key_width: u32,
        collection_schema_ref: i64,
    },
    /// Consume a Probe cursor and expose the current node's Map value.
    ///
    /// The `get` terminal: taken only after a structural comparison found an
    /// equal key. `frame[present]` receives `1` and `value` the node's
    /// `value_width` value bytes when the cursor named a node, or `0` at an
    /// empty position (a miss the lowering turns into `MissingKey`).
    /// `frame[status]` receives [`OrderedOpStatus`]. `has` never emits this op,
    /// so membership never projects a value.
    OrderedProbeValue {
        cursor: u32,
        present: u32,
        value: u32,
        status: u32,
        value_width: u32,
        collection_schema_ref: i64,
    },
    /// Materialize the canonical empty root for one ordered schema.
    OrderedEmpty {
        dst: u32,
        collection_schema_ref: i64,
    },
    /// Begin a persistent insert-or-replace handshake over one collection.
    OrderedBeginInsert {
        cursor: u32,
        status: u32,
        collection: u32,
        collection_schema_ref: i64,
    },
    /// Expose the current insertion candidate key, or an empty insertion site.
    OrderedInsertInspect {
        cursor: u32,
        present: u32,
        key: u32,
        status: u32,
        key_width: u32,
        collection_schema_ref: i64,
    },
    /// Advance an insertion cursor using the verified structural ordering word.
    OrderedInsertAdvance {
        cursor: u32,
        ordering: u32,
        ready: u32,
        status: u32,
        collection_schema_ref: i64,
    },
    /// Commit a new persistent root after the insertion path is resolved.
    OrderedInsertCommit {
        dst: u32,
        cursor: u32,
        key: u32,
        value: Option<u32>,
        status: u32,
        key_width: u32,
        value_width: u32,
        collection_schema_ref: i64,
        replace: bool,
    },
    /// Begin canonical in-order iteration over an ordered collection.
    OrderedBeginIterate {
        cursor: u32,
        status: u32,
        collection: u32,
        collection_schema_ref: i64,
    },
    /// Yield the next complete row in canonical key order.
    OrderedIterateRow {
        cursor: u32,
        present: u32,
        row: u32,
        status: u32,
        row_width: u32,
        collection_schema_ref: i64,
    },
    /// Read the persistent collection's stored cardinality.
    OrderedLen {
        dst: u32,
        status: u32,
        collection: u32,
        collection_schema_ref: i64,
    },
    /// Validate and compare a word in the closed ordered-operation status ABI.
    OrderedStatusIs {
        dst: u32,
        status: u32,
        expected: OrderedOpStatus,
    },
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
    publications: PublicationLog,
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
            publications: PublicationLog::default(),
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

    /// The task's append-only publication log, read-only.
    #[must_use]
    pub(crate) fn publications(&self) -> &PublicationLog {
        &self.publications
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
                op @ (Op::ProductConstruct { .. }
                | Op::ProductProject { .. }
                | Op::CopyValue { .. }
                | Op::EnumConstruct { .. }
                | Op::EnumIsVariant { .. }
                | Op::EnumProjectChecked { .. }) => {
                    let Some(verified) = verified else {
                        panic!("typed structural operation requires VerifiedProgram");
                    };
                    self.execute_structural(verified, fn_id, pc, base, &op)?;
                    self.frames.last_mut().expect("frame").pc += 1;
                }
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
                Op::ArrayStatusIs {
                    dst,
                    status,
                    expected,
                } => {
                    let actual = read_i64_at(&self.arena, base + status as usize);
                    let Some(actual) = ArrayOpStatus::from_word(actual) else {
                        let Some(verified) = verified else {
                            panic!("array status validation requires VerifiedProgram");
                        };
                        return Err(TaskFault::InvalidStringStatus {
                            site: fault_site(verified, fn_id, pc)?,
                            actual,
                        });
                    };
                    write_i64_at(
                        &mut self.arena,
                        base + dst as usize,
                        i64::from(actual == expected),
                    );
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
                Op::StringConcat { dst, a, b } => {
                    let a_handle = read_i64_at(&self.arena, base + a as usize);
                    let b_handle = read_i64_at(&self.arena, base + b as usize);
                    let handle = match self.molten.concat_value_bytes(
                        MemoryView::from(value_memories),
                        a_handle,
                        b_handle,
                    ) {
                        Ok(handle) => handle,
                        Err(fault) => {
                            let Some(verified) = verified else {
                                panic!("legacy raw StringConcat operand is not resident");
                            };
                            return Err(string_concat_fault(
                                fault_site(verified, fn_id, pc)?,
                                fault,
                            ));
                        }
                    };
                    write_i64_at(&mut self.arena, base + dst as usize, handle);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::StringContains { dst, text, needle } => {
                    let text = read_i64_at(&self.arena, base + text as usize);
                    let needle = read_i64_at(&self.arena, base + needle as usize);
                    let found = match string_contains_value_bytes(
                        MemoryView::from(value_memories),
                        &self.molten,
                        text,
                        needle,
                    ) {
                        Ok(found) => found,
                        Err(fault) => {
                            let Some(verified) = verified else {
                                panic!("legacy raw StringContains operand is not resident");
                            };
                            return Err(string_concat_fault(
                                fault_site(verified, fn_id, pc)?,
                                fault,
                            ));
                        }
                    };
                    write_i64_at(&mut self.arena, base + dst as usize, i64::from(found));
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::StringSplitOnce {
                    left,
                    right,
                    status,
                    text,
                    delimiter,
                } => {
                    let text = read_i64_at(&self.arena, base + text as usize);
                    let delimiter = read_i64_at(&self.arena, base + delimiter as usize);
                    let (result, left_handle, right_handle) = match self
                        .molten
                        .split_once_value_bytes(MemoryView::from(value_memories), text, delimiter)
                    {
                        Ok(result) => result,
                        Err(fault) => {
                            let Some(verified) = verified else {
                                panic!("legacy raw StringSplitOnce operand is not resident");
                            };
                            return Err(string_concat_fault(
                                fault_site(verified, fn_id, pc)?,
                                fault,
                            ));
                        }
                    };
                    write_i64_at(&mut self.arena, base + status as usize, result as i64);
                    if let (Some(left_handle), Some(right_handle)) = (left_handle, right_handle) {
                        write_i64_at(&mut self.arena, base + left as usize, left_handle);
                        write_i64_at(&mut self.arena, base + right as usize, right_handle);
                    }
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::StringParseInt { dst, status, text } => {
                    let text = read_i64_at(&self.arena, base + text as usize);
                    let (result, value) = match string_parse_int_value_bytes(
                        MemoryView::from(value_memories),
                        &self.molten,
                        text,
                    ) {
                        Ok(result) => result,
                        Err(fault) => {
                            let Some(verified) = verified else {
                                panic!("legacy raw StringParseInt operand is not resident");
                            };
                            return Err(string_concat_fault(
                                fault_site(verified, fn_id, pc)?,
                                fault,
                            ));
                        }
                    };
                    write_i64_at(&mut self.arena, base + status as usize, result as i64);
                    write_i64_at(&mut self.arena, base + dst as usize, value);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::StringStatusIs {
                    dst,
                    status,
                    expected,
                } => {
                    let actual = read_i64_at(&self.arena, base + status as usize);
                    let Some(actual) = StringOpStatus::from_word(actual) else {
                        let Some(verified) = verified else {
                            panic!("string status validation requires VerifiedProgram");
                        };
                        return Err(TaskFault::InvalidStringStatus {
                            site: fault_site(verified, fn_id, pc)?,
                            actual,
                        });
                    };
                    write_i64_at(
                        &mut self.arena,
                        base + dst as usize,
                        i64::from(actual == expected),
                    );
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::Publish {
                    site,
                    record,
                    record_width,
                    record_schema_ref,
                } => {
                    let start = base + record as usize;
                    let end = start + record_width as usize;
                    let bytes = self.arena[start..end].to_vec();
                    if let Err(PublicationFault::AllocationFailed) =
                        self.publications.publish(site, record_schema_ref, &bytes)
                    {
                        let Some(verified) = verified else {
                            panic!("legacy raw Publish allocation failed");
                        };
                        return Err(TaskFault::PublicationAllocationFailed {
                            site: fault_site(verified, fn_id, pc)?,
                        });
                    }
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
                Op::OrderedBeginProbe {
                    cursor,
                    status,
                    collection,
                    collection_schema_ref,
                } => {
                    let collection = read_i64_at(&self.arena, base + collection as usize);
                    let mut index = ORDERED_CURSOR_POISON;
                    let mut generation = 0i64;
                    let op_status = match self
                        .molten
                        .begin_ordered_probe(collection, collection_schema_ref)
                    {
                        Ok(token) => {
                            let (token_index, token_generation) = token.into_words();
                            index = token_index;
                            generation = token_generation;
                            OrderedOpStatus::Ok
                        }
                        Err(status) => status,
                    };
                    write_i64_at(&mut self.arena, base + cursor as usize, index);
                    write_i64_at(&mut self.arena, base + cursor as usize + 8, generation);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedProbeKey {
                    cursor,
                    present,
                    key,
                    left,
                    right,
                    status,
                    key_width,
                    collection_schema_ref,
                } => {
                    let index = read_i64_at(&self.arena, base + cursor as usize);
                    let generation = read_i64_at(&self.arena, base + cursor as usize + 8);
                    let key_at = base + key as usize;
                    let key_width = key_width as usize;
                    self.arena[key_at..key_at + key_width].fill(0);
                    let mut present_value = 0i64;
                    let mut left_value = ORDERED_EMPTY_HANDLE;
                    let mut right_value = ORDERED_EMPTY_HANDLE;
                    let op_status = match OrderedCursorToken::from_words(index, generation) {
                        None => OrderedOpStatus::InvalidHandle,
                        Some(token) => {
                            match self.molten.probe_ordered_key(token, collection_schema_ref) {
                                Ok(step) => {
                                    if step.present && step.key.len() != key_width {
                                        OrderedOpStatus::SchemaMismatch
                                    } else {
                                        present_value = i64::from(step.present);
                                        left_value = step.left;
                                        right_value = step.right;
                                        self.arena[key_at..key_at + step.key.len()]
                                            .copy_from_slice(&step.key);
                                        OrderedOpStatus::Ok
                                    }
                                }
                                Err(status) => status,
                            }
                        }
                    };
                    write_i64_at(&mut self.arena, base + present as usize, present_value);
                    write_i64_at(&mut self.arena, base + left as usize, left_value);
                    write_i64_at(&mut self.arena, base + right as usize, right_value);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedProbeValue {
                    cursor,
                    present,
                    value,
                    status,
                    value_width,
                    collection_schema_ref,
                } => {
                    let index = read_i64_at(&self.arena, base + cursor as usize);
                    let generation = read_i64_at(&self.arena, base + cursor as usize + 8);
                    let value_at = base + value as usize;
                    let value_width = value_width as usize;
                    self.arena[value_at..value_at + value_width].fill(0);
                    let mut present_value = 0i64;
                    let op_status = match OrderedCursorToken::from_words(index, generation) {
                        None => OrderedOpStatus::InvalidHandle,
                        Some(token) => {
                            match self
                                .molten
                                .probe_ordered_value(token, collection_schema_ref)
                            {
                                Ok((present_flag, bytes)) => {
                                    if present_flag && bytes.len() != value_width {
                                        OrderedOpStatus::SchemaMismatch
                                    } else {
                                        present_value = i64::from(present_flag);
                                        self.arena[value_at..value_at + bytes.len()]
                                            .copy_from_slice(&bytes);
                                        OrderedOpStatus::Ok
                                    }
                                }
                                Err(status) => status,
                            }
                        }
                    };
                    write_i64_at(&mut self.arena, base + present as usize, present_value);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedEmpty {
                    dst,
                    collection_schema_ref: _,
                } => {
                    write_i64_at(&mut self.arena, base + dst as usize, ORDERED_EMPTY_HANDLE);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedBeginInsert {
                    cursor,
                    status,
                    collection,
                    collection_schema_ref,
                } => {
                    let collection = read_i64_at(&self.arena, base + collection as usize);
                    let begun = self
                        .molten
                        .begin_ordered_insert(collection, collection_schema_ref);
                    let mut index = ORDERED_CURSOR_POISON;
                    let mut generation = 0i64;
                    let op_status = match begun {
                        Ok(token) => {
                            (index, generation) = token.into_words();
                            OrderedOpStatus::Ok
                        }
                        Err(status) => status,
                    };
                    write_i64_at(&mut self.arena, base + cursor as usize, index);
                    write_i64_at(&mut self.arena, base + cursor as usize + 8, generation);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedBeginIterate {
                    cursor,
                    status,
                    collection,
                    collection_schema_ref,
                } => {
                    let collection = read_i64_at(&self.arena, base + collection as usize);
                    let begun = self
                        .molten
                        .begin_ordered_iterate(collection, collection_schema_ref);
                    let mut index = ORDERED_CURSOR_POISON;
                    let mut generation = 0i64;
                    let op_status = match begun {
                        Ok(token) => {
                            (index, generation) = token.into_words();
                            OrderedOpStatus::Ok
                        }
                        Err(status) => status,
                    };
                    write_i64_at(&mut self.arena, base + cursor as usize, index);
                    write_i64_at(&mut self.arena, base + cursor as usize + 8, generation);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedInsertInspect {
                    cursor,
                    present,
                    key,
                    status,
                    key_width,
                    collection_schema_ref,
                } => {
                    let index = read_i64_at(&self.arena, base + cursor as usize);
                    let generation = read_i64_at(&self.arena, base + cursor as usize + 8);
                    let key_at = base + key as usize;
                    let key_width = key_width as usize;
                    self.arena[key_at..key_at + key_width].fill(0);
                    let mut present_value = 0i64;
                    let op_status = match OrderedCursorToken::from_words(index, generation) {
                        None => OrderedOpStatus::InvalidHandle,
                        Some(token) => match self
                            .molten
                            .inspect_ordered_insert(token, collection_schema_ref)
                        {
                            Ok(step) if !step.present || step.key.len() == key_width => {
                                present_value = i64::from(step.present);
                                self.arena[key_at..key_at + step.key.len()]
                                    .copy_from_slice(&step.key);
                                OrderedOpStatus::Ok
                            }
                            Ok(_) => OrderedOpStatus::SchemaMismatch,
                            Err(status) => status,
                        },
                    };
                    write_i64_at(&mut self.arena, base + present as usize, present_value);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedInsertAdvance {
                    cursor,
                    ordering,
                    ready,
                    status,
                    collection_schema_ref,
                } => {
                    let index = read_i64_at(&self.arena, base + cursor as usize);
                    let generation = read_i64_at(&self.arena, base + cursor as usize + 8);
                    let ordering = read_i64_at(&self.arena, base + ordering as usize);
                    let mut ready_value = 0;
                    let op_status = match OrderedCursorToken::from_words(index, generation) {
                        None => OrderedOpStatus::InvalidHandle,
                        Some(token) => match self.molten.advance_ordered_insert(
                            token,
                            collection_schema_ref,
                            ordering,
                        ) {
                            Ok(ready) => {
                                ready_value = i64::from(ready);
                                OrderedOpStatus::Ok
                            }
                            Err(status) => status,
                        },
                    };
                    write_i64_at(&mut self.arena, base + ready as usize, ready_value);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedInsertCommit {
                    dst,
                    cursor,
                    key,
                    value,
                    status,
                    key_width,
                    value_width,
                    collection_schema_ref,
                    replace,
                } => {
                    let index = read_i64_at(&self.arena, base + cursor as usize);
                    let generation = read_i64_at(&self.arena, base + cursor as usize + 8);
                    let key = self.arena
                        [base + key as usize..base + key as usize + key_width as usize]
                        .to_vec();
                    let value = value.map(|value| {
                        self.arena
                            [base + value as usize..base + value as usize + value_width as usize]
                            .to_vec()
                    });
                    let mut collection = ORDERED_CURSOR_POISON;
                    let op_status = match OrderedCursorToken::from_words(index, generation) {
                        None => OrderedOpStatus::InvalidHandle,
                        Some(token) => match self.molten.commit_ordered_insert(
                            token,
                            collection_schema_ref,
                            key,
                            value,
                            replace,
                        ) {
                            Ok(handle) => {
                                collection = handle;
                                OrderedOpStatus::Ok
                            }
                            Err(status) => status,
                        },
                    };
                    write_i64_at(&mut self.arena, base + dst as usize, collection);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedIterateRow {
                    cursor,
                    present,
                    row,
                    status,
                    row_width,
                    collection_schema_ref,
                } => {
                    let index = read_i64_at(&self.arena, base + cursor as usize);
                    let generation = read_i64_at(&self.arena, base + cursor as usize + 8);
                    let row_at = base + row as usize;
                    let row_width = row_width as usize;
                    self.arena[row_at..row_at + row_width].fill(0);
                    let mut present_value = 0;
                    let op_status = match OrderedCursorToken::from_words(index, generation) {
                        None => OrderedOpStatus::InvalidHandle,
                        Some(token) => match self
                            .molten
                            .iterate_ordered_row(token, collection_schema_ref)
                        {
                            Ok(step) if !step.present || step.row.len() == row_width => {
                                present_value = i64::from(step.present);
                                self.arena[row_at..row_at + step.row.len()]
                                    .copy_from_slice(&step.row);
                                OrderedOpStatus::Ok
                            }
                            Ok(_) => OrderedOpStatus::SchemaMismatch,
                            Err(status) => status,
                        },
                    };
                    write_i64_at(&mut self.arena, base + present as usize, present_value);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedLen {
                    dst,
                    status,
                    collection,
                    collection_schema_ref,
                } => {
                    let collection = read_i64_at(&self.arena, base + collection as usize);
                    let mut len = 0;
                    let op_status = match self
                        .molten
                        .ordered_collection_len(collection, collection_schema_ref)
                    {
                        Ok(value) => {
                            len = value;
                            OrderedOpStatus::Ok
                        }
                        Err(status) => status,
                    };
                    write_i64_at(&mut self.arena, base + dst as usize, len);
                    write_i64_at(&mut self.arena, base + status as usize, op_status as i64);
                    self.frames.last_mut().expect("frame").pc += 1;
                }
                Op::OrderedStatusIs {
                    dst,
                    status,
                    expected,
                } => {
                    let actual = read_i64_at(&self.arena, base + status as usize);
                    let Some(actual) = OrderedOpStatus::from_word(actual) else {
                        let Some(verified) = verified else {
                            panic!("ordered status validation requires VerifiedProgram");
                        };
                        return Err(TaskFault::InvalidOrderedStatus {
                            site: fault_site(verified, fn_id, pc)?,
                            actual,
                        });
                    };
                    write_i64_at(
                        &mut self.arena,
                        base + dst as usize,
                        i64::from(actual == expected),
                    );
                    self.frames.last_mut().expect("frame").pc += 1;
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

    fn execute_structural(
        &mut self,
        verified: &VerifiedProgram,
        function: FnId,
        pc: usize,
        base: usize,
        op: &Op,
    ) -> Result<(), TaskFault> {
        let contract = &verified.contract().functions[function.0 as usize];
        let region = |id: RegionId| &contract.frame.regions[id.0 as usize];
        let copy_region = |arena: &mut Vec<u8>, destination: RegionId, source: RegionId| {
            let destination = region(destination);
            let source = region(source);
            arena.copy_within(
                base + source.offset as usize
                    ..base + source.offset as usize + source.shape.words.len() * 8,
                base + destination.offset as usize,
            );
        };
        match op {
            Op::ProductConstruct { dst, fields } => {
                let value_shape = region(*dst).value_shape.unwrap();
                let crate::ValueShapeKind::Product { fields: declared } =
                    &verified.contract().value_shapes[value_shape.0 as usize].kind
                else {
                    unreachable!();
                };
                for source in fields {
                    let field = &declared[source.field as usize];
                    let source_region = region(source.source);
                    let len = field.shape.words.len() * 8;
                    self.arena.copy_within(
                        base + source_region.offset as usize
                            ..base + source_region.offset as usize + len,
                        base + region(*dst).offset as usize + field.offset as usize,
                    );
                }
            }
            Op::ProductProject {
                dst,
                product,
                field,
            } => {
                let value_shape = region(*product).value_shape.unwrap();
                let crate::ValueShapeKind::Product { fields } =
                    &verified.contract().value_shapes[value_shape.0 as usize].kind
                else {
                    unreachable!();
                };
                let field = &fields[*field as usize];
                let len = field.shape.words.len() * 8;
                self.arena.copy_within(
                    base + region(*product).offset as usize + field.offset as usize
                        ..base + region(*product).offset as usize + field.offset as usize + len,
                    base + region(*dst).offset as usize,
                );
            }
            Op::CopyValue { dst, src } => copy_region(&mut self.arena, *dst, *src),
            Op::EnumConstruct {
                dst,
                variant,
                fields,
            } => {
                let destination = region(*dst);
                let value_shape = destination.value_shape.unwrap();
                let crate::ValueShapeKind::Enum { selector, variants } =
                    &verified.contract().value_shapes[value_shape.0 as usize].kind
                else {
                    unreachable!();
                };
                let start = base + destination.offset as usize;
                self.arena[start..start + destination.shape.words.len() * 8].fill(0);
                write_i64_at(
                    &mut self.arena,
                    start + selector.offset as usize,
                    i64::from(*variant),
                );
                for source in fields {
                    let field = &variants[*variant as usize].fields[source.field as usize];
                    let source_region = region(source.source);
                    let len = field.shape.words.len() * 8;
                    self.arena.copy_within(
                        base + source_region.offset as usize
                            ..base + source_region.offset as usize + len,
                        start + field.offset as usize,
                    );
                }
            }
            Op::EnumIsVariant {
                dst,
                value,
                variant,
            } => {
                let actual =
                    self.checked_enum_selector(verified, function, pc, base, *value, op)?;
                write_i64_at(
                    &mut self.arena,
                    base + region(*dst).offset as usize,
                    i64::from(actual == i64::from(*variant)),
                );
            }
            Op::EnumProjectChecked {
                dst,
                value,
                variant,
                field,
            } => {
                let actual =
                    self.checked_enum_selector(verified, function, pc, base, *value, op)?;
                if actual != i64::from(*variant) {
                    let value_shape = region(*value).value_shape.unwrap();
                    return Err(TaskFault::EnumProjectionMismatch {
                        site: fault_site(verified, function, pc)?,
                        value_shape,
                        expected: i64::from(*variant),
                        actual,
                    });
                }
                let value_shape = region(*value).value_shape.unwrap();
                let crate::ValueShapeKind::Enum { variants, .. } =
                    &verified.contract().value_shapes[value_shape.0 as usize].kind
                else {
                    unreachable!();
                };
                let field = &variants[*variant as usize].fields[*field as usize];
                let len = field.shape.words.len() * 8;
                self.arena.copy_within(
                    base + region(*value).offset as usize + field.offset as usize
                        ..base + region(*value).offset as usize + field.offset as usize + len,
                    base + region(*dst).offset as usize,
                );
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    fn checked_enum_selector(
        &self,
        verified: &VerifiedProgram,
        function: FnId,
        pc: usize,
        base: usize,
        value: RegionId,
        op: &Op,
    ) -> Result<i64, TaskFault> {
        let region = &verified.contract().functions[function.0 as usize]
            .frame
            .regions[value.0 as usize];
        let value_shape = region.value_shape.unwrap();
        let crate::ValueShapeKind::Enum { selector, variants } =
            &verified.contract().value_shapes[value_shape.0 as usize].kind
        else {
            unreachable!();
        };
        let actual = read_i64_at(
            &self.arena,
            base + region.offset as usize + selector.offset as usize,
        );
        if usize::try_from(actual).is_err() || actual as usize >= variants.len() {
            return Err(TaskFault::InvalidEnumSelector {
                site: fault_site(verified, function, pc)?,
                value_shape,
                expected: (0..variants.len()).map(|variant| variant as i64).collect(),
                actual,
            });
        }
        let _ = op;
        Ok(actual)
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

fn string_contains_value_bytes(
    memories: MemoryView<'_>,
    molten: &MoltenArena,
    text: i64,
    needle: i64,
) -> Result<bool, StringConcatFault> {
    let text = handle_bytes(memories, molten, text)
        .map_err(|_| StringConcatFault::LeftUnresident(text))?;
    let needle = handle_bytes(memories, molten, needle)
        .map_err(|_| StringConcatFault::RightUnresident(needle))?;
    Ok(find_subslice(text, needle).is_some())
}

fn string_parse_int_value_bytes(
    memories: MemoryView<'_>,
    molten: &MoltenArena,
    text: i64,
) -> Result<(StringOpStatus, i64), StringConcatFault> {
    let text = handle_bytes(memories, molten, text)
        .map_err(|_| StringConcatFault::LeftUnresident(text))?;
    let Ok(text) = core::str::from_utf8(text) else {
        return Ok((StringOpStatus::InvalidInteger, 0));
    };
    match text.parse::<i64>() {
        Ok(value) => Ok((StringOpStatus::Ok, value)),
        Err(error)
            if error.kind() == &core::num::IntErrorKind::PosOverflow
                || error.kind() == &core::num::IntErrorKind::NegOverflow =>
        {
            Ok((StringOpStatus::IntegerOverflow, 0))
        }
        Err(_) => Ok((StringOpStatus::InvalidInteger, 0)),
    }
}

fn find_subslice(text: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    text.windows(needle.len())
        .position(|window| window == needle)
}

/// Lift a [`StringConcatFault`] to the typed task fault carried at `site`.
fn string_concat_fault(site: FaultSite, fault: StringConcatFault) -> TaskFault {
    match fault {
        StringConcatFault::LeftUnresident(handle) => TaskFault::UnresidentStringConcatOperand {
            site,
            side: CompareSide::Left,
            handle,
        },
        StringConcatFault::RightUnresident(handle) => TaskFault::UnresidentStringConcatOperand {
            site,
            side: CompareSide::Right,
            handle,
        },
        StringConcatFault::AllocationFailed => TaskFault::StringConcatAllocationFailed { site },
    }
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

    #[test]
    fn ordered_cursor_is_single_use_schema_and_operation_confined() {
        let mut arena = MoltenArena::default();
        let leaf = arena
            .alloc_ordered_node(7, vec![1, 2], Some(vec![3, 4]), None, None)
            .expect("node allocation");
        let node = &arena.ordered_nodes[leaf];
        assert_eq!(node.schema, 7);
        assert_eq!(node.key, [1, 2]);
        assert_eq!(node.value.as_deref(), Some([3, 4].as_slice()));
        assert_eq!(node.left, None);
        assert_eq!(node.right, None);
        let cursor = arena
            .begin_ordered_cursor(7, OrderedCursorOperation::Probe, Some(leaf))
            .expect("cursor allocation");
        assert_eq!(
            arena.consume_ordered_cursor(cursor, 7, OrderedCursorOperation::Probe),
            Ok(Some(leaf))
        );
        assert_eq!(
            arena.consume_ordered_cursor(cursor, 7, OrderedCursorOperation::Probe),
            Err(OrderedCursorError::Stale)
        );
        let cursor = arena
            .begin_ordered_cursor(7, OrderedCursorOperation::Insert, Some(leaf))
            .expect("cursor allocation");
        assert_eq!(
            arena.consume_ordered_cursor(cursor, 8, OrderedCursorOperation::Insert),
            Err(OrderedCursorError::SchemaMismatch)
        );
        let cursor = arena
            .begin_ordered_cursor(7, OrderedCursorOperation::Iterate, Some(leaf))
            .expect("cursor allocation");
        assert_eq!(
            arena.consume_ordered_cursor(cursor, 7, OrderedCursorOperation::Insert),
            Err(OrderedCursorError::OperationMismatch)
        );
        let mut other = MoltenArena::default();
        let foreign = other
            .begin_ordered_cursor(7, OrderedCursorOperation::Probe, None)
            .expect("foreign cursor allocation");
        assert_eq!(
            arena.consume_ordered_cursor(foreign, 7, OrderedCursorOperation::Probe),
            Err(OrderedCursorError::Invalid)
        );
    }

    #[test]
    fn begin_ordered_probe_decodes_roots_and_confines_the_cursor() {
        let mut arena = MoltenArena::default();
        // Empty collection (canonical handle 0) begins a Probe cursor at no root.
        let token = arena
            .begin_ordered_probe(ORDERED_EMPTY_HANDLE, 7)
            .expect("empty probe begins");
        let (index, generation) = token.into_words();
        assert_eq!(index, 0);
        assert_ne!(generation, 0, "a real cursor carries the task generation");
        // The cursor is a real, consumable Probe cursor confined to this arena.
        assert_eq!(
            arena.consume_ordered_cursor(token, 7, OrderedCursorOperation::Probe),
            Ok(None)
        );

        // A non-empty handle names arena node `n - 1`; a matching schema begins.
        let leaf = arena
            .alloc_ordered_node(9, vec![1, 2], Some(vec![3, 4]), None, None)
            .expect("node allocation");
        let rooted = arena
            .begin_ordered_probe(leaf as i64 + 1, 9)
            .expect("rooted probe begins");
        assert_eq!(
            arena.consume_ordered_cursor(rooted, 9, OrderedCursorOperation::Probe),
            Ok(Some(leaf))
        );

        // Wrong schema for a rooted collection is a typed status, not a panic.
        assert_eq!(
            arena.begin_ordered_probe(leaf as i64 + 1, 8),
            Err(OrderedOpStatus::SchemaMismatch)
        );
        // An out-of-range handle never touches a node: it is InvalidHandle.
        assert_eq!(
            arena.begin_ordered_probe(999, 9),
            Err(OrderedOpStatus::InvalidHandle)
        );
    }

    #[test]
    fn probe_ordered_key_exposes_nodes_and_spends_the_cursor() {
        let mut arena = MoltenArena::default();
        let left = arena
            .alloc_ordered_node(9, vec![1], Some(vec![10]), None, None)
            .unwrap();
        let right = arena
            .alloc_ordered_node(9, vec![3], Some(vec![30]), None, None)
            .unwrap();
        let root = arena
            .alloc_ordered_node(9, vec![2], Some(vec![20]), Some(left), Some(right))
            .unwrap();

        // Probe at the root exposes its key and the two child collection handles.
        let cursor = arena.begin_ordered_probe(root as i64 + 1, 9).unwrap();
        let step = arena.probe_ordered_key(cursor, 9).expect("root probe");
        assert!(step.present);
        assert_eq!(step.key, vec![2]);
        assert_eq!(step.left, left as i64 + 1);
        assert_eq!(step.right, right as i64 + 1);
        // The cursor is single-use: a second probe of the same token is stale.
        assert_eq!(
            arena.probe_ordered_key(cursor, 9),
            Err(OrderedOpStatus::Stale)
        );

        // Descending a child handle reaches a leaf whose children are empty.
        let cursor = arena.begin_ordered_probe(step.left, 9).unwrap();
        let leaf = arena.probe_ordered_key(cursor, 9).expect("leaf probe");
        assert_eq!(leaf.key, vec![1]);
        assert_eq!(leaf.left, ORDERED_EMPTY_HANDLE);
        assert_eq!(leaf.right, ORDERED_EMPTY_HANDLE);

        // The empty collection probes to a miss without exposing a key.
        let cursor = arena.begin_ordered_probe(ORDERED_EMPTY_HANDLE, 9).unwrap();
        let miss = arena.probe_ordered_key(cursor, 9).expect("empty probe");
        assert!(!miss.present);
        assert!(miss.key.is_empty());

        // A forged token (poison index) and a cross-schema consume are typed
        // statuses, never panics.
        assert!(OrderedCursorToken::from_words(ORDERED_CURSOR_POISON, 0).is_none());
        let cursor = arena.begin_ordered_probe(root as i64 + 1, 9).unwrap();
        assert_eq!(
            arena.probe_ordered_key(cursor, 8),
            Err(OrderedOpStatus::SchemaMismatch)
        );
    }

    #[test]
    fn probe_ordered_value_exposes_map_values_and_spends_the_cursor() {
        let mut arena = MoltenArena::default();
        let node = arena
            .alloc_ordered_node(9, vec![2], Some(vec![9, 9, 9]), None, None)
            .unwrap();

        // A rooted probe exposes the node's value bytes.
        let cursor = arena.begin_ordered_probe(node as i64 + 1, 9).unwrap();
        assert_eq!(
            arena.probe_ordered_value(cursor, 9),
            Ok((true, vec![9, 9, 9]))
        );

        // The empty collection is a miss with no value.
        let cursor = arena.begin_ordered_probe(ORDERED_EMPTY_HANDLE, 9).unwrap();
        assert_eq!(
            arena.probe_ordered_value(cursor, 9),
            Ok((false, Vec::new()))
        );

        // The value cursor is single-use, like the key cursor.
        let cursor = arena.begin_ordered_probe(node as i64 + 1, 9).unwrap();
        arena
            .probe_ordered_value(cursor, 9)
            .expect("first value probe");
        assert_eq!(
            arena.probe_ordered_value(cursor, 9),
            Err(OrderedOpStatus::Stale)
        );
    }

    fn insert_i64(
        arena: &mut MoltenArena,
        root: i64,
        key: i64,
        value: i64,
        replace: bool,
    ) -> Result<i64, OrderedOpStatus> {
        let cursor = arena.begin_ordered_insert(root, 9)?;
        loop {
            let step = arena.inspect_ordered_insert(cursor, 9)?;
            if !step.present {
                return arena.commit_ordered_insert(
                    cursor,
                    9,
                    key.to_le_bytes().to_vec(),
                    Some(value.to_le_bytes().to_vec()),
                    replace,
                );
            }
            let candidate = i64::from_le_bytes(step.key.try_into().expect("i64 key width"));
            let ordering = match key.cmp(&candidate) {
                core::cmp::Ordering::Less => 0,
                core::cmp::Ordering::Equal => 1,
                core::cmp::Ordering::Greater => 2,
            };
            if arena.advance_ordered_insert(cursor, 9, ordering)? {
                return arena.commit_ordered_insert(
                    cursor,
                    9,
                    key.to_le_bytes().to_vec(),
                    Some(value.to_le_bytes().to_vec()),
                    replace,
                );
            }
        }
    }

    #[test]
    fn ordered_insert_rebuilds_a_persistent_balanced_spine_and_iterates_canonically() {
        let mut arena = MoltenArena::default();
        let mut root = ORDERED_EMPTY_HANDLE;
        let mut first_root = ORDERED_EMPTY_HANDLE;
        for key in 0..4096i64 {
            root = insert_i64(&mut arena, root, key, key * 10, false).expect("distinct insert");
            if key == 0 {
                first_root = root;
            }
        }
        assert_eq!(arena.ordered_collection_len(root, 9), Ok(4096));
        assert_eq!(arena.ordered_collection_len(first_root, 9), Ok(1));
        let root_index = arena.ordered_root(root).unwrap().unwrap();
        assert!(arena.ordered_nodes[root_index].height < 20);

        assert_eq!(
            insert_i64(&mut arena, root, 2048, -1, false),
            Err(OrderedOpStatus::DuplicateKey)
        );
        let replaced = insert_i64(&mut arena, root, 2048, -1, true).expect("replacement");
        assert_eq!(arena.ordered_collection_len(replaced, 9), Ok(4096));
        assert_eq!(arena.ordered_collection_len(root, 9), Ok(4096));

        let cursor = arena.begin_ordered_iterate(replaced, 9).unwrap();
        let mut entries = Vec::new();
        loop {
            let step = arena.iterate_ordered_row(cursor, 9).unwrap();
            if !step.present {
                break;
            }
            let key = i64::from_le_bytes(step.row[..8].try_into().unwrap());
            let value = i64::from_le_bytes(step.row[8..].try_into().unwrap());
            entries.push((key, value));
        }
        assert_eq!(entries.len(), 4096);
        assert!(entries.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert_eq!(entries[2048], (2048, -1));

        let cursor = arena.begin_ordered_insert(root, 9).unwrap();
        arena.inspect_ordered_insert(cursor, 9).unwrap();
        assert_eq!(
            arena.advance_ordered_insert(cursor, 9, 99),
            Err(OrderedOpStatus::InvalidOrdering)
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
