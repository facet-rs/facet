use weavy::ProgramError;
use weavy::exec::TaskFault;
use weavy::task::FnId;

use crate::support::Span;
use crate::vir::{FunctionId, NodeId};

use super::{DemandKey, ValueId};

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
    Program(ProgramError),
    Task(TaskFault),
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFault {
    MissingMemoStoreHandle,
    MissingConstantStoreHandle,
    MissingDemandRecord { key: DemandKey },
    MissingTaskRecord,
    PureIslandYielded,
    PureIslandParked { input: u32 },
    MissingFrameAttribution { function: FnId },
    MissingTraceAttribution { trace: u32 },
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
    ) -> Self {
        Self {
            operation,
            subject: None,
            attribution,
            demand_chain: Vec::new(),
            cause: MachineCause::Program(error),
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
            cause: MachineCause::Task(error),
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

impl From<ProgramError> for MachineError {
    fn from(error: ProgramError) -> Self {
        Self::program(MachineOperation::LoweringVerification, error, None)
    }
}

impl From<TaskFault> for MachineError {
    fn from(error: TaskFault) -> Self {
        Self {
            operation: MachineOperation::Drive,
            subject: None,
            attribution: None,
            demand_chain: Vec::new(),
            cause: MachineCause::Task(error),
        }
    }
}
