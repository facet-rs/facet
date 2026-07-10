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

use crate::jit::task_lane::{JitProgram, JitTask};
use crate::task::{FnId, HostFn, Op, Task, TaskEvent, TaskStep, TraceMode, ValueMemories};
use crate::{
    CallContractId, CallSiteFacts, DriveRequirements, FunctionContract, RegionId, VerifiedProgram,
    WordKind,
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
        callee: u32,
        function_count: usize,
    },
    IndirectCalleeContractMismatch {
        site: FaultSite,
        callee: FnId,
        expected: CallContractId,
        actual: Option<CallContractId>,
    },
    UnresidentCompareValueBytes {
        site: FaultSite,
        side: CompareSide,
        handle: i64,
    },
    NativeFaultExit {
        site: FaultSite,
        code: i64,
    },
    PoisonedReDrive {
        original: Box<TaskFault>,
    },
}

/// A verified program prepared for execution.
pub struct Executable {
    verified: VerifiedProgram,
    native: Option<JitProgram>,
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
        let native_available = crate::jit::task_lane::available();
        let disabled = native_disabled_by_environment();
        let native = if native_available && !disabled {
            JitProgram::compile_with_mode(verified.program(), mode)
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
        let lane = match &self.native {
            Some(native) => Lane::Native(JitTask::spawn(native, entry)),
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
        })
    }

    fn validate_entry(&self, entry: FnId) -> Result<(), TaskFault> {
        let function = self.function(entry)?;
        for (index, region) in function.entries.iter().copied().enumerate() {
            let region_contract = &function.frame.regions[region.0 as usize];
            if region_contract.shape.words.len() != 1
                || !region_contract.shape.words[0].is_exactly(WordKind::Scalar)
            {
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
}

enum Lane {
    Interpreter(Task),
    Native(JitTask),
}

impl ExecTask<'_> {
    pub fn write_entry_i64(&mut self, index: usize, value: i64) -> Result<(), TaskFault> {
        self.check_not_poisoned()?;
        let function = self.executable.function(self.entry)?;
        let Some(region) = function.entries.get(index).copied() else {
            return Err(TaskFault::InvalidEntryShape {
                entry: self.entry,
                index,
                region: RegionId(u32::MAX),
            });
        };
        let region_contract = &function.frame.regions[region.0 as usize];
        if region_contract.shape.words.len() != 1
            || !region_contract.shape.words[0].is_exactly(WordKind::Scalar)
        {
            return Err(TaskFault::InvalidEntryShape {
                entry: self.entry,
                index,
                region,
            });
        }
        match &mut self.lane {
            Lane::Interpreter(task) => task.write_i64(region_contract.offset, value),
            Lane::Native(task) => task.write_i64(region_contract.offset, value),
        }
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
        check_drive_requirements(
            self.executable.verified.drive_requirements(),
            ready,
            awaited,
            hosts,
        )
        .map_err(|fault| self.poison(fault))?;

        let step = match (&self.executable.native, &mut self.lane) {
            (_, Lane::Interpreter(task)) => task.run_verified_with_value_memories(
                &self.executable.verified,
                ready,
                awaited,
                hosts,
                value_memories,
            ),
            (Some(native), Lane::Native(task)) => task.run_verified_with_value_memories(
                &self.executable.verified,
                native,
                ready,
                awaited,
                hosts,
                value_memories,
            ),
            (None, Lane::Native(_)) => unreachable!("native task exists only with native program"),
        };
        step.map_err(|fault| self.poison(fault))
    }

    #[must_use]
    pub fn trace(&self) -> &[TaskEvent] {
        match &self.lane {
            Lane::Interpreter(task) => &task.trace,
            Lane::Native(task) => &task.trace,
        }
    }

    #[must_use]
    pub fn result(&self) -> &[u8] {
        match &self.lane {
            Lane::Interpreter(task) => &task.result,
            Lane::Native(task) => &task.result,
        }
    }

    pub fn result_i64(&self) -> Result<i64, TaskFault> {
        self.executable.validate_result_i64(self.entry)?;
        let result = self.result();
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

    fn poison(&mut self, fault: TaskFault) -> TaskFault {
        self.poisoned = Some(fault.clone());
        fault
    }
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

pub(crate) fn fault_site(verified: &VerifiedProgram, function: FnId, pc: usize) -> FaultSite {
    let op = verified.program().fns[function.0 as usize].code[pc].clone();
    let call = verified
        .facts()
        .function(function)
        .and_then(|function| function.pc(pc))
        .and_then(|pc| pc.call());
    FaultSite {
        function,
        pc,
        op,
        call,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use super::*;
    use crate::jit::task_lane;
    use crate::mem::Layout;
    use crate::task::{ArgCopy, Fn, Program, ValueMemory};
    use crate::{
        AllowedKinds, CallContract, FrameContract, FrameRegion, FunctionContract, PayloadKind,
        ProgramContract, RegionShape, SchemaContract, SchemaRef,
    };

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    type LaneRun = Result<(TaskStep, Vec<u8>, Vec<TaskEvent>), TaskFault>;

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
                    function_contract(3, callable_regions(CallContractId(0)), &[], 2, None),
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
                    &[],
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
        verified: &VerifiedProgram,
        seed: impl FnOnce(&mut JitTask),
        value_memories: ValueMemories<'_>,
    ) -> Option<LaneRun> {
        let jit = JitProgram::compile_with_mode(verified.program(), TraceMode::Innards)?;
        let mut task = JitTask::spawn(&jit, FnId(0));
        seed(&mut task);
        Some(
            task.run_verified_with_value_memories(
                verified,
                &jit,
                &mut [],
                &[],
                &mut [],
                value_memories,
            )
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
    fn interpreter_and_native_match_results_steps_traces_and_faults() {
        let verified = verify(indirect_program());
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
            &verified,
            |task| {
                task.write_i64(0, 1);
                task.write_i64(8, 21);
            },
            ValueMemories::empty(),
        ) {
            assert_eq!(native.unwrap(), interp);
        }

        let fault_cases = [(-1, "negative"), (99, "range"), (2, "contract")];
        for (callee, name) in fault_cases {
            let verified = verify(indirect_program());
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
                &verified,
                |task| {
                    task.write_i64(0, callee);
                    task.write_i64(8, 21);
                },
                ValueMemories::empty(),
            ) {
                assert_eq!(native.expect_err(name), interp, "{name}");
            }
        }

        let verified = verify(compare_program());
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
            &verified,
            |task| {
                task.write_i64(0, 0);
                task.write_i64(8, 1);
            },
            memories,
        ) {
            assert_eq!(native.expect_err("unresident compare"), interp);
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
