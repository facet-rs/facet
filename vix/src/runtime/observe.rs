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
    /// Scheduler-owned typed document parses. This deliberately remains
    /// separate from `pure_host_calls`: document parsing crosses the host
    /// boundary once per dynamic document and is not a lowered pure op.
    pub document_parse_host_calls: u64,
    /// Generic registered-primitive dispatches that actually began. Joined or
    /// memoized demands do not increment this counter.
    pub primitive_invocations: u64,
    pub store_interns: u64,
    pub store_dedups: u64,
    pub bytes_hashed: u64,
    pub effect_spawns: u64,
    /// Scheduler progress observed while at least one independent effect was
    /// already in flight, or an effect admitted while another was pending.
    /// This is a causal overlap witness, not a wall-clock inference.
    pub overlap_observations: u64,
    /// Peak number of primitive/exec effects simultaneously owned by the
    /// scheduler. A value above one directly witnesses effect fan-out.
    pub peak_effects_in_flight: u64,
    /// Fetch effects actually performed (a memo miss that ran). A memoized
    /// re-demand of an identical pinned fetch adds nothing; this is what the
    /// `fetched(n)` trace check reads.
    pub fetches_performed: u64,
    pub scheduler_requests: u64,
    pub task_spawns: u64,
    pub task_discards: u64,
    pub native_task_spawns: u64,
    pub interpreter_task_spawns: u64,
    pub value_island_spawns: u64,
    pub successful_aggregate_freezes: u64,
    pub active_molten_selections: u64,
    pub forced_copy_selections: u64,
    pub framed_bytes: u64,
    pub peak_molten_bytes: u64,
    pub peak_molten_nodes: u64,
    /// The peak number of tasks simultaneously parked in the scheduler's
    /// off-stack frame map during a run. A non-zero peak witnesses that a
    /// wire-parked frame was retained in Runtime scheduler state — not on the
    /// recursive Rust stack — while its wire's argument demand ran.
    pub peak_parked_frames: u64,
    /// The peak number of tasks simultaneously parked on registered-primitive
    /// completions in demand-owned pending state. A non-zero peak witnesses that
    /// a yielded primitive frame survived off the recursive Rust stack while its
    /// completion was outstanding.
    pub peak_primitive_parked_frames: u64,
    /// Completions drained from the unified completion inbox. Every registered
    /// primitive and exec completion crosses this one receive authority.
    pub completion_inbox_receipts: u64,
    /// Completions delivered for a demand with no live pending waiter — a late,
    /// duplicate, or legitimate post-cancel delivery. Observed as a typed fact,
    /// never applied as a publication.
    pub stale_completions_ignored: u64,
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
    DisabledByRequest,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionFacts {
    pub selected: ExecutionLaneFact,
    pub native_available: bool,
    pub native_compiled: bool,
    pub fallback: Option<ExecutionFallbackFact>,
}

/// The verified Weavy instruction that yielded a registered primitive. The
/// source node and lowered frame/PC are carried together so a park/resume pair
/// can prove that the scheduler retained and re-entered the exact suspension
/// site on either execution lane.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimitiveSuspensionSite {
    pub function: FunctionId,
    pub node: NodeId,
    pub weavy_function: u32,
    pub weavy_pc: u32,
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
    /// An effect primitive began external work for this demand: a process was
    /// spawned. A memoized effect demand emits no spawn — that absence is what
    /// rung 069 certifies.
    EffectSpawned {
        task: TaskId,
        key: DemandKey,
    },
    /// A registered-primitive frame returned control to the scheduler and now
    /// resides in demand-owned pending state, off the drive stack.
    PrimitiveParked {
        task: TaskId,
        key: DemandKey,
        site: PrimitiveSuspensionSite,
    },
    /// The unified completion inbox resumed the exact retained task and
    /// verified suspension site named by the corresponding park event.
    PrimitiveResumed {
        task: TaskId,
        key: DemandKey,
        site: PrimitiveSuspensionSite,
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
