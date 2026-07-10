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

use std::sync::Arc;

use crate::jit::task_lane::{JitExecutable, JitTask};
use crate::task::{FnId, HostFn, Op, Task, TaskEvent, TaskStep, TraceMode, ValueMemories};
use crate::{
    CallContractId, CallSiteFacts, DriveRequirements, FrameRegion, FunctionContract, RegionId,
    SchemaRef, ValueShapeRef, VerifiedProgram, WordKind,
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

    fn as_i64(self) -> i64 {
        i64::try_from(self.index).expect("StoreHandle constructor checked i64 range")
    }
}

/// One dynamic fault location in a verified task program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaultSite {
    pub function: FnId,
    pub pc: usize,
    pub op: Op,
    pub call: Option<CallSiteFacts>,
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

    #[must_use]
    pub fn with_trace_mode(verified: VerifiedProgram, mode: TraceMode) -> Self {
        let verified = Arc::new(verified);
        let native_available = crate::jit::task_lane::available();
        let disabled = native_disabled_by_environment();
        let native = if native_available && !disabled {
            JitExecutable::compile(Arc::clone(&verified), mode)
        } else {
            None
        };
        let native_compiled = native.is_some();
        let fallback = if native_compiled {
            None
        } else if disabled {
            Some(FallbackReason::DisabledByEnvironment)
        } else {
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

    pub fn spawn(&self, entry: FnId) -> Result<ExecTask<'_>, TaskFault> {
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
            executable: self,
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

/// A running verified task bound to its [`Executable`].
pub struct ExecTask<'exec> {
    executable: &'exec Executable,
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

impl ExecTask<'_> {
    pub fn write_entry_value(&mut self, index: usize, bytes: &[u8]) -> Result<(), TaskFault> {
        self.check_not_poisoned()?;
        let function = self.executable.function(self.entry)?;
        let Some(region) = function.entries.get(index).copied() else {
            return Err(TaskFault::InvalidEntryIndex {
                entry: self.entry,
                index,
                entry_count: function.entries.len(),
            });
        };
        let region_contract = &function.frame.regions[region.0 as usize];
        let Some(value_shape) = region_contract.value_shape else {
            return Err(TaskFault::InvalidEntryShape {
                entry: self.entry,
                index,
                region,
            });
        };
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
        let expected = region_contract
            .shape
            .checked_byte_len()
            .unwrap_or(usize::MAX);
        if bytes.len() != expected {
            return Err(TaskFault::EntryValueSize {
                entry: self.entry,
                index,
                region,
                value_shape,
                expected,
                actual: bytes.len(),
            });
        }
        match &mut self.lane {
            Lane::Interpreter(task) => task.write_bytes(region_contract.offset, bytes),
            Lane::Native(task) => task.write_bytes(region_contract.offset, bytes),
        }
        self.entries_initialized[index] = true;
        Ok(())
    }

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

    pub fn result(&self) -> Result<&[u8], TaskFault> {
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
        op,
        call,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, MutexGuard};

    use super::*;
    use crate::jit::task_lane;
    use crate::mem::Layout;
    use crate::task::{ArgCopy, Fn, Program, StructuralFieldSource, ValueMemory};
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

    fn non_scalar_entry_program() -> (Program, ProgramContract) {
        let mut callable = AllowedKinds::new(WordKind::Callable(CallContractId(0)));
        callable = callable.allowing(WordKind::Opaque);
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
        let mut callable = AllowedKinds::new(WordKind::Callable(CallContractId(0)));
        callable = callable.allowing(WordKind::Opaque);
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
        let executable = Executable::new(program.verify(contract).unwrap());
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
                    &[0],
                    1,
                    None,
                )],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![value_shape],
            },
        )
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
        let executable = Executable::new(program.verify(contract).unwrap());
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
        bytes: &[u8],
        force_interpreter: bool,
    ) -> (TaskFault, Vec<TaskEvent>, TaskFault) {
        let previous = std::env::var_os("WEAVY_JIT");
        if force_interpreter {
            unsafe { std::env::set_var("WEAVY_JIT", "0") };
        } else {
            unsafe { std::env::remove_var("WEAVY_JIT") };
        }
        let executable = Executable::new(verified);
        match previous {
            Some(value) => unsafe { std::env::set_var("WEAVY_JIT", value) },
            None => unsafe { std::env::remove_var("WEAVY_JIT") },
        }
        let mut task = executable.spawn(FnId(0)).unwrap();
        task.write_entry_value(0, bytes).unwrap();
        let fault = task.drive(&mut [], &[]).unwrap_err();
        let trace = task.trace().to_vec();
        let poison = task.drive(&mut [], &[]).unwrap_err();
        (fault, trace, poison)
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
            let bytes = [selector.to_le_bytes(), 42i64.to_le_bytes()].concat();
            let interpreter = run_public_fault(verify(enum_program(op.clone())), &bytes, true);
            let native = run_public_fault(verify(enum_program(op.clone())), &bytes, false);
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
            assert_eq!(site.op, op);
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
    fn public_executable_runs_verified_program_and_caches_native_compile() {
        task_lane::reset_jit_program_compile_count();
        let executable = Executable::new(verify(scalar_add_program()));
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
            assert_eq!(task_lane::jit_program_compile_count(), 1);
            let mut second = executable.spawn(FnId(0)).expect("entry shape");
            second.write_entry_i64(0, 1).unwrap();
            second.write_entry_i64(1, 2).unwrap();
            assert_eq!(second.drive(&mut [], &[]), Ok(TaskStep::Done));
            assert_eq!(task_lane::jit_program_compile_count(), 1);
        }
    }

    #[test]
    fn drive_table_lengths_fault_before_execution() {
        let executable = Executable::new(verify(awaiting_program()));
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
        let executable = Executable::new(verify(non_scalar_entry_program()));
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
        let executable = Executable::new(verify(callable_entry_program()));
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
    fn public_entry_writer_reports_out_of_range_index_without_fake_region() {
        let executable = Executable::new(verify(scalar_identity_program()));
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
        let executable = Executable::new(verify(scalar_add_program()));
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
        let executable = Executable::new(verify(non_scalar_result_program()));
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
        let executable = Executable::new(verify(scalar_identity_program()));
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
        let executable = Executable::new(verify(entry_then_await_program()));
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
    fn result_after_done_is_available_and_redrive_faults_typed() {
        let executable = Executable::new(verify(scalar_identity_program()));
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
        let executable = Executable::new(verify(awaiting_program()));
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
        let executable = Executable::new(verify(scalar_add_program()));
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
        let executable = Executable::new(verify(scalar_identity_program()));
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
        let executable = Executable::new(verify(mixed_scalar_handle_program()));
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
        let executable = Executable::new(verify(mixed_scalar_handle_program()));
        let mut task = executable.spawn(FnId(0)).unwrap();

        task.write_entry_i64(0, 42).unwrap();
        task.write_entry_store_handle(1, schema, StoreHandle::new(0).unwrap())
            .unwrap();
        assert_eq!(task.drive(&mut [], &[]), Ok(TaskStep::Done));
        assert_eq!(task.result_i64(), Ok(42));
    }

    #[test]
    fn entry_writers_close_after_any_drive_attempt() {
        let executable = Executable::new(verify(scalar_identity_program()));
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

        let executable = Executable::new(verify(entry_then_await_program()));
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
            let executable = Executable::new(verify(indirect_program()));
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
        let executable = Executable::new(verify(compare_program()));
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
    fn environment_disabled_native_reports_fallback_fact() {
        let _guard = env_guard();
        let previous = std::env::var_os("WEAVY_JIT");
        unsafe { std::env::set_var("WEAVY_JIT", "0") };
        let executable = Executable::new(verify(scalar_add_program()));
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
