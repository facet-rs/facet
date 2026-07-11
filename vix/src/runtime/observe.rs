use crate::diagnostic::DiagnosticCode;
use crate::vir::{FunctionId, IslandId, NodeId};

use super::MachineOperation;
use super::identity::{DemandKey, LocationId, ValueId};
use super::model::{DemandState, FailureValue, MemoVerdict, TaskId, TaskState};

#[derive(facet::Facet, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    pub memo_hits_exact: u64,
    pub memo_hits_projection: u64,
    pub memo_hits_semantic: u64,
    pub memo_misses: u64,
    pub memo_hit_allocations: u64,
    pub pure_host_calls: u64,
    pub store_interns: u64,
    pub store_dedups: u64,
    pub bytes_hashed: u64,
    pub effect_spawns: u64,
    pub scheduler_requests: u64,
    pub task_spawns: u64,
    pub task_discards: u64,
    pub native_task_spawns: u64,
    pub interpreter_task_spawns: u64,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SafePointClass {
    Edge,
    Poll,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecutionLaneFact {
    Interpreter,
    Native,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecutionFallbackFact {
    NativeUnavailable,
    DisabledByEnvironment,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionFacts {
    pub selected: ExecutionLaneFact,
    pub native_available: bool,
    pub native_compiled: bool,
    pub fallback: Option<ExecutionFallbackFact>,
}

/// Stable causal event vocabulary. Event ordering is local to this runtime;
/// `sequence` makes no distributed total-order claim.
///
/// r[impl machine.obs.event-vocabulary]
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum EventKind {
    Demanded {
        key: DemandKey,
    },
    DemandTransition {
        key: DemandKey,
        from: DemandState,
        to: DemandState,
    },
    Memo {
        location: LocationId,
        verdict: MemoVerdict,
        verified: u32,
    },
    TaskSpawned {
        task: TaskId,
        key: DemandKey,
    },
    TaskTransition {
        task: TaskId,
        from: TaskState,
        to: TaskState,
    },
    ExecutionLane {
        task: TaskId,
        facts: ExecutionFacts,
    },
    MachineFailed {
        task: TaskId,
        key: DemandKey,
        operation: MachineOperation,
    },
    LanguageFailed {
        task: TaskId,
        key: DemandKey,
        failure: FailureValue,
    },
    IslandEntered {
        task: TaskId,
        island: IslandId,
    },
    SafePoint {
        task: TaskId,
        class: SafePointClass,
    },
    WeavyFrameEntered {
        task: TaskId,
        function: FunctionId,
    },
    WeavyFrameExited {
        task: TaskId,
        function: FunctionId,
    },
    WeavyParked {
        task: TaskId,
        input: u32,
    },
    WeavyResumed {
        task: TaskId,
    },
    WeavyMark {
        task: TaskId,
        function: FunctionId,
        node: NodeId,
    },
    StoreAlloc {
        identity: ValueId,
        deduped: bool,
    },
    Completed {
        key: DemandKey,
        identity: ValueId,
    },
    ConservativeFallback {
        code: DiagnosticCode,
    },
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub sequence: u64,
    pub kind: EventKind,
}

/// r[impl machine.obs.event-sink]
pub trait EventSink {
    fn event(&mut self, event: Event);
}

#[derive(Default)]
pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    #[inline(always)]
    fn event(&mut self, _event: Event) {}
}

pub struct EventLog {
    events: Vec<Event>,
}

impl EventLog {
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: Vec::with_capacity(capacity),
        }
    }

    #[must_use]
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    #[must_use]
    pub fn into_events(self) -> Vec<Event> {
        self.events
    }
}

impl Default for EventLog {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

impl EventSink for EventLog {
    fn event(&mut self, event: Event) {
        self.events.push(event);
    }
}
