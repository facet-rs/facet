//! Verified task execution owned by Weavy.
//!
//! This is the first verified-execution seam. It consumes a
//! [`VerifiedProgram`], chooses the native task lane inside Weavy, and exposes a
//! task handle whose drive methods cannot be pointed at another program.
//!
//! Legacy raw [`crate::task::Task`] and [`crate::jit::task_lane::JitTask`]
//! entry points remain during migration for existing Vix/Fable consumers. While
//! those raw APIs remain public, the full `machine.execution.verified-admission`
//! rule is not claimed.

use std::mem::size_of;
use std::rc::Rc;
use std::sync::Arc;

use crate::jit::task_lane::{JitExecutable, JitTask};
use crate::task::{
    FnId, HostFn, Op, PublicationLog, Task, TaskEvent, TaskStep, TraceMode, ValueMemories,
};
use crate::{
    CallContractId, CallSiteFacts, DriveRequirements, FrameRegion, FunctionContract, RegionId,
    SchemaRef, ValueShapeKind, ValueShapeRef, VerifiedProgram, WordKind,
};

/// Which lane an [`Executable`] selected for new tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneKind {
    Interpreter,
    Native,
}

/// Why an executable fell back to the interpreter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackReason {
    NativeUnavailable,
    DisabledByEnvironment,
    /// The caller explicitly requested the interpreter through
    /// [`LaneRequest::Interpreter`], independent of the environment. This keeps a
    /// typed per-executable interpreter selection distinguishable from the global
    /// `WEAVY_JIT=0` toggle.
    DisabledByRequest,
}

/// A typed, per-executable lane request. This is the seam that lets one process
/// build a native `Executable` and an interpreter `Executable` side by side —
/// e.g. a cross-lane differential — without touching the global `WEAVY_JIT`
/// environment variable, which would race sibling tests under a parallel runner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LaneRequest {
    /// Weavy's own policy: select the native lane iff it is [`available`] and not
    /// disabled by `WEAVY_JIT=0`; otherwise the interpreter. This is the ordinary
    /// production path.
    ///
    /// [`available`]: crate::jit::task_lane::available
    #[default]
    Auto,
    /// Force the interpreter lane regardless of native availability or
    /// environment. Never compiles the native lane.
    Interpreter,
    /// Force the native lane. Compiles the native lane iff it is [`available`] on
    /// this target, ignoring `WEAVY_JIT=0`. When native is unavailable the
    /// executable still falls back to the interpreter with
    /// [`FallbackReason::NativeUnavailable`], so a caller that requires native
    /// must assert on [`LaneFacts::selected`].
    ///
    /// [`available`]: crate::jit::task_lane::available
    Native,
}

/// Typed lane-selection facts. This is observability, not a selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneFacts {
    pub selected: LaneKind,
    pub native_available: bool,
    pub native_compiled: bool,
    pub fallback: Option<FallbackReason>,
}

/// Drive-time table checked before a verified task enters a lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveTable {
    Ready,
    Awaited,
    Hosts,
}

/// Side of a byte comparison whose dynamic handle was not resident.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompareSide {
    Left,
    Right,
}

/// Declared entry value kind accepted by a typed entry writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryWriteKind {
    Scalar,
    StoreHandle(SchemaRef),
}

/// Public completion state for a verified task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecTaskState {
    NotStarted,
    Parked { input: u32 },
    Yielded,
    Done,
}

/// Nonnegative store-backed value handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoreHandle {
    index: usize,
}

/// One inline ABI value with every nested reference represented as a typed
/// frozen value rather than an integer handle.
#[derive(Clone, Debug)]
pub struct FrozenInlineValue {
    bytes: Vec<u8>,
    references: Vec<(u32, FrozenValue)>,
}

impl FrozenInlineValue {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            references: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_reference(mut self, offset: u32, value: FrozenValue) -> Self {
        self.references.push((offset, value));
        self
    }
}

/// Handle-free execution representation accepted only while initializing a
/// verified task entry. Store references are typed `StoreHandle`s; molten and
/// ordered values are imported into that task's private namespaces.
#[derive(Clone, Debug)]
pub enum FrozenValue {
    Store {
        schema: SchemaRef,
        handle: StoreHandle,
    },
    Opaque {
        schema: SchemaRef,
        bytes: Vec<u8>,
    },
    Dense {
        schema: SchemaRef,
        elements: Vec<FrozenInlineValue>,
    },
    Ordered {
        schema: SchemaRef,
        rows: Vec<(FrozenInlineValue, Option<FrozenInlineValue>)>,
    },
    Inline(FrozenInlineValue),
}

impl StoreHandle {
    #[must_use]
    pub fn new(index: usize) -> Option<Self> {
        i64::try_from(index).ok()?;
        Some(Self { index })
    }

    #[must_use]
    pub fn index(self) -> usize {
        self.index
    }

    /// The frame-ABI word for this verified store handle.
    #[must_use]
    pub fn as_i64(self) -> i64 {
        i64::try_from(self.index).expect("StoreHandle constructor checked i64 range")
    }
}

/// One dynamic fault location in a verified task program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaultSite {
    pub function: FnId,
    pub pc: usize,
    pub op: Box<Op>,
    pub call: Option<CallSiteFacts>,
}

/// A contract-checked read-only view of a structural task result.
pub struct StructuralResult<'a> {
    bytes: &'a [u8],
    entry: FnId,
    region: RegionId,
    value_shape: ValueShapeRef,
    shape: &'a crate::ValueShapeContract,
    value_shapes: &'a [crate::ValueShapeContract],
}

/// One verifier-described structural value borrowed from a completed task.
/// Its representation bytes and offsets remain private; callers can only walk
/// the verified scalar, handle, product, and active-enum views.
#[derive(Clone, Copy)]
pub struct TaskStructuralValue<'task> {
    bytes: &'task [u8],
    entry: FnId,
    region: RegionId,
    shape: &'task crate::RegionShape,
    value_shape: Option<&'task crate::ValueShapeContract>,
    value_shapes: &'task [crate::ValueShapeContract],
}

impl<'task> TaskStructuralValue<'task> {
    pub fn scalar_word(self) -> Result<i64, TaskFault> {
        let [word] = self.shape.words.as_slice() else {
            return Err(self.invalid());
        };
        if !word.is_exactly(WordKind::Scalar) {
            return Err(self.invalid());
        }
        self.word(0)
    }

    pub fn value_ref(self) -> Result<TaskValueRef<'task>, TaskFault> {
        let [word] = self.shape.words.as_slice() else {
            return Err(self.invalid());
        };
        let [WordKind::Handle(schema)] = word.as_slice() else {
            return Err(self.invalid());
        };
        Ok(TaskValueRef {
            word: self.word(0)?,
            schema: *schema,
            lifetime: core::marker::PhantomData,
        })
    }

    pub fn product_field(self, field: u32) -> Result<Self, TaskFault> {
        let Some(shape) = self.value_shape else {
            return Err(self.invalid());
        };
        let ValueShapeKind::Product { fields } = &shape.kind else {
            return Err(self.invalid());
        };
        self.field(fields.get(field as usize).ok_or_else(|| self.invalid())?)
    }

    pub fn enum_selector(self) -> Result<u32, TaskFault> {
        let Some(shape) = self.value_shape else {
            return Err(self.invalid());
        };
        let ValueShapeKind::Enum { selector, variants } = &shape.kind else {
            return Err(self.invalid());
        };
        let actual = self.word(selector.offset)?;
        let actual = usize::try_from(actual).map_err(|_| self.invalid())?;
        if actual >= variants.len() {
            return Err(self.invalid());
        }
        u32::try_from(actual).map_err(|_| self.invalid())
    }

    pub fn enum_field(self, variant: u32, field: u32) -> Result<Self, TaskFault> {
        let Some(shape) = self.value_shape else {
            return Err(self.invalid());
        };
        let ValueShapeKind::Enum { variants, .. } = &shape.kind else {
            return Err(self.invalid());
        };
        let field = variants
            .get(variant as usize)
            .and_then(|variant| variant.fields.get(field as usize))
            .ok_or_else(|| self.invalid())?;
        self.field(field)
    }

    fn field(self, field: &'task crate::ValueFieldUse) -> Result<Self, TaskFault> {
        let start = field.offset as usize;
        let len = field
            .shape
            .checked_byte_len()
            .ok_or_else(|| self.invalid())?;
        let end = start.checked_add(len).ok_or_else(|| self.invalid())?;
        let bytes = self.bytes.get(start..end).ok_or_else(|| self.invalid())?;
        let value_shape = field
            .value_shape
            .map(|shape| self.contract_shape(shape).ok_or_else(|| self.invalid()))
            .transpose()?;
        Ok(Self {
            bytes,
            entry: self.entry,
            region: self.region,
            shape: &field.shape,
            value_shape,
            value_shapes: self.value_shapes,
        })
    }

    fn contract_shape(self, shape: ValueShapeRef) -> Option<&'task crate::ValueShapeContract> {
        self.value_shapes.get(shape.0 as usize)
    }

    fn word(self, offset: u32) -> Result<i64, TaskFault> {
        let start = offset as usize;
        let end = start
            .checked_add(size_of::<i64>())
            .ok_or_else(|| self.invalid())?;
        let bytes = self.bytes.get(start..end).ok_or_else(|| self.invalid())?;
        Ok(i64::from_le_bytes(bytes.try_into().expect("checked word")))
    }

    fn invalid(self) -> TaskFault {
        TaskFault::InvalidResultShape {
            entry: self.entry,
            region: self.region,
            size: self.bytes.len(),
        }
    }
}

impl StructuralResult<'_> {
    #[must_use]
    pub fn as_value(&self) -> TaskStructuralValue<'_> {
        TaskStructuralValue {
            bytes: self.bytes,
            entry: self.entry,
            region: self.region,
            shape: &self.shape.shape,
            value_shape: Some(self.shape),
            value_shapes: self.value_shapes,
        }
    }

    pub fn enum_selector(&self) -> Result<i64, TaskFault> {
        let ValueShapeKind::Enum { selector, variants } = &self.shape.kind else {
            return Err(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            });
        };
        let actual = self.word(selector.offset)?;
        if actual < 0 || actual as usize >= variants.len() {
            return Err(TaskFault::InvalidResultSelector {
                entry: self.entry,
                region: self.region,
                value_shape: self.value_shape,
                actual,
                variant_count: variants.len(),
            });
        }
        Ok(actual)
    }

    pub fn enum_scalar_field(&self, variant: u32, field: u32) -> Result<i64, TaskFault> {
        let ValueShapeKind::Enum { variants, .. } = &self.shape.kind else {
            return Err(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            });
        };
        let field = variants
            .get(variant as usize)
            .and_then(|variant| variant.fields.get(field as usize))
            .ok_or(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            })?;
        if field.shape.words.len() != 1 || !field.shape.words[0].is_exactly(WordKind::Scalar) {
            return Err(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            });
        }
        self.word(field.offset)
    }

    /// Read a handle-shaped field as an opaque reference tied to the completed
    /// task borrow. The raw machine word is never exposed.
    pub fn enum_value_field(
        &self,
        variant: u32,
        field: u32,
    ) -> Result<TaskValueRef<'_>, TaskFault> {
        let ValueShapeKind::Enum { variants, .. } = &self.shape.kind else {
            return Err(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            });
        };
        let field = variants
            .get(variant as usize)
            .and_then(|variant| variant.fields.get(field as usize))
            .ok_or(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            })?;
        let [word] = field.shape.words.as_slice() else {
            return Err(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            });
        };
        if !matches!(word.as_slice(), [WordKind::Handle(_)]) {
            return Err(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            });
        }
        Ok(TaskValueRef {
            word: self.word(field.offset)?,
            schema: match word.as_slice() {
                [WordKind::Handle(schema)] => *schema,
                _ => unreachable!("checked handle field"),
            },
            lifetime: core::marker::PhantomData,
        })
    }

    fn word(&self, offset: u32) -> Result<i64, TaskFault> {
        let start = offset as usize;
        let end = start
            .checked_add(size_of::<i64>())
            .ok_or(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            })?;
        let bytes = self
            .bytes
            .get(start..end)
            .ok_or(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: self.region,
                size: self.bytes.len(),
            })?;
        Ok(i64::from_le_bytes(
            bytes.try_into().expect("checked structural result word"),
        ))
    }
}

/// Opaque task-result reference. Its machine word is private and its lifetime
/// is minted only inside [`ExecTask::with_result_resolver`].
#[derive(Clone, Copy)]
pub struct TaskValueRef<'task> {
    word: i64,
    schema: SchemaRef,
    lifetime: core::marker::PhantomData<&'task mut &'task ()>,
}

/// A resolved value reference. Store indices occupy their typed nonnegative
/// namespace; task-local molten bytes remain borrowed from the task; externally
/// lent molten references retain a distinct typed namespace.
pub enum ResolvedTaskValue<'task> {
    Store(StoreHandle),
    TaskMolten(&'task [u8]),
    LentMolten { index: usize },
}

pub struct ResolvedOrderedRow<'task> {
    key: TaskStructuralValue<'task>,
    value: Option<TaskStructuralValue<'task>>,
}

impl<'task> ResolvedOrderedRow<'task> {
    #[must_use]
    pub fn key(&self) -> TaskStructuralValue<'task> {
        self.key
    }

    #[must_use]
    pub fn value(&self) -> Option<TaskStructuralValue<'task>> {
        self.value
    }
}

pub struct ResolvedOrderedCollection<'task> {
    rows: Vec<ResolvedOrderedRow<'task>>,
}

pub struct ResolvedDenseArray<'task> {
    elements: Vec<TaskStructuralValue<'task>>,
}

impl<'task> ResolvedDenseArray<'task> {
    #[must_use]
    pub fn elements(&self) -> &[TaskStructuralValue<'task>] {
        &self.elements
    }
}

impl<'task> ResolvedOrderedCollection<'task> {
    #[must_use]
    pub fn rows(&self) -> &[ResolvedOrderedRow<'task>] {
        &self.rows
    }
}

/// Borrow-scoped resolver for opaque result references. It has no constructor
/// and cannot outlive the closure passed to `with_result_resolver`.
pub struct TaskValueResolver<'task> {
    molten: &'task crate::task::MoltenArena,
    contract: &'task crate::ProgramContract,
}

impl<'task> TaskValueResolver<'task> {
    pub fn resolve(&self, value: TaskValueRef<'task>) -> Option<ResolvedTaskValue<'task>> {
        match self.molten.resolve_handle(value.word)? {
            crate::task::ResolvedHandle::Store(index) => {
                Some(ResolvedTaskValue::Store(StoreHandle::new(index)?))
            }
            crate::task::ResolvedHandle::TaskMolten(bytes) => {
                Some(ResolvedTaskValue::TaskMolten(bytes))
            }
            crate::task::ResolvedHandle::LentMolten(index) => {
                Some(ResolvedTaskValue::LentMolten { index })
            }
        }
    }

    /// Resolve one nested handle word without exposing that word. The caller
    /// supplies the typed payload offset; semantic identity is still rebuilt
    /// from element position, schema, and referent identity.
    pub fn resolve_nested(
        &self,
        bytes: &'task [u8],
        offset: usize,
    ) -> Option<ResolvedTaskValue<'task>> {
        let word = i64::from_le_bytes(bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?);
        self.resolve(TaskValueRef {
            word,
            schema: SchemaRef(u32::MAX),
            lifetime: core::marker::PhantomData,
        })
    }

    pub fn resolve_ordered(
        &self,
        value: TaskValueRef<'task>,
    ) -> Result<ResolvedOrderedCollection<'task>, TaskFault> {
        let collection = self
            .contract
            .schemas
            .get(value.schema.0 as usize)
            .ok_or_else(|| self.invalid_ordered())?;
        let crate::PayloadKind::OrderedCollection(ordered) = &collection.payload else {
            return Err(self.invalid_ordered());
        };
        let key = self
            .contract
            .schemas
            .get(ordered.key.0 as usize)
            .ok_or_else(|| self.invalid_ordered())?;
        let value_contract = ordered
            .value
            .map(|schema| {
                self.contract
                    .schemas
                    .get(schema.0 as usize)
                    .ok_or_else(|| self.invalid_ordered())
            })
            .transpose()?;
        let rows = self
            .molten
            .ordered_rows(value.word, i64::from(value.schema.0))
            .map_err(|_| self.invalid_ordered())?;
        rows.into_iter()
            .map(|row| {
                let key_value = self.structural(row.key, &key.inline, key.value_shape)?;
                let value = match (row.value, value_contract) {
                    (Some(bytes), Some(contract)) => {
                        Some(self.structural(bytes, &contract.inline, contract.value_shape)?)
                    }
                    (None, None) => None,
                    _ => return Err(self.invalid_ordered()),
                };
                Ok(ResolvedOrderedRow {
                    key: key_value,
                    value,
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|rows| ResolvedOrderedCollection { rows })
    }

    pub fn resolve_dense(
        &self,
        value: TaskValueRef<'task>,
    ) -> Result<ResolvedDenseArray<'task>, TaskFault> {
        let collection = self
            .contract
            .schemas
            .get(value.schema.0 as usize)
            .ok_or_else(|| self.invalid_ordered())?;
        let crate::PayloadKind::DenseArray { element } = collection.payload else {
            return Err(self.invalid_ordered());
        };
        let element_contract = self
            .contract
            .schemas
            .get(element.0 as usize)
            .ok_or_else(|| self.invalid_ordered())?;
        let width = element_contract
            .inline
            .checked_byte_len()
            .ok_or_else(|| self.invalid_ordered())?;
        let elements = self
            .molten
            .dense_elements(value.word, i64::from(element.0), width)
            .map_err(|_| self.invalid_ordered())?
            .into_iter()
            .map(|bytes| {
                self.structural(
                    bytes,
                    &element_contract.inline,
                    element_contract.value_shape,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ResolvedDenseArray { elements })
    }

    /// Resolve a verified host-plan word as a dense value. The caller supplies
    /// the program-local schema witness from its verified ABI plan; the raw
    /// task handle never becomes a semantic identity or escapes this borrow.
    pub fn resolve_dense_host_word(
        &self,
        word: i64,
        schema: SchemaRef,
    ) -> Result<ResolvedDenseArray<'task>, TaskFault> {
        self.resolve_dense(TaskValueRef {
            word,
            schema,
            lifetime: core::marker::PhantomData,
        })
    }

    /// Resolve one verified host-plan reference word under its program-local
    /// schema witness without exposing task arena internals.
    pub fn resolve_host_word(
        &self,
        word: i64,
        schema: SchemaRef,
    ) -> Option<ResolvedTaskValue<'task>> {
        self.resolve(TaskValueRef {
            word,
            schema,
            lifetime: core::marker::PhantomData,
        })
    }

    fn structural(
        &self,
        bytes: &'task [u8],
        shape: &'task crate::RegionShape,
        value_shape: Option<ValueShapeRef>,
    ) -> Result<TaskStructuralValue<'task>, TaskFault> {
        if shape.checked_byte_len() != Some(bytes.len()) {
            return Err(self.invalid_ordered());
        }
        let value_shape = value_shape
            .map(|shape| {
                self.contract
                    .value_shapes
                    .get(shape.0 as usize)
                    .ok_or_else(|| self.invalid_ordered())
            })
            .transpose()?;
        Ok(TaskStructuralValue {
            bytes,
            entry: FnId(0),
            region: RegionId(0),
            shape,
            value_shape,
            value_shapes: &self.contract.value_shapes,
        })
    }

    fn invalid_ordered(&self) -> TaskFault {
        TaskFault::InvalidResultShape {
            entry: FnId(0),
            region: RegionId(0),
            size: 0,
        }
    }

    #[must_use]
    pub fn molten_stats(&self) -> (usize, usize) {
        self.molten.stats()
    }
}

/// One descriptor in a completed task's append-only publication log.
///
/// The view is read-only and lives only as long as the borrow of the task. Its
/// bytes are an owned copy the task made when the publish op ran, so nothing
/// here aliases the frame, the molten arena, or any lent value memory. The
/// record type is a verifier-owned publication-record schema; the provenance
/// key is opaque front-end identity the machine never interprets.
pub struct PublishedDescriptor<'a> {
    site: u64,
    schema: SchemaRef,
    value_shape: Option<ValueShapeRef>,
    bytes: &'a [u8],
}

impl<'a> PublishedDescriptor<'a> {
    /// The opaque, front-end-assigned provenance key the publish op carried.
    #[must_use]
    pub fn provenance_key(&self) -> u64 {
        self.site
    }

    /// The publication-record schema that types this descriptor's bytes.
    #[must_use]
    pub fn record_schema(&self) -> SchemaRef {
        self.schema
    }

    /// The structural value shape of the captured frame value, if the record
    /// schema declared one.
    #[must_use]
    pub fn value_shape(&self) -> Option<ValueShapeRef> {
        self.value_shape
    }

    /// The exact captured bytes, copied by value at publish time.
    #[must_use]
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// One captured scalar word, read little-endian at `offset` within the
    /// record. The record schema admits only scalar words, so any in-range,
    /// word-sized read names a captured scalar; an out-of-range read returns
    /// `None`.
    #[must_use]
    pub fn word(&self, offset: u32) -> Option<i64> {
        let start = usize::try_from(offset).ok()?;
        let end = start.checked_add(size_of::<i64>())?;
        let bytes = self.bytes.get(start..end)?;
        Some(i64::from_le_bytes(
            bytes.try_into().expect("checked publication word"),
        ))
    }
}

/// Dynamic task fault. Static program invalidity remains [`crate::ProgramError`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskFault {
    InvalidEntryFunction {
        entry: FnId,
        function_count: usize,
    },
    InvalidEntryShape {
        entry: FnId,
        index: usize,
        region: RegionId,
    },
    InvalidEntryIndex {
        entry: FnId,
        index: usize,
        entry_count: usize,
    },
    EntryKindMismatch {
        entry: FnId,
        index: usize,
        region: RegionId,
        expected: EntryWriteKind,
        actual: WordKind,
    },
    EntryMissing {
        entry: FnId,
        index: usize,
        region: RegionId,
        kind: WordKind,
    },
    EntryAlreadyInitialized {
        entry: FnId,
        index: usize,
        region: RegionId,
    },
    EntryWriteAfterDrive {
        entry: FnId,
        index: usize,
        region: RegionId,
    },
    EntryValueSize {
        entry: FnId,
        index: usize,
        region: RegionId,
        value_shape: ValueShapeRef,
        expected: usize,
        actual: usize,
    },
    InvalidResultShape {
        entry: FnId,
        region: RegionId,
        size: usize,
    },
    InvalidResultSelector {
        entry: FnId,
        region: RegionId,
        value_shape: ValueShapeRef,
        actual: i64,
        variant_count: usize,
    },
    DriveTableLength {
        table: DriveTable,
        expected: usize,
        actual: usize,
    },
    IndirectCalleeNegative {
        site: FaultSite,
        value: i64,
    },
    IndirectCalleeOutOfRange {
        site: FaultSite,
        callee: i64,
        function_count: usize,
    },
    IndirectCalleeContractMismatch {
        site: FaultSite,
        callee: FnId,
        expected: CallContractId,
        actual: Option<CallContractId>,
    },
    MissingIndirectCallFacts {
        site: FaultSite,
    },
    UnresidentCompareValueBytes {
        site: FaultSite,
        side: CompareSide,
        handle: i64,
    },
    UnresidentStringConcatOperand {
        site: FaultSite,
        side: CompareSide,
        handle: i64,
    },
    StringConcatAllocationFailed {
        site: FaultSite,
    },
    UnresidentByteProjectSource {
        site: FaultSite,
        handle: i64,
    },
    ByteProjectionAllocationFailed {
        site: FaultSite,
    },
    IntToStringAllocationFailed {
        site: FaultSite,
    },
    UnresidentPathJoinOperand {
        site: FaultSite,
        side: CompareSide,
        handle: i64,
    },
    PathJoinAllocationFailed {
        site: FaultSite,
    },
    PublicationAllocationFailed {
        site: FaultSite,
    },
    InvalidEnumSelector {
        site: FaultSite,
        value_shape: ValueShapeRef,
        expected: Vec<i64>,
        actual: i64,
    },
    EnumProjectionMismatch {
        site: FaultSite,
        value_shape: ValueShapeRef,
        expected: i64,
        actual: i64,
    },
    InvalidArrayStatus {
        site: FaultSite,
        actual: i64,
    },
    InvalidStringStatus {
        site: FaultSite,
        actual: i64,
    },
    InvalidOrderedStatus {
        site: FaultSite,
        actual: i64,
    },
    NativeFaultExit {
        function: FnId,
        code: i64,
    },
    InvalidFaultSite {
        function: FnId,
        pc: usize,
        function_count: usize,
        op_count: Option<usize>,
    },
    PoisonedReDrive {
        original: Box<TaskFault>,
    },
    PoisonedResult {
        original: Box<TaskFault>,
    },
    ResultBeforeDone {
        state: ExecTaskState,
    },
    DriveAfterDone,
    PublicationIndexOutOfRange {
        index: usize,
        count: usize,
    },
    /// A boxed capture environment access faulted: the handle named no resident
    /// environment box in this task, was minted under another task generation,
    /// or the requested capture exceeded the box.
    Environment {
        site: FaultSite,
        kind: EnvironmentFaultKind,
        handle: i64,
    },
}

/// The closed set of boxed-environment access faults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvironmentFaultKind {
    Unresident,
    Stale,
    OutOfRange,
    AllocationFailed,
}

/// A verified program prepared for execution.
pub struct Executable {
    verified: Arc<VerifiedProgram>,
    native: Option<JitExecutable>,
    lane_facts: LaneFacts,
    mode: TraceMode,
}

impl Executable {
    #[must_use]
    pub fn new(verified: VerifiedProgram) -> Self {
        Self::with_trace_mode(verified, TraceMode::Innards)
    }

    /// Prepare with Weavy's own [`LaneRequest::Auto`] lane policy: native iff
    /// available and not `WEAVY_JIT=0`. This is the production entry point.
    #[must_use]
    pub fn with_trace_mode(verified: VerifiedProgram, mode: TraceMode) -> Self {
        Self::with_lane(verified, mode, LaneRequest::Auto)
    }

    /// Prepare with an explicit, typed per-executable [`LaneRequest`]. This is the
    /// non-environment lane seam: `Native`/`Interpreter` force a lane in-process
    /// without mutating `WEAVY_JIT`, so two lanes can be materialized at once.
    #[must_use]
    pub fn with_lane(verified: VerifiedProgram, mode: TraceMode, request: LaneRequest) -> Self {
        let verified = Arc::new(verified);
        let native_available = crate::jit::task_lane::available();
        // Whether this request permits compiling the native lane. Only `Auto`
        // consults the environment; the explicit requests are authoritative.
        let want_native = match request {
            LaneRequest::Auto => !native_disabled_by_environment(),
            LaneRequest::Interpreter => false,
            LaneRequest::Native => true,
        };
        let native = if native_available && want_native {
            JitExecutable::compile(Arc::clone(&verified), mode)
        } else {
            None
        };
        let native_compiled = native.is_some();
        let fallback = if native_compiled {
            None
        } else if !want_native {
            // The interpreter was chosen deliberately: an explicit request, or
            // `Auto` seeing `WEAVY_JIT=0`.
            match request {
                LaneRequest::Interpreter => Some(FallbackReason::DisabledByRequest),
                LaneRequest::Auto | LaneRequest::Native => {
                    Some(FallbackReason::DisabledByEnvironment)
                }
            }
        } else {
            // Native was wanted but the target has no native lane (or it declined
            // to compile this program).
            Some(FallbackReason::NativeUnavailable)
        };
        let selected = if native_compiled {
            LaneKind::Native
        } else {
            LaneKind::Interpreter
        };
        Self {
            verified,
            native,
            lane_facts: LaneFacts {
                selected,
                native_available,
                native_compiled,
                fallback,
            },
            mode,
        }
    }

    #[must_use]
    pub fn program(&self) -> &VerifiedProgram {
        &self.verified
    }

    #[must_use]
    pub fn lane_facts(&self) -> LaneFacts {
        self.lane_facts
    }

    /// Spawn a task that OWNS a reference-counted handle to this executable. The
    /// returned [`ExecTask`] is `'static`: it can be retained across drive calls
    /// and stored off the drive stack (a scheduler's parked-task registry) while
    /// its verified frame is suspended, without borrowing any transient lowered
    /// value. The inner interpreter/JIT task holds no program borrow — the
    /// program is supplied to each `run` — so this handle is the only executable
    /// reference the task keeps.
    pub fn spawn(self: &Rc<Self>, entry: FnId) -> Result<ExecTask, TaskFault> {
        self.validate_entry(entry)?;
        let entry_count = self.function(entry)?.entries.len();
        let lane = match &self.native {
            Some(native) => Lane::Native(JitTask::spawn_verified(native, entry)),
            None => Lane::Interpreter(Task::spawn_with_mode(
                self.verified.program(),
                entry,
                self.mode,
            )),
        };
        Ok(ExecTask {
            executable: Rc::clone(self),
            entry,
            lane,
            poisoned: None,
            entries_initialized: vec![false; entry_count],
            entries_closed: false,
            state: ExecTaskState::NotStarted,
        })
    }

    fn validate_entry(&self, entry: FnId) -> Result<(), TaskFault> {
        let function = self.function(entry)?;
        for (index, region) in function.entries.iter().copied().enumerate() {
            let region_contract = &function.frame.regions[region.0 as usize];
            if region_contract.value_shape.is_none() && entry_word_kind(region_contract).is_none() {
                return Err(TaskFault::InvalidEntryShape {
                    entry,
                    index,
                    region,
                });
            }
        }
        Ok(())
    }

    fn validate_result_i64(&self, entry: FnId) -> Result<(), TaskFault> {
        let function = self.function(entry)?;
        let region = function.result;
        let region_contract = &function.frame.regions[region.0 as usize];
        if region_contract.shape.words.len() != 1
            || !region_contract.shape.words[0].is_exactly(WordKind::Scalar)
        {
            return Err(TaskFault::InvalidResultShape {
                entry,
                region,
                size: region_contract
                    .shape
                    .checked_byte_len()
                    .unwrap_or(usize::MAX),
            });
        }
        Ok(())
    }

    fn function(&self, function: FnId) -> Result<&FunctionContract, TaskFault> {
        self.verified
            .contract()
            .functions
            .get(function.0 as usize)
            .ok_or(TaskFault::InvalidEntryFunction {
                entry: function,
                function_count: self.verified.contract().functions.len(),
            })
    }
}

/// A running verified task that owns a reference-counted handle to its
/// [`Executable`]. Owning the handle (rather than borrowing it) makes the task
/// `'static`, so a scheduler may retain a suspended frame off the drive stack
/// and resume it later without any lifetime tied to a transient lowered value.
pub struct ExecTask {
    executable: Rc<Executable>,
    entry: FnId,
    lane: Lane,
    poisoned: Option<TaskFault>,
    entries_initialized: Vec<bool>,
    entries_closed: bool,
    state: ExecTaskState,
}

enum Lane {
    Interpreter(Task),
    Native(JitTask),
}

impl ExecTask {
    pub fn write_entry_i64(&mut self, index: usize, value: i64) -> Result<(), TaskFault> {
        self.write_entry_word(index, value, EntryWriteKind::Scalar)
    }

    pub fn write_entry_store_handle(
        &mut self,
        index: usize,
        schema: SchemaRef,
        handle: StoreHandle,
    ) -> Result<(), TaskFault> {
        self.write_entry_word(index, handle.as_i64(), EntryWriteKind::StoreHandle(schema))
    }

    pub fn write_entry_frozen(
        &mut self,
        index: usize,
        value: &FrozenValue,
    ) -> Result<(), TaskFault> {
        self.check_not_poisoned()?;
        let function = self.executable.function(self.entry)?;
        let Some(region) = function.entries.get(index).copied() else {
            return Err(TaskFault::InvalidEntryIndex {
                entry: self.entry,
                index,
                entry_count: function.entries.len(),
            });
        };
        let declared = function.frame.regions[region.0 as usize].clone();
        if self.entries_closed {
            return Err(TaskFault::EntryWriteAfterDrive {
                entry: self.entry,
                index,
                region,
            });
        }
        if self.entries_initialized[index] {
            return Err(TaskFault::EntryAlreadyInitialized {
                entry: self.entry,
                index,
                region,
            });
        }
        let contract = self.executable.verified.contract().clone();
        let bytes = match &mut self.lane {
            Lane::Interpreter(task) => {
                materialize_frozen(value, &declared.shape, &contract, task.molten_mut())
            }
            Lane::Native(task) => {
                materialize_frozen(value, &declared.shape, &contract, task.molten_mut())
            }
        }
        .map_err(|()| TaskFault::InvalidEntryShape {
            entry: self.entry,
            index,
            region,
        })?;
        match &mut self.lane {
            Lane::Interpreter(task) => task.write_bytes(declared.offset, &bytes),
            Lane::Native(task) => task.write_bytes(declared.offset, &bytes),
        }
        self.entries_initialized[index] = true;
        Ok(())
    }

    /// Materialize one scheduler-owned host result into the active frame after
    /// a synchronous [`crate::task::Op::HostCallYield`] has returned control.
    /// The caller owns the typed ABI plan; this boundary only validates word
    /// alignment and the verified frame extent before writing the word into the
    /// suspended task on either execution lane.
    pub fn write_host_word(&mut self, offset: u32, value: i64) -> Result<(), TaskFault> {
        self.check_not_poisoned()?;
        let active = match &self.lane {
            Lane::Interpreter(task) => task.active_function(),
            Lane::Native(task) => task.active_function(),
        };
        let function = self.executable.function(active)?;
        let frame = &self.executable.program().program().fns[active.0 as usize].frame;
        let offset = usize::try_from(offset).map_err(|_| TaskFault::InvalidResultShape {
            entry: active,
            region: function.result,
            size: 0,
        })?;
        if offset % size_of::<i64>() != 0
            || offset
                .checked_add(size_of::<i64>())
                .is_none_or(|end| end > frame.size)
        {
            return Err(TaskFault::InvalidResultShape {
                entry: active,
                region: function.result,
                size: offset,
            });
        }
        match &mut self.lane {
            Lane::Interpreter(task) => task.write_i64(offset as u32, value),
            Lane::Native(task) => task.write_i64(offset as u32, value),
        }
        Ok(())
    }

    /// Materialize a dense array of fixed-width, inline-encoded `elements` into
    /// the suspended task's molten arena and return the handle word to write into
    /// the result frame. This is the write counterpart to
    /// [`TaskValueResolver::resolve_dense`]: a scheduler-owned host result (e.g. a
    /// primitive returning a `[T]` field) can materialize an aggregate the
    /// resuming task reads back through the ordinary dense-array path. The element
    /// schema and width are derived from `array_schema`'s verified
    /// [`crate::PayloadKind::DenseArray`] contract — the same derivation the
    /// reader uses — so the two never disagree; `import_dense` re-checks that every
    /// element is exactly the declared width.
    pub fn import_dense_host_array(
        &mut self,
        array_schema: SchemaRef,
        elements: &[Vec<u8>],
    ) -> Result<i64, TaskFault> {
        self.check_not_poisoned()?;
        let active = match &self.lane {
            Lane::Interpreter(task) => task.active_function(),
            Lane::Native(task) => task.active_function(),
        };
        let region = self.executable.function(active)?.result;
        let invalid = |size: usize| TaskFault::InvalidResultShape {
            entry: active,
            region,
            size,
        };
        let (element_schema, width) = {
            let contract = self.executable.verified.contract();
            let collection = contract
                .schemas
                .get(array_schema.0 as usize)
                .ok_or_else(|| invalid(0))?;
            let crate::PayloadKind::DenseArray { element } = collection.payload else {
                return Err(invalid(0));
            };
            let element_contract = contract
                .schemas
                .get(element.0 as usize)
                .ok_or_else(|| invalid(0))?;
            let width = element_contract
                .inline
                .checked_byte_len()
                .ok_or_else(|| invalid(0))?;
            (i64::from(element.0), width)
        };
        let molten = match &mut self.lane {
            Lane::Interpreter(task) => task.molten_mut(),
            Lane::Native(task) => task.molten_mut(),
        };
        molten
            .import_dense(element_schema, width, elements)
            .map_err(|_| invalid(elements.len()))
    }

    fn write_entry_word(
        &mut self,
        index: usize,
        value: i64,
        expected: EntryWriteKind,
    ) -> Result<(), TaskFault> {
        self.check_not_poisoned()?;
        let entry = self.entry_info(index)?;
        if self.entries_closed {
            return Err(TaskFault::EntryWriteAfterDrive {
                entry: self.entry,
                index,
                region: entry.region,
            });
        }
        if self.entries_initialized[index] {
            return Err(TaskFault::EntryAlreadyInitialized {
                entry: self.entry,
                index,
                region: entry.region,
            });
        }
        if !entry_write_matches(expected, entry.kind) {
            return Err(TaskFault::EntryKindMismatch {
                entry: self.entry,
                index,
                region: entry.region,
                expected,
                actual: entry.kind,
            });
        }
        match &mut self.lane {
            Lane::Interpreter(task) => task.write_i64(entry.offset, value),
            Lane::Native(task) => task.write_i64(entry.offset, value),
        }
        self.entries_initialized[index] = true;
        Ok(())
    }

    pub fn drive(&mut self, ready: &mut [bool], awaited: &[i64]) -> Result<TaskStep, TaskFault> {
        self.drive_hosted_with_value_memories(ready, awaited, &mut [], ValueMemories::empty())
    }

    pub fn drive_hosted(
        &mut self,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
    ) -> Result<TaskStep, TaskFault> {
        self.drive_hosted_with_value_memories(ready, awaited, hosts, ValueMemories::empty())
    }

    pub fn drive_hosted_with_value_memories(
        &mut self,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> Result<TaskStep, TaskFault> {
        self.check_not_poisoned()?;
        if self.state == ExecTaskState::Done {
            return Err(TaskFault::DriveAfterDone);
        }
        self.entries_closed = true;
        check_drive_requirements(
            self.executable.verified.drive_requirements(),
            ready,
            awaited,
            hosts,
        )
        .map_err(|fault| self.poison(fault))?;
        self.check_entries_initialized()
            .map_err(|fault| self.poison(fault))?;

        let step = match (&self.executable.native, &mut self.lane) {
            (_, Lane::Interpreter(task)) => task.run_verified_with_value_memories(
                &self.executable.verified,
                ready,
                awaited,
                hosts,
                value_memories,
            ),
            (Some(native), Lane::Native(task)) => {
                task.run_verified_with_value_memories(native, ready, awaited, hosts, value_memories)
            }
            (None, Lane::Native(_)) => unreachable!("native task exists only with native program"),
        };
        match step {
            Ok(step) => {
                self.state = match step {
                    TaskStep::Done => ExecTaskState::Done,
                    TaskStep::Parked { input } => ExecTaskState::Parked { input },
                    TaskStep::Yielded => ExecTaskState::Yielded,
                };
                Ok(step)
            }
            Err(fault) => Err(self.poison(fault)),
        }
    }

    #[must_use]
    pub fn state(&self) -> ExecTaskState {
        self.state
    }

    #[must_use]
    pub fn trace(&self) -> &[TaskEvent] {
        match &self.lane {
            Lane::Interpreter(task) => &task.trace,
            Lane::Native(task) => &task.trace,
        }
    }

    #[must_use]
    pub fn frame_arena_bytes(&self) -> usize {
        match &self.lane {
            Lane::Interpreter(task) => task.frame_arena_bytes(),
            Lane::Native(task) => task.frame_arena_bytes(),
        }
    }

    pub(crate) fn result(&self) -> Result<&[u8], TaskFault> {
        self.check_result_available()?;
        Ok(match &self.lane {
            Lane::Interpreter(task) => &task.result,
            Lane::Native(task) => &task.result,
        })
    }

    pub fn result_i64(&self) -> Result<i64, TaskFault> {
        self.check_result_available()?;
        self.executable.validate_result_i64(self.entry)?;
        let result = self.result()?;
        if result.len() != size_of::<i64>() {
            let function = self.executable.function(self.entry)?;
            return Err(TaskFault::InvalidResultShape {
                entry: self.entry,
                region: function.result,
                size: result.len(),
            });
        }
        Ok(i64::from_le_bytes(
            result.try_into().expect("result length checked"),
        ))
    }

    pub fn result_structural(&self) -> Result<StructuralResult<'_>, TaskFault> {
        self.check_result_available()?;
        let function = self.executable.function(self.entry)?;
        let region = function.result;
        let declared = &function.frame.regions[region.0 as usize];
        let bytes = self.result()?;
        let value_shape = declared.value_shape.ok_or(TaskFault::InvalidResultShape {
            entry: self.entry,
            region,
            size: bytes.len(),
        })?;
        let expected = declared
            .shape
            .checked_byte_len()
            .ok_or(TaskFault::InvalidResultShape {
                entry: self.entry,
                region,
                size: bytes.len(),
            })?;
        if bytes.len() != expected {
            return Err(TaskFault::InvalidResultShape {
                entry: self.entry,
                region,
                size: bytes.len(),
            });
        }
        let shape = self
            .executable
            .verified
            .contract()
            .value_shapes
            .get(value_shape.0 as usize)
            .ok_or(TaskFault::InvalidResultShape {
                entry: self.entry,
                region,
                size: bytes.len(),
            })?;
        Ok(StructuralResult {
            bytes,
            entry: self.entry,
            region,
            value_shape,
            shape,
            value_shapes: &self.executable.verified.contract().value_shapes,
        })
    }

    /// Inspect a completed structural result together with an opaque resolver
    /// borrowed from the same lane-owned molten arena. The higher-ranked
    /// closure prevents the resolver and opaque references from escaping this
    /// borrow; interpreter and native lanes expose the identical contract.
    pub fn with_result_resolver<R>(
        &self,
        use_result: impl for<'task> FnOnce(
            StructuralResult<'task>,
            TaskValueResolver<'task>,
        ) -> Result<R, TaskFault>,
    ) -> Result<R, TaskFault> {
        let result = self.result_structural()?;
        let molten = match &self.lane {
            Lane::Interpreter(task) => task.molten(),
            Lane::Native(task) => task.molten(),
        };
        use_result(
            result,
            TaskValueResolver {
                molten,
                contract: self.executable.verified.contract(),
            },
        )
    }

    /// Inspect a completed result through its verified frame-region shape,
    /// whether or not that result also has a structural product/enum shape.
    /// This is the result-side counterpart of [`Self::with_value_resolver`]:
    /// scalar and handle-only results remain typed without fabricating an enum
    /// envelope solely to make them inspectable.
    pub fn with_result_value_resolver<R>(
        &self,
        use_result: impl for<'task> FnOnce(
            TaskStructuralValue<'task>,
            TaskValueResolver<'task>,
        ) -> Result<R, TaskFault>,
    ) -> Result<R, TaskFault> {
        self.check_result_available()?;
        let function = self.executable.function(self.entry)?;
        let region = function.result;
        let declared = &function.frame.regions[region.0 as usize];
        let bytes = self.result()?;
        let expected = declared
            .shape
            .checked_byte_len()
            .ok_or(TaskFault::InvalidResultShape {
                entry: self.entry,
                region,
                size: bytes.len(),
            })?;
        if bytes.len() != expected {
            return Err(TaskFault::InvalidResultShape {
                entry: self.entry,
                region,
                size: bytes.len(),
            });
        }
        let contract = self.executable.verified.contract();
        let value_shape = declared
            .value_shape
            .map(|shape| {
                contract
                    .value_shapes
                    .get(shape.0 as usize)
                    .ok_or(TaskFault::InvalidResultShape {
                        entry: self.entry,
                        region,
                        size: bytes.len(),
                    })
            })
            .transpose()?;
        let molten = match &self.lane {
            Lane::Interpreter(task) => task.molten(),
            Lane::Native(task) => task.molten(),
        };
        use_result(
            TaskStructuralValue {
                bytes,
                entry: self.entry,
                region,
                shape: &declared.shape,
                value_shape,
                value_shapes: &contract.value_shapes,
            },
            TaskValueResolver { molten, contract },
        )
    }

    /// Borrow the active task's opaque value resolver while it is suspended at
    /// a verified host boundary. The higher-ranked closure prevents task-local
    /// handles, molten bytes, and resolver state from escaping into the host.
    pub fn with_value_resolver<R>(
        &self,
        use_resolver: impl for<'task> FnOnce(TaskValueResolver<'task>) -> R,
    ) -> R {
        let molten = match &self.lane {
            Lane::Interpreter(task) => task.molten(),
            Lane::Native(task) => task.molten(),
        };
        use_resolver(TaskValueResolver {
            molten,
            contract: self.executable.verified.contract(),
        })
    }

    /// Number of descriptors the task published, available once the task is
    /// done and never poisoned. Preserves the result lifecycle: a task that
    /// faulted surfaces [`TaskFault::PoisonedResult`], one still running
    /// surfaces [`TaskFault::ResultBeforeDone`].
    pub fn publication_count(&self) -> Result<usize, TaskFault> {
        self.check_result_available()?;
        Ok(self.publications().len())
    }

    /// The descriptor at `index` in publication order, as a read-only,
    /// contract-typed view. Gated by the same result lifecycle as
    /// [`ExecTask::publication_count`].
    pub fn publication(&self, index: usize) -> Result<PublishedDescriptor<'_>, TaskFault> {
        self.check_result_available()?;
        let log = self.publications();
        let (record, bytes) = log
            .get(index)
            .ok_or(TaskFault::PublicationIndexOutOfRange {
                index,
                count: log.len(),
            })?;
        // Publish admission proved the stored witness names a valid
        // publication-record schema, and the log copies that witness verbatim.
        let schema_index = usize::try_from(record.schema_ref)
            .ok()
            .filter(|index| *index < self.executable.verified.contract().schemas.len())
            .expect("publish admission proved record schema witness");
        let schema = SchemaRef(schema_index as u32);
        let value_shape = self.executable.verified.contract().schemas[schema_index].value_shape;
        Ok(PublishedDescriptor {
            site: record.site,
            schema,
            value_shape,
            bytes,
        })
    }

    fn publications(&self) -> &PublicationLog {
        match &self.lane {
            Lane::Interpreter(task) => task.publications(),
            Lane::Native(task) => task.publications(),
        }
    }

    fn check_not_poisoned(&self) -> Result<(), TaskFault> {
        if let Some(fault) = &self.poisoned {
            return Err(TaskFault::PoisonedReDrive {
                original: Box::new(fault.clone()),
            });
        }
        Ok(())
    }

    fn check_entries_initialized(&self) -> Result<(), TaskFault> {
        for (index, initialized) in self.entries_initialized.iter().copied().enumerate() {
            if !initialized {
                let entry = self.entry_info(index)?;
                return Err(TaskFault::EntryMissing {
                    entry: self.entry,
                    index,
                    region: entry.region,
                    kind: entry.kind,
                });
            }
        }
        Ok(())
    }

    fn entry_info(&self, index: usize) -> Result<EntryInfo, TaskFault> {
        let function = self.executable.function(self.entry)?;
        let Some(region) = function.entries.get(index).copied() else {
            return Err(TaskFault::InvalidEntryIndex {
                entry: self.entry,
                index,
                entry_count: function.entries.len(),
            });
        };
        let region_contract = &function.frame.regions[region.0 as usize];
        let Some(kind) = entry_word_kind(region_contract) else {
            return Err(TaskFault::InvalidEntryShape {
                entry: self.entry,
                index,
                region,
            });
        };
        Ok(EntryInfo {
            region,
            offset: region_contract.offset,
            kind,
        })
    }

    #[cfg(test)]
    fn adversarial_write_word_at_offset_for_test(&mut self, offset: u32, value: i64) {
        self.check_not_poisoned()
            .expect("adversarial write before poison");
        match &mut self.lane {
            Lane::Interpreter(task) => task.write_i64(offset, value),
            Lane::Native(task) => task.write_i64(offset, value),
        }
    }

    fn check_result_available(&self) -> Result<(), TaskFault> {
        if let Some(fault) = &self.poisoned {
            return Err(TaskFault::PoisonedResult {
                original: Box::new(fault.clone()),
            });
        }
        if self.state != ExecTaskState::Done {
            return Err(TaskFault::ResultBeforeDone { state: self.state });
        }
        Ok(())
    }

    fn poison(&mut self, fault: TaskFault) -> TaskFault {
        self.poisoned = Some(fault.clone());
        fault
    }
}

fn materialize_frozen(
    value: &FrozenValue,
    expected: &crate::RegionShape,
    contract: &crate::ProgramContract,
    molten: &mut crate::task::MoltenArena,
) -> Result<Vec<u8>, ()> {
    match value {
        FrozenValue::Store { schema, handle } => {
            require_handle_schema(expected, *schema)?;
            Ok(handle.as_i64().to_le_bytes().to_vec())
        }
        FrozenValue::Opaque { schema, bytes } => {
            require_handle_schema(expected, *schema)?;
            let handle = molten.import_opaque(bytes).map_err(|_| ())?;
            Ok(handle.to_le_bytes().to_vec())
        }
        FrozenValue::Dense { schema, elements } => {
            require_handle_schema(expected, *schema)?;
            let collection = contract.schemas.get(schema.0 as usize).ok_or(())?;
            let crate::PayloadKind::DenseArray { element } = &collection.payload else {
                return Err(());
            };
            let element_contract = contract.schemas.get(element.0 as usize).ok_or(())?;
            let elements = elements
                .iter()
                .map(|value| materialize_inline(value, &element_contract.inline, contract, molten))
                .collect::<Result<Vec<_>, ()>>()?;
            let handle = molten
                .import_dense(
                    i64::from(element.0),
                    element_contract.inline.checked_byte_len().ok_or(())?,
                    &elements,
                )
                .map_err(|_| ())?;
            Ok(handle.to_le_bytes().to_vec())
        }
        FrozenValue::Ordered { schema, rows } => {
            require_handle_schema(expected, *schema)?;
            let collection = contract.schemas.get(schema.0 as usize).ok_or(())?;
            let crate::PayloadKind::OrderedCollection(ordered) = &collection.payload else {
                return Err(());
            };
            let key = contract.schemas.get(ordered.key.0 as usize).ok_or(())?;
            let value_contract = ordered
                .value
                .map(|schema| contract.schemas.get(schema.0 as usize).ok_or(()))
                .transpose()?;
            let rows = rows
                .iter()
                .map(|(row_key, row_value)| {
                    let key_bytes = materialize_inline(row_key, &key.inline, contract, molten)?;
                    let value_bytes = match (row_value, value_contract) {
                        (Some(row_value), Some(value_contract)) => Some(materialize_inline(
                            row_value,
                            &value_contract.inline,
                            contract,
                            molten,
                        )?),
                        (None, None) => None,
                        _ => return Err(()),
                    };
                    Ok((key_bytes, value_bytes))
                })
                .collect::<Result<Vec<_>, ()>>()?;
            let handle = molten
                .import_ordered(i64::from(schema.0), &rows)
                .map_err(|_| ())?;
            Ok(handle.to_le_bytes().to_vec())
        }
        FrozenValue::Inline(value) => materialize_inline(value, expected, contract, molten),
    }
}

fn materialize_inline(
    value: &FrozenInlineValue,
    expected: &crate::RegionShape,
    contract: &crate::ProgramContract,
    molten: &mut crate::task::MoltenArena,
) -> Result<Vec<u8>, ()> {
    if expected.checked_byte_len() != Some(value.bytes.len()) {
        return Err(());
    }
    let mut bytes = value.bytes.clone();
    for (offset, reference) in &value.references {
        let word = usize::try_from(*offset).map_err(|_| ())? / size_of::<i64>();
        if usize::try_from(*offset).map_err(|_| ())? % size_of::<i64>() != 0 {
            return Err(());
        }
        let expected_word = expected.words.get(word).ok_or(())?;
        let schema = match reference {
            FrozenValue::Store { schema, .. }
            | FrozenValue::Opaque { schema, .. }
            | FrozenValue::Dense { schema, .. }
            | FrozenValue::Ordered { schema, .. } => *schema,
            FrozenValue::Inline(_) => return Err(()),
        };
        if !expected_word.as_slice().contains(&WordKind::Handle(schema)) {
            return Err(());
        }
        let reference = materialize_frozen(
            reference,
            &crate::RegionShape::word(WordKind::Handle(schema)),
            contract,
            molten,
        )?;
        let start = usize::try_from(*offset).map_err(|_| ())?;
        bytes[start..start + size_of::<i64>()].copy_from_slice(&reference);
    }
    Ok(bytes)
}

fn require_handle_schema(expected: &crate::RegionShape, schema: SchemaRef) -> Result<(), ()> {
    let [word] = expected.words.as_slice() else {
        return Err(());
    };
    if word.is_exactly(WordKind::Handle(schema)) {
        Ok(())
    } else {
        Err(())
    }
}

#[derive(Clone, Copy)]
struct EntryInfo {
    region: RegionId,
    offset: u32,
    kind: WordKind,
}

fn check_drive_requirements(
    requirements: DriveRequirements,
    ready: &[bool],
    awaited: &[i64],
    hosts: &[HostFn<'_>],
) -> Result<(), TaskFault> {
    check_len(DriveTable::Ready, requirements.await_inputs, ready.len())?;
    check_len(
        DriveTable::Awaited,
        requirements.await_inputs,
        awaited.len(),
    )?;
    check_len(DriveTable::Hosts, requirements.hosts, hosts.len())
}

fn check_len(table: DriveTable, expected: usize, actual: usize) -> Result<(), TaskFault> {
    if actual < expected {
        Err(TaskFault::DriveTableLength {
            table,
            expected,
            actual,
        })
    } else {
        Ok(())
    }
}

fn native_disabled_by_environment() -> bool {
    std::env::var_os("WEAVY_JIT").is_some_and(|value| value == "0")
}

fn entry_word_kind(region: &FrameRegion) -> Option<WordKind> {
    let [word] = region.shape.words.as_slice() else {
        return None;
    };
    let [kind] = word.as_slice() else {
        return None;
    };
    match kind {
        WordKind::Scalar | WordKind::Handle(_) => Some(*kind),
        WordKind::Status | WordKind::Opaque | WordKind::Callable(_) => None,
    }
}

fn entry_write_matches(expected: EntryWriteKind, actual: WordKind) -> bool {
    match (expected, actual) {
        (EntryWriteKind::Scalar, WordKind::Scalar) => true,
        (EntryWriteKind::StoreHandle(expected), WordKind::Handle(actual)) => expected == actual,
        _ => false,
    }
}

pub(crate) fn fault_site(
    verified: &VerifiedProgram,
    function: FnId,
    pc: usize,
) -> Result<FaultSite, TaskFault> {
    let Some(function_program) = verified.program().fns.get(function.0 as usize) else {
        return Err(TaskFault::InvalidFaultSite {
            function,
            pc,
            function_count: verified.program().fns.len(),
            op_count: None,
        });
    };
    let Some(op) = function_program.code.get(pc).cloned() else {
        return Err(TaskFault::InvalidFaultSite {
            function,
            pc,
            function_count: verified.program().fns.len(),
            op_count: Some(function_program.code.len()),
        });
    };
    let call = verified
        .facts()
        .function(function)
        .and_then(|function| function.pc(pc))
        .and_then(|pc| pc.call());
    Ok(FaultSite {
        function,
        pc,
        op: Box::new(op),
        call,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, MutexGuard};

    use super::*;
    use crate::jit::task_lane;
    use crate::mem::Layout;
    use crate::task::{ArgCopy, ArrayOpStatus, Fn, Program, StructuralFieldSource, ValueMemory};
    use crate::{
        AllowedKinds, CallContract, FrameContract, FrameRegion, FunctionContract, PayloadKind,
        ProgramContract, RegionShape, SchemaContract, SchemaRef, ValueFieldUse, ValueSelector,
        ValueShapeContract, ValueShapeKind, ValueShapeRef, ValueVariant,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    type LaneRun = Result<(TaskStep, Vec<u8>, Vec<TaskEvent>), TaskFault>;
    type FaultPredicate = fn(&TaskFault) -> bool;

    fn layout(words: usize) -> Layout {
        Layout {
            size: words * size_of::<i64>(),
            align: size_of::<i64>(),
        }
    }

    fn function(words: usize, code: Vec<Op>) -> Fn {
        Fn {
            frame: layout(words),
            code,
        }
    }

    fn word_region(offset: u32, kind: WordKind) -> FrameRegion {
        FrameRegion::new(offset, RegionShape::word(kind))
    }

    fn function_contract(
        words: usize,
        regions: Vec<FrameRegion>,
        entries: &[u32],
        result: u32,
        call_contract: Option<u32>,
    ) -> FunctionContract {
        FunctionContract {
            frame: FrameContract {
                layout: layout(words),
                regions,
            },
            entries: entries.iter().copied().map(RegionId).collect(),
            result: RegionId(result),
            call_contract: call_contract.map(CallContractId),
            call_abi: None,
            environment: Vec::new(),
        }
    }

    fn scalar_contract(words: usize, entries: &[u32], result: u32) -> FunctionContract {
        let regions = (0..words)
            .map(|word| word_region((word * size_of::<i64>()) as u32, WordKind::Scalar))
            .collect();
        function_contract(words, regions, entries, result, None)
    }

    fn scalar_add_program() -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(
                    3,
                    vec![
                        Op::AddI64 {
                            dst: 16,
                            a: 0,
                            b: 8,
                        },
                        Op::Trace { id: 77 },
                        Op::Ret { src: 16, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![scalar_contract(3, &[0, 1], 2)],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn scalar_identity_program() -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(1, vec![Op::Ret { src: 0, size: 8 }])],
            },
            ProgramContract {
                functions: vec![scalar_contract(1, &[0], 0)],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn mixed_scalar_handle_program() -> (Program, ProgramContract) {
        let schema = SchemaRef(0);
        (
            Program {
                fns: vec![function(2, vec![Op::Ret { src: 0, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    2,
                    vec![
                        word_region(0, WordKind::Scalar),
                        word_region(8, WordKind::Handle(schema)),
                    ],
                    &[0, 1],
                    0,
                    None,
                )],
                calls: vec![],
                schemas: vec![SchemaContract {
                    inline: RegionShape::word(WordKind::Handle(schema)),
                    value_shape: None,
                    payload: PayloadKind::OpaqueBytes {
                        byte_comparable: true,
                    },
                }],
                value_shapes: vec![],
            },
        )
    }

    fn entry_then_await_program() -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(
                    1,
                    vec![Op::Await { dst: 0, input: 0 }, Op::Ret { src: 0, size: 8 }],
                )],
            },
            ProgramContract {
                functions: vec![scalar_contract(1, &[0], 0)],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn awaiting_program() -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(
                    1,
                    vec![Op::Await { dst: 0, input: 1 }, Op::Ret { src: 0, size: 8 }],
                )],
            },
            ProgramContract {
                functions: vec![scalar_contract(1, &[], 0)],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn callable_regions(contract: CallContractId) -> Vec<FrameRegion> {
        vec![
            word_region(0, WordKind::Callable(contract)),
            word_region(8, WordKind::Scalar),
            word_region(16, WordKind::Scalar),
        ]
    }

    fn scalar_call_contract() -> CallContract {
        CallContract {
            entries: vec![word_region(0, WordKind::Scalar)],
            result: word_region(8, WordKind::Scalar),
        }
    }

    fn indirect_program() -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![
                    function(
                        3,
                        vec![
                            Op::CallIndirect {
                                callee: 0,
                                args: vec![ArgCopy {
                                    src: 8,
                                    dst: 0,
                                    size: 8,
                                }],
                                ret: 16,
                            },
                            Op::Ret { src: 16, size: 8 },
                        ],
                    ),
                    function(
                        2,
                        vec![
                            Op::AddI64 { dst: 8, a: 0, b: 0 },
                            Op::Ret { src: 8, size: 8 },
                        ],
                    ),
                    function(
                        3,
                        vec![
                            Op::ConstI64 { dst: 16, value: 9 },
                            Op::Ret { src: 16, size: 8 },
                        ],
                    ),
                ],
            },
            ProgramContract {
                functions: vec![
                    function_contract(3, callable_regions(CallContractId(0)), &[1], 2, None),
                    function_contract(
                        2,
                        vec![
                            word_region(0, WordKind::Scalar),
                            word_region(8, WordKind::Scalar),
                        ],
                        &[0],
                        1,
                        Some(0),
                    ),
                    function_contract(
                        3,
                        vec![
                            word_region(0, WordKind::Scalar),
                            word_region(8, WordKind::Scalar),
                            word_region(16, WordKind::Scalar),
                        ],
                        &[0],
                        2,
                        Some(1),
                    ),
                ],
                calls: vec![
                    scalar_call_contract(),
                    CallContract {
                        entries: vec![word_region(0, WordKind::Scalar)],
                        result: word_region(16, WordKind::Scalar),
                    },
                ],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn compare_program() -> (Program, ProgramContract) {
        let schema = SchemaRef(0);
        (
            Program {
                fns: vec![function(
                    3,
                    vec![
                        Op::CompareValueBytes {
                            dst: 16,
                            a: 0,
                            b: 8,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    3,
                    vec![
                        word_region(0, WordKind::Handle(schema)),
                        word_region(8, WordKind::Handle(schema)),
                        word_region(16, WordKind::Scalar),
                    ],
                    &[0, 1],
                    2,
                    None,
                )],
                calls: vec![],
                schemas: vec![SchemaContract {
                    inline: RegionShape::word(WordKind::Handle(schema)),
                    value_shape: None,
                    payload: PayloadKind::OpaqueBytes {
                        byte_comparable: true,
                    },
                }],
                value_shapes: vec![],
            },
        )
    }

    /// `compare((a ++ b) ++ c, expected)` — a StringConcat result feeds a second
    /// StringConcat, whose result feeds a CompareValueBytes. Entries `0..=3` are
    /// the four operand handles; the returned scalar is the three-way ordinal.
    fn string_concat_program() -> (Program, ProgramContract) {
        let schema = SchemaRef(0);
        (
            Program {
                fns: vec![function(
                    7,
                    vec![
                        Op::StringConcat {
                            dst: 32,
                            a: 0,
                            b: 8,
                        },
                        Op::StringConcat {
                            dst: 40,
                            a: 32,
                            b: 16,
                        },
                        Op::CompareValueBytes {
                            dst: 48,
                            a: 40,
                            b: 24,
                        },
                        Op::Ret { src: 48, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    7,
                    vec![
                        word_region(0, WordKind::Handle(schema)),
                        word_region(8, WordKind::Handle(schema)),
                        word_region(16, WordKind::Handle(schema)),
                        word_region(24, WordKind::Handle(schema)),
                        word_region(32, WordKind::Handle(schema)),
                        word_region(40, WordKind::Handle(schema)),
                        word_region(48, WordKind::Scalar),
                    ],
                    &[0, 1, 2, 3],
                    6,
                    None,
                )],
                calls: vec![],
                schemas: vec![SchemaContract {
                    inline: RegionShape::word(WordKind::Handle(schema)),
                    value_shape: None,
                    payload: PayloadKind::OpaqueBytes {
                        byte_comparable: true,
                    },
                }],
                value_shapes: vec![],
            },
        )
    }

    fn string_operation_program(operation: Op) -> (Program, ProgramContract) {
        let schema = SchemaRef(0);
        (
            Program {
                fns: vec![function(5, vec![operation, Op::Ret { src: 32, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    5,
                    vec![
                        word_region(0, WordKind::Handle(schema)),
                        word_region(8, WordKind::Handle(schema)),
                        word_region(16, WordKind::Handle(schema)),
                        word_region(24, WordKind::Status),
                        word_region(32, WordKind::Scalar),
                    ],
                    &[0, 1],
                    4,
                    None,
                )],
                calls: vec![],
                schemas: vec![SchemaContract {
                    inline: RegionShape::word(WordKind::Handle(schema)),
                    value_shape: None,
                    payload: PayloadKind::OpaqueBytes {
                        byte_comparable: true,
                    },
                }],
                value_shapes: vec![],
            },
        )
    }

    fn byte_project_program() -> (Program, ProgramContract) {
        let path = SchemaRef(0);
        let string = SchemaRef(1);
        (
            Program {
                fns: vec![function(
                    4,
                    vec![
                        Op::ByteProject { dst: 8, source: 0 },
                        Op::CompareValueBytes {
                            dst: 24,
                            a: 8,
                            b: 16,
                        },
                        Op::Ret { src: 24, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    4,
                    vec![
                        word_region(0, WordKind::Handle(path)),
                        word_region(8, WordKind::Handle(string)),
                        word_region(16, WordKind::Handle(string)),
                        word_region(24, WordKind::Scalar),
                    ],
                    &[0, 2],
                    3,
                    None,
                )],
                calls: vec![],
                schemas: vec![
                    SchemaContract {
                        inline: RegionShape::word(WordKind::Handle(path)),
                        value_shape: None,
                        payload: PayloadKind::OpaqueBytes {
                            byte_comparable: true,
                        },
                    },
                    SchemaContract {
                        inline: RegionShape::word(WordKind::Handle(string)),
                        value_shape: None,
                        payload: PayloadKind::OpaqueBytes {
                            byte_comparable: true,
                        },
                    },
                ],
                value_shapes: vec![],
            },
        )
    }

    fn path_join_program() -> (Program, ProgramContract) {
        let path = SchemaRef(0);
        (
            Program {
                fns: vec![function(
                    5,
                    vec![
                        Op::PathJoin {
                            dst: 16,
                            base: 0,
                            segment: 8,
                        },
                        Op::CompareValueBytes {
                            dst: 32,
                            a: 16,
                            b: 24,
                        },
                        Op::Ret { src: 32, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    5,
                    vec![
                        word_region(0, WordKind::Handle(path)),
                        word_region(8, WordKind::Handle(path)),
                        word_region(16, WordKind::Handle(path)),
                        word_region(24, WordKind::Handle(path)),
                        word_region(32, WordKind::Scalar),
                    ],
                    &[0, 1, 3],
                    4,
                    None,
                )],
                calls: vec![],
                schemas: vec![SchemaContract {
                    inline: RegionShape::word(WordKind::Handle(path)),
                    value_shape: None,
                    payload: PayloadKind::OpaqueBytes {
                        byte_comparable: true,
                    },
                }],
                value_shapes: vec![],
            },
        )
    }

    fn non_scalar_entry_program() -> (Program, ProgramContract) {
        // A callable word is non-scalar, so the entry accessor must reject it;
        // opaque cursor words are separately confined and cannot appear here.
        let callable = AllowedKinds::new(WordKind::Callable(CallContractId(0)));
        (
            Program {
                fns: vec![function(1, vec![Op::Ret { src: 0, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    1,
                    vec![FrameRegion::new(0, RegionShape::new(vec![callable]))],
                    &[0],
                    0,
                    None,
                )],
                calls: vec![CallContract {
                    entries: vec![],
                    result: word_region(0, WordKind::Scalar),
                }],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn callable_entry_program() -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(2, vec![Op::Ret { src: 8, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    2,
                    vec![
                        word_region(0, WordKind::Callable(CallContractId(0))),
                        word_region(8, WordKind::Scalar),
                    ],
                    &[0],
                    1,
                    None,
                )],
                calls: vec![scalar_call_contract()],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn non_scalar_result_program() -> (Program, ProgramContract) {
        // A callable result word is non-scalar, so the result accessor must
        // reject it; opaque cursor words are separately barred from results.
        let callable = AllowedKinds::new(WordKind::Callable(CallContractId(0)));
        (
            Program {
                fns: vec![function(1, vec![Op::Ret { src: 0, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    1,
                    vec![FrameRegion::new(0, RegionShape::new(vec![callable]))],
                    &[],
                    0,
                    None,
                )],
                calls: vec![CallContract {
                    entries: vec![],
                    result: word_region(0, WordKind::Scalar),
                }],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn verify(pair: (Program, ProgramContract)) -> VerifiedProgram {
        pair.0.verify(pair.1).expect("program verifies")
    }

    fn structural_region(
        offset: u32,
        shape: RegionShape,
        value_shape: ValueShapeRef,
    ) -> FrameRegion {
        FrameRegion::new(offset, shape).with_value_shape(value_shape)
    }

    fn structural_field(
        offset: u32,
        shape: RegionShape,
        value_shape: ValueShapeRef,
    ) -> ValueFieldUse {
        ValueFieldUse::new(offset, shape).with_value_shape(value_shape)
    }

    #[test]
    fn public_executable_constructs_projects_and_copies_nested_products() {
        let scalar = RegionShape::word(WordKind::Scalar);
        let pair_shape = RegionShape::new(vec![
            AllowedKinds::new(WordKind::Scalar),
            AllowedKinds::new(WordKind::Scalar),
        ]);
        let nested_shape = RegionShape::new(vec![
            AllowedKinds::new(WordKind::Scalar),
            AllowedKinds::new(WordKind::Scalar),
            AllowedKinds::new(WordKind::Scalar),
        ]);
        let value_shapes = vec![
            ValueShapeContract {
                shape: pair_shape.clone(),
                kind: ValueShapeKind::Product {
                    fields: vec![
                        ValueFieldUse::new(0, scalar.clone()),
                        ValueFieldUse::new(8, scalar.clone()),
                    ],
                },
            },
            ValueShapeContract {
                shape: scalar.clone(),
                kind: ValueShapeKind::Product {
                    fields: vec![ValueFieldUse::new(0, scalar.clone())],
                },
            },
            ValueShapeContract {
                shape: RegionShape::default(),
                kind: ValueShapeKind::Product { fields: vec![] },
            },
            ValueShapeContract {
                shape: nested_shape.clone(),
                kind: ValueShapeKind::Product {
                    fields: vec![
                        structural_field(0, pair_shape.clone(), ValueShapeRef(0)),
                        ValueFieldUse::new(16, scalar.clone()),
                    ],
                },
            },
        ];
        let regions = vec![
            word_region(0, WordKind::Scalar),
            word_region(8, WordKind::Scalar),
            structural_region(16, pair_shape.clone(), ValueShapeRef(0)),
            word_region(32, WordKind::Scalar),
            structural_region(40, pair_shape.clone(), ValueShapeRef(0)),
            structural_region(56, scalar.clone(), ValueShapeRef(1)),
            structural_region(64, scalar.clone(), ValueShapeRef(1)),
            structural_region(72, RegionShape::default(), ValueShapeRef(2)),
            structural_region(72, RegionShape::default(), ValueShapeRef(2)),
            structural_region(72, nested_shape.clone(), ValueShapeRef(3)),
            structural_region(96, nested_shape, ValueShapeRef(3)),
        ];
        let code = vec![
            Op::ConstI64 { dst: 0, value: 11 },
            Op::ConstI64 { dst: 8, value: 22 },
            Op::ProductConstruct {
                dst: RegionId(2),
                fields: vec![
                    StructuralFieldSource {
                        field: 0,
                        source: RegionId(0),
                    },
                    StructuralFieldSource {
                        field: 1,
                        source: RegionId(1),
                    },
                ],
            },
            Op::ProductProject {
                dst: RegionId(3),
                product: RegionId(2),
                field: 0,
            },
            Op::CopyValue {
                dst: RegionId(4),
                src: RegionId(2),
            },
            Op::ProductConstruct {
                dst: RegionId(5),
                fields: vec![StructuralFieldSource {
                    field: 0,
                    source: RegionId(3),
                }],
            },
            Op::CopyValue {
                dst: RegionId(6),
                src: RegionId(5),
            },
            Op::ProductConstruct {
                dst: RegionId(7),
                fields: vec![],
            },
            Op::CopyValue {
                dst: RegionId(8),
                src: RegionId(7),
            },
            Op::ProductConstruct {
                dst: RegionId(9),
                fields: vec![
                    StructuralFieldSource {
                        field: 0,
                        source: RegionId(4),
                    },
                    StructuralFieldSource {
                        field: 1,
                        source: RegionId(3),
                    },
                ],
            },
            Op::CopyValue {
                dst: RegionId(10),
                src: RegionId(9),
            },
            Op::Ret { src: 96, size: 24 },
        ];
        let program = Program {
            fns: vec![function(15, code)],
        };
        let contract = ProgramContract {
            functions: vec![function_contract(15, regions, &[], 10, None)],
            calls: vec![],
            schemas: vec![],
            value_shapes,
        };
        let executable = Rc::new(Executable::new(program.verify(contract).unwrap()));
        let mut task = executable.spawn(FnId(0)).unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(
            task.result().unwrap(),
            [
                11i64.to_le_bytes(),
                22i64.to_le_bytes(),
                11i64.to_le_bytes()
            ]
            .concat()
        );
    }

    fn enum_program(op: Op) -> (Program, ProgramContract) {
        let scalar = RegionShape::word(WordKind::Scalar);
        let enum_shape = RegionShape::new(vec![
            AllowedKinds::new(WordKind::Scalar),
            AllowedKinds::new(WordKind::Scalar),
        ]);
        let value_shape = ValueShapeContract {
            shape: enum_shape.clone(),
            kind: ValueShapeKind::Enum {
                selector: ValueSelector {
                    offset: 0,
                    shape: scalar.clone(),
                },
                variants: vec![
                    ValueVariant {
                        fields: vec![ValueFieldUse::new(8, scalar.clone())],
                    },
                    ValueVariant {
                        fields: vec![ValueFieldUse::new(8, scalar.clone())],
                    },
                ],
            },
        };
        (
            Program {
                fns: vec![function(3, vec![op, Op::Ret { src: 16, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    3,
                    vec![
                        structural_region(0, enum_shape, ValueShapeRef(0)),
                        word_region(16, WordKind::Scalar),
                    ],
                    &[],
                    1,
                    None,
                )],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![value_shape],
            },
        )
    }

    fn structural_entry_program(op: Op) -> (Program, ProgramContract) {
        let (program, mut contract) = enum_program(op);
        contract.functions[0].entries.push(RegionId(0));
        (program, contract)
    }

    #[test]
    fn public_executable_constructs_tests_and_projects_compact_enum_variants() {
        let scalar = RegionShape::word(WordKind::Scalar);
        let handle = RegionShape::word(WordKind::Handle(SchemaRef(0)));
        let pair = RegionShape::new(vec![
            AllowedKinds::new(WordKind::Scalar),
            AllowedKinds::new(WordKind::Scalar),
        ]);
        let nested_enum = pair.clone();
        let outer = RegionShape::new(vec![
            AllowedKinds::new(WordKind::Scalar),
            AllowedKinds::new(WordKind::Scalar).allowing(WordKind::Handle(SchemaRef(0))),
            AllowedKinds::new(WordKind::Scalar),
        ]);
        let value_shapes = vec![
            ValueShapeContract {
                shape: pair.clone(),
                kind: ValueShapeKind::Product {
                    fields: vec![
                        ValueFieldUse::new(0, scalar.clone()),
                        ValueFieldUse::new(8, scalar.clone()),
                    ],
                },
            },
            ValueShapeContract {
                shape: nested_enum.clone(),
                kind: ValueShapeKind::Enum {
                    selector: ValueSelector {
                        offset: 0,
                        shape: scalar.clone(),
                    },
                    variants: vec![
                        ValueVariant {
                            fields: vec![ValueFieldUse::new(8, scalar.clone())],
                        },
                        ValueVariant { fields: vec![] },
                    ],
                },
            },
            ValueShapeContract {
                shape: outer.clone(),
                kind: ValueShapeKind::Enum {
                    selector: ValueSelector {
                        offset: 0,
                        shape: scalar.clone(),
                    },
                    variants: vec![
                        ValueVariant {
                            fields: vec![ValueFieldUse::new(8, scalar.clone())],
                        },
                        ValueVariant {
                            fields: vec![ValueFieldUse::new(8, handle.clone())],
                        },
                        ValueVariant {
                            fields: vec![structural_field(8, pair.clone(), ValueShapeRef(0))],
                        },
                        ValueVariant {
                            fields: vec![structural_field(
                                8,
                                nested_enum.clone(),
                                ValueShapeRef(1),
                            )],
                        },
                    ],
                },
            },
        ];
        let regions = vec![
            word_region(0, WordKind::Scalar),
            word_region(8, WordKind::Scalar),
            word_region(16, WordKind::Handle(SchemaRef(0))),
            structural_region(24, pair.clone(), ValueShapeRef(0)),
            structural_region(40, nested_enum.clone(), ValueShapeRef(1)),
            structural_region(56, outer.clone(), ValueShapeRef(2)),
            word_region(80, WordKind::Scalar),
            word_region(88, WordKind::Scalar),
            word_region(96, WordKind::Handle(SchemaRef(0))),
            structural_region(104, pair, ValueShapeRef(0)),
            structural_region(120, nested_enum, ValueShapeRef(1)),
        ];
        let code = vec![
            Op::ConstI64 { dst: 0, value: 7 },
            Op::ConstI64 { dst: 8, value: 9 },
            Op::ProductConstruct {
                dst: RegionId(3),
                fields: vec![
                    StructuralFieldSource {
                        field: 0,
                        source: RegionId(0),
                    },
                    StructuralFieldSource {
                        field: 1,
                        source: RegionId(1),
                    },
                ],
            },
            Op::EnumConstruct {
                dst: RegionId(4),
                variant: 0,
                fields: vec![StructuralFieldSource {
                    field: 0,
                    source: RegionId(0),
                }],
            },
            Op::EnumConstruct {
                dst: RegionId(5),
                variant: 0,
                fields: vec![StructuralFieldSource {
                    field: 0,
                    source: RegionId(0),
                }],
            },
            Op::EnumIsVariant {
                dst: RegionId(6),
                value: RegionId(5),
                variant: 0,
            },
            Op::EnumProjectChecked {
                dst: RegionId(7),
                value: RegionId(5),
                variant: 0,
                field: 0,
            },
            Op::EnumConstruct {
                dst: RegionId(5),
                variant: 1,
                fields: vec![StructuralFieldSource {
                    field: 0,
                    source: RegionId(2),
                }],
            },
            Op::EnumProjectChecked {
                dst: RegionId(8),
                value: RegionId(5),
                variant: 1,
                field: 0,
            },
            Op::EnumConstruct {
                dst: RegionId(5),
                variant: 2,
                fields: vec![StructuralFieldSource {
                    field: 0,
                    source: RegionId(3),
                }],
            },
            Op::EnumProjectChecked {
                dst: RegionId(9),
                value: RegionId(5),
                variant: 2,
                field: 0,
            },
            Op::EnumConstruct {
                dst: RegionId(5),
                variant: 3,
                fields: vec![StructuralFieldSource {
                    field: 0,
                    source: RegionId(4),
                }],
            },
            Op::EnumProjectChecked {
                dst: RegionId(10),
                value: RegionId(5),
                variant: 3,
                field: 0,
            },
            Op::EnumConstruct {
                dst: RegionId(5),
                variant: 0,
                fields: vec![StructuralFieldSource {
                    field: 0,
                    source: RegionId(0),
                }],
            },
            Op::Ret { src: 56, size: 24 },
        ];
        let program = Program {
            fns: vec![function(17, code)],
        };
        let contract = ProgramContract {
            functions: vec![function_contract(17, regions, &[2], 5, None)],
            calls: vec![],
            schemas: vec![SchemaContract {
                inline: handle,
                value_shape: None,
                payload: PayloadKind::OpaqueBytes {
                    byte_comparable: true,
                },
            }],
            value_shapes,
        };
        let executable = Rc::new(Executable::new(program.verify(contract).unwrap()));
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_store_handle(0, SchemaRef(0), StoreHandle::new(3).unwrap())
            .unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(
            task.result().unwrap(),
            [0i64.to_le_bytes(), 7i64.to_le_bytes(), 0i64.to_le_bytes()].concat()
        );
    }

    fn run_public_fault(
        verified: VerifiedProgram,
        selector: i64,
        force_interpreter: bool,
    ) -> (TaskFault, Vec<TaskEvent>, TaskFault) {
        let previous = std::env::var_os("WEAVY_JIT");
        if force_interpreter {
            unsafe { std::env::set_var("WEAVY_JIT", "0") };
        } else {
            unsafe { std::env::remove_var("WEAVY_JIT") };
        }
        let executable = Rc::new(Executable::new(verified));
        match previous {
            Some(value) => unsafe { std::env::set_var("WEAVY_JIT", value) },
            None => unsafe { std::env::remove_var("WEAVY_JIT") },
        }
        let mut task = executable.spawn(FnId(0)).unwrap();
        // Seed a valid local enum, then corrupt only its selector through a
        // test-private frame hook before entering the verified program.
        task.adversarial_write_word_at_offset_for_test(0, 0);
        task.adversarial_write_word_at_offset_for_test(8, 42);
        task.adversarial_write_word_at_offset_for_test(0, selector);
        let fault = task.drive(&mut [], &[]).unwrap_err();
        let trace = task.trace().to_vec();
        let poison = task.drive(&mut [], &[]).unwrap_err();
        (fault, trace, poison)
    }

    fn array_status_program() -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(
                    2,
                    vec![
                        Op::ArrayStatusIs {
                            dst: 0,
                            status: 8,
                            expected: ArrayOpStatus::OutOfRange,
                        },
                        Op::Ret { src: 0, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    2,
                    vec![
                        word_region(0, WordKind::Scalar),
                        word_region(8, WordKind::Status),
                    ],
                    &[],
                    0,
                    None,
                )],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    type PublicArrayStatus = (i64, Vec<TaskEvent>);
    type PublicArrayStatusFault = (Box<TaskFault>, Vec<TaskEvent>, Box<TaskFault>);

    fn run_public_array_status(
        verified: VerifiedProgram,
        status: i64,
        force_interpreter: bool,
    ) -> Result<PublicArrayStatus, PublicArrayStatusFault> {
        let previous = std::env::var_os("WEAVY_JIT");
        if force_interpreter {
            unsafe { std::env::set_var("WEAVY_JIT", "0") };
        } else {
            unsafe { std::env::remove_var("WEAVY_JIT") };
        }
        let executable = Rc::new(Executable::new(verified));
        match previous {
            Some(value) => unsafe { std::env::set_var("WEAVY_JIT", value) },
            None => unsafe { std::env::remove_var("WEAVY_JIT") },
        }
        let mut task = executable.spawn(FnId(0)).expect("verified entry spawns");
        task.adversarial_write_word_at_offset_for_test(8, status);
        match task.drive(&mut [], &[]) {
            Ok(TaskStep::Done) => Ok((
                task.result_i64().expect("scalar result"),
                task.trace().to_vec(),
            )),
            Ok(step) => panic!("pure status program returned {step:?}"),
            Err(fault) => {
                let trace = task.trace().to_vec();
                let poison = task.drive(&mut [], &[]).expect_err("fault poisons task");
                Err((Box::new(fault), trace, Box::new(poison)))
            }
        }
    }

    #[test]
    fn array_status_discriminator_matches_across_public_executable_lanes() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interpreter = run_public_array_status(verify(array_status_program()), 6, true);
        let native = run_public_array_status(verify(array_status_program()), 6, false);
        assert_eq!(interpreter, native);
        assert_eq!(
            interpreter.expect("OutOfRange is a valid status"),
            (
                1,
                vec![
                    TaskEvent::FrameEntered(FnId(0)),
                    TaskEvent::FrameExited(FnId(0))
                ]
            )
        );

        let interpreter = run_public_array_status(verify(array_status_program()), 99, true)
            .expect_err("invalid status faults");
        let native = run_public_array_status(verify(array_status_program()), 99, false)
            .expect_err("invalid status faults");
        assert_eq!(interpreter, native);
        let (fault, trace, poison) = interpreter;
        let site = match *fault {
            TaskFault::InvalidArrayStatus { site, actual } => {
                assert_eq!(actual, 99);
                site
            }
            fault => panic!("unexpected array status fault: {fault:?}"),
        };
        assert_eq!(site.function, FnId(0));
        assert_eq!(site.pc, 0);
        assert!(matches!(site.op.as_ref(), Op::ArrayStatusIs { .. }));
        assert_eq!(trace, vec![TaskEvent::FrameEntered(FnId(0))]);
        assert!(matches!(*poison, TaskFault::PoisonedReDrive { .. }));
    }

    #[test]
    fn string_status_discriminator_matches_across_public_executable_lanes() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let string_program = || {
            let (mut program, contract) = array_status_program();
            program.fns[0].code[0] = Op::StringStatusIs {
                dst: 0,
                status: 8,
                expected: crate::task::StringOpStatus::Ok,
            };
            verify((program, contract))
        };
        let interpreter = run_public_array_status(string_program(), 99, true)
            .expect_err("invalid StringOpStatus faults in interpreter");
        let native = run_public_array_status(string_program(), 99, false)
            .expect_err("invalid StringOpStatus faults in native lane");
        assert_eq!(interpreter, native);
        let (fault, trace, poison) = interpreter;
        let site = match *fault {
            TaskFault::InvalidStringStatus { site, actual } => {
                assert_eq!(actual, 99);
                site
            }
            fault => panic!("unexpected string status fault: {fault:?}"),
        };
        assert_eq!(site.function, FnId(0));
        assert_eq!(site.pc, 0);
        assert!(matches!(site.op.as_ref(), Op::StringStatusIs { .. }));
        assert_eq!(trace, vec![TaskEvent::FrameEntered(FnId(0))]);
        assert!(matches!(*poison, TaskFault::PoisonedReDrive { .. }));
    }

    fn ordered_begin_probe_program() -> (Program, ProgramContract) {
        use crate::{OrderedCollectionContract, OrderedCollectionKind};

        let collection = SchemaRef(3);
        let opaque_cursor = RegionShape::new(vec![AllowedKinds::new(WordKind::Opaque); 2]);
        (
            Program {
                fns: vec![function(
                    4,
                    vec![
                        Op::OrderedBeginProbe {
                            cursor: 8,
                            status: 24,
                            collection: 0,
                            collection_schema_ref: 3,
                        },
                        Op::Ret { src: 24, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    4,
                    vec![
                        word_region(0, WordKind::Handle(collection)),
                        FrameRegion::new(8, opaque_cursor),
                        word_region(24, WordKind::Status),
                    ],
                    &[0],
                    2,
                    None,
                )],
                calls: vec![],
                schemas: vec![
                    SchemaContract {
                        inline: RegionShape::word(WordKind::Scalar),
                        value_shape: None,
                        payload: PayloadKind::Inline,
                    },
                    SchemaContract {
                        inline: RegionShape::word(WordKind::Scalar),
                        value_shape: None,
                        payload: PayloadKind::Inline,
                    },
                    SchemaContract {
                        inline: RegionShape::new(vec![
                            AllowedKinds::new(WordKind::Scalar),
                            AllowedKinds::new(WordKind::Scalar),
                        ]),
                        value_shape: None,
                        payload: PayloadKind::Inline,
                    },
                    SchemaContract {
                        inline: RegionShape::word(WordKind::Handle(collection)),
                        value_shape: None,
                        payload: PayloadKind::OrderedCollection(OrderedCollectionContract {
                            kind: OrderedCollectionKind::Map,
                            key: SchemaRef(0),
                            value: Some(SchemaRef(1)),
                            row: SchemaRef(2),
                            fanout: 4,
                        }),
                    },
                ],
                value_shapes: vec![],
            },
        )
    }

    fn ordered_map_schemas() -> Vec<SchemaContract> {
        use crate::{OrderedCollectionContract, OrderedCollectionKind};
        vec![
            SchemaContract {
                inline: RegionShape::word(WordKind::Scalar),
                value_shape: None,
                payload: PayloadKind::Inline,
            },
            SchemaContract {
                inline: RegionShape::word(WordKind::Scalar),
                value_shape: None,
                payload: PayloadKind::Inline,
            },
            SchemaContract {
                inline: RegionShape::new(vec![
                    AllowedKinds::new(WordKind::Scalar),
                    AllowedKinds::new(WordKind::Scalar),
                ]),
                value_shape: None,
                payload: PayloadKind::Inline,
            },
            SchemaContract {
                inline: RegionShape::word(WordKind::Handle(SchemaRef(3))),
                value_shape: None,
                payload: PayloadKind::OrderedCollection(OrderedCollectionContract {
                    kind: OrderedCollectionKind::Map,
                    key: SchemaRef(0),
                    value: Some(SchemaRef(1)),
                    row: SchemaRef(2),
                    fanout: 4,
                }),
            },
        ]
    }

    fn ordered_probe_program(code: Vec<Op>, entries: &[u32]) -> (Program, ProgramContract) {
        let collection = WordKind::Handle(SchemaRef(3));
        (
            Program {
                fns: vec![function(8, code)],
            },
            ProgramContract {
                functions: vec![function_contract(
                    8,
                    vec![
                        word_region(0, collection),
                        FrameRegion::new(
                            8,
                            RegionShape::new(vec![AllowedKinds::new(WordKind::Opaque); 2]),
                        ),
                        word_region(24, WordKind::Status),
                        word_region(32, WordKind::Scalar),
                        word_region(40, WordKind::Scalar),
                        word_region(48, collection),
                        word_region(56, collection),
                    ],
                    entries,
                    2,
                    None,
                )],
                calls: vec![],
                schemas: ordered_map_schemas(),
                value_shapes: vec![],
            },
        )
    }

    fn probe_key_op() -> Op {
        Op::OrderedProbeKey {
            cursor: 8,
            present: 32,
            key: 40,
            left: 48,
            right: 56,
            status: 24,
            key_width: 8,
            collection_schema_ref: 3,
        }
    }

    #[test]
    fn ordered_probe_key_handshake_matches_across_lanes() {
        use crate::task::OrderedOpStatus;

        let begin = Op::OrderedBeginProbe {
            cursor: 8,
            status: 24,
            collection: 0,
            collection_schema_ref: 3,
        };
        // Each case returns the probe status word; the interpreter and native
        // lanes must agree on every closed-handshake outcome.
        let cases: [(&str, (Program, ProgramContract), i64, OrderedOpStatus); 3] = [
            (
                "empty handshake miss",
                ordered_probe_program(
                    vec![begin.clone(), probe_key_op(), Op::Ret { src: 24, size: 8 }],
                    &[0],
                ),
                0,
                OrderedOpStatus::Ok,
            ),
            (
                "forged cursor",
                ordered_probe_program(vec![probe_key_op(), Op::Ret { src: 24, size: 8 }], &[]),
                -1,
                OrderedOpStatus::InvalidHandle,
            ),
            (
                "stale double probe",
                ordered_probe_program(
                    vec![
                        begin.clone(),
                        probe_key_op(),
                        probe_key_op(),
                        Op::Ret { src: 24, size: 8 },
                    ],
                    &[0],
                ),
                0,
                OrderedOpStatus::Stale,
            ),
        ];
        for (name, program, cursor_seed, expected) in cases {
            let verified = Arc::new(verify(program));
            // collection handle @0 (empty), cursor index @8, generation @16.
            let interp = run_interpreter(
                &verified,
                |task| {
                    task.write_i64(0, crate::task::ORDERED_EMPTY_HANDLE);
                    task.write_i64(8, cursor_seed);
                    task.write_i64(16, 0);
                },
                ValueMemories::empty(),
            )
            .unwrap_or_else(|fault| panic!("{name}: {fault:?}"));
            assert_eq!(interp.0, TaskStep::Done, "{name}");
            assert_eq!(
                i64::from_le_bytes(interp.1[..8].try_into().unwrap()),
                expected as i64,
                "{name} interpreter status"
            );
            if let Some(native) = run_native(
                Arc::clone(&verified),
                |task| {
                    task.write_i64(0, crate::task::ORDERED_EMPTY_HANDLE);
                    task.write_i64(8, cursor_seed);
                    task.write_i64(16, 0);
                },
                ValueMemories::empty(),
            ) {
                assert_eq!(native.expect("native probe"), interp, "{name}");
            }
        }
    }

    fn ordered_value_program(code: Vec<Op>, entries: &[u32]) -> (Program, ProgramContract) {
        let collection = WordKind::Handle(SchemaRef(3));
        (
            Program {
                fns: vec![function(6, code)],
            },
            ProgramContract {
                functions: vec![function_contract(
                    6,
                    vec![
                        word_region(0, collection),
                        FrameRegion::new(
                            8,
                            RegionShape::new(vec![AllowedKinds::new(WordKind::Opaque); 2]),
                        ),
                        word_region(24, WordKind::Status),
                        word_region(32, WordKind::Scalar),
                        word_region(40, WordKind::Scalar),
                    ],
                    entries,
                    2,
                    None,
                )],
                calls: vec![],
                schemas: ordered_map_schemas(),
                value_shapes: vec![],
            },
        )
    }

    fn probe_value_op() -> Op {
        Op::OrderedProbeValue {
            cursor: 8,
            present: 32,
            value: 40,
            status: 24,
            value_width: 8,
            collection_schema_ref: 3,
        }
    }

    #[test]
    fn ordered_probe_value_handshake_matches_across_lanes() {
        use crate::task::OrderedOpStatus;

        let begin = Op::OrderedBeginProbe {
            cursor: 8,
            status: 24,
            collection: 0,
            collection_schema_ref: 3,
        };
        let cases: [(&str, (Program, ProgramContract), i64, OrderedOpStatus); 2] = [
            (
                "empty value miss",
                ordered_value_program(
                    vec![
                        begin.clone(),
                        probe_value_op(),
                        Op::Ret { src: 24, size: 8 },
                    ],
                    &[0],
                ),
                0,
                OrderedOpStatus::Ok,
            ),
            (
                "forged value cursor",
                ordered_value_program(vec![probe_value_op(), Op::Ret { src: 24, size: 8 }], &[]),
                -1,
                OrderedOpStatus::InvalidHandle,
            ),
        ];
        for (name, program, cursor_seed, expected) in cases {
            let verified = Arc::new(verify(program));
            let interp = run_interpreter(
                &verified,
                |task| {
                    task.write_i64(0, crate::task::ORDERED_EMPTY_HANDLE);
                    task.write_i64(8, cursor_seed);
                    task.write_i64(16, 0);
                },
                ValueMemories::empty(),
            )
            .unwrap_or_else(|fault| panic!("{name}: {fault:?}"));
            assert_eq!(
                i64::from_le_bytes(interp.1[..8].try_into().unwrap()),
                expected as i64,
                "{name} interpreter status"
            );
            if let Some(native) = run_native(
                Arc::clone(&verified),
                |task| {
                    task.write_i64(0, crate::task::ORDERED_EMPTY_HANDLE);
                    task.write_i64(8, cursor_seed);
                    task.write_i64(16, 0);
                },
                ValueMemories::empty(),
            ) {
                assert_eq!(native.expect("native value probe"), interp, "{name}");
            }
        }
    }

    #[test]
    fn ordered_begin_probe_matches_across_public_executable_lanes() {
        use crate::task::OrderedOpStatus;

        // The disjoint empty ordered root begins a cursor: status Ok.
        // A handle naming no resident node is InvalidHandle. Both outcomes must
        // agree between the interpreter and the native lane.
        for (handle, expected) in [
            (crate::task::ORDERED_EMPTY_HANDLE, OrderedOpStatus::Ok),
            (5i64, OrderedOpStatus::InvalidHandle),
        ] {
            let verified = Arc::new(verify(ordered_begin_probe_program()));
            let interp = run_interpreter(
                &verified,
                |task| task.write_i64(0, handle),
                ValueMemories::empty(),
            )
            .expect("ordered begin probe runs");
            assert_eq!(interp.0, TaskStep::Done);
            assert_eq!(
                i64::from_le_bytes(interp.1[..8].try_into().unwrap()),
                expected as i64,
                "interpreter status for handle {handle}"
            );
            if let Some(native) = run_native(
                Arc::clone(&verified),
                |task| task.write_i64(0, handle),
                ValueMemories::empty(),
            ) {
                assert_eq!(native.expect("native ordered begin probe runs"), interp);
            }
        }
    }

    fn ordered_write_program() -> (Program, ProgramContract) {
        use crate::task::OrderedOpStatus;

        let status_ok = || Op::OrderedStatusIs {
            dst: 128,
            status: 40,
            expected: OrderedOpStatus::Ok,
        };
        let accumulate = || Op::MulI64 {
            dst: 120,
            a: 120,
            b: 128,
        };
        let code = vec![
            Op::OrderedEmpty {
                dst: 16,
                collection_schema_ref: 3,
            },
            Op::ConstI64 { dst: 120, value: 1 },
            Op::OrderedBeginInsert {
                cursor: 24,
                status: 40,
                collection: 16,
                collection_schema_ref: 3,
            },
            status_ok(),
            accumulate(),
            Op::OrderedInsertInspect {
                cursor: 24,
                present: 48,
                key: 56,
                status: 40,
                key_width: 8,
                collection_schema_ref: 3,
            },
            status_ok(),
            accumulate(),
            Op::OrderedInsertCommit {
                dst: 16,
                cursor: 24,
                key: 0,
                value: Some(8),
                status: 40,
                key_width: 8,
                value_width: 8,
                collection_schema_ref: 3,
                replace: false,
            },
            status_ok(),
            accumulate(),
            Op::OrderedLen {
                dst: 112,
                status: 40,
                collection: 16,
                collection_schema_ref: 3,
            },
            status_ok(),
            accumulate(),
            Op::ConstI64 { dst: 64, value: 1 },
            Op::EqI64 {
                dst: 128,
                a: 112,
                b: 64,
            },
            accumulate(),
            Op::OrderedBeginIterate {
                cursor: 80,
                status: 40,
                collection: 16,
                collection_schema_ref: 3,
            },
            status_ok(),
            accumulate(),
            Op::OrderedIterateRow {
                cursor: 80,
                present: 48,
                row: 96,
                status: 40,
                row_width: 16,
                collection_schema_ref: 3,
            },
            status_ok(),
            accumulate(),
            Op::EqI64 {
                dst: 128,
                a: 96,
                b: 0,
            },
            accumulate(),
            Op::EqI64 {
                dst: 128,
                a: 104,
                b: 8,
            },
            accumulate(),
            Op::MulI64 {
                dst: 120,
                a: 120,
                b: 48,
            },
            Op::Ret { src: 120, size: 8 },
        ];
        let collection = WordKind::Handle(SchemaRef(3));
        (
            Program {
                fns: vec![function(17, code)],
            },
            ProgramContract {
                functions: vec![function_contract(
                    17,
                    vec![
                        word_region(0, WordKind::Scalar),
                        word_region(8, WordKind::Scalar),
                        word_region(16, collection),
                        FrameRegion::new(
                            24,
                            RegionShape::new(vec![AllowedKinds::new(WordKind::Opaque); 2]),
                        ),
                        word_region(40, WordKind::Status),
                        word_region(48, WordKind::Scalar),
                        word_region(56, WordKind::Scalar),
                        word_region(64, WordKind::Scalar),
                        word_region(72, WordKind::Scalar),
                        FrameRegion::new(
                            80,
                            RegionShape::new(vec![AllowedKinds::new(WordKind::Opaque); 2]),
                        ),
                        FrameRegion::new(
                            96,
                            RegionShape::new(vec![
                                AllowedKinds::new(WordKind::Scalar),
                                AllowedKinds::new(WordKind::Scalar),
                            ]),
                        ),
                        word_region(112, WordKind::Scalar),
                        word_region(120, WordKind::Scalar),
                        word_region(128, WordKind::Scalar),
                    ],
                    &[0, 1],
                    12,
                    None,
                )],
                calls: vec![],
                schemas: ordered_map_schemas(),
                value_shapes: vec![],
            },
        )
    }

    fn run_public_ordered_write(force_interpreter: bool) -> (i64, LaneFacts) {
        let previous = std::env::var_os("WEAVY_JIT");
        if force_interpreter {
            unsafe { std::env::set_var("WEAVY_JIT", "0") };
        } else {
            unsafe { std::env::remove_var("WEAVY_JIT") };
        }
        let executable = Rc::new(Executable::new(verify(ordered_write_program())));
        match previous {
            Some(value) => unsafe { std::env::set_var("WEAVY_JIT", value) },
            None => unsafe { std::env::remove_var("WEAVY_JIT") },
        }
        let facts = executable.lane_facts();
        let mut task = executable.spawn(FnId(0)).expect("ordered task spawns");
        task.write_entry_i64(0, 7).expect("key entry");
        task.write_entry_i64(1, 70).expect("value entry");
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        (task.result_i64().expect("scalar result"), facts)
    }

    #[test]
    fn ordered_write_iteration_and_length_match_across_public_lanes() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let interpreter = run_public_ordered_write(true);
        let native = run_public_ordered_write(false);
        assert_eq!(interpreter.0, 1);
        assert_eq!(native.0, interpreter.0);
        assert_eq!(interpreter.1.selected, LaneKind::Interpreter);
        if task_lane::available() {
            assert_eq!(native.1.selected, LaneKind::Native);
        }
    }

    #[test]
    fn invalid_enum_selectors_and_projection_mismatches_fault_equivalently() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for (name, op, selector, expected_pc) in [
            (
                "invalid selector",
                Op::EnumIsVariant {
                    dst: RegionId(1),
                    value: RegionId(0),
                    variant: 0,
                },
                7i64,
                0usize,
            ),
            (
                "projection mismatch",
                Op::EnumProjectChecked {
                    dst: RegionId(1),
                    value: RegionId(0),
                    variant: 0,
                    field: 0,
                },
                1i64,
                0usize,
            ),
        ] {
            let interpreter = run_public_fault(verify(enum_program(op.clone())), selector, true);
            let native = run_public_fault(verify(enum_program(op.clone())), selector, false);
            assert_eq!(interpreter, native, "{name}");
            let site = match &interpreter.0 {
                TaskFault::InvalidEnumSelector {
                    site,
                    value_shape,
                    expected,
                    actual,
                } => {
                    assert_eq!(*value_shape, ValueShapeRef(0));
                    assert_eq!(expected, &[0, 1]);
                    assert_eq!(*actual, selector);
                    site
                }
                TaskFault::EnumProjectionMismatch {
                    site,
                    value_shape,
                    expected,
                    actual,
                } => {
                    assert_eq!(*value_shape, ValueShapeRef(0));
                    assert_eq!(*expected, 0);
                    assert_eq!(*actual, selector);
                    site
                }
                fault => panic!("unexpected {name} fault: {fault:?}"),
            };
            assert_eq!(site.function, FnId(0));
            assert_eq!(site.pc, expected_pc);
            assert_eq!(site.op.as_ref(), &op);
            assert_eq!(interpreter.1, vec![TaskEvent::FrameEntered(FnId(0))]);
            assert!(matches!(interpreter.2, TaskFault::PoisonedReDrive { .. }));
        }
    }

    fn run_interpreter(
        verified: &VerifiedProgram,
        seed: impl FnOnce(&mut Task),
        value_memories: ValueMemories<'_>,
    ) -> LaneRun {
        let mut task = Task::spawn_with_mode(verified.program(), FnId(0), TraceMode::Innards);
        seed(&mut task);
        let step =
            task.run_verified_with_value_memories(verified, &mut [], &[], &mut [], value_memories)?;
        Ok((step, task.result, task.trace))
    }

    fn run_native(
        verified: Arc<VerifiedProgram>,
        seed: impl FnOnce(&mut JitTask),
        value_memories: ValueMemories<'_>,
    ) -> Option<LaneRun> {
        let jit = JitExecutable::compile(verified, TraceMode::Innards)?;
        let mut task = JitTask::spawn_verified(&jit, FnId(0));
        seed(&mut task);
        Some(
            task.run_verified_with_value_memories(&jit, &mut [], &[], &mut [], value_memories)
                .map(|step| (step, task.result, task.trace)),
        )
    }

    #[test]
    fn boxed_environment_threads_and_faults_identically_across_lanes() {
        let scalar = || RegionShape::word(WordKind::Scalar);
        let env_program = |code: Vec<Op>| {
            let program = Program {
                fns: vec![function(4, code)],
            };
            let mut contract = function_contract(
                4,
                vec![
                    word_region(0, WordKind::Scalar),  // dst / projected capture
                    word_region(8, WordKind::Scalar),  // environment handle
                    word_region(16, WordKind::Scalar), // captured source value
                    word_region(24, WordKind::Scalar), // argument entry
                ],
                &[3],
                0,
                None,
            );
            contract.environment = vec![FrameRegion::new(0, scalar())];
            (
                program,
                ProgramContract {
                    functions: vec![contract],
                    calls: vec![],
                    schemas: vec![],
                    value_shapes: vec![],
                },
            )
        };

        // EnvBox -> EnvLoad round trip: box a captured value, project it back,
        // and return it. Both lanes reach the same env arena and agree.
        let success = env_program(vec![
            Op::ConstI64 { dst: 16, value: 42 },
            Op::EnvBox {
                dst: RegionId(1),
                callee: FnId(0),
                fields: vec![RegionId(2)],
            },
            Op::EnvLoad {
                dst: RegionId(0),
                env: RegionId(1),
                callee: FnId(0),
                field: 0,
            },
            Op::Ret { src: 0, size: 8 },
        ]);
        let verified = Arc::new(verify(success));
        let interp = run_interpreter(&verified, |_| {}, ValueMemories::empty())
            .expect("interpreter runs the boxed round trip");
        assert_eq!(interp.0, TaskStep::Done);
        assert_eq!(i64::from_le_bytes(interp.1[..8].try_into().unwrap()), 42);
        if let Some(native) = run_native(Arc::clone(&verified), |_| {}, ValueMemories::empty()) {
            assert_eq!(
                native.expect("native runs the boxed round trip"),
                interp,
                "boxed environment round trip differs across lanes",
            );
        }

        // A fabricated environment handle fails closed with the same typed fault
        // and site in both lanes.
        let stale = env_program(vec![
            Op::ConstI64 { dst: 8, value: 0 },
            Op::EnvLoad {
                dst: RegionId(0),
                env: RegionId(1),
                callee: FnId(0),
                field: 0,
            },
            Op::Ret { src: 0, size: 8 },
        ]);
        let verified = Arc::new(verify(stale));
        let interp_fault = run_interpreter(&verified, |_| {}, ValueMemories::empty())
            .expect_err("a fabricated environment handle faults");
        assert!(matches!(
            interp_fault,
            TaskFault::Environment {
                kind: EnvironmentFaultKind::Stale,
                ..
            }
        ));
        if let Some(native) = run_native(Arc::clone(&verified), |_| {}, ValueMemories::empty()) {
            assert_eq!(
                native.expect_err("native fabricated handle faults"),
                interp_fault,
                "boxed environment fault differs across lanes",
            );
        }
    }

    #[test]
    fn public_executable_runs_verified_program_and_caches_native_compile() {
        task_lane::reset_jit_program_compile_count();
        let executable = Rc::new(Executable::new(verify(scalar_add_program())));
        let mut task = executable.spawn(FnId(0)).expect("entry shape");
        task.write_entry_i64(0, 20).unwrap();
        task.write_entry_i64(1, 22).unwrap();

        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.result_i64(), Ok(42));
        assert_eq!(
            task.trace(),
            &[
                TaskEvent::FrameEntered(FnId(0)),
                TaskEvent::Mark(77),
                TaskEvent::FrameExited(FnId(0)),
            ]
        );
        if executable.lane_facts().native_compiled {
            assert!(task_lane::jit_program_compile_count() >= 1);
            let mut second = executable.spawn(FnId(0)).expect("entry shape");
            second.write_entry_i64(0, 1).unwrap();
            second.write_entry_i64(1, 2).unwrap();
            assert_eq!(second.drive(&mut [], &[]), Ok(TaskStep::Done));
            assert!(task_lane::jit_program_compile_count() >= 1);
        }
    }

    #[test]
    fn drive_table_lengths_fault_before_execution() {
        let executable = Rc::new(Executable::new(verify(awaiting_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();

        let fault = task.drive(&mut [false], &[0, 0]).unwrap_err();
        assert_eq!(
            fault,
            TaskFault::DriveTableLength {
                table: DriveTable::Ready,
                expected: 2,
                actual: 1,
            }
        );
        assert!(matches!(
            task.drive(&mut [false, false], &[0, 0]),
            Err(TaskFault::PoisonedReDrive { .. })
        ));

        let mut task = executable.spawn(FnId(0)).unwrap();
        assert_eq!(
            task.drive(&mut [false, false], &[0]).unwrap_err(),
            TaskFault::DriveTableLength {
                table: DriveTable::Awaited,
                expected: 2,
                actual: 1,
            }
        );
    }

    #[test]
    fn public_entry_accessor_rejects_non_scalar_entries() {
        let executable = Rc::new(Executable::new(verify(non_scalar_entry_program())));
        assert!(matches!(
            executable.spawn(FnId(0)),
            Err(TaskFault::InvalidEntryShape {
                entry: FnId(0),
                index: 0,
                region: RegionId(0),
            })
        ));
    }

    #[test]
    fn public_spawn_rejects_callable_entry_until_typed_writer_exists() {
        let executable = Rc::new(Executable::new(verify(callable_entry_program())));
        let Err(fault) = executable.spawn(FnId(0)) else {
            panic!("callable entry must be rejected until it has a typed writer");
        };
        assert_eq!(
            fault,
            TaskFault::InvalidEntryShape {
                entry: FnId(0),
                index: 0,
                region: RegionId(0),
            }
        );
    }

    #[test]
    fn public_spawn_accepts_structural_entry_for_typed_writer() {
        let executable = Rc::new(Executable::new(verify(structural_entry_program(
            Op::EnumIsVariant {
                dst: RegionId(1),
                value: RegionId(0),
                variant: 0,
            },
        ))));
        let mut task = executable
            .spawn(FnId(0))
            .expect("structural entry is admitted");
        task.write_entry_frozen(
            0,
            &FrozenValue::Inline(FrozenInlineValue::new(
                [0_i64.to_le_bytes(), 7_i64.to_le_bytes()].concat(),
            )),
        )
        .expect("typed writer materializes the verified enum shape");
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.result_i64(), Ok(1));
    }

    #[test]
    fn public_entry_writer_reports_out_of_range_index_without_fake_region() {
        let executable = Rc::new(Executable::new(verify(scalar_identity_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();

        assert_eq!(
            task.write_entry_i64(1, 7),
            Err(TaskFault::InvalidEntryIndex {
                entry: FnId(0),
                index: 1,
                entry_count: 1,
            })
        );
        task.write_entry_i64(0, 7).unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.result_i64(), Ok(7));
    }

    #[test]
    fn public_spawn_rejects_unknown_entry_function() {
        let executable = Rc::new(Executable::new(verify(scalar_add_program())));
        let Err(fault) = executable.spawn(FnId(99)) else {
            panic!("unknown entry function must fault");
        };
        assert_eq!(
            fault,
            TaskFault::InvalidEntryFunction {
                entry: FnId(99),
                function_count: 1,
            }
        );
    }

    #[test]
    fn result_accessor_rejects_non_scalar_result_shape() {
        let executable = Rc::new(Executable::new(verify(non_scalar_result_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert!(matches!(
            task.result_i64(),
            Err(TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: RegionId(0),
                size: 8,
            })
        ));
    }

    #[test]
    fn result_before_first_drive_faults_typed() {
        let executable = Rc::new(Executable::new(verify(scalar_identity_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_i64(0, 7).unwrap();

        assert_eq!(task.state(), ExecTaskState::NotStarted);
        assert_eq!(
            task.result_i64(),
            Err(TaskFault::ResultBeforeDone {
                state: ExecTaskState::NotStarted,
            })
        );
        assert!(matches!(
            task.result(),
            Err(TaskFault::ResultBeforeDone {
                state: ExecTaskState::NotStarted,
            })
        ));
    }

    #[test]
    fn result_after_parked_faults_typed() {
        let executable = Rc::new(Executable::new(verify(entry_then_await_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_i64(0, 5).unwrap();

        assert_eq!(
            task.drive(&mut [false], &[0]),
            Ok(TaskStep::Parked { input: 0 })
        );
        assert_eq!(task.state(), ExecTaskState::Parked { input: 0 });
        assert_eq!(
            task.result_i64(),
            Err(TaskFault::ResultBeforeDone {
                state: ExecTaskState::Parked { input: 0 },
            })
        );
    }

    #[test]
    fn suspended_task_is_retained_off_stack_and_resumes_at_frame_state() {
        // FV-D1D L2.0 ownership contract: because a task owns its executable
        // through an Arc, it is `'static` and can be moved into a registry off
        // the drive stack while its frame is suspended, then resumed later — no
        // borrow of any transient lowered value keeps it alive.
        let mut registry: Vec<ExecTask> = Vec::new();
        {
            let executable = Rc::new(Executable::new(verify(entry_then_await_program())));
            let mut task = executable.spawn(FnId(0)).unwrap();
            task.write_entry_i64(0, 5).unwrap();
            assert_eq!(
                task.drive(&mut [false], &[0]),
                Ok(TaskStep::Parked { input: 0 })
            );
            // Retain the suspended frame off the stack; the local `executable`
            // Arc is dropped at scope end, and the task keeps the executable
            // alive through its own owned handle.
            registry.push(task);
        }
        let mut task = registry.pop().expect("the suspended task was retained");
        assert_eq!(task.state(), ExecTaskState::Parked { input: 0 });
        // Resume: deliver the awaited completion. The frame continues at its
        // exact parked PC/register state and returns the resumed value.
        assert_eq!(task.drive(&mut [true], &[37]), Ok(TaskStep::Done));
        assert_eq!(task.state(), ExecTaskState::Done);
        assert_eq!(task.result_i64(), Ok(37));
    }

    #[test]
    fn result_after_done_is_available_and_redrive_faults_typed() {
        let executable = Rc::new(Executable::new(verify(scalar_identity_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_i64(0, 7).unwrap();

        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.state(), ExecTaskState::Done);
        assert_eq!(task.result_i64(), Ok(7));
        assert_eq!(
            task.write_entry_i64(0, 9),
            Err(TaskFault::EntryWriteAfterDrive {
                entry: FnId(0),
                index: 0,
                region: RegionId(0),
            })
        );

        assert_eq!(task.drive(&mut [], &[]), Err(TaskFault::DriveAfterDone));
        assert_eq!(task.state(), ExecTaskState::Done);
        assert_eq!(task.result_i64(), Ok(7));
        let expected = 7_i64.to_le_bytes();
        assert_eq!(task.result(), Ok(expected.as_slice()));
    }

    #[test]
    fn poisoned_result_precedes_incomplete_state_fault() {
        let executable = Rc::new(Executable::new(verify(awaiting_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();

        assert_eq!(
            task.drive(&mut [false], &[0, 0]),
            Err(TaskFault::DriveTableLength {
                table: DriveTable::Ready,
                expected: 2,
                actual: 1,
            })
        );
        assert_eq!(task.state(), ExecTaskState::NotStarted);
        assert!(matches!(
            task.result_i64(),
            Err(TaskFault::PoisonedResult { .. })
        ));
    }

    #[test]
    fn drive_faults_when_declared_entry_was_not_written() {
        let executable = Rc::new(Executable::new(verify(scalar_add_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_i64(0, 20).unwrap();

        assert!(matches!(
            task.drive(&mut [], &[]),
            Err(TaskFault::EntryMissing {
                entry: FnId(0),
                index: 1,
                region: RegionId(1),
                kind: WordKind::Scalar,
            })
        ));
        assert!(matches!(
            task.write_entry_i64(1, 22),
            Err(TaskFault::PoisonedReDrive { .. })
        ));
    }

    #[test]
    fn duplicate_entry_write_faults_without_mutating() {
        let executable = Rc::new(Executable::new(verify(scalar_identity_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_i64(0, 7).unwrap();
        assert_eq!(
            task.write_entry_i64(0, 9),
            Err(TaskFault::EntryAlreadyInitialized {
                entry: FnId(0),
                index: 0,
                region: RegionId(0),
            })
        );

        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.result_i64(), Ok(7));
    }

    #[test]
    fn wrong_entry_writer_faults_without_mutating_or_initializing() {
        let schema = SchemaRef(0);
        let handle = StoreHandle::new(7).unwrap();
        let executable = Rc::new(Executable::new(verify(mixed_scalar_handle_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();

        assert_eq!(
            task.write_entry_store_handle(0, schema, handle),
            Err(TaskFault::EntryKindMismatch {
                entry: FnId(0),
                index: 0,
                region: RegionId(0),
                expected: EntryWriteKind::StoreHandle(schema),
                actual: WordKind::Scalar,
            })
        );
        assert_eq!(
            task.write_entry_i64(1, 99),
            Err(TaskFault::EntryKindMismatch {
                entry: FnId(0),
                index: 1,
                region: RegionId(1),
                expected: EntryWriteKind::Scalar,
                actual: WordKind::Handle(schema),
            })
        );

        task.write_entry_i64(0, 42).unwrap();
        task.write_entry_store_handle(1, schema, handle).unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.result_i64(), Ok(42));
    }

    #[test]
    fn mixed_scalar_and_handle_entries_initialize_completely() {
        let schema = SchemaRef(0);
        let executable = Rc::new(Executable::new(verify(mixed_scalar_handle_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();

        task.write_entry_i64(0, 42).unwrap();
        task.write_entry_store_handle(1, schema, StoreHandle::new(0).unwrap())
            .unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.result_i64(), Ok(42));
    }

    #[test]
    fn entry_writers_close_after_any_drive_attempt() {
        let executable = Rc::new(Executable::new(verify(scalar_identity_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_i64(0, 7).unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(
            task.write_entry_i64(0, 9),
            Err(TaskFault::EntryWriteAfterDrive {
                entry: FnId(0),
                index: 0,
                region: RegionId(0),
            })
        );

        let executable = Rc::new(Executable::new(verify(entry_then_await_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_i64(0, 5).unwrap();
        assert_eq!(
            task.drive(&mut [false], &[0]),
            Ok(TaskStep::Parked { input: 0 })
        );
        assert_eq!(
            task.write_entry_i64(0, 6),
            Err(TaskFault::EntryWriteAfterDrive {
                entry: FnId(0),
                index: 0,
                region: RegionId(0),
            })
        );
    }

    #[test]
    fn public_executable_reports_indirect_faults_and_poisons() {
        let oversized = i64::from(u32::MAX) + 1;
        let cases: [(i64, &str, FaultPredicate); 4] = [
            (-1, "negative", |fault: &TaskFault| {
                matches!(
                    fault,
                    TaskFault::IndirectCalleeNegative {
                        value: -1,
                        site: FaultSite {
                            function: FnId(0),
                            pc: 0,
                            ..
                        },
                    }
                )
            }),
            (oversized, "oversized", |fault: &TaskFault| {
                matches!(
                    fault,
                    TaskFault::IndirectCalleeOutOfRange {
                        callee,
                        function_count: 3,
                        site: FaultSite {
                            function: FnId(0),
                            pc: 0,
                            ..
                        },
                    } if *callee == i64::from(u32::MAX) + 1
                )
            }),
            (99, "range", |fault: &TaskFault| {
                matches!(
                    fault,
                    TaskFault::IndirectCalleeOutOfRange {
                        callee: 99,
                        function_count: 3,
                        site: FaultSite {
                            function: FnId(0),
                            pc: 0,
                            ..
                        },
                    }
                )
            }),
            (2, "contract", |fault: &TaskFault| {
                matches!(
                    fault,
                    TaskFault::IndirectCalleeContractMismatch {
                        callee: FnId(2),
                        expected: CallContractId(0),
                        actual: Some(CallContractId(1)),
                        site: FaultSite {
                            function: FnId(0),
                            pc: 0,
                            ..
                        },
                    }
                )
            }),
        ];

        for (callee, name, matches_expected) in cases {
            let executable = Rc::new(Executable::new(verify(indirect_program())));
            let mut task = executable.spawn(FnId(0)).unwrap();
            task.adversarial_write_word_at_offset_for_test(0, callee);
            task.write_entry_i64(0, 21).unwrap();
            let fault = task.drive(&mut [], &[]).expect_err(name);
            assert!(matches_expected(&fault), "{name}: {fault:?}");
            assert!(matches!(
                task.drive(&mut [], &[]),
                Err(TaskFault::PoisonedReDrive { .. })
            ));
            assert!(matches!(
                task.result_i64(),
                Err(TaskFault::PoisonedResult { .. })
            ));
        }
    }

    #[test]
    fn public_executable_reports_unresident_compare_and_hides_result() {
        let store = [ValueMemory::empty()];
        let memories = ValueMemories {
            store: &store,
            molten: &[],
        };
        let executable = Rc::new(Executable::new(verify(compare_program())));
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_store_handle(0, SchemaRef(0), StoreHandle::new(0).unwrap())
            .unwrap();
        task.write_entry_store_handle(1, SchemaRef(0), StoreHandle::new(0).unwrap())
            .unwrap();
        let fault = task
            .drive_hosted_with_value_memories(&mut [], &[], &mut [], memories)
            .expect_err("equal unresident compare");
        assert!(matches!(
            fault,
            TaskFault::UnresidentCompareValueBytes {
                side: CompareSide::Left,
                handle: 0,
                site: FaultSite {
                    function: FnId(0),
                    pc: 0,
                    ..
                },
            }
        ));
        assert!(matches!(
            task.drive(&mut [], &[]),
            Err(TaskFault::PoisonedReDrive { .. })
        ));
        assert!(matches!(
            task.result(),
            Err(TaskFault::PoisonedResult { .. })
        ));
    }

    #[test]
    fn missing_indirect_call_facts_fault_instead_of_skipping_contract() {
        let mut verified = verify(indirect_program());
        verified.clear_call_facts_for_test(FnId(0), 0);
        let verified = Arc::new(verified);
        let interp = run_interpreter(
            &verified,
            |task| {
                task.write_i64(0, 1);
                task.write_i64(8, 21);
            },
            ValueMemories::empty(),
        )
        .expect_err("missing call facts");
        assert!(matches!(
            interp,
            TaskFault::MissingIndirectCallFacts {
                site: FaultSite {
                    function: FnId(0),
                    pc: 0,
                    call: None,
                    ..
                }
            }
        ));
        if let Some(native) = run_native(
            Arc::clone(&verified),
            |task| {
                task.write_i64(0, 1);
                task.write_i64(8, 21);
            },
            ValueMemories::empty(),
        ) {
            assert_eq!(native.expect_err("missing call facts"), interp);
        }
    }

    #[test]
    fn interpreter_and_native_match_results_steps_traces_and_faults() {
        let verified = Arc::new(verify(indirect_program()));
        let interp = run_interpreter(
            &verified,
            |task| {
                task.write_i64(0, 1);
                task.write_i64(8, 21);
            },
            ValueMemories::empty(),
        )
        .unwrap();
        if let Some(native) = run_native(
            Arc::clone(&verified),
            |task| {
                task.write_i64(0, 1);
                task.write_i64(8, 21);
            },
            ValueMemories::empty(),
        ) {
            assert_eq!(native.unwrap(), interp);
        }

        let fault_cases = [
            (-1, "negative"),
            (i64::from(u32::MAX) + 1, "oversized"),
            (99, "range"),
            (2, "contract"),
        ];
        for (callee, name) in fault_cases {
            let verified = Arc::new(verify(indirect_program()));
            let interp = run_interpreter(
                &verified,
                |task| {
                    task.write_i64(0, callee);
                    task.write_i64(8, 21);
                },
                ValueMemories::empty(),
            )
            .expect_err(name);
            if let Some(native) = run_native(
                Arc::clone(&verified),
                |task| {
                    task.write_i64(0, callee);
                    task.write_i64(8, 21);
                },
                ValueMemories::empty(),
            ) {
                assert_eq!(native.expect_err(name), interp, "{name}");
            }
        }

        let verified = Arc::new(verify(compare_program()));
        let store = [ValueMemory::from_slice(b"left"), ValueMemory::empty()];
        let memories = ValueMemories {
            store: &store,
            molten: &[],
        };
        let interp = run_interpreter(
            &verified,
            |task| {
                task.write_i64(0, 0);
                task.write_i64(8, 1);
            },
            memories,
        )
        .expect_err("unresident compare");
        assert!(matches!(
            interp,
            TaskFault::UnresidentCompareValueBytes {
                side: CompareSide::Right,
                handle: 1,
                ..
            }
        ));
        if let Some(native) = run_native(
            Arc::clone(&verified),
            |task| {
                task.write_i64(0, 0);
                task.write_i64(8, 1);
            },
            memories,
        ) {
            assert_eq!(native.expect_err("unresident compare"), interp);
        }

        let verified = Arc::new(verify(compare_program()));
        let store = [ValueMemory::empty()];
        let memories = ValueMemories {
            store: &store,
            molten: &[],
        };
        let interp = run_interpreter(
            &verified,
            |task| {
                task.write_i64(0, 0);
                task.write_i64(8, 0);
            },
            memories,
        )
        .expect_err("equal unresident compare");
        assert!(matches!(
            interp,
            TaskFault::UnresidentCompareValueBytes {
                side: CompareSide::Left,
                handle: 0,
                ..
            }
        ));
        if let Some(native) = run_native(
            Arc::clone(&verified),
            |task| {
                task.write_i64(0, 0);
                task.write_i64(8, 0);
            },
            memories,
        ) {
            assert_eq!(native.expect_err("equal unresident compare"), interp);
        }
    }

    #[test]
    fn string_concat_result_feeds_concat_and_compare_across_lanes() {
        let verified = Arc::new(verify(string_concat_program()));
        let store = [
            ValueMemory::from_slice(b"a"),
            ValueMemory::from_slice(b"b"),
            ValueMemory::from_slice(b"c"),
            ValueMemory::from_slice(b"abc"),
        ];
        let memories = ValueMemories {
            store: &store,
            molten: &[],
        };
        let seed = |task: &mut Task| {
            task.write_i64(0, 0);
            task.write_i64(8, 1);
            task.write_i64(16, 2);
            task.write_i64(24, 3);
        };
        let interp = run_interpreter(&verified, seed, memories).expect("string concat runs");
        // The nested concatenation equals the interned "abc": ordinal 1 = equal.
        assert_eq!(interp.1, 1i64.to_le_bytes().to_vec());
        if let Some(native) = run_native(
            Arc::clone(&verified),
            |task: &mut JitTask| {
                task.write_i64(0, 0);
                task.write_i64(8, 1);
                task.write_i64(16, 2);
                task.write_i64(24, 3);
            },
            memories,
        ) {
            assert_eq!(native.expect("string concat runs on native"), interp);
        }

        // A non-resident operand faults with the precise side on both lanes.
        let store = [
            ValueMemory::from_slice(b"a"),
            ValueMemory::empty(),
            ValueMemory::from_slice(b"c"),
            ValueMemory::from_slice(b"abc"),
        ];
        let memories = ValueMemories {
            store: &store,
            molten: &[],
        };
        let interp = run_interpreter(
            &verified,
            |task: &mut Task| {
                task.write_i64(0, 0);
                task.write_i64(8, 1);
                task.write_i64(16, 2);
                task.write_i64(24, 3);
            },
            memories,
        )
        .expect_err("unresident string concat operand");
        assert!(matches!(
            interp,
            TaskFault::UnresidentStringConcatOperand {
                side: CompareSide::Right,
                handle: 1,
                ..
            }
        ));
        if let Some(native) = run_native(
            Arc::clone(&verified),
            |task: &mut JitTask| {
                task.write_i64(0, 0);
                task.write_i64(8, 1);
                task.write_i64(16, 2);
                task.write_i64(24, 3);
            },
            memories,
        ) {
            assert_eq!(
                native.expect_err("unresident string concat operand"),
                interp
            );
        }
    }

    #[test]
    fn string_byte_operations_preserve_unresident_faults_across_lanes() {
        let cases = [
            (
                "contains-left",
                Op::StringContains {
                    dst: 32,
                    text: 0,
                    needle: 8,
                },
                1,
                CompareSide::Left,
            ),
            (
                "contains-right",
                Op::StringContains {
                    dst: 32,
                    text: 0,
                    needle: 8,
                },
                1,
                CompareSide::Right,
            ),
            (
                "split-text",
                Op::StringSplitOnce {
                    left: 16,
                    right: 16,
                    status: 24,
                    text: 0,
                    delimiter: 8,
                },
                1,
                CompareSide::Left,
            ),
            (
                "split-delimiter",
                Op::StringSplitOnce {
                    left: 16,
                    right: 16,
                    status: 24,
                    text: 0,
                    delimiter: 8,
                },
                1,
                CompareSide::Right,
            ),
            (
                "parse-text",
                Op::StringParseInt {
                    dst: 32,
                    status: 24,
                    text: 0,
                },
                1,
                CompareSide::Left,
            ),
        ];
        for (name, operation, bad, side) in cases {
            let verified = Arc::new(verify(string_operation_program(operation)));
            let store = [ValueMemory::from_slice(b"a"), ValueMemory::empty()];
            let memories = ValueMemories {
                store: &store,
                molten: &[],
            };
            let text = if side == CompareSide::Left { 1 } else { 0 };
            let needle = if side == CompareSide::Right { 1 } else { 0 };
            let interp = run_interpreter(
                &verified,
                |task| {
                    task.write_i64(0, text);
                    task.write_i64(8, needle);
                },
                memories,
            )
            .expect_err(name);
            assert!(
                matches!(interp, TaskFault::UnresidentStringConcatOperand { side: actual, handle, site: FaultSite { function: FnId(0), pc: 0, .. } } if actual == side && handle == bad)
            );
            if let Some(native) = run_native(
                Arc::clone(&verified),
                |task| {
                    task.write_i64(0, text);
                    task.write_i64(8, needle);
                },
                memories,
            ) {
                assert_eq!(native.expect_err(name), interp);
            }
        }
    }

    #[test]
    fn byte_project_copies_across_distinct_schemas_across_lanes() {
        let verified = Arc::new(verify(byte_project_program()));
        let store = [
            ValueMemory::from_slice(b"crates/taxon/Cargo.toml"),
            ValueMemory::from_slice(b"crates/taxon/Cargo.toml"),
        ];
        let memories = ValueMemories {
            store: &store,
            molten: &[],
        };
        let interp = run_interpreter(
            &verified,
            |task: &mut Task| {
                task.write_i64(0, 0);
                task.write_i64(16, 1);
            },
            memories,
        )
        .expect("byte projection runs in the interpreter");
        assert_eq!(interp.1, 1i64.to_le_bytes().to_vec());
        let native = run_native(
            Arc::clone(&verified),
            |task: &mut JitTask| {
                task.write_i64(0, 0);
                task.write_i64(16, 1);
            },
            memories,
        );
        if cfg!(weavy_jit_active) {
            assert_eq!(
                native
                    .expect("native byte projection is available when the JIT is active")
                    .expect("byte projection runs natively"),
                interp,
            );
        } else {
            assert!(native.is_none(), "disabled JIT must not run a native lane");
        }
    }

    #[test]
    fn path_join_is_root_aware_and_faults_for_unresident_operands_across_lanes() {
        let verified = Arc::new(verify(path_join_program()));
        for (base, segment, expected) in [
            (b"".as_slice(), b"x".as_slice(), b"x".as_slice()),
            (b"a".as_slice(), b"x".as_slice(), b"a/x".as_slice()),
        ] {
            let store = [
                ValueMemory::from_slice(base),
                ValueMemory::from_slice(segment),
                ValueMemory::from_slice(expected),
            ];
            let memories = ValueMemories {
                store: &store,
                molten: &[],
            };
            let interp = run_interpreter(
                &verified,
                |task: &mut Task| {
                    task.write_i64(0, 0);
                    task.write_i64(8, 1);
                    task.write_i64(24, 2);
                },
                memories,
            )
            .expect("Path join runs in the interpreter");
            assert_eq!(interp.1, 1i64.to_le_bytes().to_vec());
            let native = run_native(
                Arc::clone(&verified),
                |task: &mut JitTask| {
                    task.write_i64(0, 0);
                    task.write_i64(8, 1);
                    task.write_i64(24, 2);
                },
                memories,
            );
            if cfg!(weavy_jit_active) {
                assert_eq!(
                    native
                        .expect("native Path join is available when the JIT is active")
                        .expect("Path join runs natively"),
                    interp,
                );
            } else {
                assert!(native.is_none(), "disabled JIT must not run a native lane");
            }
        }

        let store = [
            ValueMemory::empty(),
            ValueMemory::from_slice(b"x"),
            ValueMemory::from_slice(b"x"),
        ];
        let memories = ValueMemories {
            store: &store,
            molten: &[],
        };
        let interp = run_interpreter(
            &verified,
            |task: &mut Task| {
                task.write_i64(0, 0);
                task.write_i64(8, 1);
                task.write_i64(24, 2);
            },
            memories,
        )
        .expect_err("unresident Path join base faults");
        assert!(matches!(
            interp,
            TaskFault::UnresidentPathJoinOperand {
                side: CompareSide::Left,
                handle: 0,
                ..
            }
        ));
        let native = run_native(
            Arc::clone(&verified),
            |task: &mut JitTask| {
                task.write_i64(0, 0);
                task.write_i64(8, 1);
                task.write_i64(24, 2);
            },
            memories,
        );
        if cfg!(weavy_jit_active) {
            assert_eq!(
                native
                    .expect("native Path join is available when the JIT is active")
                    .expect_err("unresident Path join base faults natively"),
                interp,
            );
        } else {
            assert!(native.is_none(), "disabled JIT must not run a native lane");
        }
    }

    /// A generator-shaped program: entry `n` at word 0 drives control flow. It
    /// always publishes one descriptor `(n, marker)` under site `0xAAAA`, then
    /// publishes a second `(n, marker)` under site `0xBBBB` only on the
    /// nonzero-`n` path. The returned scalar is `n`. The record is a two-scalar
    /// product built with `ProductConstruct`, exactly as a real lowering would
    /// assemble a descriptor before publishing it.
    fn publication_program() -> (Program, ProgramContract) {
        let pair_shape = RegionShape::new(vec![
            AllowedKinds::new(WordKind::Scalar),
            AllowedKinds::new(WordKind::Scalar),
        ]);
        let value_shapes = vec![ValueShapeContract {
            shape: pair_shape.clone(),
            kind: ValueShapeKind::Product {
                fields: vec![
                    ValueFieldUse::new(0, RegionShape::word(WordKind::Scalar)),
                    ValueFieldUse::new(8, RegionShape::word(WordKind::Scalar)),
                ],
            },
        }];
        (
            Program {
                fns: vec![function(
                    5,
                    vec![
                        Op::ConstI64 { dst: 8, value: 7 },
                        Op::ProductConstruct {
                            dst: RegionId(2),
                            fields: vec![
                                StructuralFieldSource {
                                    field: 0,
                                    source: RegionId(0),
                                },
                                StructuralFieldSource {
                                    field: 1,
                                    source: RegionId(1),
                                },
                            ],
                        },
                        Op::Publish {
                            site: 0xAAAA,
                            record: 16,
                            record_width: 16,
                            record_schema_ref: 0,
                        },
                        Op::CopyI64 { dst: 32, src: 0 },
                        Op::JumpIfZero {
                            value: 0,
                            target: 6,
                        },
                        Op::Publish {
                            site: 0xBBBB,
                            record: 16,
                            record_width: 16,
                            record_schema_ref: 0,
                        },
                        Op::Ret { src: 32, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    5,
                    vec![
                        word_region(0, WordKind::Scalar),
                        word_region(8, WordKind::Scalar),
                        structural_region(16, pair_shape.clone(), ValueShapeRef(0)),
                        word_region(32, WordKind::Scalar),
                    ],
                    &[0],
                    3,
                    None,
                )],
                calls: vec![],
                schemas: vec![SchemaContract {
                    inline: pair_shape,
                    value_shape: Some(ValueShapeRef(0)),
                    payload: PayloadKind::PublicationRecord,
                }],
                value_shapes,
            },
        )
    }

    /// Drive `publication_program` with entry `n` on one lane, returning the
    /// completed log as `(site, schema_ref, bytes)` in publication order.
    type LanePublications = Vec<(u64, i64, Vec<u8>)>;

    fn interpreter_publications(verified: &VerifiedProgram, n: i64) -> LanePublications {
        let mut task = Task::spawn_with_mode(verified.program(), FnId(0), TraceMode::Innards);
        task.write_i64(0, n);
        let step = task
            .run_verified_with_value_memories(
                verified,
                &mut [],
                &[],
                &mut [],
                ValueMemories::empty(),
            )
            .expect("publication program runs");
        assert_eq!(step, TaskStep::Done);
        let log = task.publications();
        (0..log.len())
            .map(|index| {
                let (record, bytes) = log.get(index).expect("descriptor in range");
                (record.site, record.schema_ref, bytes.to_vec())
            })
            .collect()
    }

    fn native_publications(verified: Arc<VerifiedProgram>, n: i64) -> Option<LanePublications> {
        let jit = JitExecutable::compile(verified, TraceMode::Innards)?;
        let mut task = JitTask::spawn_verified(&jit, FnId(0));
        task.write_i64(0, n);
        let step = task
            .run_verified_with_value_memories(&jit, &mut [], &[], &mut [], ValueMemories::empty())
            .expect("publication program runs on native");
        assert_eq!(step, TaskStep::Done);
        let log = task.publications();
        Some(
            (0..log.len())
                .map(|index| {
                    let (record, bytes) = log.get(index).expect("descriptor in range");
                    (record.site, record.schema_ref, bytes.to_vec())
                })
                .collect(),
        )
    }

    #[test]
    fn public_publication_log_captures_branch_multiplicity_and_copies() {
        let executable = Rc::new(Executable::new(verify(publication_program())));

        // Nonzero n takes the second publish: two descriptors, each an exact
        // copy of the captured (n, marker) frame value.
        let mut task = executable.spawn(FnId(0)).expect("entry shape");
        task.write_entry_i64(0, 3).unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.result_i64(), Ok(3));
        assert_eq!(task.publication_count(), Ok(2));

        let first = task.publication(0).expect("first descriptor");
        assert_eq!(first.provenance_key(), 0xAAAA);
        assert_eq!(first.record_schema(), SchemaRef(0));
        assert_eq!(first.value_shape(), Some(ValueShapeRef(0)));
        let mut expected = 3i64.to_le_bytes().to_vec();
        expected.extend_from_slice(&7i64.to_le_bytes());
        assert_eq!(first.bytes(), expected.as_slice());
        assert_eq!(first.word(0), Some(3));
        assert_eq!(first.word(8), Some(7));
        assert_eq!(first.word(16), None);

        let second = task.publication(1).expect("second descriptor");
        assert_eq!(second.provenance_key(), 0xBBBB);
        assert_eq!(second.bytes(), expected.as_slice());

        assert!(matches!(
            task.publication(2),
            Err(TaskFault::PublicationIndexOutOfRange { index: 2, count: 2 })
        ));
    }

    #[test]
    fn public_publication_log_records_single_descriptor_on_zero_branch() {
        let executable = Rc::new(Executable::new(verify(publication_program())));
        let mut task = executable.spawn(FnId(0)).expect("entry shape");
        task.write_entry_i64(0, 0).unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.publication_count(), Ok(1));
        let only = task.publication(0).expect("only descriptor");
        assert_eq!(only.provenance_key(), 0xAAAA);
        assert_eq!(only.word(0), Some(0));
        assert_eq!(only.word(8), Some(7));
    }

    #[test]
    fn public_publication_log_is_empty_when_nothing_is_published() {
        let executable = Rc::new(Executable::new(verify(scalar_add_program())));
        let mut task = executable.spawn(FnId(0)).expect("entry shape");
        task.write_entry_i64(0, 20).unwrap();
        task.write_entry_i64(1, 22).unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.publication_count(), Ok(0));
        assert!(matches!(
            task.publication(0),
            Err(TaskFault::PublicationIndexOutOfRange { index: 0, count: 0 })
        ));
    }

    #[test]
    fn public_publication_preserves_result_lifecycle() {
        let executable = Rc::new(Executable::new(verify(publication_program())));

        // Before the task is done the log is not observable.
        let task = executable.spawn(FnId(0)).expect("entry shape");
        assert!(matches!(
            task.publication_count(),
            Err(TaskFault::ResultBeforeDone {
                state: ExecTaskState::NotStarted,
            })
        ));

        // A poisoned task surfaces its original fault through the log surface,
        // never a stale or partial log.
        let mut task = executable.spawn(FnId(0)).expect("entry shape");
        // Missing the required entry poisons the task on drive.
        let poison = task.drive(&mut [], &[]).expect_err("missing entry poisons");
        assert!(matches!(poison, TaskFault::EntryMissing { .. }));
        assert!(matches!(
            task.publication_count(),
            Err(TaskFault::PoisonedResult { .. })
        ));
        assert!(matches!(
            task.publication(0),
            Err(TaskFault::PoisonedResult { .. })
        ));
    }

    #[test]
    fn publication_log_matches_across_native_and_interpreter_lanes() {
        let verified = Arc::new(verify(publication_program()));
        for n in [0i64, 1, 5, -4] {
            let interp = interpreter_publications(&verified, n);
            // Site keys and captured bytes are exactly as published.
            let expected_first = {
                let mut bytes = n.to_le_bytes().to_vec();
                bytes.extend_from_slice(&7i64.to_le_bytes());
                bytes
            };
            assert_eq!(interp[0], (0xAAAA, 0, expected_first.clone()));
            assert_eq!(interp.len(), if n == 0 { 1 } else { 2 });
            if let Some(native) = native_publications(Arc::clone(&verified), n) {
                assert_eq!(native, interp, "lane publication logs diverged at n={n}");
            }
        }
    }

    #[test]
    fn environment_disabled_native_reports_fallback_fact() {
        let _guard = env_guard();
        let previous = std::env::var_os("WEAVY_JIT");
        unsafe { std::env::set_var("WEAVY_JIT", "0") };
        let executable = Rc::new(Executable::new(verify(scalar_add_program())));
        restore_env(previous);

        assert_eq!(executable.lane_facts().selected, LaneKind::Interpreter);
        assert_eq!(
            executable.lane_facts().fallback,
            Some(FallbackReason::DisabledByEnvironment)
        );
    }

    fn env_guard() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().expect("env lock")
    }

    fn restore_env(previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => unsafe { std::env::set_var("WEAVY_JIT", value) },
            None => unsafe { std::env::remove_var("WEAVY_JIT") },
        }
    }
}
