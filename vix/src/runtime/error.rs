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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeFault {
    MissingMemoStoreHandle,
    MissingConstantStoreHandle,
    MissingDemandRecord {
        key: DemandKey,
    },
    MissingTaskRecord,
    PureIslandYielded,
    PureIslandParked {
        input: u32,
    },
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
    LanguageFailurePending {
        site: u32,
        index: i64,
        length: i64,
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
