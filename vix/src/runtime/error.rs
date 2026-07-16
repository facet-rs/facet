use weavy::ProgramError;
use weavy::exec::TaskFault;
use weavy::task::FnId;

use crate::support::Span;
use crate::vir::{FunctionId, NodeId};

use super::{DemandKey, PrimitiveMachineError, ValueId};

/// The production machine failure plane. Language diagnostics and Vix Failure
/// values remain separate from this error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineError {
    pub operation: MachineOperation,
    pub subject: Option<ValueId>,
    pub attribution: Option<MachineAttribution>,
    pub demand_chain: Vec<DemandKey>,
    pub cause: MachineCause,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MachineCause {
    Program(Box<ProgramError>),
    Task(Box<TaskFault>),
    Runtime(RuntimeFault),
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MachineOperation {
    LoweringVerification,
    MemoRead,
    Spawn,
    EntryBinding,
    Drive,
    Result,
    DemandTransition,
    TaskTransition,
    TraceAttribution,
    /// A machine-plane primitive demand (tree projection, glob, fetch,
    /// extract) evaluated by the runtime effect plane.
    Effect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFault {
    MissingMemoStoreHandle,
    MissingConstantStoreHandle,
    MissingValueInputStoreHandle,
    ValueInputCardinality {
        expected: usize,
        actual: usize,
    },
    ValueInputSchemaMismatch,
    MissingDemandRecord {
        key: DemandKey,
    },
    MissingTaskRecord,
    PureIslandYielded,
    /// A verified `HostCallYield` reached the registered primitive boundary,
    /// but its typed call plan, request value, completion, or active-frame
    /// materialization violated the machine ABI.
    PrimitiveHost {
        detail: String,
    },
    /// A registered primitive completed with a typed transient machine error.
    /// This outcome is never a language failure and is not memoized.
    PrimitiveMachine {
        error: PrimitiveMachineError,
    },
    PureIslandParked {
        input: u32,
    },
    /// A wire forced a demand that is already being evaluated on the demand
    /// stack: a cyclic/re-entrant demand. The demand state machine detects it as
    /// a typed fault rather than recursing forever.
    ReentrantDemand {
        key: DemandKey,
    },
    /// The scheduler's runnable/parked loop went quiescent — no runnable task
    /// remains — while a submitted root demand is still unresolved. A stuck
    /// waiter graph is a machine invariant violation, never a language failure.
    QuiescentUnresolvedDemand {
        key: DemandKey,
    },
    /// The unified completion inbox was closed while a primitive or exec demand
    /// was still outstanding. A completion sender's death is a loud typed fault
    /// (`machine.scheduler.completion-resumes-direct`), never a swallowed
    /// disconnect error at a per-demand drain site.
    LostCompletion,
    MissingFrameAttribution {
        function: FnId,
    },
    MissingTraceAttribution {
        trace: u32,
    },
    ArrayMachineStatus {
        site: u32,
        status: weavy::task::ArrayOpStatus,
    },
    OrderedMachineStatus {
        site: u32,
        status: weavy::task::OrderedOpStatus,
    },
    LanguageFailurePending {
        site: u32,
        index: i64,
        length: i64,
    },
    /// The exec effect's host seam failed before the termination grammar could
    /// interpret anything: the capability's program could not be spawned or
    /// waited on. A machine/host fault, never a language failure — a process
    /// that RAN and exited nonzero is [`super::FailureValue::ProcessFailure`].
    EffectHostFailure {
        detail: String,
    },
    /// An effect island was evaluated whose output is not an effect demand, or
    /// whose inputs did not carry the capability the effect requires.
    MalformedEffectIsland,
    /// A machine invariant on the effect plane: a malformed effect island (an
    /// op the plane does not interpret, a missing operand, a store handle that
    /// vanished). Never a language failure — those are typed
    /// [`super::FailureValue`]s.
    EffectPlane {
        detail: &'static str,
    },
    UnsupportedEffectOperation {
        operation: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineAttribution {
    pub function: FunctionId,
    pub node: NodeId,
    pub span: Span,
    pub weavy_function: Option<FnId>,
    pub weavy_pc: Option<usize>,
}

impl MachineError {
    #[must_use]
    pub fn program(
        operation: MachineOperation,
        error: ProgramError,
        attribution: Option<MachineAttribution>,
        demand: DemandKey,
    ) -> Self {
        Self {
            operation,
            subject: None,
            attribution,
            demand_chain: vec![demand],
            cause: error.into(),
        }
    }

    #[must_use]
    pub fn task(
        operation: MachineOperation,
        error: TaskFault,
        attribution: Option<MachineAttribution>,
        demand: DemandKey,
    ) -> Self {
        Self {
            operation,
            subject: None,
            attribution,
            demand_chain: vec![demand],
            cause: error.into(),
        }
    }

    #[must_use]
    pub fn runtime(
        operation: MachineOperation,
        fault: RuntimeFault,
        attribution: Option<MachineAttribution>,
        demand: Option<DemandKey>,
    ) -> Self {
        Self {
            operation,
            subject: None,
            attribution,
            demand_chain: demand.into_iter().collect(),
            cause: MachineCause::Runtime(fault),
        }
    }
}

impl From<ProgramError> for MachineCause {
    fn from(error: ProgramError) -> Self {
        Self::Program(Box::new(error))
    }
}

impl From<TaskFault> for MachineCause {
    fn from(error: TaskFault) -> Self {
        Self::Task(Box::new(error))
    }
}
