use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{BufRead, BufReader, Read};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use weavy::exec::{
    FallbackReason, FaultSite, LaneKind, ResolvedTaskValue, TaskFault, TaskStructuralValue,
    TaskValueResolver,
};
use weavy::task::{FnId, HostFn, TaskEvent as WeavyTaskEvent, TaskStep};

use crate::lowering::{LoweringArtifact, LoweringAttribution, ValueInputBinding};
use crate::schema::SchemaRef;
use crate::support::Span;
use crate::vir::{
    CommandPiece, ExternKind, Function, FunctionId, Island, IslandId, NodeId, OPTION_NONE_VARIANT,
    OPTION_SOME_VARIANT, Op, ProgressiveProjection, Type, VariantPayload,
};

use super::fixture::{FixtureEntryKind, FixtureReadError, FixtureStore, TarMember, parse_ustar};
use super::identity::{
    DemandKey, DemandPreimage, Digest, Location, LocationId, RecipeId, ValueId, hash_framed,
};
use super::identity::{FramedField, FramedNode, FramedValue};
use super::model::{
    DemandRecord, DemandState, FailureContext, FailureValue, MemoVerdict, ProcessTermination,
    ReadObservation, ReadProjection, ReadWitness, Receipt, TaskId, TaskRecord, TaskState,
};
use super::observe::{
    Counters, Event, EventKind, EventSink, ExecutionFacts, ExecutionFallbackFact,
    ExecutionLaneFact, PrimitiveSuspensionSite, SafePointClass,
};
use super::store::{
    FrozenValue, Handle, Interned, Store, StoreEntry, StoreJournal, StoreJournalError,
    StoreJournalLoadReport,
};
use super::{
    DecodePrimitive, EffectCtx, EffectTicket, ObservePrimitive, PinnedFetchPrimitive,
    PrimitiveCompletion, PrimitiveDispatcher, PrimitiveField, PrimitiveFieldValue,
    PrimitiveMachineError, PrimitiveRegistry, PrimitiveValue, PrimitiveValueBody,
    StagedEffectAuthority, TicketSubscription, blob_id_type, origin_hint_type,
};
use super::{MachineAttribution, MachineError, MachineOperation, RuntimeFault};

#[derive(Clone, Debug)]
struct MemoEntry {
    location: Location,
    key: DemandKey,
    preimage: DemandPreimage,
    result: Handle,
    receipt: Option<Receipt>,
    current_receipt: bool,
}

#[derive(Clone)]
struct EffectValue {
    identity: ValueId,
    resident: Vec<u8>,
    frozen: Option<FrozenValue>,
    node: Option<FramedNode>,
}

enum EffectTerm {
    Value(EffectValue),
    Glob { tree: EffectValue, pattern: String },
}

struct DemandExecution<'a> {
    artifact: &'a LoweringArtifact,
    demand_key: DemandKey,
    demand_preimage: DemandPreimage,
}

impl<'a> DemandExecution<'a> {
    fn new(artifact: &'a LoweringArtifact, arguments: Vec<ValueId>) -> Self {
        let demand_preimage = DemandPreimage {
            closure: artifact.recipe,
            arguments,
        };
        let demand_key = DemandKey::from_preimage(&demand_preimage);
        Self {
            artifact,
            demand_key,
            demand_preimage,
        }
    }
}

impl Deref for DemandExecution<'_> {
    type Target = LoweringArtifact;

    fn deref(&self) -> &Self::Target {
        self.artifact
    }
}

#[derive(Clone, Debug)]
struct PrimitiveHostRequest {
    plan: usize,
    frame: Vec<u8>,
}

#[derive(Default)]
struct PrimitiveHostQueue {
    requests: Vec<PrimitiveHostRequest>,
    fault: Option<String>,
}

enum PrimitiveHostFailure {
    Abi(String),
    Machine(PrimitiveMachineError),
}

impl From<String> for PrimitiveHostFailure {
    fn from(detail: String) -> Self {
        Self::Abi(detail)
    }
}

fn primitive_runtime_fault(failure: PrimitiveHostFailure) -> RuntimeFault {
    match failure {
        PrimitiveHostFailure::Abi(detail) => RuntimeFault::PrimitiveHost { detail },
        PrimitiveHostFailure::Machine(error) => RuntimeFault::PrimitiveMachine { error },
    }
}

/// One completion delivered to the scheduler-owned inbox from an isolated
/// backend/worker boundary. The completion crosses the thread boundary; only the
/// scheduler thread drains the inbox and materializes the result into a retained
/// frame. Both registered-primitive tickets and the exec process boundary deliver
/// here, so the scheduler blocks on exactly one unified completion event source
/// (`machine.scheduler.block-on-event`, `machine.scheduler.no-shadow-scheduler`).
enum DeliveredCompletion {
    /// A registered-primitive ticket completed (decode/fetch/observe/...).
    Primitive {
        demand: DemandKey,
        publication: super::PrimitivePublication,
    },
    /// An exec process terminated at its isolated worker boundary. The raw
    /// termination is interned by the scheduler thread, never by the worker.
    Exec {
        demand: DemandKey,
        output: Result<std::process::Output, String>,
    },
    /// One command-grammar-authorized immutable exec product. The worker
    /// snapshots the announced file immediately; only these bytes cross to the
    /// scheduler, never a filesystem readiness guess.
    ExecProgress {
        demand: DemandKey,
        product: Result<ExecProgress, String>,
    },
}

struct ExecProgress {
    path: String,
    bytes: Vec<u8>,
}

/// The unified inbox was closed — every completion sender dropped — while work
/// was still outstanding. A lost completion is a loud typed scheduler fault
/// (`machine.scheduler.completion-resumes-direct`), never a swallowed disconnect.
struct LostCompletion;

/// The scheduler's single typed completion mailbox (`machine.scheduler.block-on-event`).
/// Every ticket/worker delivers here through a `Send` sender clone; the scheduler
/// blocks on exactly one `recv` when no task is runnable and routes whatever
/// arrives through the shared `apply_completion` path. There is no per-demand
/// take and no synchronous drain of a specific effect.
struct CompletionInbox {
    sender: std::sync::mpsc::Sender<DeliveredCompletion>,
    receiver: std::sync::mpsc::Receiver<DeliveredCompletion>,
}

impl Default for CompletionInbox {
    fn default() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        Self { sender, receiver }
    }
}

impl CompletionInbox {
    /// A `Send + 'static` ticket waiter that forwards a registered-primitive
    /// completion for `demand` into this inbox. The primitive runtime calls it
    /// (possibly on another thread); the scheduler alone consumes it.
    fn primitive_waiter(
        &self,
        demand: DemandKey,
    ) -> impl FnOnce(super::PrimitivePublication) + Send + 'static {
        let sender = self.sender.clone();
        move |publication| {
            let _ = sender.send(DeliveredCompletion::Primitive {
                demand,
                publication,
            });
        }
    }

    /// A `Send + 'static` sender clone the exec worker uses to deliver a process
    /// termination for `demand` through the same unified event source.
    fn exec_sender(&self) -> std::sync::mpsc::Sender<DeliveredCompletion> {
        self.sender.clone()
    }

    /// Block on the one inbox for exactly one delivered completion, whichever
    /// demand it belongs to. The scheduler drains this only when no task is
    /// runnable and routes the result through the shared `apply_completion`
    /// path; there is no per-demand take. A closed inbox (every sender dropped)
    /// with work still outstanding is a typed lost-completion fault, never a
    /// stringly disconnect error swallowed at a call site.
    fn recv(&self) -> Result<DeliveredCompletion, LostCompletion> {
        self.receiver.recv().map_err(|_| LostCompletion)
    }
}

impl PrimitiveHostQueue {
    fn call(&mut self, frame: &mut [u8]) {
        let plan = frame
            .get(..8)
            .and_then(|bytes| bytes.try_into().ok())
            .map(i64::from_le_bytes)
            .and_then(|value| usize::try_from(value).ok());
        match plan {
            Some(plan) => self.requests.push(PrimitiveHostRequest {
                plan,
                frame: frame.to_vec(),
            }),
            None => self.fault = Some("invalid primitive host ABI header".to_owned()),
        }
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChaosPolicy {
    pub kill_first_running_task: bool,
    /// Discard the first task after it has yielded a registered primitive,
    /// leaving the demand-owned ticket alive so replay must join it.
    pub kill_first_parked_primitive: bool,
}

/// The inputs an island evaluation consumes: its pre-published shared value
/// arguments (already realized), and its demand wires (unresolved). A wire is
/// resolved lazily — only when the task actually parks on it — through the
/// canonical `DemandPreimage`/memo path, never pre-resolved.
///
/// The inputs are owned so that a task suspended on a wire can retain them off
/// the recursive Rust stack: the scheduler moves the parked task's whole
/// suspension context (its owned artifact handle, wires, and realized
/// arguments) into a scheduler-owned frame map and resumes it later without
/// re-borrowing any submission-time state.
#[derive(Clone)]
pub struct IslandInputs {
    pub arguments: Vec<Evaluation>,
    pub wires: Vec<WireDemand>,
}

/// One demand wire an island may force: the canonical argument demand the
/// scheduler evaluates through the existing memo machinery when the consuming
/// task parks on the wire's `AwaitWire` input. It carries everything needed to
/// evaluate that argument island — its retained recipe artifact, cost-model
/// location, realized arguments, and its own nested wires — plus the callee
/// identity used to record the realized dependency. A wire is never evaluated
/// unless the task parks on it. Every field is owned so a suspended parent can
/// keep its unforced wires while it waits off the drive stack.
#[derive(Clone)]
pub struct WireDemand {
    pub island: IslandId,
    pub location: Location,
    pub lowered: Rc<LoweringArtifact>,
    pub attribution: Rc<LoweringAttribution>,
    pub arguments: Vec<Evaluation>,
    pub wires: Vec<WireDemand>,
    pub function: FunctionId,
    /// The canonical scalar argument identities of this invocation, recorded in
    /// the realized-demand log when the wire actually computes (a memo miss).
    /// `Some(&[])` for a zero-argument callee; `None` when the invocation has a
    /// composite or computed argument, which no call-site literal can select. A
    /// memoized re-force adds no entry, so the log counts one realization per
    /// distinct demand identity.
    pub demand_arguments: Option<Vec<ValueId>>,
    /// The canonical structural preimage of this invocation in the authored
    /// graph — the content key a binding-level described wire selects on.
    pub preimage: String,
}

/// One scheduler-owned runnable/parked task: the owned suspension context plus
/// the retained Weavy executable task. The scheduler moves this in and out of
/// its runnable stack and parked map (`take-run-put`) instead of holding a task
/// on the recursive Rust stack, so a wire-parked frame lives in scheduler state
/// off the drive stack and resumes to the same identity.
///
/// r[impl machine.scheduler.no-shadow-scheduler]
/// r[impl machine.scheduler.tickets-outlive-tasks]
struct TaskContext {
    task_id: TaskId,
    island: IslandId,
    demand_key: DemandKey,
    demand_preimage: DemandPreimage,
    lowered: Rc<LoweringArtifact>,
    location: Location,
    attribution: Rc<LoweringAttribution>,
    arguments: Vec<Evaluation>,
    wires: Vec<WireDemand>,
    /// Root-to-self demand keys (excluding this task's own key). A wire that
    /// forces a `Running`/`Queued` ancestor is a re-entrant demand fault; a
    /// `Running`/`Queued` non-ancestor joins its in-flight completion.
    ancestry: Vec<DemandKey>,
    /// The described invocation this task realizes when it owns a fresh miss.
    /// Joiners carry no copy, so publishing one shared demand records exactly
    /// one realization.
    realized_as: Option<RealizedWireDemand>,
    task: weavy::exec::ExecTask,
    ready: Vec<bool>,
    awaited: Vec<i64>,
    primitive_reads: Vec<ReadWitness>,
    value_memory_overrides: Vec<(Handle, Vec<u8>)>,
    kill_on_primitive_park: bool,
}

/// One parent task suspended on a wire, waiting for the wire's demand to
/// complete so its awaited input slot can be filled and the frame resumed.
struct WireWaiter {
    task_id: TaskId,
    wire_index: usize,
}

/// The outcome of driving a runnable task one `take-run-put` segment.
enum DriveOutcome {
    /// The task returned control to the scheduler at an `AwaitWire` on this
    /// input index, its frame retained as owned suspended state.
    Parked(u32),
    /// The task reached a registered-primitive `HostCallYield`. Control returns
    /// to the loop so the frame can park in demand-owned pending state and its
    /// completion can cross the unified inbox.
    YieldedPrimitive(PrimitiveHostRequest),
    /// The task ran to `Done`; its realized (or language-failed) result is
    /// published to waiters. Boxed so the common `Parked` step stays small.
    Completed(Box<Evaluation>),
}

/// A registered-primitive demand in flight. Owned by the DEMAND, not the
/// requesting task (`machine.scheduler.tickets-outlive-tasks`): killing a
/// waiting task leaves the ticket alive. The first caller installs the single
/// authority and ticket subscription; every waiter (the first plus any joiner)
/// is a task parked mid-drive, resumed by materializing the one admitted value
/// through its own ABI plan when the completion crosses the inbox.
struct PrimitivePending {
    /// The demand-owned ticket remains live even if a waiting task is discarded;
    /// replay joins this same ticket instead of beginning a second effect.
    ticket: EffectTicket,
    /// The first caller's staged authority; joiners never construct another.
    authority: Arc<StagedEffectAuthority>,
    /// The single ticket subscription that delivers this demand's completion
    /// into the unified inbox.
    subscription: TicketSubscription,
    /// FIFO of parked tasks awaiting this completion, each with its own ABI
    /// plan describing the output frame region it materializes into.
    waiters: Vec<PrimitiveWaiter>,
}

/// One task parked on a registered-primitive completion, plus the ABI plan the
/// admitted value is materialized through into its retained frame.
struct PrimitiveWaiter {
    ctx: TaskContext,
    plan: crate::lowering::PrimitiveCall,
    site: PrimitiveSuspensionSite,
}

/// An exec demand in flight at its isolated worker-thread process boundary. The
/// scheduler holds only the memoization context; the raw termination crosses
/// the unified inbox and is interned and memoized solely by `apply_completion`.
struct ExecPending {
    task_id: TaskId,
    location: Location,
    demand_preimage: DemandPreimage,
    receipt: Receipt,
    result_ty: Type,
    plan_recipe: RecipeId,
    function: FunctionId,
    node: NodeId,
    span: Span,
    realized_as: Option<RealizedWireDemand>,
    workspace: ExecWorkspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecProtocol {
    ExitOnly,
    ProgressiveLinesV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExecProjectionAuthority {
    Protocol,
    ProcessExit,
}

struct ExecProjectionPending {
    task_id: TaskId,
    location: Location,
    demand_preimage: DemandPreimage,
    execution: DemandKey,
    capability: ValueId,
    path: String,
    function: FunctionId,
    node: NodeId,
    span: Span,
}

struct ExecWorkspace {
    path: PathBuf,
}

impl ExecWorkspace {
    fn create() -> Result<Self, String> {
        static NEXT_WORKSPACE: AtomicU64 = AtomicU64::new(0);
        let ordinal = NEXT_WORKSPACE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("vix-exec-{}-{ordinal}", std::process::id()));
        std::fs::create_dir(&path)
            .map_err(|error| format!("create exec workspace `{}`: {error}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for ExecWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One island demand submission: everything needed to resolve it from memo,
/// join an in-flight demand, or spawn a fresh runnable task. Bundled so the
/// submission and spawn seams take a single typed request rather than a long
/// positional argument list.
struct SubmitRequest {
    island: IslandId,
    location: Location,
    lowered: Rc<LoweringArtifact>,
    attribution: Rc<LoweringAttribution>,
    arguments: Vec<Evaluation>,
    wires: Vec<WireDemand>,
    chaos: ChaosPolicy,
    /// Root-to-parent demand chain of the submitting task (empty for the root).
    ancestry: Vec<DemandKey>,
    /// The described invocation recorded only if this submission owns a fresh
    /// miss. An in-flight join drops this metadata.
    realized_as: Option<RealizedWireDemand>,
}

/// The outcome of submitting a demand (a root evaluation or a forced wire).
enum SubmitOutcome {
    /// Resolved without a task: a memo hit or an argument-failure cascade.
    Ready(Evaluation),
    /// A fresh task was spawned and pushed runnable under this demand key.
    Spawned(DemandKey),
    /// The demand is already in flight (a `Running`/`Queued` non-ancestor); the
    /// caller joins its completion rather than starting a second backend run.
    Joined(DemandKey),
}

/// One realized invocation recorded for described-wire observation: which user
/// function was demanded, with which canonical argument identities (when the
/// invocation is literal-selectable), and under which canonical structural
/// preimage. Recorded only when a demand actually computes (a memo miss that
/// ran), so the log counts realizations, never re-demands of an
/// already-memoized key.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct RealizedWireDemand {
    pub function: FunctionId,
    /// Canonical scalar argument identities for a literal-selectable
    /// invocation; `None` when an argument is composite or computed.
    pub arguments: Option<Vec<ValueId>>,
    /// Canonical structural preimage of the invocation subtree in the authored
    /// graph. Equal preimages denote one semantic demand.
    pub preimage: String,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Evaluation {
    pub handle: Handle,
    pub identity: ValueId,
    pub passed: bool,
    pub memo: MemoVerdict,
    pub failure: Option<FailureValue>,
    pub failure_context: Option<FailureContext>,
}

/// The result of submitting one top-level demand without synchronously
/// draining it. A ready demand resolved immediately; a pending demand remains
/// owned by the scheduler until a shared harvest call observes its publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RootSubmission {
    Ready(Evaluation),
    Pending(DemandKey),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecSubmission {
    pub demand: DemandKey,
    pub root: RootSubmission,
}

pub struct ExecProjectionRequest {
    pub execution: DemandKey,
    pub capability: ValueId,
    pub completed: Option<Evaluation>,
    pub projection: ProgressiveProjection,
    pub location: Location,
    pub function: FunctionId,
    pub node: NodeId,
    pub span: Span,
}

/// One top-level pure/value demand submitted by the runner. The request owns
/// every frame input so the scheduler may retain it across suspension.
pub struct ValueRootRequest {
    pub island: IslandId,
    pub location: Location,
    pub lowered: Rc<LoweringArtifact>,
    pub attribution: Rc<LoweringAttribution>,
    pub inputs: IslandInputs,
    pub chaos: ChaosPolicy,
    /// Described invocation metadata owned by the fresh computing task.
    pub realized_as: Option<RealizedWireDemand>,
}

/// The outcome of driving one generator task to completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GeneratorOutcome {
    /// The taken sites' raw provenance selectors, in publication order.
    Sites(Vec<u64>),
    /// The generator's scrutinee control language-failed before deciding a
    /// branch. A language failure, never a machine invariant. The typed failure
    /// is boxed so the common `Sites` path stays small.
    LanguageFailure {
        failure: Box<FailureValue>,
        context: Option<FailureContext>,
    },
}

/// The scheduler owns passive maps and admission bookkeeping; Weavy owns the
/// executable task and any suspension state.
///
/// r[impl machine.runtime.state-machines]
/// r[impl machine.scheduler.passive-no-loop]
/// r[impl machine.scheduler.no-shadow-scheduler]
pub struct Runtime<S> {
    sink: S,
    sequence: u64,
    store: Store,
    memo: BTreeMap<LocationId, MemoEntry>,
    memo_suffix_index: BTreeMap<Vec<String>, Vec<LocationId>>,
    demands: BTreeMap<DemandKey, DemandRecord>,
    tasks: BTreeMap<TaskId, TaskRecord>,
    counters: Counters,
    next_task: u64,
    /// One entry per realized wire demand — a callee invocation the memo path
    /// actually computed (a miss that ran), recorded as its callee function,
    /// canonical argument identities, and canonical structural preimage.
    /// Memoized re-demands add no entry, so this log counts realizations. It
    /// backs the described-wire trace checks and retains only the
    /// callee/argument/preimage selectors a descriptor can name.
    wire_demands: Vec<RealizedWireDemand>,
    fixture_store: FixtureStore,
    primitive_dispatcher: PrimitiveDispatcher,
    primitive_services: super::PrimitiveServices,
    /// The scheduler-owned typed completion mailbox for registered primitives.
    completion_inbox: CompletionInbox,
    authoritative_rerun_audit: bool,
    /// Transient scheduler state for the runnable/parked task loop. A runnable
    /// task is one ready to drive; a parked task is suspended on an unresolved
    /// wire; `wire_waiters` maps a wire's demand key to the parents waiting on
    /// it. These maps are private, non-persistent, and asserted empty once a
    /// root resolves — none is ever copied into `PersistentRuntimeState`.
    runnable: Vec<TaskContext>,
    parked: BTreeMap<TaskId, TaskContext>,
    wire_waiters: BTreeMap<DemandKey, Vec<WireWaiter>>,
    /// Completed demand results, keyed by demand. A resolved root is read from
    /// here; every wire/primitive/exec completion publishes into it. Transient:
    /// cleared once a root resolves, never extracted into persistent state.
    root_results: BTreeMap<DemandKey, Evaluation>,
    /// Registered-primitive demands in flight, keyed by the primitive demand.
    /// A yielded frame parks here off the recursive Rust stack until its
    /// completion crosses the unified inbox.
    primitive_pending: BTreeMap<DemandKey, PrimitivePending>,
    /// Exec demands in flight at the isolated worker-thread boundary, keyed by
    /// the exec demand. The scheduler thread performs no synchronous wait.
    exec_pending: BTreeMap<DemandKey, ExecPending>,
    /// Progressive exec product roots waiting on a command-protocol event.
    exec_projection_pending: BTreeMap<DemandKey, ExecProjectionPending>,
    /// Products that arrived before their projection root was submitted.
    exec_progress_ready: BTreeMap<(DemandKey, String), Vec<u8>>,
}

#[derive(Clone, Default)]
pub struct PersistentRuntimeState {
    store: Store,
    memo: BTreeMap<LocationId, MemoEntry>,
}

impl PersistentRuntimeState {
    #[must_use]
    pub fn to_journal(&self) -> PersistentRuntimeJournal {
        PersistentRuntimeJournal {
            store: self.store.to_journal(),
            claims: self
                .memo
                .values()
                .filter_map(|entry| {
                    let store_entry = self.store.entry(entry.result)?;
                    Some(PersistentMemoClaim {
                        location: entry.location.clone(),
                        key: entry.key,
                        preimage: entry.preimage.clone(),
                        result: store_entry.identity.clone(),
                        receipt: Some(entry.receipt.clone().unwrap_or_else(|| Receipt {
                            demand: entry.key,
                            reads: Vec::new(),
                        })),
                    })
                })
                .collect(),
        }
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PersistentRuntimeJournal {
    pub store: StoreJournal,
    pub claims: Vec<PersistentMemoClaim>,
}

impl PersistentRuntimeJournal {
    pub fn to_json(&self) -> Result<String, PersistentRuntimeJournalError> {
        facet_json::to_string(self).map_err(|error| PersistentRuntimeJournalError::Json {
            detail: error.to_string(),
        })
    }

    pub fn from_json(text: &str) -> Result<Self, PersistentRuntimeJournalError> {
        facet_json::from_str(text).map_err(|error| PersistentRuntimeJournalError::Json {
            detail: error.to_string(),
        })
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PersistentMemoClaim {
    pub location: Location,
    pub key: DemandKey,
    pub preimage: DemandPreimage,
    pub result: ValueId,
    pub receipt: Option<Receipt>,
}

#[derive(facet::Facet, Clone, Debug, Default, PartialEq, Eq)]
pub struct PersistentRuntimeJournalLoadReport {
    pub store: StoreJournalLoadReport,
    pub claims_seen: u64,
    pub claims_loaded: u64,
    pub claims_rejected: u64,
    pub rejected_claims: Vec<PersistentClaimRejection>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PersistentClaimRejection {
    pub location: Location,
    pub key: DemandKey,
    pub reason: PersistentClaimRejectionReason,
    pub receipt: Option<Receipt>,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PersistentClaimRejectionReason {
    KeyMismatch,
    MissingValue,
    MissingReceipt,
    ReceiptDemandMismatch,
    UnverifiableReceipt,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PersistentRuntimeJournalError {
    Json { detail: String },
    Store(Box<StoreJournalError>),
}

impl From<StoreJournalError> for PersistentRuntimeJournalError {
    fn from(error: StoreJournalError) -> Self {
        Self::Store(Box::new(error))
    }
}

fn location_suffixes(location: &Location) -> impl Iterator<Item = Vec<String>> + '_ {
    (1..location.segments.len()).map(|start| location.segments[start..].to_vec())
}

fn build_memo_suffix_index(
    memo: &BTreeMap<LocationId, MemoEntry>,
) -> BTreeMap<Vec<String>, Vec<LocationId>> {
    let mut index = BTreeMap::<Vec<String>, Vec<LocationId>>::new();
    for entry in memo.values() {
        for suffix in location_suffixes(&entry.location) {
            index.entry(suffix).or_default().push(entry.location.id);
        }
    }
    index
}

fn default_primitive_dispatcher() -> PrimitiveDispatcher {
    let mut registry = PrimitiveRegistry::default();
    registry
        .register(Arc::new(DecodePrimitive::default()))
        .expect("the built-in decode primitive is registered once");
    registry
        .register(Arc::new(PinnedFetchPrimitive::default()))
        .expect("the built-in pinned fetch primitive is registered once");
    registry
        .register(Arc::new(ObservePrimitive::default()))
        .expect("the built-in observe primitive is registered once");
    PrimitiveDispatcher::new(Arc::new(registry))
}

impl<S: EventSink> Runtime<S> {
    #[must_use]
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            sequence: 0,
            store: Store::default(),
            memo: BTreeMap::new(),
            memo_suffix_index: BTreeMap::new(),
            demands: BTreeMap::new(),
            tasks: BTreeMap::new(),
            counters: Counters::default(),
            next_task: 0,
            wire_demands: Vec::new(),
            fixture_store: FixtureStore::default(),
            primitive_dispatcher: default_primitive_dispatcher(),
            primitive_services: super::PrimitiveServices::default(),
            completion_inbox: CompletionInbox::default(),
            authoritative_rerun_audit: false,
            runnable: Vec::new(),
            parked: BTreeMap::new(),
            wire_waiters: BTreeMap::new(),
            root_results: BTreeMap::new(),
            primitive_pending: BTreeMap::new(),
            exec_pending: BTreeMap::new(),
            exec_projection_pending: BTreeMap::new(),
            exec_progress_ready: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_persistent_state(sink: S, state: PersistentRuntimeState) -> Self {
        let memo = state
            .memo
            .into_iter()
            .map(|(location, mut entry)| {
                entry.current_receipt = false;
                (location, entry)
            })
            .collect();
        let memo_suffix_index = build_memo_suffix_index(&memo);
        Self {
            sink,
            sequence: 0,
            store: state.store,
            memo,
            memo_suffix_index,
            demands: BTreeMap::new(),
            tasks: BTreeMap::new(),
            counters: Counters::default(),
            next_task: 0,
            wire_demands: Vec::new(),
            fixture_store: FixtureStore::default(),
            primitive_dispatcher: default_primitive_dispatcher(),
            primitive_services: super::PrimitiveServices::default(),
            completion_inbox: CompletionInbox::default(),
            authoritative_rerun_audit: false,
            runnable: Vec::new(),
            parked: BTreeMap::new(),
            wire_waiters: BTreeMap::new(),
            root_results: BTreeMap::new(),
            primitive_pending: BTreeMap::new(),
            exec_pending: BTreeMap::new(),
            exec_projection_pending: BTreeMap::new(),
            exec_progress_ready: BTreeMap::new(),
        }
    }

    pub fn set_fixture_rerun_overlay(&mut self, rerun_with: Option<String>) {
        self.fixture_store = self.fixture_store.clone().with_rerun_overlay(rerun_with);
    }

    pub fn set_primitive_services(&mut self, services: super::PrimitiveServices) {
        if let Some(fixture_store) = services.fixture_store() {
            self.fixture_store = fixture_store;
        }
        self.primitive_services = services;
    }

    pub fn set_authoritative_rerun_audit(&mut self, enabled: bool) {
        self.authoritative_rerun_audit = enabled;
    }

    pub fn with_persistent_journal_values(
        sink: S,
        journal: &PersistentRuntimeJournal,
    ) -> Result<(Self, PersistentRuntimeJournalLoadReport), PersistentRuntimeJournalError> {
        let (store, store_report) = Store::from_journal(journal.store.clone())?;
        Ok((
            Self {
                sink,
                sequence: 0,
                store,
                memo: BTreeMap::new(),
                memo_suffix_index: BTreeMap::new(),
                demands: BTreeMap::new(),
                tasks: BTreeMap::new(),
                counters: Counters::default(),
                next_task: 0,
                wire_demands: Vec::new(),
                fixture_store: FixtureStore::default(),
                primitive_dispatcher: default_primitive_dispatcher(),
                primitive_services: super::PrimitiveServices::default(),
                completion_inbox: CompletionInbox::default(),
                authoritative_rerun_audit: false,
                runnable: Vec::new(),
                parked: BTreeMap::new(),
                wire_waiters: BTreeMap::new(),
                root_results: BTreeMap::new(),
                primitive_pending: BTreeMap::new(),
                exec_pending: BTreeMap::new(),
                exec_projection_pending: BTreeMap::new(),
                exec_progress_ready: BTreeMap::new(),
            },
            PersistentRuntimeJournalLoadReport {
                store: store_report,
                ..PersistentRuntimeJournalLoadReport::default()
            },
        ))
    }

    pub fn load_persistent_journal_claims(
        &mut self,
        journal: &PersistentRuntimeJournal,
        report: &mut PersistentRuntimeJournalLoadReport,
    ) {
        for claim in &journal.claims {
            report.claims_seen += 1;
            let reason = if DemandKey::from_preimage(&claim.preimage) != claim.key {
                Some(PersistentClaimRejectionReason::KeyMismatch)
            } else if claim.receipt.is_none() {
                Some(PersistentClaimRejectionReason::MissingReceipt)
            } else if claim
                .receipt
                .as_ref()
                .is_some_and(|receipt| receipt.demand != claim.key)
            {
                Some(PersistentClaimRejectionReason::ReceiptDemandMismatch)
            } else if !claim
                .receipt
                .as_ref()
                .is_some_and(|receipt| self.reverify_receipt(receipt))
            {
                Some(PersistentClaimRejectionReason::UnverifiableReceipt)
            } else if self.store.handle_for_identity(&claim.result).is_none() {
                Some(PersistentClaimRejectionReason::MissingValue)
            } else {
                None
            };
            if let Some(reason) = reason {
                report.claims_rejected += 1;
                report.rejected_claims.push(PersistentClaimRejection {
                    location: claim.location.clone(),
                    key: claim.key,
                    reason,
                    receipt: claim.receipt.clone(),
                });
                continue;
            }
            let result = self
                .store
                .handle_for_identity(&claim.result)
                .expect("claim result was checked above");
            self.insert_memo(MemoEntry {
                location: claim.location.clone(),
                key: claim.key,
                preimage: claim.preimage.clone(),
                result,
                receipt: claim.receipt.clone(),
                current_receipt: false,
            });
            report.claims_loaded += 1;
        }
    }

    #[must_use]
    pub fn into_persistent_state(self) -> PersistentRuntimeState {
        self.assert_scheduler_quiescent();
        PersistentRuntimeState {
            store: self.store,
            memo: self.memo,
        }
    }

    /// Assert every transient scheduler map is empty before persistent-state
    /// extraction. This runs always (not `debug_assert`): a non-empty runnable,
    /// parked, waiter, result, or pending map at this boundary would mean live
    /// scheduler state was about to be confused with persistent value/claim
    /// state. `PersistentRuntimeState` carries only `store` + `memo`; scheduler
    /// state never enters it.
    fn assert_scheduler_quiescent(&self) {
        assert!(
            self.runnable.is_empty()
                && self.parked.is_empty()
                && self.wire_waiters.is_empty()
                && self.root_results.is_empty()
                && self.primitive_pending.is_empty()
                && self.exec_pending.is_empty()
                && self.exec_projection_pending.is_empty()
                && self.exec_progress_ready.is_empty(),
            "scheduler is not quiescent at persistent-state extraction: \
             runnable={}, parked={}, wire_waiters={}, root_results={}, \
             primitive_pending={}, exec_pending={}, exec_projection_pending={}, \
             exec_progress_ready={}",
            self.runnable.len(),
            self.parked.len(),
            self.wire_waiters.len(),
            self.root_results.len(),
            self.primitive_pending.len(),
            self.exec_pending.len(),
            self.exec_projection_pending.len(),
            self.exec_progress_ready.len(),
        );
    }

    /// The frozen log of realized wire demands: each callee invocation the memo
    /// path computed, by callee function, canonical argument identities, and
    /// canonical structural preimage.
    #[must_use]
    pub fn realized_wire_demands(&self) -> &[RealizedWireDemand] {
        &self.wire_demands
    }

    /// Record one realized wire demand — a callee invocation the memo path
    /// actually computed. The runner calls this only on a memo miss, so a
    /// memoized re-demand of the same recipe+argument adds no entry.
    pub fn record_wire_demand(
        &mut self,
        function: FunctionId,
        arguments: Option<Vec<ValueId>>,
        preimage: String,
    ) {
        self.wire_demands.push(RealizedWireDemand {
            function,
            arguments,
            preimage,
        });
    }

    fn reverify_receipt(&self, receipt: &Receipt) -> bool {
        receipt
            .reads
            .iter()
            .all(|read| self.reverify_read_witness(read))
    }

    fn reverify_read_witness(&self, read: &ReadWitness) -> bool {
        match &read.observation {
            ReadObservation::Value(observed) => {
                if matches!(read.projection, ReadProjection::Document) {
                    return observed == &read.source;
                }
                if matches!(read.projection, ReadProjection::RegistryManifest) {
                    return self
                        .fixture_store
                        .registry_manifest()
                        .is_ok_and(|manifest| {
                            effect_leaf(&Type::String, manifest.into_bytes()).identity == *observed
                        });
                }
                match &read.projection {
                    ReadProjection::TreePath { path } => {
                        if let Ok(bytes) = self.fixture_store.tree_file_bytes(path) {
                            return effect_leaf(&Type::String, bytes).identity == *observed;
                        }
                    }
                    ReadProjection::Origin { coordinate } => {
                        if let Ok(bytes) = self.fixture_store.fetch_url(coordinate) {
                            return effect_leaf(&Type::Extern(ExternKind::Blob), bytes).identity
                                == *observed;
                        }
                    }
                    ReadProjection::ExecTreePath { .. } => {}
                    ReadProjection::Whole
                    | ReadProjection::Document
                    | ReadProjection::RegistryManifest
                    | ReadProjection::CapabilityProgram => {}
                }
                false
            }
            ReadObservation::Missing => matches!(
                &read.projection,
                ReadProjection::TreePath { path }
                    if matches!(
                        self.fixture_store.tree_file_bytes(path),
                        Err(FixtureReadError::Missing)
                    )
            ),
            ReadObservation::Directory { digest } => match &read.projection {
                ReadProjection::TreePath { path } => self
                    .fixture_store
                    .tree_dir_entries(path)
                    .is_ok_and(|entries| directory_observation_digest(&entries) == *digest),
                _ => false,
            },
            ReadObservation::Unverifiable => false,
        }
    }

    fn exact_memo_replayable(&self, entry: &MemoEntry) -> bool {
        !self.authoritative_rerun_audit
            || entry
                .receipt
                .as_ref()
                .is_none_or(|receipt| self.reverify_receipt(receipt))
    }

    fn insert_memo(&mut self, entry: MemoEntry) {
        let id = entry.location.id;
        if let Some(previous) = self.memo.insert(id, entry.clone()) {
            for suffix in location_suffixes(&previous.location) {
                let remove_suffix =
                    if let Some(candidates) = self.memo_suffix_index.get_mut(&suffix) {
                        candidates.retain(|candidate| *candidate != id);
                        candidates.is_empty()
                    } else {
                        false
                    };
                if remove_suffix {
                    self.memo_suffix_index.remove(&suffix);
                }
            }
        }
        for suffix in location_suffixes(&entry.location) {
            self.memo_suffix_index.entry(suffix).or_default().push(id);
        }
    }

    fn suffix_memo_candidate(
        &self,
        location: &Location,
        key: DemandKey,
        preimage: &DemandPreimage,
    ) -> Option<&MemoEntry> {
        location_suffixes(location)
            .flat_map(|suffix| self.memo_suffix_index.get(&suffix).into_iter().flatten())
            .filter_map(|id| self.memo.get(id))
            .find(|entry| {
                entry.location != *location
                    && entry.key == key
                    && entry.preimage == *preimage
                    && self.exact_memo_replayable(entry)
            })
    }

    /// The scalar result word of a resolved wire demand, read from its interned
    /// store handle. Used to supply an awaiting task's ready wire input; a
    /// wire's callee always publishes a scalar.
    #[must_use]
    pub fn scalar_word(&self, handle: Handle) -> Option<i64> {
        let bytes = self.store.entry(handle)?.resident_bytes()?;
        let mut word = [0u8; 8];
        let width = bytes.len().min(8);
        word[..width].copy_from_slice(&bytes[..width]);
        Some(i64::from_le_bytes(word))
    }

    /// Evaluate one island demand to its realized result. This is a thin
    /// single-root wrapper over the scheduler-owned runnable/parked loop: it
    /// submits the root demand, then drives runnable tasks with `take-run-put`
    /// until the root resolves. No island evaluation happens on the recursive
    /// Rust stack — a wire-parked frame lives in scheduler state and resumes
    /// from a demand waiter.
    ///
    /// r[impl machine.scheduler.passive-no-loop]
    /// r[impl machine.scheduler.no-shadow-scheduler]
    /// r[impl machine.scheduler.completion-resumes-direct]
    pub fn submit_value(
        &mut self,
        request: ValueRootRequest,
    ) -> Result<RootSubmission, Box<MachineError>> {
        let ValueRootRequest {
            island,
            location,
            lowered,
            attribution,
            inputs,
            chaos,
            realized_as,
        } = request;
        let IslandInputs { arguments, wires } = inputs;
        match self.submit_demand(SubmitRequest {
            island,
            location,
            lowered,
            attribution,
            arguments,
            wires,
            chaos,
            ancestry: Vec::new(),
            realized_as,
        })? {
            SubmitOutcome::Ready(evaluation) => Ok(RootSubmission::Ready(evaluation)),
            SubmitOutcome::Spawned(root) | SubmitOutcome::Joined(root) => {
                Ok(RootSubmission::Pending(root))
            }
        }
    }

    /// Evaluate one island demand to its realized result. This remains the
    /// single-root compatibility wrapper over [`Self::submit_value`]; the
    /// production frontier submits several roots before harvesting any.
    pub fn evaluate(
        &mut self,
        island: IslandId,
        location: &Location,
        lowered: Rc<LoweringArtifact>,
        attribution: &LoweringAttribution,
        inputs: IslandInputs,
        chaos: ChaosPolicy,
    ) -> Result<Evaluation, Box<MachineError>> {
        let outcome = self.submit_value(ValueRootRequest {
            island,
            location: location.clone(),
            lowered,
            attribution: Rc::new(attribution.clone()),
            inputs,
            chaos,
            realized_as: None,
        })?;
        let evaluation = match outcome {
            RootSubmission::Ready(evaluation) => evaluation,
            RootSubmission::Pending(root) => self.run_until_root(root)?,
        };
        self.finish_root_batch();
        Ok(evaluation)
    }

    /// Whether a demand is already in flight (queued to run or running) and so
    /// may be joined rather than started a second time.
    fn demand_in_flight(&self, key: DemandKey) -> bool {
        self.demands.get(&key).is_some_and(|record| {
            matches!(record.state, DemandState::Running | DemandState::Queued)
        })
    }

    /// Submit a demand into the scheduler: resolve it from memo, join an
    /// in-flight demand, or spawn a fresh runnable task. Returns without driving.
    ///
    /// `ancestry` is the root-to-parent demand chain of the submitting task
    /// (empty for the root). A wire that forces a `Running`/`Queued` ancestor is
    /// a re-entrant demand fault; a `Running`/`Queued` non-ancestor joins its
    /// in-flight completion instead of starting a second backend run.
    fn submit_demand(
        &mut self,
        request: SubmitRequest,
    ) -> Result<SubmitOutcome, Box<MachineError>> {
        let SubmitRequest {
            island,
            location,
            lowered,
            attribution,
            arguments,
            wires,
            chaos,
            ancestry,
            realized_as,
        } = request;
        let demand_preimage = DemandPreimage {
            closure: lowered.recipe,
            arguments: arguments
                .iter()
                .map(|argument| argument.identity.clone())
                .collect(),
        };
        let demand_key = DemandKey::from_preimage(&demand_preimage);
        self.emit(EventKind::Demanded { key: demand_key });

        // Phase A resolves the demand without a live task where it can:
        // a memo hit, an in-flight join, a re-entrant fault, or a failed
        // argument. Only a genuine miss falls through to spawn a task. The
        // borrowed `DemandExecution` lives only for this scope so the owned
        // artifact handle is free to move into the spawned task afterward.
        {
            let invocation =
                DemandExecution::new(lowered.as_ref(), demand_preimage.arguments.clone());
            if let Some(evaluation) = self.try_memo_hit(
                &location,
                demand_key,
                &demand_preimage,
                &invocation,
                attribution.as_ref(),
            )? {
                return Ok(SubmitOutcome::Ready(evaluation));
            }

            // A wire that forces a demand already in flight is either a cyclic
            // re-entrant demand (an ancestor) — a typed fault — or a concurrent
            // non-ancestor the caller joins.
            if self.demand_in_flight(demand_key) {
                if ancestry.contains(&demand_key) {
                    return Err(Box::new(MachineError::runtime(
                        MachineOperation::Drive,
                        RuntimeFault::ReentrantDemand { key: demand_key },
                        self.output_attribution(lowered.as_ref(), attribution.as_ref()),
                        Some(demand_key),
                    )));
                }
                self.counters.demand_joins += 1;
                self.counters.memo_hits_exact += 1;
                self.emit(EventKind::Memo {
                    location: location.id,
                    verdict: MemoVerdict::Exact,
                    verified: 0,
                });
                return Ok(SubmitOutcome::Joined(demand_key));
            }

            self.counters.memo_misses += 1;
            self.emit(EventKind::Memo {
                location: location.id,
                verdict: MemoVerdict::Miss,
                verified: 0,
            });
            self.demands.insert(
                demand_key,
                DemandRecord {
                    key: demand_key,
                    state: DemandState::Queued,
                    result: None,
                },
            );
            self.emit(EventKind::DemandTransition {
                key: demand_key,
                from: DemandState::Absent,
                to: DemandState::Queued,
            });

            if let Some(argument) = arguments.iter().find(|argument| argument.failure.is_some()) {
                let failure = argument.failure.clone().expect("selected failed argument");
                self.memo.insert(
                    location.id,
                    MemoEntry {
                        location: location.clone(),
                        key: demand_key,
                        preimage: demand_preimage.clone(),
                        result: argument.handle,
                        receipt: None,
                        current_receipt: false,
                    },
                );
                if let Some(demand) = self.demands.get_mut(&demand_key) {
                    demand.result = Some(argument.handle);
                }
                self.transition_demand(demand_key, DemandState::Failed)?;
                if let Some(realized) = realized_as {
                    self.wire_demands.push(realized);
                }
                return Ok(SubmitOutcome::Ready(Evaluation {
                    handle: argument.handle,
                    identity: argument.identity.clone(),
                    passed: false,
                    memo: MemoVerdict::Miss,
                    failure: Some(failure),
                    failure_context: self
                        .output_attribution(lowered.as_ref(), attribution.as_ref())
                        .map(|source| FailureContext {
                            function: source.function,
                            node: source.node,
                            span: source.span,
                            demand_chain: vec![demand_key],
                        }),
                }));
            }

            if lowered.value_inputs.len() != arguments.len() {
                return Err(Box::new(MachineError::runtime(
                    MachineOperation::EntryBinding,
                    RuntimeFault::ValueInputCardinality {
                        expected: lowered.value_inputs.len(),
                        actual: arguments.len(),
                    },
                    None,
                    Some(demand_key),
                )));
            }
        }

        self.spawn_context(
            SubmitRequest {
                island,
                location,
                lowered,
                attribution,
                arguments,
                wires,
                chaos,
                ancestry,
                realized_as,
            },
            demand_key,
            demand_preimage,
        )?;
        Ok(SubmitOutcome::Spawned(demand_key))
    }

    /// Resolve a demand from the value memo without running a task. Returns the
    /// realized [`Evaluation`] on an exact, semantic, or projection hit; `None`
    /// on a miss. Emits the same `Memo` event and updates the same counters as
    /// the historical inline memo fast paths.
    fn try_memo_hit(
        &mut self,
        location: &Location,
        demand_key: DemandKey,
        demand_preimage: &DemandPreimage,
        lowered: &DemandExecution<'_>,
        attribution: &LoweringAttribution,
    ) -> Result<Option<Evaluation>, Box<MachineError>> {
        if let Some(entry) = self.memo.get(&location.id)
            && entry.location == *location
            && entry.key == demand_key
            && entry.preimage == *demand_preimage
            && self.exact_memo_replayable(entry)
        {
            let handle = entry.result;
            let evaluation =
                self.memo_evaluation(handle, MemoVerdict::Exact, lowered, attribution)?;
            self.counters.memo_hits_exact += 1;
            self.emit(EventKind::Memo {
                location: location.id,
                verdict: MemoVerdict::Exact,
                verified: 0,
            });
            return Ok(Some(evaluation));
        }
        if let Some(entry) = self
            .suffix_memo_candidate(location, demand_key, demand_preimage)
            .cloned()
        {
            let handle = entry.result;
            let evaluation =
                self.memo_evaluation(handle, MemoVerdict::Semantic, lowered, attribution)?;
            self.counters.memo_hits_semantic += 1;
            self.emit(EventKind::Memo {
                location: location.id,
                verdict: MemoVerdict::Semantic,
                verified: entry
                    .receipt
                    .as_ref()
                    .map_or(0, |receipt| receipt.reads.len() as u32),
            });
            return Ok(Some(evaluation));
        }
        if let Some(entry) = self.memo.get(&location.id).cloned()
            && entry.location == *location
            && entry
                .receipt
                .as_ref()
                .is_some_and(|receipt| self.reverify_receipt(receipt))
        {
            let handle = entry.result;
            let evaluation =
                self.memo_evaluation(handle, MemoVerdict::Projection, lowered, attribution)?;
            self.counters.memo_hits_projection += 1;
            self.emit(EventKind::Memo {
                location: location.id,
                verdict: MemoVerdict::Projection,
                verified: entry
                    .receipt
                    .as_ref()
                    .map_or(0, |receipt| receipt.reads.len() as u32),
            });
            return Ok(Some(evaluation));
        }
        Ok(None)
    }

    /// Build the [`Evaluation`] a memo hit publishes from its stored result
    /// handle: its identity, pass/fail verdict, and any recorded failure with
    /// rebuilt source context.
    fn memo_evaluation(
        &self,
        handle: Handle,
        verdict: MemoVerdict,
        lowered: &DemandExecution<'_>,
        attribution: &LoweringAttribution,
    ) -> Result<Evaluation, Box<MachineError>> {
        let failure = self
            .store
            .entry(handle)
            .and_then(StoreEntry::failure)
            .cloned();
        let identity = self
            .store
            .entry(handle)
            .ok_or_else(|| {
                MachineError::runtime(
                    MachineOperation::MemoRead,
                    RuntimeFault::MissingMemoStoreHandle,
                    None,
                    None,
                )
            })?
            .identity
            .clone();
        let passed = failure.is_none()
            && self
                .store
                .entry(handle)
                .and_then(StoreEntry::resident_bytes)
                .is_some_and(|bytes| bytes == [1]);
        Ok(Evaluation {
            handle,
            identity,
            passed,
            memo: verdict,
            failure_context: failure
                .as_ref()
                .and_then(|failure| failure_context(failure, lowered, attribution)),
            failure,
        })
    }

    /// Spawn a fresh runnable task for a missed demand: materialize its
    /// constants, honour the chaos discard, spawn and bind the Weavy task, and
    /// push the owned [`TaskContext`] onto the runnable stack. Never drives.
    fn spawn_context(
        &mut self,
        request: SubmitRequest,
        demand_key: DemandKey,
        demand_preimage: DemandPreimage,
    ) -> Result<(), Box<MachineError>> {
        let SubmitRequest {
            island,
            location,
            lowered,
            attribution,
            arguments,
            wires,
            chaos,
            ancestry,
            realized_as,
        } = request;
        let invocation = DemandExecution::new(lowered.as_ref(), demand_preimage.arguments.clone());
        let lowered_ex = &invocation;
        let attribution_ref = attribution.as_ref();
        let constants = self.materialize_constants(lowered_ex.artifact);
        let mut kill_armed = chaos.kill_first_running_task;
        let (task_id, mut task) = loop {
            self.counters.scheduler_requests += 1;
            let task_id = self.spawn_task(demand_key);
            if matches!(
                lowered_ex.output_type,
                Type::Array(_) | Type::Map { .. } | Type::Set(_) | Type::Enum(_)
            ) {
                self.counters.value_island_spawns += 1;
            }
            self.transition_demand(demand_key, DemandState::Running)?;
            self.transition_task(task_id, TaskState::Running)?;
            self.emit(EventKind::IslandEntered {
                task: task_id,
                island,
            });
            self.emit(EventKind::SafePoint {
                task: task_id,
                class: SafePointClass::Edge,
            });

            if kill_armed {
                kill_armed = false;
                self.counters.task_discards += 1;
                self.transition_task(task_id, TaskState::Discarded)?;
                self.transition_demand(demand_key, DemandState::Queued)?;
                continue;
            }

            let task = match lowered_ex.executable_rc().spawn(FnId(0)) {
                Ok(task) => task,
                Err(fault) => {
                    let error = self.task_fault(
                        MachineOperation::Spawn,
                        fault,
                        lowered_ex,
                        attribution_ref,
                        None,
                    );
                    return Err(Box::new(
                        self.terminate_machine_fault(task_id, demand_key, error),
                    ));
                }
            };
            break (task_id, task);
        };
        let lane_facts = execution_facts(lowered_ex.executable().lane_facts());
        match lane_facts.selected {
            ExecutionLaneFact::Interpreter => self.counters.interpreter_task_spawns += 1,
            ExecutionLaneFact::Native => self.counters.native_task_spawns += 1,
        }
        self.emit(EventKind::ExecutionLane {
            task: task_id,
            facts: lane_facts,
        });
        for (constant, handle) in lowered_ex.constants.iter().zip(constants) {
            let handle = match self.store.weavy_handle(handle) {
                Some(handle) => handle,
                None => {
                    let error = MachineError::runtime(
                        MachineOperation::EntryBinding,
                        RuntimeFault::MissingConstantStoreHandle,
                        self.constant_attribution(constant.node, attribution_ref),
                        Some(demand_key),
                    );
                    return Err(Box::new(
                        self.terminate_machine_fault(task_id, demand_key, error),
                    ));
                }
            };
            if let Err(fault) =
                task.write_entry_store_handle(constant.root.entry, constant.root.schema, handle)
            {
                let error = self.task_fault(
                    MachineOperation::EntryBinding,
                    fault,
                    lowered_ex,
                    attribution_ref,
                    self.constant_attribution(constant.node, attribution_ref),
                );
                return Err(Box::new(
                    self.terminate_machine_fault(task_id, demand_key, error),
                ));
            }
        }
        for (binding, argument) in lowered_ex.value_inputs.iter().zip(&arguments) {
            let frozen = self
                .store
                .entry(argument.handle)
                .and_then(StoreEntry::frozen)
                .map(|frozen| frozen_to_weavy(frozen, &binding.ty, binding, &self.store))
                .transpose()
                .map_err(|()| {
                    Box::new(MachineError::runtime(
                        MachineOperation::EntryBinding,
                        RuntimeFault::ValueInputSchemaMismatch,
                        None,
                        Some(demand_key),
                    ))
                })?;
            let result = if let Some(frozen) = &frozen {
                task.write_entry_frozen(binding.entry, frozen)
            } else {
                if binding.store_schema != argument.identity.schema {
                    let error = MachineError::runtime(
                        MachineOperation::EntryBinding,
                        RuntimeFault::ValueInputSchemaMismatch,
                        None,
                        Some(demand_key),
                    );
                    return Err(Box::new(
                        self.terminate_machine_fault(task_id, demand_key, error),
                    ));
                }
                let Some(handle) = self.store.weavy_handle(argument.handle) else {
                    let error = MachineError::runtime(
                        MachineOperation::EntryBinding,
                        RuntimeFault::MissingValueInputStoreHandle,
                        None,
                        Some(demand_key),
                    );
                    return Err(Box::new(
                        self.terminate_machine_fault(task_id, demand_key, error),
                    ));
                };
                task.write_entry_store_handle(
                    binding.entry,
                    binding.schema.ok_or_else(|| {
                        Box::new(MachineError::runtime(
                            MachineOperation::EntryBinding,
                            RuntimeFault::ValueInputSchemaMismatch,
                            None,
                            Some(demand_key),
                        ))
                    })?,
                    handle,
                )
            };
            if let Err(fault) = result {
                let error = self.task_fault(
                    MachineOperation::EntryBinding,
                    fault,
                    lowered_ex,
                    attribution_ref,
                    None,
                );
                return Err(Box::new(
                    self.terminate_machine_fault(task_id, demand_key, error),
                ));
            }
        }
        let mut value_memory_overrides = Vec::new();
        for (binding, argument) in lowered_ex.value_inputs.iter().zip(&arguments) {
            let Some(element_schema) = binding.payload_element_schema else {
                continue;
            };
            let resident = self
                .store
                .entry(argument.handle)
                .and_then(StoreEntry::resident_bytes)
                .ok_or_else(|| {
                    Box::new(MachineError::runtime(
                        MachineOperation::EntryBinding,
                        RuntimeFault::MissingValueInputStoreHandle,
                        None,
                        Some(demand_key),
                    ))
                })?;
            let mut abi_view = resident.to_vec();
            let schema_bytes = abi_view.get_mut(8..16).ok_or_else(|| {
                Box::new(MachineError::runtime(
                    MachineOperation::EntryBinding,
                    RuntimeFault::ValueInputSchemaMismatch,
                    None,
                    Some(demand_key),
                ))
            })?;
            schema_bytes.copy_from_slice(&i64::from(element_schema.0).to_le_bytes());
            value_memory_overrides.push((argument.handle, abi_view));
        }
        let ready = vec![false; wires.len()];
        let awaited = vec![0i64; wires.len()];
        drop(invocation);
        self.runnable.push(TaskContext {
            task_id,
            island,
            demand_key,
            demand_preimage,
            lowered,
            location,
            attribution,
            arguments,
            wires,
            ancestry,
            realized_as,
            task,
            ready,
            awaited,
            primitive_reads: Vec::new(),
            value_memory_overrides,
            kill_on_primitive_park: chaos.kill_first_parked_primitive,
        });
        Ok(())
    }

    /// Drive the entire runnable/parked graph until the root demand resolves.
    /// Each iteration takes one runnable task, drives it one segment, and puts
    /// it back parked or publishes its completion. A quiescent loop that leaves
    /// the root unresolved is a typed machine fault.
    ///
    /// r[impl machine.scheduler.block-on-event]
    /// r[impl machine.scheduler.completion-resumes-direct]
    fn run_until_root(&mut self, root: DemandKey) -> Result<Evaluation, Box<MachineError>> {
        let result = self.pump_until(|runtime| runtime.root_results.contains_key(&root));
        match result {
            Ok(()) => match self.root_results.remove(&root) {
                Some(evaluation) => Ok(evaluation),
                None => {
                    let error = Box::new(MachineError::runtime(
                        MachineOperation::Drive,
                        RuntimeFault::QuiescentUnresolvedDemand { key: root },
                        None,
                        Some(root),
                    ));
                    self.clear_transient_scheduler_state();
                    Err(error)
                }
            },
            Err(error) => {
                // A machine fault abandons the run; clear every transient
                // scheduler map so a reused runtime never drives stale frames
                // and no leftover state can reach persistent extraction.
                self.clear_transient_scheduler_state();
                Err(error)
            }
        }
    }

    /// Drive the shared scheduler until any one of `roots` publishes, then
    /// remove and return that root's evaluation. Runnable work across every
    /// submitted root drains before the loop blocks on the unified inbox.
    ///
    /// The caller retains the remaining roots and may submit newly-ready
    /// dependents before harvesting again; scheduler state therefore persists
    /// across frontier rounds instead of being torn down per island.
    pub fn run_until_any(
        &mut self,
        roots: &[DemandKey],
    ) -> Result<(DemandKey, Evaluation), Box<MachineError>> {
        let unresolved = roots
            .first()
            .copied()
            .expect("run_until_any requires at least one submitted root");
        let result = self.pump_until(|runtime| {
            roots
                .iter()
                .any(|root| runtime.root_results.contains_key(root))
        });
        if let Err(error) = result {
            self.clear_transient_scheduler_state();
            return Err(error);
        }
        for root in roots {
            if let Some(evaluation) = self.root_results.remove(root) {
                return Ok((*root, evaluation));
            }
        }
        let error = Box::new(MachineError::runtime(
            MachineOperation::Drive,
            RuntimeFault::QuiescentUnresolvedDemand { key: unresolved },
            None,
            Some(unresolved),
        ));
        self.clear_transient_scheduler_state();
        Err(error)
    }

    /// Finish one multi-root frontier after all submitted roots were harvested.
    /// Child-demand result entries are transient scratch and are discarded only
    /// after the scheduler has no runnable, parked, waiting, or pending work.
    pub fn finish_root_batch(&mut self) {
        assert!(
            self.runnable.is_empty()
                && self.parked.is_empty()
                && self.wire_waiters.is_empty()
                && self.primitive_pending.is_empty()
                && self.exec_pending.is_empty()
                && self.exec_projection_pending.is_empty(),
            "root batch finished with live scheduler work: \
             runnable={}, parked={}, wire_waiters={}, primitive_pending={}, exec_pending={}, \
             exec_projection_pending={}",
            self.runnable.len(),
            self.parked.len(),
            self.wire_waiters.len(),
            self.primitive_pending.len(),
            self.exec_pending.len(),
            self.exec_projection_pending.len(),
        );
        self.root_results.clear();
        self.exec_progress_ready.clear();
    }

    /// Explicitly abandon one unresolved island demand. Every retained frame
    /// for that demand is discarded. If it was the final waiter on a primitive
    /// ticket, the demand-owned ticket is cancelled and retired without
    /// publishing a memo, claim, receipt, or partial value.
    ///
    /// A chaos kill does not call this method: it transitions the demand back
    /// to `Queued`, preserving the ticket obligation so replay joins it.
    pub fn abandon_demand(&mut self, demand: DemandKey) -> Result<bool, Box<MachineError>> {
        let mut task_ids = self
            .runnable
            .iter()
            .chain(self.parked.values())
            .filter(|ctx| ctx.demand_key == demand)
            .map(|ctx| ctx.task_id)
            .collect::<Vec<_>>();
        for pending in self.primitive_pending.values() {
            task_ids.extend(
                pending
                    .waiters
                    .iter()
                    .filter(|waiter| waiter.ctx.demand_key == demand)
                    .map(|waiter| waiter.ctx.task_id),
            );
        }
        task_ids.sort();
        task_ids.dedup();
        let existed = !task_ids.is_empty()
            || self.demands.contains_key(&demand)
            || self.root_results.contains_key(&demand);

        for task_id in &task_ids {
            self.transition_task(*task_id, TaskState::Discarded)?;
        }
        self.runnable.retain(|ctx| ctx.demand_key != demand);
        self.parked.retain(|_, ctx| ctx.demand_key != demand);
        for waiters in self.wire_waiters.values_mut() {
            waiters.retain(|waiter| !task_ids.contains(&waiter.task_id));
        }
        self.wire_waiters.retain(|_, waiters| !waiters.is_empty());

        for pending in self.primitive_pending.values_mut() {
            pending
                .waiters
                .retain(|waiter| waiter.ctx.demand_key != demand);
        }
        let abandoned_effects = self
            .primitive_pending
            .iter()
            .filter_map(|(primitive_demand, pending)| {
                pending.waiters.is_empty().then_some(*primitive_demand)
            })
            .collect::<Vec<_>>();
        for primitive_demand in abandoned_effects {
            if let Some(pending) = self.primitive_pending.remove(&primitive_demand) {
                drop(pending.subscription);
                if pending.ticket.cancel_demand() {
                    self.counters.effect_cancellations += 1;
                }
                self.primitive_dispatcher.retire(primitive_demand);
            }
        }

        self.root_results.remove(&demand);
        if self.demands.contains_key(&demand) {
            self.transition_demand(demand, DemandState::Absent)?;
            self.demands.remove(&demand);
        }
        Ok(existed)
    }

    /// Drive the scheduler's runnable/parked/pending graph until `done` holds.
    /// Each round takes one runnable task and drives it a `take-run-put`
    /// segment; only when no task is runnable does the scheduler block on the
    /// unified completion inbox for one event and apply it. A quiescent graph —
    /// nothing runnable and nothing pending — that has not satisfied `done` is a
    /// typed machine fault raised by the caller.
    ///
    /// r[impl machine.scheduler.block-on-event]
    /// r[impl machine.scheduler.effect-overlap]
    fn pump_until(&mut self, done: impl Fn(&Self) -> bool) -> Result<(), Box<MachineError>> {
        while !done(self) {
            if let Some(mut ctx) = self.runnable.pop() {
                if !self.primitive_pending.is_empty() || !self.exec_pending.is_empty() {
                    self.counters.overlap_observations += 1;
                }
                match self.drive_context(&mut ctx)? {
                    DriveOutcome::Completed(evaluation) => {
                        if evaluation.memo == MemoVerdict::Miss
                            && let Some(realized) = ctx.realized_as.take()
                        {
                            self.wire_demands.push(realized);
                        }
                        self.publish(ctx.demand_key, *evaluation)?;
                    }
                    DriveOutcome::Parked(input) => {
                        self.handle_park(ctx, input)?;
                    }
                    DriveOutcome::YieldedPrimitive(request) => {
                        self.begin_primitive(ctx, request)?;
                    }
                }
            } else if !self.primitive_pending.is_empty() || !self.exec_pending.is_empty() {
                self.drain_one_completion()?;
            } else {
                // Quiescent: nothing runnable, nothing pending, `done` unmet.
                break;
            }
        }
        Ok(())
    }

    /// Block on the unified inbox for exactly one completion and apply it. The
    /// scheduler thread performs no per-demand synchronous drain — it consumes
    /// whatever the one receive authority delivers and routes it through the
    /// shared `apply_completion` path.
    fn drain_one_completion(&mut self) -> Result<(), Box<MachineError>> {
        let completion = self.completion_inbox.recv().map_err(|LostCompletion| {
            Box::new(MachineError::runtime(
                MachineOperation::Drive,
                RuntimeFault::LostCompletion,
                None,
                None,
            ))
        })?;
        self.counters.completion_inbox_receipts += 1;
        self.apply_completion(completion)
    }

    /// Clear every transient scheduler map. Called on machine-error teardown so
    /// a faulted runtime carries no runnable/parked/pending/result residue into
    /// a later run or persistent-state extraction.
    fn clear_transient_scheduler_state(&mut self) {
        self.runnable.clear();
        self.parked.clear();
        self.wire_waiters.clear();
        self.root_results.clear();
        let pending = std::mem::take(&mut self.primitive_pending);
        for (demand, pending) in pending {
            drop(pending.subscription);
            if pending.ticket.cancel_demand() {
                self.counters.effect_cancellations += 1;
            }
            self.primitive_dispatcher.retire(demand);
        }
        self.exec_pending.clear();
        self.exec_projection_pending.clear();
        self.exec_progress_ready.clear();
    }

    /// Observe the scheduler-owned effect frontier after admitting a fresh
    /// primitive or exec demand. Two simultaneous pending effects are causal
    /// overlap regardless of worker completion timing.
    fn observe_effect_frontier(&mut self) {
        let pending = self.primitive_pending.len() + self.exec_pending.len();
        self.counters.peak_effects_in_flight =
            self.counters.peak_effects_in_flight.max(pending as u64);
        if pending > 1 {
            self.counters.overlap_observations += 1;
        }
    }

    /// Publish a demand's completion: record its result and resume every parent
    /// parked on it, filling the awaited slot and returning the parent to the
    /// runnable stack (or cascading a failure).
    fn publish(
        &mut self,
        demand: DemandKey,
        evaluation: Evaluation,
    ) -> Result<(), Box<MachineError>> {
        self.root_results.insert(demand, evaluation.clone());
        let waiters = self.wire_waiters.remove(&demand).unwrap_or_default();
        for waiter in waiters {
            let parent = self
                .parked
                .remove(&waiter.task_id)
                .expect("a wire waiter names a parked task");
            self.resume_parent(parent, waiter.wire_index, evaluation.clone())?;
        }
        Ok(())
    }

    /// Resume a parent suspended on wire `wire_index` with the wire's resolved
    /// result. Propagates a language failure to the parent demand (omitting
    /// `transition_task`, as the historical wire-failure cascade did), or fills
    /// the awaited slot and returns the parent to the runnable stack. Realized
    /// demand recording belongs to the computing child task, never this waiter
    /// edge, so multiple parents joining one child still observe one realization.
    fn resume_parent(
        &mut self,
        mut parent: TaskContext,
        wire_index: usize,
        resolved: Evaluation,
    ) -> Result<(), Box<MachineError>> {
        if let Some(failure) = resolved.failure {
            // A demanded argument failed on the language plane; propagate the
            // typed failure with its authored source site to the parent demand.
            // The parent task is abandoned without a `transition_task`, exactly
            // as the recursive wire-failure cascade left it.
            self.memo.insert(
                parent.location.id,
                MemoEntry {
                    location: parent.location.clone(),
                    key: parent.demand_key,
                    preimage: parent.demand_preimage.clone(),
                    result: resolved.handle,
                    receipt: None,
                    current_receipt: false,
                },
            );
            if let Some(demand) = self.demands.get_mut(&parent.demand_key) {
                demand.result = Some(resolved.handle);
            }
            self.transition_demand(parent.demand_key, DemandState::Failed)?;
            let evaluation = Evaluation {
                handle: resolved.handle,
                identity: resolved.identity,
                passed: false,
                memo: MemoVerdict::Miss,
                failure: Some(failure),
                failure_context: resolved.failure_context,
            };
            self.publish(parent.demand_key, evaluation)?;
            return Ok(());
        }
        let word = self.scalar_word(resolved.handle).ok_or_else(|| {
            Box::new(MachineError::runtime(
                MachineOperation::Drive,
                RuntimeFault::PureIslandParked {
                    input: wire_index as u32,
                },
                None,
                Some(parent.demand_key),
            ))
        })?;
        parent.awaited[wire_index] = word;
        parent.ready[wire_index] = true;
        self.emit(EventKind::WeavyResumed {
            task: parent.task_id,
        });
        self.runnable.push(parent);
        Ok(())
    }

    /// Handle a task that parked on wire `input`: resolve the wire's demand.
    /// On a memo hit or immediate failure the parent resumes inline; on a fresh
    /// spawn or a join the parent is registered as a waiter and parked off the
    /// drive stack until the wire's demand completes.
    fn handle_park(&mut self, ctx: TaskContext, input: u32) -> Result<(), Box<MachineError>> {
        let index = input as usize;
        let Some(wire) = ctx.wires.get(index) else {
            let error = MachineError::runtime(
                MachineOperation::Drive,
                RuntimeFault::PureIslandParked { input },
                None,
                Some(ctx.demand_key),
            );
            return Err(Box::new(self.terminate_machine_fault(
                ctx.task_id,
                ctx.demand_key,
                error,
            )));
        };
        self.emit(EventKind::WeavyParked {
            task: ctx.task_id,
            input,
        });
        let wire = wire.clone();
        let realized_as = Some(RealizedWireDemand {
            function: wire.function,
            arguments: wire.demand_arguments.clone(),
            preimage: wire.preimage.clone(),
        });
        let mut child_ancestry = ctx.ancestry.clone();
        child_ancestry.push(ctx.demand_key);
        let outcome = self.submit_demand(SubmitRequest {
            island: wire.island,
            location: wire.location,
            lowered: wire.lowered,
            attribution: wire.attribution,
            arguments: wire.arguments,
            wires: wire.wires,
            chaos: ChaosPolicy::default(),
            ancestry: child_ancestry,
            realized_as,
        })?;
        match outcome {
            SubmitOutcome::Ready(resolved) => {
                self.resume_parent(ctx, index, resolved)?;
            }
            SubmitOutcome::Spawned(child) | SubmitOutcome::Joined(child) => {
                self.wire_waiters
                    .entry(child)
                    .or_default()
                    .push(WireWaiter {
                        task_id: ctx.task_id,
                        wire_index: index,
                    });
                self.parked.insert(ctx.task_id, ctx);
                // Witness that a wire-parked frame now resides in the scheduler's
                // off-stack parked map rather than on the recursive Rust stack.
                self.counters.peak_parked_frames = self
                    .counters
                    .peak_parked_frames
                    .max(self.parked.len() as u64);
            }
        }
        Ok(())
    }

    /// Drive one runnable task one `take-run-put` segment: run the retained
    /// Weavy task until it parks on an unresolved wire (returned to the loop) or
    /// runs to `Done` (decoded, memoized, and published). Registered-primitive
    /// yields are serviced inline through the unified completion inbox.
    fn drive_context(&mut self, ctx: &mut TaskContext) -> Result<DriveOutcome, Box<MachineError>> {
        let invocation = DemandExecution::new(
            ctx.lowered.as_ref(),
            ctx.arguments
                .iter()
                .map(|argument| argument.identity.clone())
                .collect(),
        );
        let lowered = &invocation;
        let attribution = Rc::clone(&ctx.attribution);
        let attribution = attribution.as_ref();
        let task_id = ctx.task_id;
        let demand_key = ctx.demand_key;
        let mut primitive_host = PrimitiveHostQueue::default();
        let step = {
            let mut call_primitive = |frame: &mut [u8]| primitive_host.call(frame);
            let mut hosts: [HostFn<'_>; 1] = [&mut call_primitive];
            match self.store.with_value_memory_overrides(
                &ctx.value_memory_overrides,
                |value_memories| {
                    ctx.task
                        .drive_hosted_with_value_memories(
                            &mut ctx.ready,
                            &ctx.awaited,
                            &mut hosts,
                            value_memories,
                        )
                        .map_err(Box::new)
                },
            ) {
                Ok(step) => step,
                Err(fault) => {
                    let error = self.task_fault(
                        MachineOperation::Drive,
                        *fault,
                        lowered,
                        attribution,
                        None,
                    );
                    return Err(Box::new(
                        self.terminate_machine_fault(task_id, demand_key, error),
                    ));
                }
            }
        };
        match step {
            TaskStep::Done => {}
            TaskStep::Yielded => {
                // A registered-primitive `HostCallYield`. Do not complete it
                // inline: return control to the loop so the frame parks in
                // demand-owned pending state and its completion crosses the
                // unified inbox. Only an ABI-shape fault terminates here.
                match (primitive_host.fault, primitive_host.requests.as_slice()) {
                    (None, [request]) => {
                        return Ok(DriveOutcome::YieldedPrimitive(request.clone()));
                    }
                    (Some(detail), _) => {
                        let error = MachineError::runtime(
                            MachineOperation::Drive,
                            primitive_runtime_fault(PrimitiveHostFailure::Abi(detail)),
                            None,
                            Some(demand_key),
                        );
                        return Err(Box::new(
                            self.terminate_machine_fault(task_id, demand_key, error),
                        ));
                    }
                    (None, requests) => {
                        let error = MachineError::runtime(
                            MachineOperation::Drive,
                            primitive_runtime_fault(PrimitiveHostFailure::Abi(format!(
                                "primitive yield recorded {} host requests",
                                requests.len()
                            ))),
                            None,
                            Some(demand_key),
                        );
                        return Err(Box::new(
                            self.terminate_machine_fault(task_id, demand_key, error),
                        ));
                    }
                }
            }
            TaskStep::Parked { input } => {
                return Ok(DriveOutcome::Parked(input));
            }
        }
        for event in ctx.task.trace() {
            if let Err(error) = self.emit_weavy(task_id, *event, attribution, demand_key) {
                return Err(Box::new(
                    self.terminate_machine_fault(task_id, demand_key, *error),
                ));
            }
        }
        let primitive_receipt = (!ctx.primitive_reads.is_empty()).then(|| Receipt {
            demand: demand_key,
            reads: ctx.primitive_reads.clone(),
        });
        let location = &ctx.location;
        let passed = match decode_result(&ctx.task, lowered) {
            Ok(DecodedResult::OkScalar(passed)) => passed,
            Ok(DecodedResult::OkScalarValue(word)) => {
                // A hoisted wire invocation published its demanded scalar. It
                // interns under its semantic schema exactly as an evaluated
                // scalar would, so equal recipe+argument demands share one
                // identity and memoize once.
                let width = lowered
                    .output_type
                    .word_width()
                    .and_then(|words| words.checked_mul(8))
                    .unwrap_or(8);
                let bytes = &word.to_le_bytes()[..width.min(8)];
                let interned = self
                    .store
                    .intern_realized(semantic_schema_ref(&lowered.output_type), bytes);
                self.store
                    .attach_frozen(interned.handle, FrozenValue::Inline(bytes.to_vec()));
                self.observe_interned(&interned);
                self.memo.insert(
                    location.id,
                    MemoEntry {
                        location: location.clone(),
                        key: demand_key,
                        preimage: lowered.demand_preimage.clone(),
                        result: interned.handle,
                        receipt: primitive_receipt.clone(),
                        current_receipt: primitive_receipt.is_some(),
                    },
                );
                if let Some(demand) = self.demands.get_mut(&demand_key) {
                    demand.result = Some(interned.handle);
                }
                self.transition_task(task_id, TaskState::Completed)?;
                self.transition_demand(demand_key, DemandState::Ready)?;
                self.emit(EventKind::Completed {
                    key: demand_key,
                    identity: interned.identity.clone(),
                });
                return Ok(DriveOutcome::Completed(Box::new(Evaluation {
                    handle: interned.handle,
                    identity: interned.identity.clone(),
                    passed: true,
                    memo: MemoVerdict::Miss,
                    failure: None,
                    failure_context: None,
                })));
            }
            Ok(DecodedResult::OkValue) => {
                let realized = match realize_value(&ctx.task, lowered.artifact, &self.store) {
                    Ok(realized) => realized,
                    Err(fault) => {
                        let error = self.task_fault(
                            MachineOperation::Result,
                            fault,
                            lowered,
                            attribution,
                            self.output_attribution(lowered.artifact, attribution),
                        );
                        return Err(Box::new(
                            self.terminate_machine_fault(task_id, demand_key, error),
                        ));
                    }
                };
                self.counters.peak_molten_nodes = self
                    .counters
                    .peak_molten_nodes
                    .max(realized.molten_nodes as u64);
                self.counters.peak_molten_bytes = self
                    .counters
                    .peak_molten_bytes
                    .max(realized.molten_bytes as u64);
                self.counters.framed_bytes += realized.framed_bytes as u64;
                let interned = self.store.intern_tree(&realized.node, &realized.resident);
                if let Some(frozen) = realized.frozen {
                    self.store.attach_frozen(interned.handle, frozen);
                }
                self.observe_interned(&interned);
                self.counters.successful_aggregate_freezes += 1;
                if lowered.forced_copy_value {
                    self.counters.forced_copy_selections += 1;
                } else {
                    self.counters.active_molten_selections += 1;
                }
                self.memo.insert(
                    location.id,
                    MemoEntry {
                        location: location.clone(),
                        key: demand_key,
                        preimage: lowered.demand_preimage.clone(),
                        result: interned.handle,
                        receipt: primitive_receipt.clone(),
                        current_receipt: primitive_receipt.is_some(),
                    },
                );
                if let Some(demand) = self.demands.get_mut(&demand_key) {
                    demand.result = Some(interned.handle);
                }
                self.transition_task(task_id, TaskState::Completed)?;
                self.transition_demand(demand_key, DemandState::Ready)?;
                self.emit(EventKind::Completed {
                    key: demand_key,
                    identity: interned.identity.clone(),
                });
                return Ok(DriveOutcome::Completed(Box::new(Evaluation {
                    handle: interned.handle,
                    identity: interned.identity.clone(),
                    passed: true,
                    memo: MemoVerdict::Miss,
                    failure: None,
                    failure_context: None,
                })));
            }
            Ok(DecodedResult::ArrayMachine { site, status }) => {
                let error = MachineError::runtime(
                    MachineOperation::Result,
                    RuntimeFault::ArrayMachineStatus { site, status },
                    self.output_attribution(lowered, attribution),
                    Some(demand_key),
                );
                return Err(Box::new(
                    self.terminate_machine_fault(task_id, demand_key, error),
                ));
            }
            Ok(DecodedResult::OrderedMachine { site, status }) => {
                let error = MachineError::runtime(
                    MachineOperation::Result,
                    RuntimeFault::OrderedMachineStatus { site, status },
                    self.output_attribution(lowered, attribution),
                    Some(demand_key),
                );
                return Err(Box::new(
                    self.terminate_machine_fault(task_id, demand_key, error),
                ));
            }
            // r[impl machine.error.index-out-of-bounds]
            Ok(DecodedResult::IndexOutOfBounds {
                site,
                index,
                length,
            }) => {
                let failure = FailureValue::IndexOutOfBounds {
                    recipe: lowered.recipe,
                    site,
                    index,
                    length,
                    subject: None,
                };
                let report_context = failure_context(&failure, lowered, attribution);
                let interned = self.store.intern_failure(failure.clone(), &[]);
                self.observe_interned(&interned);
                self.memo.insert(
                    location.id,
                    MemoEntry {
                        location: location.clone(),
                        key: demand_key,
                        preimage: lowered.demand_preimage.clone(),
                        result: interned.handle,
                        receipt: primitive_receipt.clone(),
                        current_receipt: primitive_receipt.is_some(),
                    },
                );
                if let Some(demand) = self.demands.get_mut(&demand_key) {
                    demand.result = Some(interned.handle);
                }
                self.transition_task(task_id, TaskState::Completed)?;
                self.transition_demand(demand_key, DemandState::Failed)?;
                self.emit(EventKind::LanguageFailed {
                    task: task_id,
                    key: demand_key,
                    failure: failure.clone(),
                });
                return Ok(DriveOutcome::Completed(Box::new(Evaluation {
                    handle: interned.handle,
                    identity: interned.identity.clone(),
                    passed: false,
                    memo: MemoVerdict::Miss,
                    failure: Some(failure),
                    failure_context: report_context,
                })));
            }
            Ok(DecodedResult::MissingKey { site }) => {
                let failure = FailureValue::MissingKey {
                    recipe: lowered.recipe,
                    site,
                };
                return self
                    .complete_language_failure(task_id, location, lowered, attribution, failure)
                    .map(|evaluation| DriveOutcome::Completed(Box::new(evaluation)));
            }
            Ok(DecodedResult::DuplicateKey { site }) => {
                let failure = FailureValue::DuplicateKey {
                    recipe: lowered.recipe,
                    site,
                };
                return self
                    .complete_language_failure(task_id, location, lowered, attribution, failure)
                    .map(|evaluation| DriveOutcome::Completed(Box::new(evaluation)));
            }
            Ok(DecodedResult::MissingDelimiter { site }) => {
                return self
                    .complete_language_failure(
                        task_id,
                        location,
                        lowered,
                        attribution,
                        FailureValue::MissingDelimiter {
                            recipe: lowered.recipe,
                            site,
                        },
                    )
                    .map(|evaluation| DriveOutcome::Completed(Box::new(evaluation)));
            }
            Ok(DecodedResult::InvalidInteger { site }) => {
                return self
                    .complete_language_failure(
                        task_id,
                        location,
                        lowered,
                        attribution,
                        FailureValue::InvalidInteger {
                            recipe: lowered.recipe,
                            site,
                        },
                    )
                    .map(|evaluation| DriveOutcome::Completed(Box::new(evaluation)));
            }
            Ok(DecodedResult::IntegerOverflow { site }) => {
                return self
                    .complete_language_failure(
                        task_id,
                        location,
                        lowered,
                        attribution,
                        FailureValue::IntegerOverflow {
                            recipe: lowered.recipe,
                            site,
                        },
                    )
                    .map(|evaluation| DriveOutcome::Completed(Box::new(evaluation)));
            }
            Ok(DecodedResult::IntDivisionByZero { site }) => {
                return self
                    .complete_language_failure(
                        task_id,
                        location,
                        lowered,
                        attribution,
                        FailureValue::DivisionByZero {
                            recipe: lowered.recipe,
                            site,
                        },
                    )
                    .map(|evaluation| DriveOutcome::Completed(Box::new(evaluation)));
            }
            Err(fault) => {
                let fallback =
                    result_shape_attribution(&fault, self.output_attribution(lowered, attribution));
                let error = self.task_fault(
                    MachineOperation::Result,
                    *fault,
                    lowered,
                    attribution,
                    fallback,
                );
                return Err(Box::new(
                    self.terminate_machine_fault(task_id, demand_key, error),
                ));
            }
        };
        let interned = self
            .store
            .intern_realized(Type::Check.schema_ref(), &[u8::from(passed)]);
        self.observe_interned(&interned);

        self.memo.insert(
            location.id,
            MemoEntry {
                location: location.clone(),
                key: demand_key,
                preimage: lowered.demand_preimage.clone(),
                result: interned.handle,
                receipt: primitive_receipt.clone(),
                current_receipt: primitive_receipt.is_some(),
            },
        );
        if let Some(demand) = self.demands.get_mut(&demand_key) {
            demand.result = Some(interned.handle);
        }
        self.transition_task(task_id, TaskState::Completed)?;
        self.transition_demand(demand_key, DemandState::Ready)?;
        self.emit(EventKind::Completed {
            key: demand_key,
            identity: interned.identity.clone(),
        });
        Ok(DriveOutcome::Completed(Box::new(Evaluation {
            handle: interned.handle,
            identity: interned.identity,
            passed,
            memo: MemoVerdict::Miss,
            failure: None,
            failure_context: None,
        })))
    }

    /// Evaluate one machine-plane effect island. Effects use the same demand,
    /// task, memo, store, and receipt authority as Weavy islands; only their
    /// operation interpreter is different. The fixture root is reachable here
    /// and nowhere else in the production runner.
    pub fn evaluate_effect(
        &mut self,
        island: IslandId,
        location: &Location,
        fingerprint: &str,
        effect: &Island,
        arguments: &[Evaluation],
        chaos: ChaosPolicy,
    ) -> Result<Evaluation, Box<MachineError>> {
        let recipe = RecipeId::from_effect_fingerprint(fingerprint);
        let preimage = DemandPreimage {
            closure: recipe,
            arguments: arguments
                .iter()
                .map(|argument| argument.identity.clone())
                .collect(),
        };
        let key = DemandKey::from_preimage(&preimage);
        self.emit(EventKind::Demanded { key });
        let effect_output = effect
            .nodes
            .iter()
            .find(|node| node.id == effect.output)
            .ok_or_else(|| {
                Box::new(MachineError::runtime(
                    MachineOperation::Effect,
                    RuntimeFault::EffectPlane {
                        detail: "effect island output node was missing",
                    },
                    None,
                    Some(key),
                ))
            })?;
        let force_miss = self.effect_fixture_overlay_active(effect);
        let memo_handle = (!force_miss)
            .then(|| {
                self.memo.get(&location.id).and_then(|entry| {
                    (entry.location == *location
                        && entry.key == key
                        && entry.preimage == preimage
                        && self.exact_memo_replayable(entry))
                    .then_some(entry.result)
                })
            })
            .flatten();
        if let Some(handle) = memo_handle {
            let (identity, failure) = match self.store.entry(handle) {
                Some(stored) => (stored.identity.clone(), stored.failure().cloned()),
                None => {
                    return Err(Box::new(MachineError::runtime(
                        MachineOperation::MemoRead,
                        RuntimeFault::MissingMemoStoreHandle,
                        None,
                        Some(key),
                    )));
                }
            };
            self.counters.memo_hits_exact += 1;
            self.emit(EventKind::Memo {
                location: location.id,
                verdict: MemoVerdict::Exact,
                verified: 0,
            });
            return Ok(Evaluation {
                handle,
                identity,
                passed: failure.is_none(),
                memo: MemoVerdict::Exact,
                failure,
                failure_context: None,
            });
        }
        if !force_miss
            && let Some(entry) = self.memo.get(&location.id).cloned()
            && entry.location == *location
            && entry
                .receipt
                .as_ref()
                .is_some_and(|receipt| self.reverify_receipt(receipt))
        {
            let (identity, failure) = match self.store.entry(entry.result) {
                Some(stored) => (stored.identity.clone(), stored.failure().cloned()),
                None => {
                    return Err(Box::new(MachineError::runtime(
                        MachineOperation::MemoRead,
                        RuntimeFault::MissingMemoStoreHandle,
                        None,
                        Some(key),
                    )));
                }
            };
            self.counters.memo_hits_projection += 1;
            self.emit(EventKind::Memo {
                location: location.id,
                verdict: MemoVerdict::Projection,
                verified: entry
                    .receipt
                    .as_ref()
                    .map_or(0, |receipt| receipt.reads.len() as u32),
            });
            return Ok(Evaluation {
                handle: entry.result,
                identity,
                passed: failure.is_none(),
                memo: MemoVerdict::Projection,
                failure,
                failure_context: None,
            });
        }
        self.counters.memo_misses += 1;
        self.emit(EventKind::Memo {
            location: location.id,
            verdict: MemoVerdict::Miss,
            verified: 0,
        });
        self.demands.insert(
            key,
            DemandRecord {
                key,
                state: DemandState::Queued,
                result: None,
            },
        );
        let output_ty = effect_output.ty.clone();
        self.emit(EventKind::DemandTransition {
            key,
            from: DemandState::Absent,
            to: DemandState::Queued,
        });
        let mut kill_armed = chaos.kill_first_running_task;
        loop {
            self.counters.scheduler_requests += 1;
            let task = self.spawn_task(key);
            self.transition_demand(key, DemandState::Running)?;
            self.transition_task(task, TaskState::Running)?;
            self.emit(EventKind::IslandEntered { task, island });
            self.emit(EventKind::SafePoint {
                task,
                class: SafePointClass::Edge,
            });
            if kill_armed {
                kill_armed = false;
                self.counters.task_discards += 1;
                self.transition_task(task, TaskState::Discarded)?;
                self.transition_demand(key, DemandState::Queued)?;
                continue;
            }
            self.counters.effect_spawns += 1;
            let mut reads = Vec::new();
            let arguments = self.effect_arguments(arguments)?;
            let value = self.evaluate_effect_node(
                effect,
                effect.function,
                effect.output,
                &arguments,
                &mut reads,
            )?;
            let EffectTerm::Value(value) = value else {
                return Err(Box::new(self.terminate_machine_fault(
                    task,
                    key,
                    MachineError::runtime(
                        MachineOperation::Effect,
                        RuntimeFault::EffectPlane {
                            detail: "effect island output was unresolved codata",
                        },
                        None,
                        Some(key),
                    ),
                )));
            };
            let node = value.node.unwrap_or_else(|| {
                FramedNode::leaf(effect_schema(&output_ty), value.resident.clone())
            });
            let interned = self.store.intern_tree(&node, &value.resident);
            if let Some(frozen) = value.frozen {
                self.store.attach_frozen(interned.handle, frozen);
            }
            self.observe_interned(&interned);
            self.memo.insert(
                location.id,
                MemoEntry {
                    location: location.clone(),
                    key,
                    preimage: preimage.clone(),
                    result: interned.handle,
                    receipt: Some(Receipt {
                        demand: key,
                        reads: reads.clone(),
                    }),
                    current_receipt: true,
                },
            );
            if let Some(demand) = self.demands.get_mut(&key) {
                demand.result = Some(interned.handle);
            }
            self.transition_task(task, TaskState::Completed)?;
            self.transition_demand(key, DemandState::Ready)?;
            self.emit(EventKind::Completed {
                key,
                identity: interned.identity.clone(),
            });
            return Ok(Evaluation {
                handle: interned.handle,
                identity: interned.identity,
                passed: true,
                memo: MemoVerdict::Miss,
                failure: None,
                failure_context: None,
            });
        }
    }

    fn effect_arguments(
        &self,
        arguments: &[Evaluation],
    ) -> Result<Vec<EffectValue>, Box<MachineError>> {
        arguments
            .iter()
            .map(|argument| {
                let stored = self.store.entry(argument.handle).ok_or_else(|| {
                    Box::new(MachineError::runtime(
                        MachineOperation::Effect,
                        RuntimeFault::EffectPlane {
                            detail: "effect argument store handle vanished",
                        },
                        None,
                        None,
                    ))
                })?;
                Ok(EffectValue {
                    identity: argument.identity.clone(),
                    resident: stored.resident_bytes().unwrap_or_default().to_vec(),
                    frozen: stored.frozen().cloned(),
                    node: None,
                })
            })
            .collect()
    }

    fn effect_function(
        island: &Island,
        function: FunctionId,
    ) -> Option<(
        &[crate::vir::Parameter],
        &[crate::vir::Node],
        Option<NodeId>,
    )> {
        if island.function == function {
            return Some((&island.parameters, &island.nodes, Some(island.output)));
        }
        island
            .callees
            .iter()
            .find(|callee| callee.id == function)
            .map(|callee: &Function| {
                (
                    callee.parameters.as_slice(),
                    callee.nodes.as_slice(),
                    callee.output,
                )
            })
    }

    fn effect_fixture_overlay_active(&self, effect: &Island) -> bool {
        let Some(overlay) = self.fixture_store.rerun_overlay() else {
            return false;
        };
        let Some(output) = effect.nodes.iter().find(|node| node.id == effect.output) else {
            return false;
        };
        if !matches!(output.op, Op::FixtureTree) {
            return false;
        }
        let Some(name_node) = output
            .inputs
            .first()
            .and_then(|input| effect.nodes.iter().find(|node| node.id == *input))
        else {
            return false;
        };
        matches!(&name_node.op, Op::String(name) if name == overlay)
    }

    fn evaluate_effect_node(
        &mut self,
        island: &Island,
        function: FunctionId,
        node: NodeId,
        arguments: &[EffectValue],
        reads: &mut Vec<super::model::ReadWitness>,
    ) -> Result<EffectTerm, Box<MachineError>> {
        let (_, nodes, _) = Self::effect_function(island, function).ok_or_else(|| {
            Box::new(MachineError::runtime(
                MachineOperation::Effect,
                RuntimeFault::EffectPlane {
                    detail: "effect island referenced a missing function",
                },
                None,
                None,
            ))
        })?;
        let node = nodes
            .iter()
            .find(|candidate| candidate.id == node)
            .ok_or_else(|| {
                Box::new(MachineError::runtime(
                    MachineOperation::Effect,
                    RuntimeFault::EffectPlane {
                        detail: "effect island referenced a missing node",
                    },
                    None,
                    None,
                ))
            })?;
        let mut input = |index: usize, this: &mut Self| {
            let id = *node.inputs.get(index).ok_or_else(|| {
                Box::new(MachineError::runtime(
                    MachineOperation::Effect,
                    RuntimeFault::EffectPlane {
                        detail: "effect primitive is missing an operand",
                    },
                    None,
                    None,
                ))
            })?;
            this.evaluate_effect_node(island, function, id, arguments, reads)
        };
        match &node.op {
            Op::Parameter(id) => {
                let argument = arguments.get(id.0 as usize).ok_or_else(|| {
                    Box::new(MachineError::runtime(
                        MachineOperation::Effect,
                        RuntimeFault::EffectPlane {
                            detail: "effect parameter has no published argument",
                        },
                        None,
                        None,
                    ))
                })?;
                Ok(EffectTerm::Value(argument.clone()))
            }
            Op::Int(value) => Ok(EffectTerm::Value(effect_leaf(
                &node.ty,
                value.to_le_bytes().to_vec(),
            ))),
            Op::Bool(value) => Ok(EffectTerm::Value(effect_leaf(
                &node.ty,
                i64::from(*value).to_le_bytes().to_vec(),
            ))),
            Op::String(value) | Op::Path(value) => Ok(EffectTerm::Value(effect_leaf(
                &node.ty,
                value.as_bytes().to_vec(),
            ))),
            Op::Schema(reference) => Ok(EffectTerm::Value(effect_leaf(
                &node.ty,
                reference.canonical_bytes(),
            ))),
            Op::Call(callee) => {
                let (_, _, output) = Self::effect_function(island, *callee).ok_or_else(|| {
                    Box::new(MachineError::runtime(
                        MachineOperation::Effect,
                        RuntimeFault::EffectPlane {
                            detail: "effect call target was not carried by the island",
                        },
                        None,
                        None,
                    ))
                })?;
                let output = output
                    .ok_or_else(|| effect_machine_error("effect call target has no output"))?;
                let mut callee_arguments = Vec::with_capacity(node.inputs.len());
                for index in 0..node.inputs.len() {
                    let EffectTerm::Value(value) = input(index, self)? else {
                        return effect_fault("effect call argument was codata");
                    };
                    callee_arguments.push(value);
                }
                self.evaluate_effect_node(island, *callee, output, &callee_arguments, reads)
            }
            Op::PathJoin => {
                let EffectTerm::Value(left) = input(0, self)? else {
                    return effect_fault("Path join left operand was codata");
                };
                let EffectTerm::Value(right) = input(1, self)? else {
                    return effect_fault("Path join right operand was codata");
                };
                let mut path = left.resident;
                if !path.is_empty() {
                    path.push(b'/');
                }
                path.extend(right.resident);
                Ok(EffectTerm::Value(effect_leaf(&node.ty, path)))
            }
            Op::StringConcat => {
                let EffectTerm::Value(left) = input(0, self)? else {
                    return effect_fault("String concat left operand was codata");
                };
                let EffectTerm::Value(right) = input(1, self)? else {
                    return effect_fault("String concat right operand was codata");
                };
                let mut text = left.resident;
                text.extend(right.resident);
                Ok(EffectTerm::Value(effect_leaf(&node.ty, text)))
            }
            Op::IntToString => {
                let EffectTerm::Value(value) = input(0, self)? else {
                    return effect_fault("Int.to_string receiver was codata");
                };
                let bytes = read_i64(&value.resident)
                    .ok_or_else(|| effect_machine_error("Int.to_string receiver was malformed"))?
                    .to_string()
                    .into_bytes();
                Ok(EffectTerm::Value(effect_leaf(&node.ty, bytes)))
            }
            Op::StringLines => {
                let EffectTerm::Value(value) = input(0, self)? else {
                    return effect_fault("String.lines receiver was codata");
                };
                let text = core::str::from_utf8(&value.resident)
                    .map_err(|_| effect_machine_error("String.lines receiver was not UTF-8"))?;
                let elements = text
                    .lines()
                    .map(|line| FrozenValue::Opaque(line.as_bytes().to_vec()))
                    .collect::<Vec<_>>();
                Ok(EffectTerm::Value(effect_value_from_frozen(
                    &node.ty,
                    FrozenValue::DenseArray(elements),
                )?))
            }
            Op::ArrayLen => {
                let EffectTerm::Value(value) = input(0, self)? else {
                    return effect_fault("Array.len receiver was codata");
                };
                let len = match value.frozen.as_ref() {
                    Some(FrozenValue::DenseArray(elements)) => elements.len(),
                    _ => return effect_fault("Array.len receiver was not frozen as a dense array"),
                };
                let bytes = i64::try_from(len)
                    .map_err(|_| effect_machine_error("Array length did not fit Int"))?
                    .to_le_bytes()
                    .to_vec();
                let mut value = effect_leaf(&node.ty, bytes.clone());
                value.frozen = Some(FrozenValue::Inline(bytes));
                Ok(EffectTerm::Value(value))
            }
            Op::Record => {
                let Type::Record(record) = &node.ty else {
                    return effect_fault("effect Record node had a non-record type");
                };
                if record.fields.len() != node.inputs.len() {
                    return effect_fault("effect Record field count disagreed with its type");
                }
                let mut fields = Vec::with_capacity(record.fields.len());
                for (index, field) in record.fields.iter().enumerate() {
                    let EffectTerm::Value(value) = input(index, self)? else {
                        return effect_fault("effect Record field was codata");
                    };
                    fields.push(primitive_field_from_effect(&field.ty, value)?);
                }
                Ok(EffectTerm::Value(effect_value_from_primitive(
                    &node.ty,
                    PrimitiveValue {
                        schema: node.ty.schema_ref(),
                        body: PrimitiveValueBody::Product(fields),
                    },
                )?))
            }
            Op::Project { index } => {
                let EffectTerm::Value(value) = input(0, self)? else {
                    return effect_fault("project receiver was codata");
                };
                let frozen = match value.frozen.as_ref() {
                    Some(FrozenValue::Product(fields)) => fields.get(*index as usize),
                    Some(FrozenValue::Variant { fields, .. }) => fields.get(*index as usize),
                    _ => None,
                }
                .ok_or_else(|| effect_machine_error("project receiver had no frozen field"))?;
                Ok(EffectTerm::Value(effect_value_from_frozen(
                    &node.ty,
                    frozen.clone(),
                )?))
            }
            Op::FixtureTree => {
                let EffectTerm::Value(name) = input(0, self)? else {
                    return effect_fault("fixture_tree name was codata");
                };
                let mut resident = b"fixture-tree\0".to_vec();
                resident.extend(&name.resident);
                if let Ok(name_text) = core::str::from_utf8(&name.resident)
                    && self.fixture_store.rerun_overlay() == Some(name_text)
                {
                    resident.extend(b"\0rerun");
                    resident.extend(name_text.as_bytes());
                }
                Ok(EffectTerm::Value(effect_leaf(&node.ty, resident)))
            }
            Op::FixtureRegistry => Ok(EffectTerm::Value(effect_leaf(
                &node.ty,
                b"fixture-registry".to_vec(),
            ))),
            Op::TreeProject => {
                let EffectTerm::Value(tree) = input(0, self)? else {
                    return effect_fault("tree projection receiver was codata");
                };
                let EffectTerm::Value(path) = input(1, self)? else {
                    return effect_fault("tree projection path was codata");
                };
                let (root, prefix) = if tree.resident.starts_with(b"tree-entry\0") {
                    let (root, prefix) = split_tree_entry(&tree.resident)?;
                    (root.to_vec(), prefix.to_vec())
                } else {
                    (tree.resident, Vec::new())
                };
                let mut resident = b"tree-entry\0".to_vec();
                resident.extend_from_slice(&(root.len() as u64).to_le_bytes());
                resident.extend(root);
                if !prefix.is_empty() {
                    resident.extend(prefix);
                    resident.push(b'/');
                }
                resident.extend(path.resident);
                Ok(EffectTerm::Value(effect_leaf(&node.ty, resident)))
            }
            Op::TreeEntryText => {
                let EffectTerm::Value(entry) = input(0, self)? else {
                    return effect_fault("tree text receiver was codata");
                };
                let (source, projection, bytes) = self.tree_entry_text(&entry)?;
                let value = effect_leaf(&node.ty, bytes);
                reads.push(super::model::ReadWitness {
                    source,
                    projection: ReadProjection::TreePath { path: projection },
                    observation: ReadObservation::Value(value.identity.clone()),
                });
                Ok(EffectTerm::Value(value))
            }
            Op::TreeGlob => {
                let EffectTerm::Value(tree) = input(0, self)? else {
                    return effect_fault("tree glob receiver was codata");
                };
                let EffectTerm::Value(pattern) = input(1, self)? else {
                    return effect_fault("tree glob pattern was codata");
                };
                let pattern = String::from_utf8(pattern.resident)
                    .map_err(|_| effect_machine_error("tree glob pattern was not UTF-8"))?;
                Ok(EffectTerm::Glob { tree, pattern })
            }
            Op::StreamCollect => {
                let EffectTerm::Glob { tree, pattern } = input(0, self)? else {
                    return effect_fault("effect Stream.collect receiver was not a tree glob");
                };
                let paths = self.tree_glob_paths(&tree, &pattern, reads)?;
                let mut rows = Vec::with_capacity(paths.len());
                let mut frozen = Vec::with_capacity(paths.len());
                for path in paths {
                    let path_node =
                        FramedNode::leaf(effect_schema(&Type::Path), path.as_bytes().to_vec());
                    let interned = self.store.intern_tree(&path_node, path.as_bytes());
                    self.observe_interned(&interned);
                    rows.push((interned.identity.clone(), interned.identity.clone()));
                    frozen.push((
                        FrozenValue::Reference(interned.identity.clone()),
                        FrozenValue::Reference(interned.identity.clone()),
                    ));
                }
                rows.sort();
                let map_node = FramedNode::OrderedMap {
                    schema: effect_schema(&node.ty),
                    rows,
                };
                Ok(EffectTerm::Value(EffectValue {
                    identity: map_node.identity(),
                    resident: Vec::new(),
                    frozen: Some(FrozenValue::OrderedMap(frozen)),
                    node: Some(map_node),
                }))
            }
            Op::RegistryUrl => {
                let EffectTerm::Value(registry) = input(0, self)? else {
                    return effect_fault("registry URL receiver was codata");
                };
                let EffectTerm::Value(name) = input(1, self)? else {
                    return effect_fault("registry URL name was codata");
                };
                let name = String::from_utf8(name.resident)
                    .map_err(|_| effect_machine_error("registry artifact name was not UTF-8"))?;
                let manifest = self.fixture_store.registry_manifest().map_err(|_| {
                    effect_machine_error("fixture registry manifest was unavailable")
                })?;
                reads.push(super::model::ReadWitness {
                    source: registry.identity.clone(),
                    projection: ReadProjection::RegistryManifest,
                    observation: ReadObservation::Value(
                        effect_leaf(&Type::String, manifest.clone().into_bytes()).identity,
                    ),
                });
                let row = manifest.lines().find_map(|line| {
                    let mut fields = line.split_whitespace();
                    let artifact = fields.next()?;
                    let url = fields.next()?;
                    let hash = fields.next()?;
                    let upstream = fields.next().map(str::to_owned);
                    (artifact == name).then(|| (url.to_owned(), hash.to_owned(), upstream))
                });
                let (url, hash, upstream) = row
                    .ok_or_else(|| effect_machine_error("fixture registry artifact was absent"))?;
                let blob_schema = Type::Extern(ExternKind::Blob).schema_ref();
                let blob_id = PrimitiveValue {
                    schema: blob_id_type().schema_ref(),
                    body: PrimitiveValueBody::Product(vec![
                        primitive_child_field(PrimitiveValue::bytes(
                            Type::Extern(ExternKind::Schema).schema_ref(),
                            blob_schema.canonical_bytes(),
                        )),
                        primitive_child_field(PrimitiveValue::bytes(
                            Type::String.schema_ref(),
                            hash.into_bytes(),
                        )),
                    ]),
                };
                let capability =
                    primitive_value_from_effect(&Type::Extern(ExternKind::Registry), &registry)?;
                let origin = PrimitiveValue {
                    schema: origin_hint_type().schema_ref(),
                    body: PrimitiveValueBody::Product(vec![
                        primitive_child_field(capability),
                        primitive_child_field(PrimitiveValue::bytes(
                            Type::String.schema_ref(),
                            url.into_bytes(),
                        )),
                    ]),
                };
                effect_value_from_primitive(
                    &node.ty,
                    PrimitiveValue {
                        schema: node.ty.schema_ref(),
                        body: PrimitiveValueBody::Product(vec![
                            primitive_child_field(blob_id),
                            primitive_child_field(PrimitiveValue {
                                schema: Type::array(origin_hint_type()).schema_ref(),
                                body: PrimitiveValueBody::Sequence {
                                    element_schema: origin_hint_type().schema_ref(),
                                    elements: vec![origin],
                                },
                            }),
                            primitive_child_field(PrimitiveValue {
                                schema: Type::option(Type::String).schema_ref(),
                                body: upstream.map_or_else(
                                    || PrimitiveValueBody::Variant {
                                        tag: OPTION_NONE_VARIANT,
                                        fields: Vec::new(),
                                    },
                                    |upstream| PrimitiveValueBody::Variant {
                                        tag: OPTION_SOME_VARIANT,
                                        fields: vec![primitive_child_field(PrimitiveValue::bytes(
                                            Type::String.schema_ref(),
                                            upstream.into_bytes(),
                                        ))],
                                    },
                                ),
                            }),
                        ]),
                    },
                )
                .map(EffectTerm::Value)
            }
            Op::RegistryCoordinate => {
                let EffectTerm::Value(registry) = input(0, self)? else {
                    return effect_fault("registry coordinate receiver was codata");
                };
                let EffectTerm::Value(name) = input(1, self)? else {
                    return effect_fault("registry coordinate name was codata");
                };
                let name = String::from_utf8(name.resident)
                    .map_err(|_| effect_machine_error("registry artifact name was not UTF-8"))?;
                let manifest = self.fixture_store.registry_manifest().map_err(|_| {
                    effect_machine_error("fixture registry manifest was unavailable")
                })?;
                reads.push(super::model::ReadWitness {
                    source: registry.identity.clone(),
                    projection: ReadProjection::RegistryManifest,
                    observation: ReadObservation::Value(
                        effect_leaf(&Type::String, manifest.clone().into_bytes()).identity,
                    ),
                });
                let url = manifest
                    .lines()
                    .find_map(|line| {
                        let mut fields = line.split_whitespace();
                        let artifact = fields.next()?;
                        let url = fields.next()?;
                        (artifact == name).then(|| url.to_owned())
                    })
                    .ok_or_else(|| effect_machine_error("fixture registry artifact was absent"))?;
                let capability =
                    primitive_value_from_effect(&Type::Extern(ExternKind::Registry), &registry)?;
                effect_value_from_primitive(
                    &node.ty,
                    PrimitiveValue {
                        schema: node.ty.schema_ref(),
                        body: PrimitiveValueBody::Product(vec![
                            primitive_child_field(capability),
                            primitive_child_field(PrimitiveValue::bytes(
                                Type::String.schema_ref(),
                                url.into_bytes(),
                            )),
                        ]),
                    },
                )
                .map(EffectTerm::Value)
            }
            Op::Untar => {
                let EffectTerm::Value(blob) = input(0, self)? else {
                    return effect_fault("untar input was codata");
                };
                parse_ustar(&blob.resident)
                    .map_err(|_| effect_machine_error("archive was not plain ustar"))?;
                let canonical = canonical_archive_tree(&blob.resident);
                Ok(EffectTerm::Value(EffectValue {
                    identity: FramedNode::leaf(effect_schema(&node.ty), canonical.clone())
                        .identity(),
                    resident: blob.resident,
                    frozen: None,
                    node: Some(FramedNode::leaf(effect_schema(&node.ty), canonical)),
                }))
            }
            Op::BlobLen => {
                let EffectTerm::Value(blob) = input(0, self)? else {
                    return effect_fault("Blob.len receiver was codata");
                };
                let bytes = i64::try_from(blob.resident.len())
                    .map_err(|_| effect_machine_error("Blob length did not fit Int"))?
                    .to_le_bytes()
                    .to_vec();
                Ok(EffectTerm::Value(EffectValue {
                    identity: FramedNode::leaf(effect_schema(&node.ty), bytes.clone()).identity(),
                    resident: bytes.clone(),
                    frozen: Some(FrozenValue::Inline(bytes)),
                    node: None,
                }))
            }
            Op::If { .. } => effect_fault("effect island contained an If operation"),
            Op::StringContains => {
                effect_fault("effect island contained a String.contains operation")
            }
            Op::Eq => effect_fault("effect island contained an Eq operation"),
            Op::Ne => effect_fault("effect island contained a Ne operation"),
            Op::Array => effect_fault("effect island contained an Array operation"),
            Op::ArrayConcat => effect_fault("effect island contained an ArrayConcat operation"),
            Op::Map => effect_fault("effect island contained a Map operation"),
            Op::MapWith => effect_fault("effect island contained a Map.with operation"),
            Op::Variant { .. } => effect_fault("effect island contained a Variant operation"),
            _ => Err(Box::new(MachineError::runtime(
                MachineOperation::Effect,
                RuntimeFault::UnsupportedEffectOperation {
                    operation: format!("{:?}", node.op),
                },
                None,
                None,
            ))),
        }
    }

    fn tree_entry_text(
        &self,
        entry: &EffectValue,
    ) -> Result<(ValueId, String, Vec<u8>), Box<MachineError>> {
        let (tree, path) = split_tree_entry(&entry.resident)?;
        if let Some(name) = fixture_tree_name(tree) {
            let name = core::str::from_utf8(name)
                .map_err(|_| effect_machine_error("fixture tree name was not UTF-8"))?;
            let path = core::str::from_utf8(path)
                .map_err(|_| effect_machine_error("tree path was not UTF-8"))?;
            let projection = format!("{name}/{path}");
            let bytes = self
                .fixture_store
                .tree_file_bytes(&projection)
                .map_err(|_| effect_machine_error("fixture tree entry was not a file"))?;
            return Ok((entry.identity.clone(), projection, bytes));
        }
        let path = core::str::from_utf8(path)
            .map_err(|_| effect_machine_error("archive tree path was not UTF-8"))?;
        let member = parse_ustar(tree)
            .map_err(|_| effect_machine_error("archive tree resident bytes were malformed"))?
            .into_iter()
            .find_map(|member| match member {
                TarMember::File {
                    path: candidate,
                    bytes,
                    ..
                } if candidate == path => Some(bytes),
                _ => None,
            })
            .ok_or_else(|| effect_machine_error("archive tree entry was not a file"))?;
        Ok((entry.identity.clone(), path.to_owned(), member))
    }

    fn tree_glob_paths(
        &self,
        tree: &EffectValue,
        pattern: &str,
        reads: &mut Vec<super::model::ReadWitness>,
    ) -> Result<Vec<String>, Box<MachineError>> {
        let (directory, wildcard) = pattern
            .rsplit_once('/')
            .map_or(("", pattern), |(directory, wildcard)| (directory, wildcard));
        let (prefix, suffix) = wildcard.split_once('*').unwrap_or((wildcard, ""));
        let matches = |path: &str| {
            let name = path.rsplit('/').next().unwrap_or(path);
            (directory.is_empty()
                || path
                    .strip_prefix(directory)
                    .is_some_and(|rest| rest.starts_with('/')))
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        };
        if let Some(name) = fixture_tree_name(&tree.resident) {
            let name = core::str::from_utf8(name)
                .map_err(|_| effect_machine_error("fixture tree name was not UTF-8"))?;
            let projection = if directory.is_empty() {
                name.to_owned()
            } else {
                format!("{name}/{directory}")
            };
            let entries = self
                .fixture_store
                .tree_dir_entries(&projection)
                .map_err(|_| effect_machine_error("fixture glob directory was unavailable"))?;
            reads.push(super::model::ReadWitness {
                source: tree.identity.clone(),
                projection: ReadProjection::TreePath { path: projection },
                observation: ReadObservation::Directory {
                    digest: directory_observation_digest(&entries),
                },
            });
            let mut paths = entries
                .into_iter()
                .filter_map(|(entry, kind)| {
                    (kind == super::fixture::FixtureEntryKind::File).then_some(entry)
                })
                .map(|entry| {
                    if directory.is_empty() {
                        entry
                    } else {
                        format!("{directory}/{entry}")
                    }
                })
                .filter(|path| matches(path))
                .collect::<Vec<_>>();
            paths.sort();
            return Ok(paths);
        }
        let mut paths = parse_ustar(&tree.resident)
            .map_err(|_| effect_machine_error("archive tree resident bytes were malformed"))?
            .into_iter()
            .filter_map(|member| match member {
                TarMember::File { path, .. } if matches(&path) => Some(path),
                _ => None,
            })
            .collect::<Vec<_>>();
        paths.sort();
        Ok(paths)
    }

    /// Terminate a task whose registered-primitive request violated the machine
    /// ABI or completed as a transient error, returning the typed machine fault.
    fn terminate_primitive(
        &mut self,
        task_id: TaskId,
        demand_key: DemandKey,
        failure: PrimitiveHostFailure,
    ) -> Box<MachineError> {
        let error = MachineError::runtime(
            MachineOperation::Drive,
            primitive_runtime_fault(failure),
            None,
            Some(demand_key),
        );
        Box::new(self.terminate_machine_fault(task_id, demand_key, error))
    }

    /// Terminate every waiter of a failed/errored primitive demand and return
    /// the first waiter's typed machine fault. In the common single-waiter case
    /// this is exactly the historical inline failure.
    fn terminate_primitive_waiters(
        &mut self,
        waiters: Vec<PrimitiveWaiter>,
        failure: PrimitiveHostFailure,
    ) -> Box<MachineError> {
        let mut waiters = waiters.into_iter();
        let first = waiters
            .next()
            .expect("a primitive pending always has at least one waiter");
        for waiter in waiters {
            let derived = MachineError::runtime(
                MachineOperation::Drive,
                RuntimeFault::PrimitiveHost {
                    detail: "joined primitive demand failed".to_owned(),
                },
                None,
                Some(waiter.ctx.demand_key),
            );
            let _ =
                self.terminate_machine_fault(waiter.ctx.task_id, waiter.ctx.demand_key, derived);
        }
        self.terminate_primitive(first.ctx.task_id, first.ctx.demand_key, failure)
    }

    /// Begin or join a registered-primitive demand for a yielded task, then park
    /// the frame in demand-owned pending state. The first caller installs the
    /// single staged authority and ticket subscription and delivers the one
    /// completion into the unified inbox; a joiner reuses that authority and
    /// only adds itself as a waiter. The frame is retained off the recursive
    /// Rust stack until `apply_completion` materializes the admitted value
    /// through this waiter's own ABI plan.
    ///
    /// r[impl machine.scheduler.tickets-outlive-tasks]
    /// r[impl machine.scheduler.join-atomic]
    fn begin_primitive(
        &mut self,
        ctx: TaskContext,
        request: PrimitiveHostRequest,
    ) -> Result<(), Box<MachineError>> {
        let task_id = ctx.task_id;
        let demand_key = ctx.demand_key;
        let replay = ctx.kill_on_primitive_park.then(|| {
            (
                SubmitRequest {
                    island: ctx.island,
                    location: ctx.location.clone(),
                    lowered: ctx.lowered.clone(),
                    attribution: ctx.attribution.clone(),
                    arguments: ctx.arguments.clone(),
                    wires: ctx.wires.clone(),
                    chaos: ChaosPolicy::default(),
                    ancestry: ctx.ancestry.clone(),
                    realized_as: ctx.realized_as.clone(),
                },
                ctx.demand_preimage.clone(),
            )
        });
        let Some(plan) = ctx.lowered.primitive_calls.get(request.plan).cloned() else {
            return Err(self.terminate_primitive(
                task_id,
                demand_key,
                PrimitiveHostFailure::Abi(format!(
                    "primitive host plan {} is absent",
                    request.plan
                )),
            ));
        };
        let Some(source) = ctx.attribution.source_for_node(plan.node) else {
            return Err(self.terminate_primitive(
                task_id,
                demand_key,
                PrimitiveHostFailure::Abi(format!(
                    "primitive host plan {} has no source attribution",
                    request.plan
                )),
            ));
        };
        let Some((weavy_function, weavy_pc)) = ctx
            .lowered
            .program()
            .fns
            .iter()
            .zip(&ctx.lowered.pc_nodes)
            .enumerate()
            .find_map(|(function, (lowered_function, nodes))| {
                lowered_function
                    .code
                    .iter()
                    .zip(nodes)
                    .enumerate()
                    .find_map(|(pc, (op, node))| {
                        (*node == plan.node && matches!(op, weavy::task::Op::HostCallYield { .. }))
                            .then_some((function, pc))
                    })
            })
        else {
            return Err(self.terminate_primitive(
                task_id,
                demand_key,
                PrimitiveHostFailure::Abi(format!(
                    "primitive host plan {} has no verified yield site",
                    request.plan
                )),
            ));
        };
        let site = PrimitiveSuspensionSite {
            function: source.function,
            node: source.node,
            weavy_function: u32::try_from(weavy_function).map_err(|_| {
                self.terminate_primitive(
                    task_id,
                    demand_key,
                    PrimitiveHostFailure::Abi("primitive frame index overflowed".to_owned()),
                )
            })?,
            weavy_pc: u32::try_from(weavy_pc).map_err(|_| {
                self.terminate_primitive(
                    task_id,
                    demand_key,
                    PrimitiveHostFailure::Abi("primitive program counter overflowed".to_owned()),
                )
            })?,
        };
        let request_value = match ctx.task.with_value_resolver(|resolver| {
            primitive_value_from_frame(
                &request.frame,
                plan.input,
                &plan.request,
                &self.store,
                &resolver,
                &plan.abi_schemas,
            )
        }) {
            Ok(value) => value,
            Err(detail) => {
                return Err(self.terminate_primitive(
                    task_id,
                    demand_key,
                    PrimitiveHostFailure::Abi(detail),
                ));
            }
        };
        let request_id = request_value.identity();
        let primitive_demand = primitive_demand_key(&plan.primitive, &request_id);
        if let Some(pending) = self.primitive_pending.get_mut(&primitive_demand) {
            // Join an in-flight primitive demand: reuse the first caller's single
            // authority and subscription; only add this frame as a waiter.
            pending.ticket.renew_lease();
            pending.waiters.push(PrimitiveWaiter { ctx, plan, site });
        } else {
            // First caller: stage the one authority, begin the ticket, and
            // register the single subscription delivering into the unified inbox.
            let mut catalog = BTreeMap::new();
            insert_schema_type(&plan.request, &mut catalog);
            insert_schema_type(&plan.response, &mut catalog);
            let mut authority_inputs = vec![(request_id.clone(), request_value.clone())];
            authority_inputs.extend(self.store.resident_entries().filter_map(|entry| {
                let bytes = entry.resident_bytes()?;
                Some((
                    entry.identity.clone(),
                    PrimitiveValue::bytes(entry.identity.schema.clone(), bytes.to_vec()),
                ))
            }));
            let mut authority =
                StagedEffectAuthority::new(authority_inputs).with_schema_types(catalog);
            if let Some(persistence) = self.primitive_services.value_persistence() {
                authority = authority.with_value_persistence(persistence);
            }
            if let Some(claims) = self.primitive_services.claim_history() {
                authority = authority.with_claim_history(claims);
            }
            authority = authority.with_origin_adapter(
                self.primitive_services
                    .origin()
                    .unwrap_or_else(|| Arc::new(self.fixture_store.clone())),
            );
            let authority = Arc::new(authority);
            let ticket = match self.primitive_dispatcher.begin_or_join(
                &plan.primitive,
                request_id.clone(),
                EffectCtx::new(primitive_demand, authority.clone()),
            ) {
                Ok(ticket) => ticket,
                Err(error) => {
                    return Err(self.terminate_primitive(
                        task_id,
                        demand_key,
                        PrimitiveHostFailure::Abi(format!(
                            "registered primitive dispatch failed: {error:?}"
                        )),
                    ));
                }
            };
            self.counters.primitive_invocations += 1;
            let subscription =
                ticket.join(self.completion_inbox.primitive_waiter(primitive_demand));
            self.primitive_pending.insert(
                primitive_demand,
                PrimitivePending {
                    ticket,
                    authority,
                    subscription,
                    waiters: vec![PrimitiveWaiter { ctx, plan, site }],
                },
            );
            self.observe_effect_frontier();
        }
        self.emit(EventKind::PrimitiveParked {
            task: task_id,
            key: primitive_demand,
            site,
        });
        // Witness that a yielded primitive frame now resides off the recursive
        // Rust stack in demand-owned pending state.
        let parked = self
            .primitive_pending
            .values()
            .map(|pending| pending.waiters.len())
            .sum::<usize>();
        self.counters.peak_primitive_parked_frames = self
            .counters
            .peak_primitive_parked_frames
            .max(parked as u64);
        if let Some((request, demand_preimage)) = replay {
            // The demand remains obligated to complete: queue it before
            // dropping the parked frame, and leave the demand-owned ticket in
            // `primitive_pending`. The replay reaches the same host yield and
            // joins that ticket rather than beginning another backend effect.
            self.counters.task_discards += 1;
            self.transition_task(task_id, TaskState::Discarded)?;
            self.transition_demand(demand_key, DemandState::Queued)?;
            let waiter = self
                .primitive_pending
                .get_mut(&primitive_demand)
                .and_then(|pending| pending.waiters.pop())
                .expect("the just-parked primitive waiter remains present");
            debug_assert_eq!(waiter.ctx.task_id, task_id);
            drop(waiter);
            self.spawn_context(request, demand_key, demand_preimage)?;
        }
        Ok(())
    }

    /// Apply one completion drained from the unified inbox. This is the sole
    /// path that writes primitive frames, resolves exec demands, or retires
    /// tickets (`machine.scheduler.completion-resumes-direct`).
    fn apply_completion(
        &mut self,
        completion: DeliveredCompletion,
    ) -> Result<(), Box<MachineError>> {
        match completion {
            DeliveredCompletion::Primitive {
                demand,
                publication,
            } => self.apply_primitive_completion(demand, publication),
            DeliveredCompletion::Exec { demand, output } => {
                self.apply_exec_completion(demand, output)
            }
            DeliveredCompletion::ExecProgress { demand, product } => {
                self.apply_exec_progress(demand, product)
            }
        }
    }

    fn apply_exec_progress(
        &mut self,
        execution: DemandKey,
        product: Result<ExecProgress, String>,
    ) -> Result<(), Box<MachineError>> {
        let product = product.map_err(|detail| {
            Box::new(MachineError::runtime(
                MachineOperation::Effect,
                RuntimeFault::EffectHostFailure { detail },
                None,
                Some(execution),
            ))
        })?;
        let key = self
            .exec_projection_pending
            .iter()
            .find_map(|(key, pending)| {
                (pending.execution == execution && pending.path == product.path).then_some(*key)
            });
        if let Some(key) = key {
            self.publish_exec_projection(key, product.bytes, ExecProjectionAuthority::Protocol)?;
        } else if self.exec_pending.contains_key(&execution) {
            self.exec_progress_ready
                .insert((execution, product.path), product.bytes);
        } else {
            self.counters.stale_completions_ignored += 1;
        }
        Ok(())
    }

    /// Apply a registered-primitive completion: admit the value once, then
    /// materialize it into every waiter's frame through its own ABI plan and
    /// return each resumed frame to the runnable stack.
    fn apply_primitive_completion(
        &mut self,
        demand: DemandKey,
        publication: super::PrimitivePublication,
    ) -> Result<(), Box<MachineError>> {
        let Some(pending) = self.primitive_pending.remove(&demand) else {
            // Late, duplicate, or legitimate post-cancel delivery: no live
            // waiter. Observe as a typed fact; never apply as a publication.
            self.counters.stale_completions_ignored += 1;
            return Ok(());
        };
        let PrimitivePending {
            ticket: _,
            authority,
            subscription,
            waiters,
        } = pending;
        drop(subscription);
        self.primitive_dispatcher.retire(demand);
        self.counters.fetches_performed += publication
            .receipt
            .reads
            .iter()
            .filter(|read| matches!(read.projection, ReadProjection::Origin { .. }))
            .count() as u64;
        let identity = match publication.completion {
            PrimitiveCompletion::Ok(identity) => identity,
            PrimitiveCompletion::Failed(identity) => {
                return Err(self.terminate_primitive_waiters(
                    waiters,
                    PrimitiveHostFailure::Abi(format!(
                        "registered primitive returned semantic failure {identity:?}"
                    )),
                ));
            }
            PrimitiveCompletion::MachineError(error) => {
                return Err(
                    self.terminate_primitive_waiters(waiters, PrimitiveHostFailure::Machine(error))
                );
            }
        };
        let Some(value) = authority.admitted_value(&identity) else {
            return Err(self.terminate_primitive_waiters(
                waiters,
                PrimitiveHostFailure::Abi(format!(
                    "primitive result {identity:?} was not admitted"
                )),
            ));
        };
        for PrimitiveWaiter {
            mut ctx,
            plan,
            site,
        } in waiters
        {
            if value.schema != plan.response.schema_ref() {
                let failure = PrimitiveHostFailure::Abi(format!(
                    "primitive result schema {} disagrees with response schema {}",
                    value.schema,
                    plan.response.schema_ref()
                ));
                return Err(self.terminate_primitive(ctx.task_id, ctx.demand_key, failure));
            }
            let mut clear = Ok(());
            for index in 0..plan.output.words().as_usize() {
                let Some(slot) = plan.output.word(index) else {
                    clear = Err("primitive output region overflowed".to_owned());
                    break;
                };
                if let Err(fault) = ctx.task.write_host_word(slot.byte_offset(), 0) {
                    clear = Err(format!("primitive output clear failed: {fault:?}"));
                    break;
                }
            }
            if let Err(detail) = clear {
                return Err(self.terminate_primitive(
                    ctx.task_id,
                    ctx.demand_key,
                    PrimitiveHostFailure::Abi(detail),
                ));
            }
            let mut interned = Vec::new();
            if let Err(detail) = write_primitive_value(
                &mut ctx.task,
                plan.output,
                0,
                &plan.response,
                &value,
                &mut self.store,
                &mut interned,
            ) {
                return Err(self.terminate_primitive(
                    ctx.task_id,
                    ctx.demand_key,
                    PrimitiveHostFailure::Abi(detail),
                ));
            }
            for value in &interned {
                self.observe_interned(value);
            }
            ctx.primitive_reads
                .extend(publication.receipt.reads.iter().cloned());
            self.emit(EventKind::PrimitiveResumed {
                task: ctx.task_id,
                key: demand,
                site,
            });
            self.runnable.push(ctx);
        }
        Ok(())
    }

    /// Apply an exec process termination drained from the unified inbox: intern
    /// the outcome, memoize it, resolve the exec demand, and publish its result.
    /// This is the only place an exec outcome is interned; the scheduler thread
    /// never waits on the process (`machine.scheduler.block-on-event`).
    fn apply_exec_completion(
        &mut self,
        demand: DemandKey,
        output: Result<std::process::Output, String>,
    ) -> Result<(), Box<MachineError>> {
        let Some(pending) = self.exec_pending.remove(&demand) else {
            self.counters.stale_completions_ignored += 1;
            return Ok(());
        };
        let ExecPending {
            task_id,
            location,
            demand_preimage,
            receipt,
            result_ty,
            plan_recipe,
            function,
            node,
            span,
            realized_as,
            workspace,
        } = pending;
        let output = output.map_err(|detail| {
            Box::new(MachineError::runtime(
                MachineOperation::Drive,
                RuntimeFault::EffectHostFailure { detail },
                None,
                Some(demand),
            ))
        })?;
        // The parked exec frame resumes on the scheduler thread: transition it
        // back to running before recording its outcome, exactly as the former
        // inline wait did.
        self.transition_task(task_id, TaskState::Running)?;
        if output.status.success() {
            self.publish_completed_exec_projections(demand, &workspace.path)?;
            let tree = archive_directory(&workspace.path).map_err(|detail| {
                Box::new(MachineError::runtime(
                    MachineOperation::Result,
                    RuntimeFault::EffectHostFailure { detail },
                    None,
                    Some(demand),
                ))
            })?;
            let interned =
                self.intern_exec_outcome(&result_ty, &tree, &output.stdout, &output.stderr);
            self.memo.insert(
                location.id,
                MemoEntry {
                    location: location.clone(),
                    key: demand,
                    preimage: demand_preimage,
                    result: interned.handle,
                    receipt: Some(receipt),
                    current_receipt: true,
                },
            );
            if let Some(record) = self.demands.get_mut(&demand) {
                record.result = Some(interned.handle);
            }
            self.transition_task(task_id, TaskState::Completed)?;
            self.transition_demand(demand, DemandState::Ready)?;
            self.emit(EventKind::Completed {
                key: demand,
                identity: interned.identity.clone(),
            });
            if let Some(realized) = realized_as {
                self.wire_demands.push(realized);
            }
            self.root_results.insert(
                demand,
                Evaluation {
                    handle: interned.handle,
                    identity: interned.identity,
                    passed: true,
                    memo: MemoVerdict::Miss,
                    failure: None,
                    failure_context: None,
                },
            );
            return Ok(());
        }
        let termination = match output.status.code() {
            Some(code) => ProcessTermination::Exited {
                code: i64::from(code),
            },
            None => {
                #[cfg(unix)]
                let signal = {
                    use std::os::unix::process::ExitStatusExt as _;
                    i64::from(output.status.signal().unwrap_or_default())
                };
                #[cfg(not(unix))]
                let signal = 0;
                ProcessTermination::Signaled { signal }
            }
        };
        let failure = FailureValue::ProcessFailure {
            recipe: plan_recipe,
            site: node.0,
            termination,
        };
        let report_context =
            matches!(&failure, FailureValue::ProcessFailure { recipe, .. } if *recipe == plan_recipe)
                .then(|| FailureContext {
                    function,
                    node,
                    span,
                    demand_chain: vec![demand],
                });
        let interned = self.store.intern_failure(failure.clone(), &output.stderr);
        self.observe_interned(&interned);
        self.memo.insert(
            location.id,
            MemoEntry {
                location: location.clone(),
                key: demand,
                preimage: demand_preimage,
                result: interned.handle,
                receipt: Some(receipt),
                current_receipt: true,
            },
        );
        if let Some(record) = self.demands.get_mut(&demand) {
            record.result = Some(interned.handle);
        }
        self.transition_task(task_id, TaskState::Completed)?;
        self.transition_demand(demand, DemandState::Failed)?;
        self.emit(EventKind::LanguageFailed {
            task: task_id,
            key: demand,
            failure: failure.clone(),
        });
        if let Some(realized) = realized_as {
            self.wire_demands.push(realized);
        }
        self.root_results.insert(
            demand,
            Evaluation {
                handle: interned.handle,
                identity: interned.identity,
                passed: false,
                memo: MemoVerdict::Miss,
                failure: Some(failure),
                failure_context: report_context,
            },
        );
        Ok(())
    }

    fn publish_completed_exec_projections(
        &mut self,
        execution: DemandKey,
        workspace: &Path,
    ) -> Result<(), Box<MachineError>> {
        let projections = self
            .exec_projection_pending
            .iter()
            .filter(|(_, pending)| pending.execution == execution)
            .map(|(demand, pending)| (*demand, pending.path.clone()))
            .collect::<Vec<_>>();
        for (demand, path) in projections {
            let bytes = std::fs::read(workspace.join(&path)).map_err(|error| {
                Box::new(MachineError::runtime(
                    MachineOperation::Result,
                    RuntimeFault::EffectHostFailure {
                        detail: format!(
                            "read exec product `{path}` at process completion: {error}"
                        ),
                    },
                    None,
                    Some(demand),
                ))
            })?;
            self.publish_exec_projection(demand, bytes, ExecProjectionAuthority::ProcessExit)?;
        }
        Ok(())
    }

    /// Drive one generator task to `Done` and return its outcome: either the
    /// taken sites' raw provenance selectors in publication order, or a language
    /// failure raised while constructing the generator's control. The generator
    /// runs only real `Match`/`If` control and publishes; it never evaluates a
    /// check operand. Publication arrival order is a live schedule artifact — the
    /// caller re-keys the completed check family by provenance. A scrutinee
    /// language failure stays on the language plane; only a machine invariant
    /// violation is a `MachineError`.
    pub fn drive_generator(
        &mut self,
        island: IslandId,
        lowered: &LoweringArtifact,
        attribution: &LoweringAttribution,
        arguments: &[Evaluation],
        chaos: ChaosPolicy,
    ) -> Result<GeneratorOutcome, Box<MachineError>> {
        let invocation = DemandExecution::new(
            lowered,
            arguments
                .iter()
                .map(|argument| argument.identity.clone())
                .collect(),
        );
        let lowered = &invocation;
        self.emit(EventKind::Demanded {
            key: lowered.demand_key,
        });
        self.demands.insert(
            lowered.demand_key,
            DemandRecord {
                key: lowered.demand_key,
                state: DemandState::Queued,
                result: None,
            },
        );
        self.emit(EventKind::DemandTransition {
            key: lowered.demand_key,
            from: DemandState::Absent,
            to: DemandState::Queued,
        });
        if lowered.value_inputs.len() != arguments.len() {
            return Err(Box::new(MachineError::runtime(
                MachineOperation::EntryBinding,
                RuntimeFault::ValueInputCardinality {
                    expected: lowered.value_inputs.len(),
                    actual: arguments.len(),
                },
                None,
                Some(lowered.demand_key),
            )));
        }
        if let Some(argument) = arguments.iter().find(|argument| argument.failure.is_some()) {
            return Ok(GeneratorOutcome::LanguageFailure {
                failure: Box::new(argument.failure.clone().expect("failed argument")),
                context: self
                    .output_attribution(lowered.artifact, attribution)
                    .map(|source| FailureContext {
                        function: source.function,
                        node: source.node,
                        span: source.span,
                        demand_chain: vec![lowered.demand_key],
                    }),
            });
        }
        let constants = self.materialize_constants(lowered.artifact);
        let mut kill_armed = chaos.kill_first_running_task;
        loop {
            self.counters.scheduler_requests += 1;
            let task_id = self.spawn_task(lowered.demand_key);
            self.transition_demand(lowered.demand_key, DemandState::Running)?;
            self.transition_task(task_id, TaskState::Running)?;
            self.emit(EventKind::IslandEntered {
                task: task_id,
                island,
            });
            self.emit(EventKind::SafePoint {
                task: task_id,
                class: SafePointClass::Edge,
            });

            if kill_armed {
                kill_armed = false;
                self.counters.task_discards += 1;
                self.transition_task(task_id, TaskState::Discarded)?;
                self.transition_demand(lowered.demand_key, DemandState::Queued)?;
                continue;
            }

            let mut task = match lowered.executable_rc().spawn(FnId(0)) {
                Ok(task) => task,
                Err(fault) => {
                    let error =
                        self.task_fault(MachineOperation::Spawn, fault, lowered, attribution, None);
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
            };
            let lane_facts = execution_facts(lowered.executable().lane_facts());
            match lane_facts.selected {
                ExecutionLaneFact::Interpreter => self.counters.interpreter_task_spawns += 1,
                ExecutionLaneFact::Native => self.counters.native_task_spawns += 1,
            }
            self.emit(EventKind::ExecutionLane {
                task: task_id,
                facts: lane_facts,
            });
            for (constant, handle) in lowered.constants.iter().zip(constants.iter().copied()) {
                let handle = match self.store.weavy_handle(handle) {
                    Some(handle) => handle,
                    None => {
                        let error = MachineError::runtime(
                            MachineOperation::EntryBinding,
                            RuntimeFault::MissingConstantStoreHandle,
                            self.constant_attribution(constant.node, attribution),
                            Some(lowered.demand_key),
                        );
                        return Err(Box::new(self.terminate_machine_fault(
                            task_id,
                            lowered.demand_key,
                            error,
                        )));
                    }
                };
                if let Err(fault) =
                    task.write_entry_store_handle(constant.root.entry, constant.root.schema, handle)
                {
                    let error = self.task_fault(
                        MachineOperation::EntryBinding,
                        fault,
                        lowered,
                        attribution,
                        self.constant_attribution(constant.node, attribution),
                    );
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
            }
            for (binding, argument) in lowered.value_inputs.iter().zip(arguments) {
                let frozen = self
                    .store
                    .entry(argument.handle)
                    .and_then(StoreEntry::frozen)
                    .map(|frozen| frozen_to_weavy(frozen, &binding.ty, binding, &self.store))
                    .transpose()
                    .map_err(|()| {
                        Box::new(MachineError::runtime(
                            MachineOperation::EntryBinding,
                            RuntimeFault::ValueInputSchemaMismatch,
                            None,
                            Some(lowered.demand_key),
                        ))
                    })?;
                let result = if let Some(frozen) = &frozen {
                    task.write_entry_frozen(binding.entry, frozen)
                } else {
                    if binding.store_schema != argument.identity.schema {
                        return Err(Box::new(MachineError::runtime(
                            MachineOperation::EntryBinding,
                            RuntimeFault::ValueInputSchemaMismatch,
                            None,
                            Some(lowered.demand_key),
                        )));
                    }
                    let handle = self.store.weavy_handle(argument.handle).ok_or_else(|| {
                        Box::new(MachineError::runtime(
                            MachineOperation::EntryBinding,
                            RuntimeFault::MissingValueInputStoreHandle,
                            None,
                            Some(lowered.demand_key),
                        ))
                    })?;
                    task.write_entry_store_handle(
                        binding.entry,
                        binding.schema.ok_or_else(|| {
                            Box::new(MachineError::runtime(
                                MachineOperation::EntryBinding,
                                RuntimeFault::ValueInputSchemaMismatch,
                                None,
                                Some(lowered.demand_key),
                            ))
                        })?,
                        handle,
                    )
                };
                if let Err(fault) = result {
                    let error = self.task_fault(
                        MachineOperation::EntryBinding,
                        fault,
                        lowered,
                        attribution,
                        None,
                    );
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
            }
            let mut value_memory_overrides = Vec::new();
            for (binding, argument) in lowered.value_inputs.iter().zip(arguments) {
                let Some(element_schema) = binding.payload_element_schema else {
                    continue;
                };
                let resident = self
                    .store
                    .entry(argument.handle)
                    .and_then(StoreEntry::resident_bytes)
                    .ok_or_else(|| {
                        Box::new(MachineError::runtime(
                            MachineOperation::EntryBinding,
                            RuntimeFault::MissingValueInputStoreHandle,
                            None,
                            Some(lowered.demand_key),
                        ))
                    })?;
                let mut abi_view = resident.to_vec();
                let schema_bytes = abi_view.get_mut(8..16).ok_or_else(|| {
                    Box::new(MachineError::runtime(
                        MachineOperation::EntryBinding,
                        RuntimeFault::ValueInputSchemaMismatch,
                        None,
                        Some(lowered.demand_key),
                    ))
                })?;
                schema_bytes.copy_from_slice(&i64::from(element_schema.0).to_le_bytes());
                value_memory_overrides.push((argument.handle, abi_view));
            }
            let mut primitive_host = PrimitiveHostQueue::default();
            let step = {
                let mut call_primitive = |frame: &mut [u8]| primitive_host.call(frame);
                let mut hosts: [HostFn<'_>; 1] = [&mut call_primitive];
                match self.store.with_value_memory_overrides(
                    &value_memory_overrides,
                    |value_memories| {
                        task.drive_hosted_with_value_memories(
                            &mut [],
                            &[],
                            &mut hosts,
                            value_memories,
                        )
                        .map_err(Box::new)
                    },
                ) {
                    Ok(step) => step,
                    Err(fault) => {
                        let error = self.task_fault(
                            MachineOperation::Drive,
                            *fault,
                            lowered,
                            attribution,
                            None,
                        );
                        return Err(Box::new(self.terminate_machine_fault(
                            task_id,
                            lowered.demand_key,
                            error,
                        )));
                    }
                }
            };
            match step {
                TaskStep::Done => {}
                TaskStep::Yielded => {
                    // A generator runs only real `Match`/`If` control and
                    // never reaches a registered-primitive `HostCallYield`.
                    // A yield here is a machine invariant violation, so the
                    // one primitive completion authority stays in the main
                    // task loop's demand-owned pending path.
                    let error = MachineError::runtime(
                        MachineOperation::Drive,
                        RuntimeFault::PureIslandYielded,
                        None,
                        Some(lowered.demand_key),
                    );
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
                TaskStep::Parked { input } => {
                    let error = MachineError::runtime(
                        MachineOperation::Drive,
                        RuntimeFault::PureIslandParked { input },
                        None,
                        Some(lowered.demand_key),
                    );
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
            }
            for event in task.trace() {
                if let Err(error) =
                    self.emit_weavy(task_id, *event, attribution, lowered.demand_key)
                {
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        *error,
                    )));
                }
            }
            // The generator's placeholder value is unused; its taken sites live in
            // the publication log. `Ok` drains them; a typed collection language
            // failure while constructing control stays on the language plane; a
            // machine-invariant status is a machine fault.
            match decode_result(&task, lowered) {
                // The generator's placeholder result word is unused whether it
                // decodes as a `Check` verdict or a scalar value.
                Ok(DecodedResult::OkScalar(_) | DecodedResult::OkScalarValue(_)) => {
                    let count = match task.publication_count() {
                        Ok(count) => count,
                        Err(fault) => {
                            let error = self.task_fault(
                                MachineOperation::Result,
                                fault,
                                lowered,
                                attribution,
                                None,
                            );
                            return Err(Box::new(self.terminate_machine_fault(
                                task_id,
                                lowered.demand_key,
                                error,
                            )));
                        }
                    };
                    let mut sites = Vec::with_capacity(count);
                    for index in 0..count {
                        match task.publication(index) {
                            Ok(descriptor) => sites.push(descriptor.provenance_key()),
                            Err(fault) => {
                                let error = self.task_fault(
                                    MachineOperation::Result,
                                    fault,
                                    lowered,
                                    attribution,
                                    None,
                                );
                                return Err(Box::new(self.terminate_machine_fault(
                                    task_id,
                                    lowered.demand_key,
                                    error,
                                )));
                            }
                        }
                    }
                    self.transition_task(task_id, TaskState::Completed)?;
                    self.transition_demand(lowered.demand_key, DemandState::Ready)?;
                    return Ok(GeneratorOutcome::Sites(sites));
                }
                Ok(DecodedResult::OkValue) => {
                    unreachable!("generator placeholder cannot be a value publication")
                }
                Ok(DecodedResult::IndexOutOfBounds {
                    site,
                    index,
                    length,
                }) => {
                    let failure = FailureValue::IndexOutOfBounds {
                        recipe: lowered.recipe,
                        site,
                        index,
                        length,
                        subject: None,
                    };
                    return self.complete_generator_language_failure(
                        task_id,
                        lowered,
                        attribution,
                        failure,
                    );
                }
                Ok(DecodedResult::MissingKey { site }) => {
                    let failure = FailureValue::MissingKey {
                        recipe: lowered.recipe,
                        site,
                    };
                    return self.complete_generator_language_failure(
                        task_id,
                        lowered,
                        attribution,
                        failure,
                    );
                }
                Ok(DecodedResult::DuplicateKey { site }) => {
                    let failure = FailureValue::DuplicateKey {
                        recipe: lowered.recipe,
                        site,
                    };
                    return self.complete_generator_language_failure(
                        task_id,
                        lowered,
                        attribution,
                        failure,
                    );
                }
                Ok(DecodedResult::MissingDelimiter { site }) => {
                    let failure = FailureValue::MissingDelimiter {
                        recipe: lowered.recipe,
                        site,
                    };
                    return self.complete_generator_language_failure(
                        task_id,
                        lowered,
                        attribution,
                        failure,
                    );
                }
                Ok(DecodedResult::InvalidInteger { site }) => {
                    let failure = FailureValue::InvalidInteger {
                        recipe: lowered.recipe,
                        site,
                    };
                    return self.complete_generator_language_failure(
                        task_id,
                        lowered,
                        attribution,
                        failure,
                    );
                }
                Ok(DecodedResult::IntegerOverflow { site }) => {
                    let failure = FailureValue::IntegerOverflow {
                        recipe: lowered.recipe,
                        site,
                    };
                    return self.complete_generator_language_failure(
                        task_id,
                        lowered,
                        attribution,
                        failure,
                    );
                }
                Ok(DecodedResult::IntDivisionByZero { site }) => {
                    let failure = FailureValue::DivisionByZero {
                        recipe: lowered.recipe,
                        site,
                    };
                    return self.complete_generator_language_failure(
                        task_id,
                        lowered,
                        attribution,
                        failure,
                    );
                }
                Ok(DecodedResult::ArrayMachine { site, status }) => {
                    let error = MachineError::runtime(
                        MachineOperation::Result,
                        RuntimeFault::ArrayMachineStatus { site, status },
                        self.output_attribution(lowered, attribution),
                        Some(lowered.demand_key),
                    );
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
                Ok(DecodedResult::OrderedMachine { site, status }) => {
                    let error = MachineError::runtime(
                        MachineOperation::Result,
                        RuntimeFault::OrderedMachineStatus { site, status },
                        self.output_attribution(lowered, attribution),
                        Some(lowered.demand_key),
                    );
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
                Err(fault) => {
                    let error = self.task_fault(
                        MachineOperation::Result,
                        *fault,
                        lowered,
                        attribution,
                        self.output_attribution(lowered, attribution),
                    );
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
            }
        }
    }

    /// Complete a generator task whose scrutinee control language-failed: intern
    /// the typed failure by its semantic identity, mark the generator demand
    /// failed, and surface it on the language plane. It is never reclassified as
    /// a machine invariant.
    fn complete_generator_language_failure(
        &mut self,
        task: TaskId,
        lowered: &DemandExecution<'_>,
        attribution: &LoweringAttribution,
        failure: FailureValue,
    ) -> Result<GeneratorOutcome, Box<MachineError>> {
        let context = failure_context(&failure, lowered, attribution);
        let interned = self.store.intern_failure(failure.clone(), &[]);
        self.observe_interned(&interned);
        self.transition_task(task, TaskState::Completed)?;
        self.transition_demand(lowered.demand_key, DemandState::Failed)?;
        self.emit(EventKind::LanguageFailed {
            task,
            key: lowered.demand_key,
            failure: failure.clone(),
        });
        Ok(GeneratorOutcome::LanguageFailure {
            failure: Box::new(failure),
            context,
        })
    }

    fn materialize_constants(&mut self, lowered: &LoweringArtifact) -> Vec<Handle> {
        lowered
            .constants
            .iter()
            .map(|constant| {
                let interned = self
                    .store
                    .intern_realized(constant.store_schema.clone(), &constant.bytes);
                self.observe_interned(&interned);
                interned.handle
            })
            .collect()
    }

    fn observe_interned(&mut self, interned: &Interned) {
        self.counters.bytes_hashed += interned.bytes_hashed;
        if interned.deduped {
            self.counters.store_dedups += 1;
        } else {
            self.counters.store_interns += 1;
        }
        self.emit(EventKind::StoreAlloc {
            identity: interned.identity.clone(),
            deduped: interned.deduped,
        });
    }

    fn spawn_task(&mut self, demand: DemandKey) -> TaskId {
        let id = TaskId(self.next_task);
        self.next_task += 1;
        self.counters.task_spawns += 1;
        self.tasks.insert(
            id,
            TaskRecord {
                id,
                demand,
                state: TaskState::Runnable,
            },
        );
        self.emit(EventKind::TaskSpawned {
            task: id,
            key: demand,
        });
        id
    }

    fn transition_demand(
        &mut self,
        key: DemandKey,
        to: DemandState,
    ) -> Result<(), Box<MachineError>> {
        let demand = self.demands.get_mut(&key).ok_or_else(|| {
            MachineError::runtime(
                MachineOperation::DemandTransition,
                RuntimeFault::MissingDemandRecord { key },
                None,
                Some(key),
            )
        })?;
        let from = demand.state;
        demand.state = to;
        self.emit(EventKind::DemandTransition { key, from, to });
        Ok(())
    }

    fn transition_task(&mut self, id: TaskId, to: TaskState) -> Result<(), Box<MachineError>> {
        let task = self.tasks.get_mut(&id).ok_or_else(|| {
            MachineError::runtime(
                MachineOperation::TaskTransition,
                RuntimeFault::MissingTaskRecord,
                None,
                None,
            )
        })?;
        let from = task.state;
        task.state = to;
        self.emit(EventKind::TaskTransition { task: id, from, to });
        Ok(())
    }

    fn emit_weavy(
        &mut self,
        task: TaskId,
        event: WeavyTaskEvent,
        attribution: &LoweringAttribution,
        demand: DemandKey,
    ) -> Result<(), Box<MachineError>> {
        let kind = match event {
            WeavyTaskEvent::FrameEntered(function) => EventKind::WeavyFrameEntered {
                task,
                function: attribution.function_for_frame(function.0).ok_or_else(|| {
                    MachineError::runtime(
                        MachineOperation::TraceAttribution,
                        RuntimeFault::MissingFrameAttribution { function },
                        None,
                        Some(demand),
                    )
                })?,
            },
            WeavyTaskEvent::FrameExited(function) => EventKind::WeavyFrameExited {
                task,
                function: attribution.function_for_frame(function.0).ok_or_else(|| {
                    MachineError::runtime(
                        MachineOperation::TraceAttribution,
                        RuntimeFault::MissingFrameAttribution { function },
                        None,
                        Some(demand),
                    )
                })?,
            },
            WeavyTaskEvent::Parked { input } => EventKind::WeavyParked { task, input },
            WeavyTaskEvent::Resumed => EventKind::WeavyResumed { task },
            WeavyTaskEvent::Mark(id) => {
                let source = attribution.source_for_trace(id).ok_or_else(|| {
                    MachineError::runtime(
                        MachineOperation::TraceAttribution,
                        RuntimeFault::MissingTraceAttribution { trace: id },
                        None,
                        Some(demand),
                    )
                })?;
                EventKind::WeavyMark {
                    task,
                    function: source.function,
                    node: source.node,
                }
            }
        };
        self.emit(kind);
        Ok(())
    }

    fn terminate_machine_fault(
        &mut self,
        task: TaskId,
        demand: DemandKey,
        error: MachineError,
    ) -> MachineError {
        if let Err(transition) = self.transition_task(task, TaskState::Failed) {
            return *transition;
        }
        if let Err(transition) = self.transition_demand(demand, DemandState::MachineFailed) {
            return *transition;
        }
        self.emit(EventKind::MachineFailed {
            task,
            key: demand,
            operation: error.operation,
        });
        error
    }

    fn complete_language_failure(
        &mut self,
        task: TaskId,
        location: &Location,
        lowered: &DemandExecution<'_>,
        attribution: &LoweringAttribution,
        failure: FailureValue,
    ) -> Result<Evaluation, Box<MachineError>> {
        let report_context = failure_context(&failure, lowered, attribution);
        let interned = self.store.intern_failure(failure.clone(), &[]);
        self.observe_interned(&interned);
        self.memo.insert(
            location.id,
            MemoEntry {
                location: location.clone(),
                key: lowered.demand_key,
                preimage: lowered.demand_preimage.clone(),
                result: interned.handle,
                receipt: None,
                current_receipt: false,
            },
        );
        if let Some(demand) = self.demands.get_mut(&lowered.demand_key) {
            demand.result = Some(interned.handle);
        }
        self.transition_task(task, TaskState::Completed)?;
        self.transition_demand(lowered.demand_key, DemandState::Failed)?;
        self.emit(EventKind::LanguageFailed {
            task,
            key: lowered.demand_key,
            failure: failure.clone(),
        });
        Ok(Evaluation {
            handle: interned.handle,
            identity: interned.identity,
            passed: false,
            memo: MemoVerdict::Miss,
            failure: Some(failure),
            failure_context: report_context,
        })
    }

    fn constant_attribution(
        &self,
        node: crate::vir::NodeRef,
        attribution: &LoweringAttribution,
    ) -> Option<MachineAttribution> {
        let source = attribution.source_for_node(node)?;
        let weavy_function = attribution
            .functions
            .iter()
            .position(|function| *function == source.function)
            .and_then(|frame| u32::try_from(frame).ok())
            .map(FnId);
        Some(MachineAttribution {
            function: source.function,
            node: source.node,
            span: source.span,
            weavy_function,
            weavy_pc: None,
        })
    }

    fn output_attribution(
        &self,
        lowered: &LoweringArtifact,
        attribution: &LoweringAttribution,
    ) -> Option<MachineAttribution> {
        let (pc, node) = lowered
            .pc_nodes
            .first()
            .and_then(|nodes| nodes.iter().enumerate().next_back())?;
        let source = attribution.source_for_node(*node)?;
        Some(MachineAttribution {
            function: source.function,
            node: source.node,
            span: source.span,
            weavy_function: Some(FnId(0)),
            weavy_pc: Some(pc),
        })
    }

    fn task_fault(
        &self,
        operation: MachineOperation,
        fault: TaskFault,
        lowered: &DemandExecution<'_>,
        attribution: &LoweringAttribution,
        fallback: Option<MachineAttribution>,
    ) -> MachineError {
        let source = task_fault_site(&fault)
            .and_then(|site| task_fault_attribution(site, lowered, attribution))
            .or(fallback);
        MachineError::task(operation, fault, source, lowered.demand_key)
    }

    fn emit(&mut self, kind: EventKind) {
        let event = Event {
            sequence: self.sequence,
            kind,
        };
        self.sequence += 1;
        self.sink.event(event);
    }

    #[must_use]
    pub fn counters(&self) -> Counters {
        self.counters
    }

    /// The number of distinct memo entries standing at this point in the run.
    /// This is the live table size, not a cumulative counter, so it is the
    /// quantity a `memo_entries_at_most` trace check bounds. Reads never mutate
    /// the table, so inspecting it costs no memo entry of its own.
    #[must_use]
    pub fn memo_entries(&self) -> u64 {
        self.memo.len() as u64
    }

    pub fn demands(&self) -> impl Iterator<Item = &DemandRecord> {
        self.demands.values()
    }

    pub fn tasks(&self) -> impl Iterator<Item = &TaskRecord> {
        self.tasks.values()
    }

    pub fn receipts(&self) -> impl Iterator<Item = &Receipt> {
        self.memo
            .values()
            .filter(|entry| entry.current_receipt)
            .filter_map(|entry| entry.receipt.as_ref())
    }

    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Intern one harness-supplied capability value: an opaque record whose
    /// single field is the executable identity. The demand root calls this
    /// before any island of the test runs; every consuming island receives the
    /// capability as an ordinary pre-published value input, so its `ValueId`
    /// enters each effect demand's preimage. The resident bytes carry the
    /// program name — a non-identity storage concern the exec primitive reads
    /// back at spawn time.
    ///
    /// r[impl machine.primitive.capabilities-by-identity]
    pub fn publish_capability(&mut self, ty: &Type, program: &str) -> Evaluation {
        let string_schema = semantic_schema_ref(&Type::String);
        let program_leaf = FramedNode::leaf(string_schema.clone(), program.as_bytes().to_vec());
        let node = FramedNode::Variant {
            schema: semantic_schema_ref(ty),
            tag: 0,
            fields: vec![FramedField {
                schema: string_schema,
                value: FramedValue::Optional(Some(program_leaf.identity())),
            }],
        };
        let interned = self.store.intern_tree(&node, program.as_bytes());
        self.store.attach_frozen(
            interned.handle,
            FrozenValue::Product(vec![FrozenValue::Opaque(program.as_bytes().to_vec())]),
        );
        self.observe_interned(&interned);
        Evaluation {
            handle: interned.handle,
            identity: interned.identity,
            passed: true,
            memo: MemoVerdict::Miss,
            failure: None,
            failure_context: None,
        }
    }

    /// Evaluate one exec effect island: a scheduler-owned effect demand. The
    /// demand key is the tier-1 exec identity — normalized plan × capability
    /// identity — so the same command under the same capability is ONE demand
    /// no matter how many source sites spell it; a second demand is a memo hit
    /// and spawns nothing. A miss spawns the process, parks the demand, and is
    /// resumed by process completion; the termination grammar then maps the
    /// exit to the typed outcome or a typed `ProcessFailure`.
    ///
    /// r[impl machine.primitive.exec-identity]
    /// r[impl machine.primitive.exec-outcome]
    /// r[impl machine.primitive.exit-status-is-not-a-value]
    pub fn evaluate_exec(
        &mut self,
        island: &Island,
        location: &Location,
        capability: &Evaluation,
        chaos: ChaosPolicy,
    ) -> Result<Evaluation, Box<MachineError>> {
        let evaluation = match self
            .submit_exec(
                island,
                location,
                core::slice::from_ref(capability),
                chaos,
                None,
            )?
            .root
        {
            RootSubmission::Ready(evaluation) => evaluation,
            RootSubmission::Pending(root) => self.run_until_root(root)?,
        };
        self.finish_root_batch();
        Ok(evaluation)
    }

    /// Submit one exec root without synchronously draining its process
    /// completion. An identical in-flight plan joins the same demand; a fresh
    /// process parks at the worker boundary and returns its demand key to the
    /// shared multi-root frontier.
    pub fn submit_exec(
        &mut self,
        island: &Island,
        location: &Location,
        arguments: &[Evaluation],
        chaos: ChaosPolicy,
        realized_as: Option<RealizedWireDemand>,
    ) -> Result<ExecSubmission, Box<MachineError>> {
        let malformed = || {
            Box::new(MachineError::runtime(
                MachineOperation::Drive,
                RuntimeFault::MalformedEffectIsland,
                None,
                None,
            ))
        };
        let node = island.effect_output().ok_or_else(malformed)?.clone();
        let Op::Exec { argv } = &node.op else {
            return Err(malformed());
        };
        let published_arguments = self.effect_arguments(arguments)?;
        let mut resolved_inputs = Vec::with_capacity(node.inputs.len());
        let mut reads = Vec::new();
        for input in &node.inputs {
            let value = self.evaluate_effect_node(
                island,
                island.function,
                *input,
                &published_arguments,
                &mut reads,
            )?;
            let EffectTerm::Value(value) = value else {
                return Err(malformed());
            };
            let ty = island
                .nodes
                .iter()
                .find(|candidate| candidate.id == *input)
                .map(|candidate| candidate.ty.clone())
                .ok_or_else(malformed)?;
            resolved_inputs.push((ty, value));
        }
        let (capability_ty, capability) = resolved_inputs.first().ok_or_else(malformed)?;
        let protocol = match capability_ty {
            Type::Record(record) if record.name == "ProgressiveSh" => {
                ExecProtocol::ProgressiveLinesV1
            }
            _ => ExecProtocol::ExitOnly,
        };
        let mut materialized_argv = Vec::with_capacity(argv.len());
        for argument in argv {
            let mut rendered = String::new();
            for piece in &argument.pieces {
                match piece {
                    CommandPiece::Literal(literal) => rendered.push_str(literal),
                    CommandPiece::Input { index } => {
                        let (ty, value) = resolved_inputs
                            .get(usize::try_from(*index).map_err(|_| malformed())?)
                            .ok_or_else(malformed)?;
                        match ty {
                            Type::Int => {
                                let value = read_i64(&value.resident).ok_or_else(malformed)?;
                                write!(&mut rendered, "{value}").expect("writing to String");
                            }
                            Type::String | Type::Path => {
                                rendered.push_str(
                                    core::str::from_utf8(&value.resident)
                                        .map_err(|_| malformed())?,
                                );
                            }
                            _ => return Err(malformed()),
                        }
                    }
                }
            }
            materialized_argv.push(rendered);
        }
        let plan_recipe = exec_plan_recipe(&materialized_argv);
        let demand_preimage = DemandPreimage {
            closure: plan_recipe,
            arguments: vec![capability.identity.clone()],
        };
        let demand_key = DemandKey::from_preimage(&demand_preimage);
        let receipt = Receipt {
            demand: demand_key,
            reads: vec![ReadWitness {
                source: capability.identity.clone(),
                projection: ReadProjection::CapabilityProgram,
                observation: ReadObservation::Unverifiable,
            }],
        };
        self.emit(EventKind::Demanded { key: demand_key });
        let effect_context = |failure: &FailureValue| -> Option<FailureContext> {
            matches!(failure, FailureValue::ProcessFailure { recipe, .. } if *recipe == plan_recipe)
                .then(|| FailureContext {
                    function: island.function,
                    node: node.id,
                    span: node.span,
                    demand_chain: vec![demand_key],
                })
        };

        // Location memo, exactly as a pure demand consults it.
        if let Some(entry) = self.memo.get(&location.id)
            && entry.location == *location
            && entry.key == demand_key
            && entry.preimage == demand_preimage
            && self.exact_memo_replayable(entry)
        {
            let handle = entry.result;
            return Ok(ExecSubmission {
                demand: demand_key,
                root: RootSubmission::Ready(self.effect_memo_hit(
                    location.id,
                    handle,
                    &effect_context,
                )?),
            });
        }
        // Same-run demand-key reuse: the same plan under the same capability at
        // a DIFFERENT source location is the same demand. The memo path serves
        // it without a second spawn (rung 069's whole content).
        //
        // r[impl machine.memo.no-recompute-at-lookup]
        if let Some(record) = self.demands.get(&demand_key) {
            match record.state {
                DemandState::Ready | DemandState::Failed => {
                    if let Some(handle) = record.result {
                        let evaluation =
                            self.effect_memo_hit(location.id, handle, &effect_context)?;
                        self.memo.insert(
                            location.id,
                            MemoEntry {
                                location: location.clone(),
                                key: demand_key,
                                preimage: demand_preimage.clone(),
                                result: handle,
                                receipt: None,
                                current_receipt: false,
                            },
                        );
                        return Ok(ExecSubmission {
                            demand: demand_key,
                            root: RootSubmission::Ready(evaluation),
                        });
                    }
                }
                DemandState::Running | DemandState::Queued => {
                    self.counters.demand_joins += 1;
                    self.counters.memo_hits_exact += 1;
                    self.emit(EventKind::Memo {
                        location: location.id,
                        verdict: MemoVerdict::Exact,
                        verified: 0,
                    });
                    return Ok(ExecSubmission {
                        demand: demand_key,
                        root: RootSubmission::Pending(demand_key),
                    });
                }
                _ => {}
            }
        }

        self.counters.memo_misses += 1;
        self.emit(EventKind::Memo {
            location: location.id,
            verdict: MemoVerdict::Miss,
            verified: 0,
        });
        self.demands.insert(
            demand_key,
            DemandRecord {
                key: demand_key,
                state: DemandState::Queued,
                result: None,
            },
        );
        self.emit(EventKind::DemandTransition {
            key: demand_key,
            from: DemandState::Absent,
            to: DemandState::Queued,
        });

        // The capability's executable identity travels as the value's resident
        // bytes; the value identity already entered the demand key above.
        let program = String::from_utf8(capability.resident.clone()).map_err(|_| malformed())?;

        let mut kill_armed = chaos.kill_first_running_task;
        loop {
            self.counters.scheduler_requests += 1;
            let task_id = self.spawn_task(demand_key);
            self.transition_demand(demand_key, DemandState::Running)?;
            self.transition_task(task_id, TaskState::Running)?;
            self.emit(EventKind::IslandEntered {
                task: task_id,
                island: island.id,
            });
            self.emit(EventKind::SafePoint {
                task: task_id,
                class: SafePointClass::Edge,
            });
            if kill_armed {
                // The chaos kill lands at the edge safepoint: the task is
                // discarded, the demand requeued, and the replay — which is the
                // semantics — performs the effect exactly once.
                kill_armed = false;
                self.counters.task_discards += 1;
                self.transition_task(task_id, TaskState::Discarded)?;
                self.transition_demand(demand_key, DemandState::Queued)?;
                continue;
            }

            // Spawn, then PARK: the scheduler holds no busy loop while the
            // process runs. The blocking wait runs on an isolated worker thread
            // that owns the process boundary and delivers the raw termination
            // through the scheduler's one completion event source; the parked
            // exec frame resumes only when the scheduler drains that completion
            // and `apply_exec_completion` interns it
            // (machine.scheduler.no-shadow-scheduler, block-on-event).
            self.counters.effect_spawns += 1;
            self.emit(EventKind::EffectSpawned {
                task: task_id,
                key: demand_key,
            });
            let host_fault = |detail: String| {
                Box::new(MachineError::runtime(
                    MachineOperation::Drive,
                    RuntimeFault::EffectHostFailure { detail },
                    None,
                    Some(demand_key),
                ))
            };
            let workspace = ExecWorkspace::create().map_err(host_fault)?;
            let mut child = std::process::Command::new(&program)
                .args(&materialized_argv)
                .current_dir(&workspace.path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|error| host_fault(format!("spawn `{program}`: {error}")))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| host_fault(format!("`{program}` stdout was not piped")))?;
            let stderr = child
                .stderr
                .take()
                .ok_or_else(|| host_fault(format!("`{program}` stderr was not piped")))?;
            let workspace_path = workspace.path.clone();
            self.transition_task(task_id, TaskState::Parked)?;
            self.exec_pending.insert(
                demand_key,
                ExecPending {
                    task_id,
                    location: location.clone(),
                    demand_preimage: demand_preimage.clone(),
                    receipt: receipt.clone(),
                    result_ty: node.ty.clone(),
                    plan_recipe,
                    function: island.function,
                    node: node.id,
                    span: node.span,
                    realized_as,
                    workspace,
                },
            );
            self.observe_effect_frontier();
            // `wait_with_output` lives ONLY inside this worker-thread closure;
            // the scheduler thread never waits on the process boundary.
            let exec_sender = self.completion_inbox.exec_sender();
            let worker_program = program.clone();
            std::thread::spawn(move || {
                let progress_sender = exec_sender.clone();
                let stdout_reader = std::thread::spawn(move || {
                    read_exec_stdout(
                        stdout,
                        protocol,
                        &workspace_path,
                        demand_key,
                        &progress_sender,
                    )
                });
                let stderr_reader = std::thread::spawn(move || {
                    let mut bytes = Vec::new();
                    let mut stderr = stderr;
                    stderr
                        .read_to_end(&mut bytes)
                        .map_err(|error| format!("read `{worker_program}` stderr: {error}"))?;
                    Ok::<_, String>(bytes)
                });
                let output = (|| {
                    let status = child
                        .wait()
                        .map_err(|error| format!("wait `{program}`: {error}"))?;
                    let stdout = stdout_reader
                        .join()
                        .map_err(|_| format!("read `{program}` stdout worker panicked"))??;
                    let stderr = stderr_reader
                        .join()
                        .map_err(|_| format!("read `{program}` stderr worker panicked"))??;
                    Ok(std::process::Output {
                        status,
                        stdout,
                        stderr,
                    })
                })();
                let _ = exec_sender.send(DeliveredCompletion::Exec {
                    demand: demand_key,
                    output,
                });
            });
            break;
        }
        Ok(ExecSubmission {
            demand: demand_key,
            root: RootSubmission::Pending(demand_key),
        })
    }

    /// Submit one immutable product projection of a running exec demand. The
    /// projection has its own demand identity and memo location, but its
    /// readiness comes only from the producer command grammar (or, as the safe
    /// fallback, completed process output), never from filesystem polling.
    pub fn submit_exec_projection(
        &mut self,
        request: ExecProjectionRequest,
    ) -> Result<RootSubmission, Box<MachineError>> {
        let ExecProjectionRequest {
            execution,
            capability,
            completed,
            projection,
            location,
            function,
            node,
            span,
        } = request;
        let ProgressiveProjection::ExecTreeText { path } = projection;
        validate_exec_product_path(&path).map_err(|detail| {
            Box::new(MachineError::runtime(
                MachineOperation::Effect,
                RuntimeFault::EffectHostFailure { detail },
                None,
                Some(execution),
            ))
        })?;
        let fingerprint = format!("exec-tree-text:{}:{path}", execution.0.hex());
        let demand_preimage = DemandPreimage {
            closure: RecipeId::from_effect_fingerprint(&fingerprint),
            arguments: Vec::new(),
        };
        let demand = DemandKey::from_preimage(&demand_preimage);
        self.emit(EventKind::Demanded { key: demand });
        if let Some(entry) = self.memo.get(&location.id)
            && entry.location == location
            && entry.key == demand
            && entry.preimage == demand_preimage
            && self.exact_memo_replayable(entry)
        {
            return Ok(RootSubmission::Ready(self.effect_memo_hit(
                location.id,
                entry.result,
                &|_| None,
            )?));
        }
        if let Some(record) = self.demands.get(&demand) {
            match record.state {
                DemandState::Ready | DemandState::Failed => {
                    if let Some(handle) = record.result {
                        return Ok(RootSubmission::Ready(self.effect_memo_hit(
                            location.id,
                            handle,
                            &|_| None,
                        )?));
                    }
                }
                DemandState::Queued | DemandState::Running => {
                    self.counters.demand_joins += 1;
                    return Ok(RootSubmission::Pending(demand));
                }
                DemandState::Absent | DemandState::MachineFailed => {}
            }
        }

        self.counters.memo_misses += 1;
        self.emit(EventKind::Memo {
            location: location.id,
            verdict: MemoVerdict::Miss,
            verified: 0,
        });
        self.demands.insert(
            demand,
            DemandRecord {
                key: demand,
                state: DemandState::Queued,
                result: None,
            },
        );
        self.emit(EventKind::DemandTransition {
            key: demand,
            from: DemandState::Absent,
            to: DemandState::Queued,
        });
        self.counters.scheduler_requests += 1;
        let task_id = self.spawn_task(demand);
        self.transition_demand(demand, DemandState::Running)?;
        self.transition_task(task_id, TaskState::Running)?;
        self.transition_task(task_id, TaskState::Parked)?;
        self.exec_projection_pending.insert(
            demand,
            ExecProjectionPending {
                task_id,
                location,
                demand_preimage,
                execution,
                capability,
                path: path.clone(),
                function,
                node,
                span,
            },
        );

        let ready = self
            .exec_progress_ready
            .remove(&(execution, path.clone()))
            .map(|bytes| (bytes, ExecProjectionAuthority::Protocol))
            .or_else(|| {
                completed.as_ref().and_then(|evaluation| {
                    self.exec_tree_text_from_outcome(evaluation.handle, &path)
                        .ok()
                        .map(|bytes| (bytes, ExecProjectionAuthority::ProcessExit))
                })
            });
        if let Some((bytes, authority)) = ready {
            self.publish_exec_projection(demand, bytes, authority)?;
            return Ok(RootSubmission::Ready(
                self.root_results
                    .remove(&demand)
                    .expect("published progressive root has a result"),
            ));
        }
        Ok(RootSubmission::Pending(demand))
    }

    fn publish_exec_projection(
        &mut self,
        demand: DemandKey,
        bytes: Vec<u8>,
        authority: ExecProjectionAuthority,
    ) -> Result<(), Box<MachineError>> {
        let pending = self
            .exec_projection_pending
            .remove(&demand)
            .ok_or_else(|| {
                Box::new(MachineError::runtime(
                    MachineOperation::Effect,
                    RuntimeFault::QuiescentUnresolvedDemand { key: demand },
                    None,
                    Some(demand),
                ))
            })?;
        core::str::from_utf8(&bytes).map_err(|_| {
            Box::new(MachineError::runtime(
                MachineOperation::Effect,
                RuntimeFault::EffectHostFailure {
                    detail: format!("progressive exec product `{}` was not UTF-8", pending.path),
                },
                Some(MachineAttribution {
                    function: pending.function,
                    node: pending.node,
                    span: pending.span,
                    weavy_function: None,
                    weavy_pc: None,
                }),
                Some(demand),
            ))
        })?;
        self.transition_task(pending.task_id, TaskState::Running)?;
        let node = FramedNode::leaf(semantic_schema_ref(&Type::String), bytes.clone());
        let interned = self.store.intern_tree(&node, &bytes);
        self.store
            .attach_frozen(interned.handle, FrozenValue::Opaque(bytes));
        self.observe_interned(&interned);
        let receipt = Receipt {
            demand,
            reads: vec![ReadWitness {
                source: pending.capability,
                projection: ReadProjection::ExecTreePath {
                    execution: pending.execution,
                    path: pending.path,
                },
                observation: ReadObservation::Value(interned.identity.clone()),
            }],
        };
        self.memo.insert(
            pending.location.id,
            MemoEntry {
                location: pending.location,
                key: demand,
                preimage: pending.demand_preimage,
                result: interned.handle,
                receipt: Some(receipt),
                current_receipt: true,
            },
        );
        if let Some(record) = self.demands.get_mut(&demand) {
            record.result = Some(interned.handle);
        }
        self.transition_task(pending.task_id, TaskState::Completed)?;
        self.transition_demand(demand, DemandState::Ready)?;
        self.emit(EventKind::Completed {
            key: demand,
            identity: interned.identity.clone(),
        });
        match authority {
            ExecProjectionAuthority::Protocol => {
                self.counters.progressive_exec_protocol_publications += 1;
            }
            ExecProjectionAuthority::ProcessExit => {
                self.counters.progressive_exec_exit_publications += 1;
            }
        }
        self.root_results.insert(
            demand,
            Evaluation {
                handle: interned.handle,
                identity: interned.identity,
                passed: true,
                memo: MemoVerdict::Miss,
                failure: None,
                failure_context: None,
            },
        );
        Ok(())
    }

    fn exec_tree_text_from_outcome(&self, handle: Handle, path: &str) -> Result<Vec<u8>, String> {
        let frozen = self
            .store
            .entry(handle)
            .and_then(StoreEntry::frozen)
            .ok_or_else(|| "completed exec outcome had no frozen value".to_owned())?;
        let FrozenValue::Product(fields) = frozen else {
            return Err("completed exec outcome was not a frozen product".to_owned());
        };
        let Some(FrozenValue::Opaque(tree)) = fields.first() else {
            return Err("completed exec outcome had no frozen Tree".to_owned());
        };
        parse_ustar(tree)
            .map_err(|_| "completed exec Tree was not plain ustar".to_owned())?
            .into_iter()
            .find_map(|member| match member {
                TarMember::File {
                    path: candidate,
                    bytes,
                    ..
                } if candidate == path => Some(bytes),
                _ => None,
            })
            .ok_or_else(|| format!("completed exec Tree had no file `{path}`"))
    }

    /// Serve one effect demand from an existing store result: the shared exact-
    /// hit path of the location memo and the same-run demand-key reuse.
    fn effect_memo_hit(
        &mut self,
        location: LocationId,
        handle: Handle,
        effect_context: &dyn Fn(&FailureValue) -> Option<FailureContext>,
    ) -> Result<Evaluation, Box<MachineError>> {
        let entry = self.store.entry(handle).ok_or_else(|| {
            MachineError::runtime(
                MachineOperation::MemoRead,
                RuntimeFault::MissingMemoStoreHandle,
                None,
                None,
            )
        })?;
        let identity = entry.identity.clone();
        let failure = entry.failure().cloned();
        self.counters.memo_hits_exact += 1;
        self.emit(EventKind::Memo {
            location,
            verdict: MemoVerdict::Exact,
            verified: 0,
        });
        Ok(Evaluation {
            handle,
            identity,
            passed: failure.is_none(),
            memo: MemoVerdict::Exact,
            failure_context: failure.as_ref().and_then(effect_context),
            failure,
        })
    }

    /// Intern one completed `ExecOutcome`: the workspace becomes one canonical
    /// immutable Tree, while stdout and stderr are UTF-8 line-framed streams.
    fn intern_exec_outcome(
        &mut self,
        outcome_ty: &Type,
        tree: &[u8],
        stdout: &[u8],
        stderr: &[u8],
    ) -> Interned {
        let (tree_ty, stream_ty, lines_ty) = match outcome_ty {
            Type::Record(record) => {
                let tree_ty = record.fields[0].ty.clone();
                let stream_ty = record.fields[1].ty.clone();
                let lines_ty = match &stream_ty {
                    Type::Record(stream) => stream.fields[0].ty.clone(),
                    _ => Type::map(Type::Int, Type::String),
                };
                (tree_ty, stream_ty, lines_ty)
            }
            _ => {
                let lines_ty = Type::map(Type::Int, Type::String);
                (Type::Extern(ExternKind::Tree), outcome_ty.clone(), lines_ty)
            }
        };
        let int_schema = semantic_schema_ref(&Type::Int);
        let string_schema = semantic_schema_ref(&Type::String);
        let stream_value = |bytes: &[u8]| -> (FramedNode, FrozenValue) {
            let text = String::from_utf8_lossy(bytes);
            let text_bytes = text.as_bytes().to_vec();
            let mut rows = Vec::new();
            let mut frozen_rows = Vec::new();
            for (index, line) in text.lines().enumerate() {
                let key =
                    FramedNode::leaf(int_schema.clone(), (index as i64).to_le_bytes().to_vec());
                let value = FramedNode::leaf(string_schema.clone(), line.as_bytes().to_vec());
                rows.push((key.identity(), value.identity()));
                frozen_rows.push((
                    FrozenValue::Inline((index as i64).to_le_bytes().to_vec()),
                    FrozenValue::Opaque(line.as_bytes().to_vec()),
                ));
            }
            let map = FramedNode::OrderedMap {
                schema: semantic_schema_ref(&lines_ty),
                rows,
            };
            let record = FramedNode::Variant {
                schema: semantic_schema_ref(&stream_ty),
                tag: 0,
                fields: vec![
                    FramedField {
                        schema: semantic_schema_ref(&lines_ty),
                        value: FramedValue::Optional(Some(map.identity())),
                    },
                    FramedField {
                        schema: string_schema.clone(),
                        value: FramedValue::Optional(Some(
                            FramedNode::leaf(string_schema.clone(), text_bytes.clone()).identity(),
                        )),
                    },
                ],
            };
            (
                record,
                FrozenValue::Product(vec![
                    FrozenValue::OrderedMap(frozen_rows),
                    FrozenValue::Opaque(text_bytes),
                ]),
            )
        };
        let canonical_tree = canonical_archive_tree(tree);
        let tree_node = FramedNode::leaf(semantic_schema_ref(&tree_ty), canonical_tree);
        let (stdout_node, stdout_frozen) = stream_value(stdout);
        let (stderr_node, stderr_frozen) = stream_value(stderr);
        let outcome = FramedNode::Variant {
            schema: semantic_schema_ref(outcome_ty),
            tag: 0,
            fields: vec![
                FramedField {
                    schema: semantic_schema_ref(&tree_ty),
                    value: FramedValue::Optional(Some(tree_node.identity())),
                },
                FramedField {
                    schema: semantic_schema_ref(&stream_ty),
                    value: FramedValue::Optional(Some(stdout_node.identity())),
                },
                FramedField {
                    schema: semantic_schema_ref(&stream_ty),
                    value: FramedValue::Optional(Some(stderr_node.identity())),
                },
            ],
        };
        let interned = self.store.intern_tree(&outcome, &[]);
        self.store.attach_frozen(
            interned.handle,
            FrozenValue::Product(vec![
                FrozenValue::Opaque(tree.to_vec()),
                stdout_frozen,
                stderr_frozen,
            ]),
        );
        self.observe_interned(&interned);
        interned
    }

    /// Construct the `Result` value a postfix `?` catches an operand edge
    /// into: `Ok(value)` for a successful publication, `Err(failure)` for a
    /// typed language failure — the failure participates as an ordinary value,
    /// referenced by its identity. No task runs and no demand key is minted:
    /// the operand demand IS the memoized computation; the catch only reframes
    /// its published outcome.
    pub fn publish_catch(
        &mut self,
        result_type: &Type,
        operand: &Evaluation,
    ) -> Result<Evaluation, Box<MachineError>> {
        let malformed = || {
            Box::new(MachineError::runtime(
                MachineOperation::Result,
                RuntimeFault::MalformedEffectIsland,
                None,
                None,
            ))
        };
        let Type::Enum(enumeration) = result_type else {
            return Err(malformed());
        };
        let payload_type = |tag: usize| -> Result<Type, Box<MachineError>> {
            match &enumeration.variants.get(tag).ok_or_else(malformed)?.payload {
                VariantPayload::Tuple(elements) if elements.len() == 1 => Ok(elements[0].clone()),
                _ => Err(malformed()),
            }
        };
        let (tag, field_schema_ty, field, frozen_field) = match &operand.failure {
            None => {
                let ok_ty = payload_type(0)?;
                let entry = self.store.entry(operand.handle).ok_or_else(malformed)?;
                let (value, frozen) = match &ok_ty {
                    Type::Bool | Type::Int => {
                        let mut word = [0u8; 8];
                        let bytes = entry.resident_bytes().ok_or_else(malformed)?;
                        let width = bytes.len().min(8);
                        word[..width].copy_from_slice(&bytes[..width]);
                        (
                            FramedValue::Bytes(word.to_vec()),
                            FrozenValue::Inline(word.to_vec()),
                        )
                    }
                    Type::String
                    | Type::Path
                    | Type::Array(_)
                    | Type::Map { .. }
                    | Type::Set(_) => (
                        FramedValue::Optional(Some(operand.identity.clone())),
                        FrozenValue::Reference(operand.identity.clone()),
                    ),
                    _ => (
                        FramedValue::Optional(Some(operand.identity.clone())),
                        entry.frozen().cloned().ok_or_else(malformed)?,
                    ),
                };
                (0u64, ok_ty, value, frozen)
            }
            Some(_) => {
                // The caught failure, as a value: an opaque record carrying the
                // failure's identity. The full typed failure stays in the store
                // under that identity.
                let err_ty = payload_type(1)?;
                let rendered = format!(
                    "{}:{}",
                    operand.identity.schema,
                    operand.identity.content.hex()
                );
                let string_schema = semantic_schema_ref(&Type::String);
                let leaf = FramedNode::leaf(string_schema.clone(), rendered.as_bytes().to_vec());
                let marker = FramedNode::Variant {
                    schema: semantic_schema_ref(&err_ty),
                    tag: 0,
                    fields: vec![FramedField {
                        schema: string_schema,
                        value: FramedValue::Optional(Some(leaf.identity())),
                    }],
                };
                let frozen = FrozenValue::Product(vec![FrozenValue::Opaque(rendered.into_bytes())]);
                (
                    1u64,
                    err_ty.clone(),
                    FramedValue::Optional(Some(marker.identity())),
                    frozen,
                )
            }
        };
        let node = FramedNode::Variant {
            schema: semantic_schema_ref(result_type),
            tag,
            fields: vec![FramedField {
                schema: semantic_schema_ref(&field_schema_ty),
                value: field,
            }],
        };
        let interned = self.store.intern_tree(&node, &[]);
        self.store.attach_frozen(
            interned.handle,
            FrozenValue::Variant {
                tag: u32::try_from(tag).expect("result tag fits u32"),
                fields: vec![frozen_field],
            },
        );
        self.observe_interned(&interned);
        Ok(Evaluation {
            handle: interned.handle,
            identity: interned.identity,
            passed: true,
            memo: MemoVerdict::Miss,
            failure: None,
            failure_context: None,
        })
    }

    /// Render a published snapshot value structurally from its frozen store tree.
    /// The walk is type-directed and resolves string and aggregate references
    /// through the store, so the text is a stable harness artifact — byte-
    /// identical across the plain and chaos lanes and the native and interpreter
    /// execution lanes.
    ///
    /// A render fault is a machine invariant (the published structure did not
    /// match the declared type), returned as a typed detail so the harness can
    /// attribute it to the snapshot site instead of aborting the whole run.
    pub(crate) fn render_snapshot(&self, handle: Handle, ty: &Type) -> Result<String, String> {
        let frozen = self
            .store
            .entry(handle)
            .and_then(StoreEntry::frozen)
            .ok_or_else(|| "published snapshot value has no frozen structure".to_owned())?;
        let mut out = String::new();
        render_frozen(&self.store, ty, frozen, 0, &mut out)?;
        Ok(out)
    }

    #[must_use]
    pub fn sink(&self) -> &S {
        &self.sink
    }

    #[must_use]
    pub fn into_sink(self) -> S {
        self.sink
    }

    #[must_use]
    pub fn into_sink_and_persistent_state(self) -> (S, PersistentRuntimeState) {
        self.assert_scheduler_quiescent();
        (
            self.sink,
            PersistentRuntimeState {
                store: self.store,
                memo: self.memo,
            },
        )
    }
}

fn directory_observation_digest(entries: &[(String, FixtureEntryKind)]) -> Digest {
    let mut fields = Vec::with_capacity(entries.len() * 2);
    for (name, kind) in entries {
        fields.push(name.as_bytes());
        fields.push(match kind {
            FixtureEntryKind::File => b"file".as_slice(),
            FixtureEntryKind::Dir => b"dir".as_slice(),
            FixtureEntryKind::Symlink => b"symlink".as_slice(),
        });
    }
    hash_framed(b"vix.fixture.directory-observation.v1", &fields)
}

/// Type-directed structural rendering of a published snapshot value. It mirrors
/// the structure of [`realize_structural_node`] — walking a record/tuple/enum/
/// collection guided by the VIR type — but emits stable text instead of a store
/// tree. String and aggregate references are resolved through the store. This is
/// never a `Debug` impl: the shape and field names come from the type, not from
/// any Rust formatting of a machine value.
fn render_frozen(
    store: &Store,
    ty: &Type,
    frozen: &FrozenValue,
    indent: usize,
    out: &mut String,
) -> Result<(), String> {
    // An aggregate value may be published as a reference to a frozen tree stored
    // by an earlier publication; follow it before matching on structure.
    if let FrozenValue::Reference(id) = frozen
        && matches!(
            ty,
            Type::Array(_)
                | Type::Set(_)
                | Type::Map { .. }
                | Type::Record(_)
                | Type::Enum(_)
                | Type::Tuple(_)
        )
    {
        let referent = deref_frozen(store, id.clone())?;
        return render_frozen(store, ty, referent, indent, out);
    }
    match ty {
        Type::Bool => {
            let bytes = leaf_bytes(store, frozen)?;
            let word = bytes.first().copied().unwrap_or(0);
            out.push_str(if word != 0 { "true" } else { "false" });
        }
        Type::Int => {
            let bytes = leaf_bytes(store, frozen)?;
            let word = i64::from_le_bytes(
                bytes
                    .get(..8)
                    .ok_or_else(|| "snapshot Int is not a machine word".to_owned())?
                    .try_into()
                    .expect("eight bytes"),
            );
            let _ = write!(out, "{word}");
        }
        Type::String | Type::Path => {
            let bytes = leaf_bytes(store, frozen)?;
            let text = core::str::from_utf8(&bytes)
                .map_err(|_| "snapshot string is not utf-8".to_owned())?;
            escape_vix_string(text, out);
        }
        Type::Extern(kind) => {
            // Machine-plane values render as their kind plus canonical resident
            // bytes: text when UTF-8, a hex spelling otherwise.
            let bytes = leaf_bytes(store, frozen)?;
            let _ = write!(out, "{}(", kind.name());
            match core::str::from_utf8(&bytes) {
                Ok(text) => escape_vix_string(text, out),
                Err(_) => {
                    let _ = write!(out, "0x{}", hex::encode(&bytes));
                }
            }
            out.push(')');
        }
        Type::Record(record) => {
            let FrozenValue::Product(fields) = frozen else {
                return Err(render_mismatch(ty));
            };
            if fields.len() != record.fields.len() {
                return Err(render_mismatch(ty));
            }
            let _ = write!(out, "{} {{", record.name);
            out.push('\n');
            for (field, value) in record.fields.iter().zip(fields) {
                push_indent(out, indent + 1);
                let _ = write!(out, "{}: ", field.name);
                render_frozen(store, &field.ty, value, indent + 1, out)?;
                out.push_str(",\n");
            }
            push_indent(out, indent);
            out.push('}');
        }
        Type::Tuple(elements) => {
            let FrozenValue::Product(fields) = frozen else {
                return Err(render_mismatch(ty));
            };
            if fields.len() != elements.len() {
                return Err(render_mismatch(ty));
            }
            out.push('(');
            for (index, (element, value)) in elements.iter().zip(fields).enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                render_frozen(store, element, value, indent, out)?;
            }
            out.push(')');
        }
        Type::Enum(enumeration) => {
            let FrozenValue::Variant { tag, fields } = frozen else {
                return Err(render_mismatch(ty));
            };
            let variant = enumeration
                .variants
                .get(*tag as usize)
                .ok_or_else(|| render_mismatch(ty))?;
            out.push_str(&variant.name);
            match &variant.payload {
                VariantPayload::Unit => {}
                VariantPayload::Tuple(elements) => {
                    out.push('(');
                    for (index, (element, value)) in elements.iter().zip(fields).enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        render_frozen(store, element, value, indent, out)?;
                    }
                    out.push(')');
                }
                VariantPayload::Record(record_fields) => {
                    out.push_str(" {\n");
                    for (field, value) in record_fields.iter().zip(fields) {
                        push_indent(out, indent + 1);
                        let _ = write!(out, "{}: ", field.name);
                        render_frozen(store, &field.ty, value, indent + 1, out)?;
                        out.push_str(",\n");
                    }
                    push_indent(out, indent);
                    out.push('}');
                }
            }
        }
        Type::Array(element) => {
            let FrozenValue::DenseArray(elements) = frozen else {
                return Err(render_mismatch(ty));
            };
            render_sequence(store, element, elements, indent, out)?;
        }
        Type::Set(element) => {
            let FrozenValue::OrderedSet(elements) = frozen else {
                return Err(render_mismatch(ty));
            };
            render_sequence(store, element, elements, indent, out)?;
        }
        Type::Map { key, value } => {
            let FrozenValue::OrderedMap(rows) = frozen else {
                return Err(render_mismatch(ty));
            };
            if rows.is_empty() {
                out.push_str("{}");
            } else {
                out.push_str("{\n");
                for (row_key, row_value) in rows {
                    push_indent(out, indent + 1);
                    render_frozen(store, key, row_key, indent + 1, out)?;
                    out.push_str(": ");
                    render_frozen(store, value, row_value, indent + 1, out)?;
                    out.push_str(",\n");
                }
                push_indent(out, indent);
                out.push('}');
            }
        }
        Type::Check
        | Type::StreamCheck
        | Type::Stream { .. }
        | Type::Order(_)
        | Type::Function { .. } => {
            return Err(render_mismatch(ty));
        }
    }
    Ok(())
}

fn render_sequence(
    store: &Store,
    element: &Type,
    elements: &[FrozenValue],
    indent: usize,
    out: &mut String,
) -> Result<(), String> {
    if elements.is_empty() {
        out.push_str("[]");
        return Ok(());
    }
    out.push_str("[\n");
    for value in elements {
        push_indent(out, indent + 1);
        render_frozen(store, element, value, indent + 1, out)?;
        out.push_str(",\n");
    }
    push_indent(out, indent);
    out.push(']');
    Ok(())
}

/// Resolve a leaf value to its byte payload: inline scalar bytes, opaque molten
/// bytes, or a store reference's resident bytes (a string/path constant).
fn leaf_bytes(store: &Store, frozen: &FrozenValue) -> Result<Vec<u8>, String> {
    match frozen {
        FrozenValue::Inline(bytes) | FrozenValue::Opaque(bytes) => Ok(bytes.clone()),
        FrozenValue::Reference(id) => {
            let handle = store
                .handle_for_identity(id)
                .ok_or_else(|| "snapshot reference is not resident in the store".to_owned())?;
            store
                .entry(handle)
                .and_then(StoreEntry::resident_bytes)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| "snapshot reference has no resident bytes".to_owned())
        }
        _ => Err("snapshot leaf is not a byte value".to_owned()),
    }
}

fn deref_frozen(store: &Store, id: ValueId) -> Result<&FrozenValue, String> {
    let handle = store
        .handle_for_identity(&id)
        .ok_or_else(|| "snapshot reference is not resident in the store".to_owned())?;
    store
        .entry(handle)
        .and_then(StoreEntry::frozen)
        .ok_or_else(|| "snapshot reference has no frozen structure".to_owned())
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}

fn render_mismatch(ty: &Type) -> String {
    format!("snapshot value shape does not match type {}", ty.name())
}

/// Canonical Vix string escaping for snapshot rendering. This is a defined rule,
/// not Rust's `Debug`: the text is wrapped in double quotes; backslash and double
/// quote are backslash-escaped; the three named whitespace controls use `\n`,
/// `\t`, `\r`; every other C0 control (below `0x20`) and `0x7f` uses a lowercase
/// `\u{h}` hex escape with no leading zeros; and every other scalar — including
/// all printable non-ASCII — is emitted verbatim as UTF-8. Fixing this here means
/// the escaping is a property of Vix, independent of the host language.
fn escape_vix_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{7f}' => out.push_str("\\u{7f}"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

struct RealizedValue {
    node: FramedNode,
    resident: Vec<u8>,
    framed_bytes: usize,
    molten_nodes: usize,
    molten_bytes: usize,
    frozen: Option<FrozenValue>,
}

fn realize_value(
    task: &weavy::exec::ExecTask,
    lowered: &LoweringArtifact,
    store: &Store,
) -> Result<RealizedValue, TaskFault> {
    task.with_result_value_resolver(|result, resolver| {
        let value = if lowered.array_outcome.is_some() {
            let selector = result.enum_selector()?;
            result.enum_field(selector, 0)?
        } else {
            result
        };
        // A snapshot island realizes EVERY type through the structural walker so
        // scalars, strings, and collections all attach a renderable frozen tree.
        // Identity comes from the framed node, so the (empty) resident is fine.
        if lowered.publishes_snapshot {
            let (node, frozen, framed_bytes) =
                realize_structural_node(&resolver, value, &lowered.output_type, store, lowered)?;
            let (molten_nodes, molten_bytes) = resolver.molten_stats();
            return Ok(RealizedValue {
                node,
                resident: Vec::new(),
                framed_bytes,
                molten_nodes,
                molten_bytes,
                frozen: Some(frozen),
            });
        }
        let (node, resident, framed_bytes, frozen) = match &lowered.output_type {
            Type::Map {
                key,
                value: map_value,
            } => {
                let (node, frozen, framed_bytes) = realize_ordered(
                    &resolver,
                    value,
                    key,
                    Some(map_value),
                    &lowered.output_type,
                    store,
                    lowered,
                )?;
                (node, Vec::new(), framed_bytes, Some(frozen))
            }
            Type::Set(element) => {
                let (node, frozen, framed_bytes) = realize_ordered(
                    &resolver,
                    value,
                    element,
                    None,
                    &lowered.output_type,
                    store,
                    lowered,
                )?;
                (node, Vec::new(), framed_bytes, Some(frozen))
            }
            Type::Enum(_) | Type::Tuple(_) | Type::Record(_) => {
                let (node, frozen, framed_bytes) = realize_structural_node(
                    &resolver,
                    value,
                    &lowered.output_type,
                    store,
                    lowered,
                )?;
                (node, Vec::new(), framed_bytes, Some(frozen))
            }
            Type::Array(element) => {
                let value_ref = value.value_ref()?;
                let resolved = resolver
                    .resolve(value_ref)
                    .ok_or_else(|| invalid_realized_result(lowered, 0))?;
                let ResolvedTaskValue::TaskMolten(bytes) = resolved else {
                    return Err(invalid_realized_result(lowered, 0));
                };
                let (node, resident, framed_bytes) =
                    realize_array(&resolver, value_ref, bytes, element, store, lowered)?;
                // A non-snapshot published array is not frozen: freezing is extra
                // structural work with no consumer off the snapshot path.
                (node, resident, framed_bytes, None)
            }
            _ => {
                let value = value.value_ref()?;
                let resolved = resolver
                    .resolve(value)
                    .ok_or_else(|| invalid_realized_result(lowered, 0))?;
                let (node, resident, framed_bytes) =
                    realize_resolved(resolved, &lowered.output_type, store, lowered)?;
                (node, resident, framed_bytes, None)
            }
        };
        let (molten_nodes, molten_bytes) = resolver.molten_stats();
        Ok(RealizedValue {
            node,
            resident,
            framed_bytes,
            molten_nodes,
            molten_bytes,
            frozen,
        })
    })
}

fn realize_ordered<'task>(
    resolver: &TaskValueResolver<'task>,
    value: TaskStructuralValue<'task>,
    key_ty: &Type,
    value_ty: Option<&Type>,
    collection_ty: &Type,
    store: &Store,
    lowered: &LoweringArtifact,
) -> Result<(FramedNode, FrozenValue, usize), TaskFault> {
    let collection = resolver.resolve_ordered(value.value_ref()?)?;
    let mut framed_bytes = 0usize;
    if let Some(value_ty) = value_ty {
        let mut identities = Vec::with_capacity(collection.rows().len());
        let mut frozen = Vec::with_capacity(collection.rows().len());
        for row in collection.rows() {
            let (key, frozen_key, key_bytes) =
                realize_structural_node(resolver, row.key(), key_ty, store, lowered)?;
            let row_value = row
                .value()
                .ok_or_else(|| invalid_realized_result(lowered, 0))?;
            let (value, frozen_value, value_bytes) =
                realize_structural_node(resolver, row_value, value_ty, store, lowered)?;
            framed_bytes = framed_bytes
                .saturating_add(key_bytes)
                .saturating_add(value_bytes);
            identities.push((key.identity(), value.identity()));
            frozen.push((frozen_key, frozen_value));
        }
        Ok((
            FramedNode::OrderedMap {
                schema: semantic_schema_ref(collection_ty),
                rows: identities,
            },
            FrozenValue::OrderedMap(frozen),
            framed_bytes,
        ))
    } else {
        let mut identities = Vec::with_capacity(collection.rows().len());
        let mut frozen = Vec::with_capacity(collection.rows().len());
        for row in collection.rows() {
            if row.value().is_some() {
                return Err(invalid_realized_result(lowered, 0));
            }
            let (element, frozen_element, bytes) =
                realize_structural_node(resolver, row.key(), key_ty, store, lowered)?;
            framed_bytes = framed_bytes.saturating_add(bytes);
            identities.push(element.identity());
            frozen.push(frozen_element);
        }
        Ok((
            FramedNode::OrderedSet {
                schema: semantic_schema_ref(collection_ty),
                elements: identities,
            },
            FrozenValue::OrderedSet(frozen),
            framed_bytes,
        ))
    }
}

fn realize_structural_node<'task>(
    resolver: &TaskValueResolver<'task>,
    value: TaskStructuralValue<'task>,
    ty: &Type,
    store: &Store,
    lowered: &LoweringArtifact,
) -> Result<(FramedNode, FrozenValue, usize), TaskFault> {
    match ty {
        Type::Bool | Type::Int | Type::Check => {
            let bytes = value.scalar_word()?.to_le_bytes().to_vec();
            Ok((
                FramedNode::leaf(semantic_schema_ref(ty), bytes.clone()),
                FrozenValue::Inline(bytes),
                8,
            ))
        }
        Type::String | Type::Path | Type::Extern(_) => {
            let resolved = resolver
                .resolve(value.value_ref()?)
                .ok_or_else(|| invalid_realized_result(lowered, 0))?;
            match resolved {
                ResolvedTaskValue::Store(handle) => {
                    let entry = store
                        .entry_by_weavy_handle(handle)
                        .ok_or_else(|| invalid_realized_result(lowered, 0))?;
                    Ok((
                        FramedNode::Reference(entry.identity.clone()),
                        FrozenValue::Reference(entry.identity.clone()),
                        0,
                    ))
                }
                ResolvedTaskValue::TaskMolten(bytes) => Ok((
                    FramedNode::leaf(semantic_schema_ref(ty), bytes.to_vec()),
                    FrozenValue::Opaque(bytes.to_vec()),
                    bytes.len(),
                )),
                ResolvedTaskValue::LentMolten { .. } => Err(invalid_realized_result(lowered, 0)),
            }
        }
        Type::Map {
            key,
            value: map_value,
        } => realize_ordered(resolver, value, key, Some(map_value), ty, store, lowered),
        Type::Set(element) => realize_ordered(resolver, value, element, None, ty, store, lowered),
        Type::Tuple(elements) => realize_structural_fields(
            resolver,
            value,
            ty,
            0,
            elements.iter(),
            RealizeContext { store, lowered },
            false,
        ),
        Type::Record(record) => realize_structural_fields(
            resolver,
            value,
            ty,
            0,
            record.fields.iter().map(|field| &field.ty),
            RealizeContext { store, lowered },
            false,
        ),
        Type::Enum(enumeration) => {
            let tag = value.enum_selector()?;
            let variant = enumeration
                .variants
                .get(tag as usize)
                .ok_or_else(|| invalid_realized_result(lowered, 0))?;
            let fields = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(elements) => elements.iter().collect(),
                VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
            };
            realize_structural_fields(
                resolver,
                value,
                ty,
                tag,
                fields.into_iter(),
                RealizeContext { store, lowered },
                true,
            )
        }
        Type::Array(_) => {
            let value_ref = value.value_ref()?;
            let resolved = resolver
                .resolve(value_ref)
                .ok_or_else(|| invalid_realized_result(lowered, 0))?;
            match resolved {
                ResolvedTaskValue::Store(handle) => {
                    let entry = store
                        .entry_by_weavy_handle(handle)
                        .ok_or_else(|| invalid_realized_result(lowered, 0))?;
                    let node = FramedNode::Reference(entry.identity.clone());
                    Ok((node, FrozenValue::Reference(entry.identity.clone()), 0))
                }
                ResolvedTaskValue::TaskMolten(bytes) => {
                    let Type::Array(element) = ty else {
                        unreachable!("array arm has array type")
                    };
                    let (node, _, framed_bytes) =
                        realize_array(resolver, value_ref, bytes, element, store, lowered)?;
                    let frozen = freeze_dense_array(resolver, value_ref, element, store, lowered)?;
                    Ok((node, frozen, framed_bytes))
                }
                ResolvedTaskValue::LentMolten { .. } => Err(invalid_realized_result(lowered, 0)),
            }
        }
        Type::Function { .. } | Type::StreamCheck | Type::Stream { .. } | Type::Order(_) => {
            Err(invalid_realized_result(lowered, 0))
        }
    }
}

#[derive(Clone, Copy)]
struct RealizeContext<'a> {
    store: &'a Store,
    lowered: &'a LoweringArtifact,
}

fn realize_structural_fields<'task, 'ty>(
    resolver: &TaskValueResolver<'task>,
    value: TaskStructuralValue<'task>,
    ty: &Type,
    tag: u32,
    field_types: impl Iterator<Item = &'ty Type>,
    context: RealizeContext<'_>,
    enumeration: bool,
) -> Result<(FramedNode, FrozenValue, usize), TaskFault> {
    let mut fields = Vec::new();
    let mut frozen = Vec::new();
    let mut framed_bytes = 0usize;
    for (index, field_ty) in field_types.enumerate() {
        let field = if enumeration {
            value.enum_field(tag, index as u32)?
        } else {
            value.product_field(index as u32)?
        };
        let (node, frozen_field, bytes) =
            realize_structural_node(resolver, field, field_ty, context.store, context.lowered)?;
        let identity = node.identity();
        framed_bytes = framed_bytes.saturating_add(bytes);
        fields.push(FramedField {
            schema: semantic_schema_ref(field_ty),
            value: if matches!(field_ty, Type::Bool | Type::Int | Type::Check) {
                let FrozenValue::Inline(bytes) = &frozen_field else {
                    return Err(invalid_realized_result(context.lowered, 0));
                };
                FramedValue::Bytes(bytes.clone())
            } else {
                FramedValue::Optional(Some(identity))
            },
        });
        frozen.push(frozen_field);
    }
    Ok((
        FramedNode::Variant {
            schema: semantic_schema_ref(ty),
            tag: u64::from(tag),
            fields,
        },
        if enumeration {
            FrozenValue::Variant {
                tag,
                fields: frozen,
            }
        } else {
            FrozenValue::Product(frozen)
        },
        framed_bytes,
    ))
}

fn realize_resolved<'task>(
    resolved: ResolvedTaskValue<'task>,
    ty: &Type,
    store: &Store,
    lowered: &LoweringArtifact,
) -> Result<(FramedNode, Vec<u8>, usize), TaskFault> {
    match resolved {
        ResolvedTaskValue::TaskMolten(bytes) => match ty {
            Type::String | Type::Path => Ok((
                FramedNode::leaf(semantic_schema_ref(ty), bytes.to_vec()),
                bytes.to_vec(),
                bytes.len(),
            )),
            _ => Err(invalid_realized_result(lowered, bytes.len())),
        },
        ResolvedTaskValue::Store(handle) => {
            let entry = store
                .entry_by_weavy_handle(handle)
                .ok_or_else(|| invalid_realized_result(lowered, 0))?;
            if entry.identity.schema != ty.schema_ref() {
                return Err(invalid_realized_result(lowered, 0));
            }
            let resident = entry
                .resident_bytes()
                .ok_or_else(|| invalid_realized_result(lowered, 0))?
                .to_vec();
            Ok((FramedNode::Reference(entry.identity.clone()), resident, 0))
        }
        ResolvedTaskValue::LentMolten { .. } => Err(invalid_realized_result(lowered, 0)),
    }
}

fn freeze_dense_array<'task>(
    resolver: &TaskValueResolver<'task>,
    value: weavy::exec::TaskValueRef<'task>,
    element: &Type,
    store: &Store,
    lowered: &LoweringArtifact,
) -> Result<FrozenValue, TaskFault> {
    let elements = resolver
        .resolve_dense(value)?
        .elements()
        .iter()
        .copied()
        .map(|value| {
            realize_structural_node(resolver, value, element, store, lowered)
                .map(|(_, frozen, _)| frozen)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FrozenValue::DenseArray(elements))
}

fn realize_array<'task>(
    resolver: &TaskValueResolver<'task>,
    value: weavy::exec::TaskValueRef<'task>,
    bytes: &'task [u8],
    element: &Type,
    store: &Store,
    lowered: &LoweringArtifact,
) -> Result<(FramedNode, Vec<u8>, usize), TaskFault> {
    const HEADER: usize = 32;
    let tag =
        read_payload_word(bytes, 0).ok_or_else(|| invalid_realized_result(lowered, bytes.len()))?;
    let count = usize::try_from(
        read_payload_word(bytes, 16)
            .ok_or_else(|| invalid_realized_result(lowered, bytes.len()))?,
    )
    .map_err(|_| invalid_realized_result(lowered, bytes.len()))?;
    let width = usize::try_from(
        read_payload_word(bytes, 24)
            .ok_or_else(|| invalid_realized_result(lowered, bytes.len()))?,
    )
    .map_err(|_| invalid_realized_result(lowered, bytes.len()))?;
    let data_len = count
        .checked_mul(width)
        .ok_or_else(|| invalid_realized_result(lowered, bytes.len()))?;
    let data = bytes
        .get(
            HEADER
                ..HEADER
                    .checked_add(data_len)
                    .ok_or_else(|| invalid_realized_result(lowered, bytes.len()))?,
        )
        .ok_or_else(|| invalid_realized_result(lowered, bytes.len()))?;
    if tag != 1 || width == 0 || HEADER + data_len != bytes.len() {
        return Err(invalid_realized_result(lowered, bytes.len()));
    }
    let array_schema = semantic_schema_ref(&Type::Array(Box::new(element.clone())));
    let element_schema = semantic_schema_ref(element);
    if !type_contains_handle(element) {
        let expected_width = element
            .word_width()
            .and_then(|words| words.checked_mul(8))
            .ok_or_else(|| invalid_realized_result(lowered, bytes.len()))?;
        if width != expected_width {
            return Err(invalid_realized_result(lowered, bytes.len()));
        }
        return Ok((
            FramedNode::SeqInline {
                schema: array_schema,
                element_schema,
                element_width: u32::try_from(width)
                    .map_err(|_| invalid_realized_result(lowered, bytes.len()))?,
                canonical_bytes: data.to_vec(),
            },
            bytes.to_vec(),
            data.len(),
        ));
    }
    let expected_width = element
        .word_width()
        .and_then(|words| words.checked_mul(8))
        .ok_or_else(|| invalid_realized_result(lowered, bytes.len()))?;
    if width != expected_width {
        return Err(invalid_realized_result(lowered, bytes.len()));
    }
    let dense = resolver.resolve_dense(value)?;
    if dense.elements().len() != count {
        return Err(invalid_realized_result(lowered, bytes.len()));
    }
    let mut children = Vec::with_capacity(count);
    let mut framed_bytes = 0usize;
    for element_value in dense.elements().iter().copied() {
        let (node, _, nested_bytes) =
            realize_structural_node(resolver, element_value, element, store, lowered)?;
        framed_bytes = framed_bytes.saturating_add(nested_bytes);
        children.push(node.identity());
    }
    Ok((
        FramedNode::SeqChildren {
            schema: array_schema,
            element_schema,
            children,
        },
        bytes.to_vec(),
        framed_bytes,
    ))
}

fn type_contains_handle(ty: &Type) -> bool {
    match ty {
        Type::String
        | Type::Path
        | Type::Extern(_)
        | Type::Array(_)
        | Type::Map { .. }
        | Type::Set(_) => true,
        Type::Tuple(elements) => elements.iter().any(type_contains_handle),
        Type::Record(record) => record
            .fields
            .iter()
            .any(|field| type_contains_handle(&field.ty)),
        Type::Enum(enumeration) => {
            enumeration
                .variants
                .iter()
                .any(|variant| match &variant.payload {
                    crate::vir::VariantPayload::Unit => false,
                    crate::vir::VariantPayload::Tuple(elements) => {
                        elements.iter().any(type_contains_handle)
                    }
                    crate::vir::VariantPayload::Record(fields) => {
                        fields.iter().any(|field| type_contains_handle(&field.ty))
                    }
                })
        }
        Type::Function { .. } => true,
        Type::Bool
        | Type::Int
        | Type::Check
        | Type::StreamCheck
        | Type::Stream { .. }
        | Type::Order(_) => false,
    }
}

struct FrozenInline {
    bytes: Vec<u8>,
    references: Vec<(u32, weavy::exec::FrozenValue)>,
}

impl FrozenInline {
    fn into_weavy(self) -> weavy::exec::FrozenInlineValue {
        self.references.into_iter().fold(
            weavy::exec::FrozenInlineValue::new(self.bytes),
            |value, (offset, reference)| value.with_reference(offset, reference),
        )
    }
}

fn frozen_to_weavy(
    frozen: &FrozenValue,
    ty: &Type,
    binding: &ValueInputBinding,
    store: &Store,
) -> Result<weavy::exec::FrozenValue, ()> {
    match (frozen, ty) {
        (FrozenValue::Inline(_), Type::Bool | Type::Int | Type::Check) => {
            Ok(weavy::exec::FrozenValue::Inline(
                frozen_inline(frozen, ty, binding, store)?.into_weavy(),
            ))
        }
        (FrozenValue::Reference(identity), _) => {
            let schema = publication_schema(binding, ty)?;
            let handle = store
                .handle_for_identity(identity)
                .and_then(|handle| store.weavy_handle(handle))
                .ok_or(())?;
            Ok(weavy::exec::FrozenValue::Store { schema, handle })
        }
        (FrozenValue::Opaque(bytes), Type::String | Type::Path | Type::Extern(_)) => {
            Ok(weavy::exec::FrozenValue::Opaque {
                schema: publication_schema(binding, ty)?,
                bytes: bytes.clone(),
            })
        }
        (FrozenValue::OrderedMap(rows), Type::Map { key, value }) => {
            let rows = rows
                .iter()
                .map(|(row_key, row_value)| {
                    Ok((
                        frozen_inline(row_key, key, binding, store)?.into_weavy(),
                        Some(frozen_inline(row_value, value, binding, store)?.into_weavy()),
                    ))
                })
                .collect::<Result<Vec<_>, ()>>()?;
            Ok(weavy::exec::FrozenValue::Ordered {
                schema: publication_schema(binding, ty)?,
                rows,
            })
        }
        (FrozenValue::OrderedSet(elements), Type::Set(element)) => {
            let rows = elements
                .iter()
                .map(|value| {
                    Ok((
                        frozen_inline(value, element, binding, store)?.into_weavy(),
                        None,
                    ))
                })
                .collect::<Result<Vec<_>, ()>>()?;
            Ok(weavy::exec::FrozenValue::Ordered {
                schema: publication_schema(binding, ty)?,
                rows,
            })
        }
        (FrozenValue::DenseArray(elements), Type::Array(element)) => {
            let elements = elements
                .iter()
                .map(|value| Ok(frozen_inline(value, element, binding, store)?.into_weavy()))
                .collect::<Result<Vec<_>, ()>>()?;
            Ok(weavy::exec::FrozenValue::Dense {
                schema: publication_schema(binding, ty)?,
                elements,
            })
        }
        (FrozenValue::Product(_) | FrozenValue::Variant { .. }, _) => {
            Ok(weavy::exec::FrozenValue::Inline(
                frozen_inline(frozen, ty, binding, store)?.into_weavy(),
            ))
        }
        _ => Err(()),
    }
}

fn frozen_inline(
    frozen: &FrozenValue,
    ty: &Type,
    binding: &ValueInputBinding,
    store: &Store,
) -> Result<FrozenInline, ()> {
    match ty {
        Type::Bool | Type::Int | Type::Check => {
            let FrozenValue::Inline(bytes) = frozen else {
                return Err(());
            };
            Ok(FrozenInline {
                bytes: bytes.clone(),
                references: Vec::new(),
            })
        }
        Type::String
        | Type::Path
        | Type::Extern(_)
        | Type::Array(_)
        | Type::Map { .. }
        | Type::Set(_) => Ok(FrozenInline {
            bytes: vec![0; 8],
            references: vec![(0, frozen_to_weavy(frozen, ty, binding, store)?)],
        }),
        Type::Tuple(elements) => {
            let FrozenValue::Product(fields) = frozen else {
                return Err(());
            };
            frozen_product(fields, elements.iter(), binding, store, 0)
        }
        Type::Record(record) => {
            let FrozenValue::Product(fields) = frozen else {
                return Err(());
            };
            frozen_product(
                fields,
                record.fields.iter().map(|field| &field.ty),
                binding,
                store,
                0,
            )
        }
        Type::Enum(enumeration) => {
            let FrozenValue::Variant { tag, fields } = frozen else {
                return Err(());
            };
            let variant = enumeration.variants.get(*tag as usize).ok_or(())?;
            let field_types = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(elements) => elements.iter().collect(),
                VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
            };
            let width = ty.word_width().ok_or(())?.checked_mul(8).ok_or(())?;
            let mut result = frozen_product(fields, field_types.into_iter(), binding, store, 8)?;
            result.bytes.resize(width, 0);
            result.bytes[..8].copy_from_slice(&i64::from(*tag).to_le_bytes());
            Ok(result)
        }
        Type::Function { .. } | Type::StreamCheck | Type::Stream { .. } | Type::Order(_) => Err(()),
    }
}

fn frozen_product<'a>(
    fields: &[FrozenValue],
    field_types: impl Iterator<Item = &'a Type>,
    binding: &ValueInputBinding,
    store: &Store,
    base: usize,
) -> Result<FrozenInline, ()> {
    let field_types = field_types.collect::<Vec<_>>();
    if fields.len() != field_types.len() {
        return Err(());
    }
    let mut bytes = vec![0; base];
    let mut references = Vec::new();
    let mut cursor = base;
    for (field, ty) in fields.iter().zip(field_types) {
        let inline = frozen_inline(field, ty, binding, store)?;
        let width = ty.word_width().ok_or(())?.checked_mul(8).ok_or(())?;
        if inline.bytes.len() != width {
            return Err(());
        }
        bytes.extend_from_slice(&inline.bytes);
        for (offset, reference) in inline.references {
            references.push((
                u32::try_from(cursor.checked_add(offset as usize).ok_or(())?).map_err(|_| ())?,
                reference,
            ));
        }
        cursor = cursor.checked_add(width).ok_or(())?;
    }
    Ok(FrozenInline { bytes, references })
}

fn publication_schema(binding: &ValueInputBinding, ty: &Type) -> Result<weavy::SchemaRef, ()> {
    binding
        .publication_schemas
        .iter()
        .find_map(|(candidate, schema)| (candidate == ty).then_some(*schema))
        .ok_or(())
}

fn read_payload_word(bytes: &[u8], offset: usize) -> Option<i64> {
    Some(i64::from_le_bytes(
        bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn semantic_schema_ref(ty: &Type) -> SchemaRef {
    ty.schema_ref()
}

fn effect_schema(ty: &Type) -> SchemaRef {
    semantic_schema_ref(ty)
}

fn effect_leaf(ty: &Type, resident: Vec<u8>) -> EffectValue {
    let node = FramedNode::leaf(effect_schema(ty), resident.clone());
    let identity = node.identity();
    EffectValue {
        identity,
        resident,
        frozen: None,
        node: Some(node),
    }
}

fn primitive_demand_key(primitive: &super::PrimitiveId, request: &ValueId) -> DemandKey {
    let version = primitive.version.to_le_bytes();
    let recipe = RecipeId(hash_framed(
        b"vix.primitive.recipe.v1",
        &[
            primitive.namespace.as_bytes(),
            primitive.name.as_bytes(),
            &version,
        ],
    ));
    DemandKey::from_preimage(&DemandPreimage {
        closure: recipe,
        arguments: vec![request.clone()],
    })
}

fn primitive_value_from_frame(
    frame: &[u8],
    region: super::FrameRegion,
    ty: &Type,
    store: &Store,
    resolver: &TaskValueResolver<'_>,
    abi_schemas: &[(Type, weavy::SchemaRef)],
) -> Result<PrimitiveValue, String> {
    let expected = ty
        .word_width()
        .ok_or_else(|| format!("{} has no primitive frame representation", ty.name()))?;
    if expected != region.words().as_usize() {
        return Err(format!(
            "primitive request region has {} words for {}-word type {}",
            region.words().as_usize(),
            expected,
            ty.name()
        ));
    }
    primitive_value_from_frame_at(frame, region, 0, ty, store, resolver, abi_schemas)
}

fn primitive_value_from_frame_at(
    frame: &[u8],
    region: super::FrameRegion,
    offset: usize,
    ty: &Type,
    store: &Store,
    resolver: &TaskValueResolver<'_>,
    abi_schemas: &[(Type, weavy::SchemaRef)],
) -> Result<PrimitiveValue, String> {
    let schema = ty.schema_ref();
    match ty {
        Type::Bool | Type::Int | Type::Check => Ok(PrimitiveValue::bytes(
            schema,
            frame_word(frame, region, offset)?.to_le_bytes().to_vec(),
        )),
        Type::String | Type::Path | Type::Extern(_) => {
            let word = frame_word(frame, region, offset)?;
            let abi_schema = abi_schema_for_type(ty, abi_schemas)?;
            let bytes = match resolver
                .resolve_host_word(word, abi_schema)
                .ok_or_else(|| "primitive reference handle is absent".to_owned())?
            {
                ResolvedTaskValue::Store(handle) => {
                    let entry = store
                        .entry_by_weavy_handle(handle)
                        .ok_or_else(|| "primitive Store handle is absent".to_owned())?;
                    if entry.identity.schema != schema {
                        return Err(format!(
                            "primitive Store handle schema {} disagrees with {}",
                            entry.identity.schema, schema
                        ));
                    }
                    entry
                        .resident_bytes()
                        .ok_or_else(|| "primitive Store handle is not resident".to_owned())?
                        .to_vec()
                }
                ResolvedTaskValue::TaskMolten(bytes) => bytes.to_vec(),
                ResolvedTaskValue::LentMolten { .. } => {
                    return Err("primitive request cannot retain lent molten bytes".to_owned());
                }
            };
            Ok(PrimitiveValue::bytes(schema, bytes))
        }
        Type::Tuple(elements) => {
            let mut cursor = offset;
            let mut fields = Vec::with_capacity(elements.len());
            for element in elements {
                let value = primitive_value_from_frame_at(
                    frame,
                    region,
                    cursor,
                    element,
                    store,
                    resolver,
                    abi_schemas,
                )?;
                fields.push(primitive_field(element, value)?);
                cursor += primitive_type_words(element)?;
            }
            Ok(PrimitiveValue {
                schema,
                body: PrimitiveValueBody::Product(fields),
            })
        }
        Type::Record(record) => {
            let mut cursor = offset;
            let mut fields = Vec::with_capacity(record.fields.len());
            for field in &record.fields {
                let value = primitive_value_from_frame_at(
                    frame,
                    region,
                    cursor,
                    &field.ty,
                    store,
                    resolver,
                    abi_schemas,
                )?;
                fields.push(primitive_field(&field.ty, value)?);
                cursor += primitive_type_words(&field.ty)?;
            }
            Ok(PrimitiveValue {
                schema,
                body: PrimitiveValueBody::Product(fields),
            })
        }
        Type::Enum(enumeration) => {
            let tag_word = frame_word(frame, region, offset)?;
            let tag = u32::try_from(tag_word)
                .map_err(|_| format!("primitive enum tag {tag_word} is invalid"))?;
            let variant = enumeration
                .variants
                .get(tag as usize)
                .ok_or_else(|| format!("primitive enum tag {tag} is out of range"))?;
            let field_types = variant_field_types(&variant.payload);
            let mut cursor = offset + 1;
            let mut fields = Vec::with_capacity(field_types.len());
            for field_ty in field_types {
                let value = primitive_value_from_frame_at(
                    frame,
                    region,
                    cursor,
                    field_ty,
                    store,
                    resolver,
                    abi_schemas,
                )?;
                fields.push(primitive_field(field_ty, value)?);
                cursor += primitive_type_words(field_ty)?;
            }
            Ok(PrimitiveValue {
                schema,
                body: PrimitiveValueBody::Variant { tag, fields },
            })
        }
        Type::Array(element) => {
            let word = frame_word(frame, region, offset)?;
            let schema = abi_schema_for_type(ty, abi_schemas)?;
            let dense = resolver
                .resolve_dense_host_word(word, schema)
                .map_err(|fault| format!("primitive array resolution failed: {fault:?}"))?;
            let elements = dense
                .elements()
                .iter()
                .copied()
                .map(|value| primitive_value_from_task(value, element, store, resolver))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PrimitiveValue {
                schema: ty.schema_ref(),
                body: PrimitiveValueBody::Sequence {
                    element_schema: element.schema_ref(),
                    elements,
                },
            })
        }
        Type::Function { .. }
        | Type::Map { .. }
        | Type::Set(_)
        | Type::Stream { .. }
        | Type::Order(_)
        | Type::StreamCheck => Err(format!(
            "primitive frame codec does not admit {}",
            ty.name()
        )),
    }
}

fn primitive_field(ty: &Type, value: PrimitiveValue) -> Result<PrimitiveField, String> {
    if matches!(ty, Type::Bool | Type::Int | Type::Check) {
        let PrimitiveValue { schema, body } = value;
        let PrimitiveValueBody::Bytes(bytes) = body else {
            return Err(format!(
                "inline primitive field {} was structural",
                ty.name()
            ));
        };
        Ok(PrimitiveField {
            schema,
            value: PrimitiveFieldValue::Inline(bytes),
        })
    } else {
        Ok(PrimitiveField {
            schema: value.schema.clone(),
            value: PrimitiveFieldValue::Child(Box::new(value)),
        })
    }
}

fn primitive_value_from_task(
    value: TaskStructuralValue<'_>,
    ty: &Type,
    store: &Store,
    resolver: &TaskValueResolver<'_>,
) -> Result<PrimitiveValue, String> {
    let schema = ty.schema_ref();
    match ty {
        Type::Bool | Type::Int | Type::Check => Ok(PrimitiveValue::bytes(
            schema,
            value
                .scalar_word()
                .map_err(|fault| format!("primitive scalar resolution failed: {fault:?}"))?
                .to_le_bytes()
                .to_vec(),
        )),
        Type::String | Type::Path | Type::Extern(_) => {
            let reference = value
                .value_ref()
                .map_err(|fault| format!("primitive reference resolution failed: {fault:?}"))?;
            let bytes = match resolver
                .resolve(reference)
                .ok_or_else(|| "primitive reference handle is absent".to_owned())?
            {
                ResolvedTaskValue::Store(handle) => store
                    .entry_by_weavy_handle(handle)
                    .and_then(StoreEntry::resident_bytes)
                    .ok_or_else(|| "primitive Store reference is not resident".to_owned())?
                    .to_vec(),
                ResolvedTaskValue::TaskMolten(bytes) => bytes.to_vec(),
                ResolvedTaskValue::LentMolten { .. } => {
                    return Err("primitive request cannot retain a lent molten value".to_owned());
                }
            };
            Ok(PrimitiveValue::bytes(schema, bytes))
        }
        Type::Tuple(elements) => {
            let fields = elements
                .iter()
                .enumerate()
                .map(|(index, ty)| {
                    let index = u32::try_from(index)
                        .map_err(|_| "primitive tuple field index overflowed".to_owned())?;
                    let field = value.product_field(index).map_err(|fault| {
                        format!("primitive tuple field resolution failed: {fault:?}")
                    })?;
                    primitive_value_from_task(field, ty, store, resolver)
                        .and_then(|value| primitive_field(ty, value))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PrimitiveValue {
                schema,
                body: PrimitiveValueBody::Product(fields),
            })
        }
        Type::Record(record) => {
            let fields = record
                .fields
                .iter()
                .enumerate()
                .map(|(index, declared)| {
                    let index = u32::try_from(index)
                        .map_err(|_| "primitive record field index overflowed".to_owned())?;
                    let field = value.product_field(index).map_err(|fault| {
                        format!("primitive record field resolution failed: {fault:?}")
                    })?;
                    primitive_value_from_task(field, &declared.ty, store, resolver)
                        .and_then(|value| primitive_field(&declared.ty, value))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PrimitiveValue {
                schema,
                body: PrimitiveValueBody::Product(fields),
            })
        }
        Type::Enum(enumeration) => {
            let tag = value
                .enum_selector()
                .map_err(|fault| format!("primitive enum resolution failed: {fault:?}"))?;
            let variant = enumeration
                .variants
                .get(tag as usize)
                .ok_or_else(|| format!("primitive enum tag {tag} is out of range"))?;
            let fields = variant_field_types(&variant.payload)
                .into_iter()
                .enumerate()
                .map(|(index, ty)| {
                    let index = u32::try_from(index)
                        .map_err(|_| "primitive enum field index overflowed".to_owned())?;
                    let field = value.enum_field(tag, index).map_err(|fault| {
                        format!("primitive enum field resolution failed: {fault:?}")
                    })?;
                    primitive_value_from_task(field, ty, store, resolver)
                        .and_then(|value| primitive_field(ty, value))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PrimitiveValue {
                schema,
                body: PrimitiveValueBody::Variant { tag, fields },
            })
        }
        Type::Array(element) => {
            let reference = value
                .value_ref()
                .map_err(|fault| format!("primitive array reference failed: {fault:?}"))?;
            let dense = resolver
                .resolve_dense(reference)
                .map_err(|fault| format!("primitive array resolution failed: {fault:?}"))?;
            let elements = dense
                .elements()
                .iter()
                .copied()
                .map(|value| primitive_value_from_task(value, element, store, resolver))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PrimitiveValue {
                schema,
                body: PrimitiveValueBody::Sequence {
                    element_schema: element.schema_ref(),
                    elements,
                },
            })
        }
        Type::Function { .. }
        | Type::Map { .. }
        | Type::Set(_)
        | Type::Stream { .. }
        | Type::Order(_)
        | Type::StreamCheck => Err(format!("primitive task codec does not admit {}", ty.name())),
    }
}

fn abi_schema_for_type(
    ty: &Type,
    abi_schemas: &[(Type, weavy::SchemaRef)],
) -> Result<weavy::SchemaRef, String> {
    abi_schemas
        .iter()
        .find_map(|(candidate, schema)| (candidate == ty).then_some(*schema))
        .ok_or_else(|| format!("{} is absent from the primitive ABI catalog", ty.name()))
}

fn write_primitive_value(
    task: &mut weavy::exec::ExecTask,
    region: super::FrameRegion,
    offset: usize,
    ty: &Type,
    value: &PrimitiveValue,
    store: &mut Store,
    interned: &mut Vec<Interned>,
) -> Result<(), String> {
    if value.schema != ty.schema_ref() {
        return Err(format!(
            "primitive value schema {} disagrees with {}",
            value.schema,
            ty.schema_ref()
        ));
    }
    match (ty, &value.body) {
        (Type::Bool | Type::Int | Type::Check, PrimitiveValueBody::Bytes(bytes)) => {
            let word = i64::from_le_bytes(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| format!("primitive scalar {} is not one word", ty.name()))?,
            );
            write_primitive_word(task, region, offset, word)
        }
        (Type::String | Type::Path | Type::Extern(_), PrimitiveValueBody::Bytes(bytes)) => {
            let stored = store.intern_realized(ty.schema_ref(), bytes);
            let handle = store
                .weavy_handle(stored.handle)
                .ok_or_else(|| "new primitive result has no Store handle".to_owned())?;
            write_primitive_word(task, region, offset, handle.as_i64())?;
            interned.push(stored);
            Ok(())
        }
        (Type::Tuple(elements), PrimitiveValueBody::Product(fields))
            if elements.len() == fields.len() =>
        {
            let mut cursor = offset;
            for (element, field) in elements.iter().zip(fields) {
                write_primitive_field(task, region, cursor, element, field, store, interned)?;
                cursor += primitive_type_words(element)?;
            }
            Ok(())
        }
        (Type::Record(record), PrimitiveValueBody::Product(fields))
            if record.fields.len() == fields.len() =>
        {
            let mut cursor = offset;
            for (declared, field) in record.fields.iter().zip(fields) {
                write_primitive_field(task, region, cursor, &declared.ty, field, store, interned)?;
                cursor += primitive_type_words(&declared.ty)?;
            }
            Ok(())
        }
        (Type::Enum(enumeration), PrimitiveValueBody::Variant { tag, fields }) => {
            let variant = enumeration
                .variants
                .get(*tag as usize)
                .ok_or_else(|| format!("primitive result enum tag {tag} is out of range"))?;
            let field_types = variant_field_types(&variant.payload);
            if field_types.len() != fields.len() {
                return Err(
                    "primitive result variant field count disagrees with its type".to_owned(),
                );
            }
            write_primitive_word(task, region, offset, i64::from(*tag))?;
            let mut cursor = offset + 1;
            for (field_ty, field) in field_types.into_iter().zip(fields) {
                write_primitive_field(task, region, cursor, field_ty, field, store, interned)?;
                cursor += primitive_type_words(field_ty)?;
            }
            Ok(())
        }
        _ => Err(format!(
            "primitive value body disagrees with frame type {}",
            ty.name()
        )),
    }
}

fn write_primitive_field(
    task: &mut weavy::exec::ExecTask,
    region: super::FrameRegion,
    offset: usize,
    ty: &Type,
    field: &PrimitiveField,
    store: &mut Store,
    interned: &mut Vec<Interned>,
) -> Result<(), String> {
    if field.schema != ty.schema_ref() {
        return Err(format!(
            "primitive field schema {} disagrees with {}",
            field.schema,
            ty.schema_ref()
        ));
    }
    let value = match &field.value {
        PrimitiveFieldValue::Inline(bytes) => {
            PrimitiveValue::bytes(field.schema.clone(), bytes.clone())
        }
        PrimitiveFieldValue::Child(value) => (**value).clone(),
    };
    write_primitive_value(task, region, offset, ty, &value, store, interned)
}

fn write_primitive_word(
    task: &mut weavy::exec::ExecTask,
    region: super::FrameRegion,
    offset: usize,
    word: i64,
) -> Result<(), String> {
    let slot = region
        .word(offset)
        .ok_or_else(|| format!("primitive frame word {offset} is outside its region"))?;
    task.write_host_word(slot.byte_offset(), word)
        .map_err(|fault| format!("primitive frame write failed: {fault:?}"))
}

fn frame_word(frame: &[u8], region: super::FrameRegion, offset: usize) -> Result<i64, String> {
    let slot = region
        .word(offset)
        .ok_or_else(|| format!("primitive frame word {offset} is outside its region"))?;
    let start = slot.byte_offset() as usize;
    let end = start
        .checked_add(8)
        .ok_or_else(|| "primitive frame word offset overflowed".to_owned())?;
    let bytes = frame
        .get(start..end)
        .ok_or_else(|| format!("primitive frame word {offset} is absent"))?;
    Ok(i64::from_le_bytes(
        bytes
            .try_into()
            .expect("primitive frame word is eight bytes"),
    ))
}

fn primitive_type_words(ty: &Type) -> Result<usize, String> {
    ty.word_width()
        .ok_or_else(|| format!("{} has no primitive frame representation", ty.name()))
}

fn variant_field_types(payload: &VariantPayload) -> Vec<&Type> {
    match payload {
        VariantPayload::Unit => Vec::new(),
        VariantPayload::Tuple(elements) => elements.iter().collect(),
        VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
    }
}

fn insert_schema_type(ty: &Type, catalog: &mut BTreeMap<SchemaRef, Type>) {
    catalog.entry(ty.schema_ref()).or_insert_with(|| ty.clone());
    match ty {
        Type::Function { parameter, result } => {
            insert_schema_type(parameter, catalog);
            insert_schema_type(result, catalog);
        }
        Type::Tuple(elements) => {
            for element in elements {
                insert_schema_type(element, catalog);
            }
        }
        Type::Record(record) => {
            for field in &record.fields {
                insert_schema_type(&field.ty, catalog);
            }
        }
        Type::Enum(enumeration) => {
            for variant in &enumeration.variants {
                match &variant.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Tuple(elements) => {
                        for element in elements {
                            insert_schema_type(element, catalog);
                        }
                    }
                    VariantPayload::Record(fields) => {
                        for field in fields {
                            insert_schema_type(&field.ty, catalog);
                        }
                    }
                }
            }
        }
        Type::Array(element) | Type::Set(element) | Type::Order(element) => {
            insert_schema_type(element, catalog);
        }
        Type::Map { key, value } | Type::Stream { key, value } => {
            insert_schema_type(key, catalog);
            insert_schema_type(value, catalog);
        }
        Type::Bool
        | Type::Int
        | Type::Check
        | Type::StreamCheck
        | Type::String
        | Type::Path
        | Type::Extern(_) => {}
    }
}

fn primitive_field_from_effect(
    ty: &Type,
    value: EffectValue,
) -> Result<PrimitiveField, Box<MachineError>> {
    let value = primitive_value_from_effect(ty, &value)?;
    if matches!(ty, Type::Bool | Type::Int | Type::Check) {
        let PrimitiveValue {
            schema,
            body: PrimitiveValueBody::Bytes(bytes),
        } = value
        else {
            return effect_fault("inline primitive field was not bytes");
        };
        Ok(PrimitiveField {
            schema,
            value: PrimitiveFieldValue::Inline(bytes),
        })
    } else {
        Ok(PrimitiveField {
            schema: value.schema.clone(),
            value: PrimitiveFieldValue::Child(Box::new(value)),
        })
    }
}

fn primitive_child_field(value: PrimitiveValue) -> PrimitiveField {
    PrimitiveField {
        schema: value.schema.clone(),
        value: PrimitiveFieldValue::Child(Box::new(value)),
    }
}

fn primitive_value_from_effect(
    ty: &Type,
    value: &EffectValue,
) -> Result<PrimitiveValue, Box<MachineError>> {
    if value.identity.schema != ty.schema_ref() {
        return effect_fault("effect value schema disagreed with its declared type");
    }
    match ty {
        Type::Bool | Type::Int | Type::Check | Type::String | Type::Path | Type::Extern(_) => Ok(
            PrimitiveValue::bytes(ty.schema_ref(), value.resident.clone()),
        ),
        _ => value
            .frozen
            .as_ref()
            .ok_or_else(|| effect_machine_error("structural effect value was not frozen"))
            .and_then(|frozen| primitive_value_from_frozen(ty, frozen)),
    }
}

fn primitive_value_from_frozen(
    ty: &Type,
    frozen: &FrozenValue,
) -> Result<PrimitiveValue, Box<MachineError>> {
    match (ty, frozen) {
        (Type::Bool | Type::Int | Type::Check, FrozenValue::Inline(bytes))
        | (Type::String | Type::Path | Type::Extern(_), FrozenValue::Opaque(bytes)) => {
            Ok(PrimitiveValue::bytes(ty.schema_ref(), bytes.clone()))
        }
        (Type::Record(record), FrozenValue::Product(values))
            if record.fields.len() == values.len() =>
        {
            let fields = record
                .fields
                .iter()
                .zip(values)
                .map(|(field, value)| {
                    primitive_value_from_frozen(&field.ty, value).and_then(|value| {
                        if matches!(field.ty, Type::Bool | Type::Int | Type::Check) {
                            let PrimitiveValueBody::Bytes(bytes) = value.body else {
                                return effect_fault("inline frozen field was not bytes");
                            };
                            Ok(PrimitiveField {
                                schema: value.schema,
                                value: PrimitiveFieldValue::Inline(bytes),
                            })
                        } else {
                            Ok(PrimitiveField {
                                schema: value.schema.clone(),
                                value: PrimitiveFieldValue::Child(Box::new(value)),
                            })
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PrimitiveValue {
                schema: ty.schema_ref(),
                body: PrimitiveValueBody::Product(fields),
            })
        }
        (Type::Enum(enumeration), FrozenValue::Variant { tag, fields }) => {
            let variant = enumeration
                .variants
                .get(*tag as usize)
                .ok_or_else(|| effect_machine_error("frozen primitive variant tag was invalid"))?;
            let field_types = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(elements) => elements.iter().collect(),
                VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
            };
            if field_types.len() != fields.len() {
                return effect_fault("frozen primitive variant field count was invalid");
            }
            let fields = field_types
                .into_iter()
                .zip(fields)
                .map(|(ty, value)| {
                    primitive_value_from_frozen(ty, value).and_then(|value| {
                        if matches!(ty, Type::Bool | Type::Int | Type::Check) {
                            let PrimitiveValueBody::Bytes(bytes) = value.body else {
                                return effect_fault("inline frozen variant field was not bytes");
                            };
                            Ok(PrimitiveField {
                                schema: value.schema,
                                value: PrimitiveFieldValue::Inline(bytes),
                            })
                        } else {
                            Ok(PrimitiveField {
                                schema: value.schema.clone(),
                                value: PrimitiveFieldValue::Child(Box::new(value)),
                            })
                        }
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PrimitiveValue {
                schema: ty.schema_ref(),
                body: PrimitiveValueBody::Variant { tag: *tag, fields },
            })
        }
        (Type::Array(element), FrozenValue::DenseArray(values)) => values
            .iter()
            .map(|value| primitive_value_from_frozen(element, value))
            .collect::<Result<Vec<_>, _>>()
            .map(|elements| PrimitiveValue {
                schema: ty.schema_ref(),
                body: PrimitiveValueBody::Sequence {
                    element_schema: element.schema_ref(),
                    elements,
                },
            }),
        _ => effect_fault("frozen value cannot become a primitive value"),
    }
}

fn effect_value_from_primitive(
    ty: &Type,
    value: PrimitiveValue,
) -> Result<EffectValue, Box<MachineError>> {
    if value.schema != ty.schema_ref() {
        return effect_fault("primitive result schema disagreed with the invocation type");
    }
    let node = value.framed_node();
    let identity = node.identity();
    let resident = value.resident_bytes().to_vec();
    let frozen = primitive_value_to_frozen(ty, &value)?;
    Ok(EffectValue {
        identity,
        resident,
        frozen: Some(frozen),
        node: Some(node),
    })
}

fn primitive_value_to_frozen(
    ty: &Type,
    value: &PrimitiveValue,
) -> Result<FrozenValue, Box<MachineError>> {
    match (ty, &value.body) {
        (Type::Bool | Type::Int | Type::Check, PrimitiveValueBody::Bytes(bytes)) => {
            Ok(FrozenValue::Inline(bytes.clone()))
        }
        (Type::String | Type::Path | Type::Extern(_), PrimitiveValueBody::Bytes(bytes)) => {
            Ok(FrozenValue::Opaque(bytes.clone()))
        }
        (Type::Record(record), PrimitiveValueBody::Product(fields))
            if record.fields.len() == fields.len() =>
        {
            let fields = record
                .fields
                .iter()
                .zip(fields)
                .map(|(declared, field)| match &field.value {
                    PrimitiveFieldValue::Inline(bytes) => Ok(FrozenValue::Inline(bytes.clone())),
                    PrimitiveFieldValue::Child(value) => {
                        primitive_value_to_frozen(&declared.ty, value)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FrozenValue::Product(fields))
        }
        (Type::Enum(enumeration), PrimitiveValueBody::Variant { tag, fields }) => {
            let variant = enumeration
                .variants
                .get(*tag as usize)
                .ok_or_else(|| effect_machine_error("primitive result variant tag was invalid"))?;
            let field_types = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(elements) => elements.iter().collect(),
                VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
            };
            if field_types.len() != fields.len() {
                return effect_fault("primitive result variant field count was invalid");
            }
            let fields = field_types
                .into_iter()
                .zip(fields)
                .map(|(ty, field)| match &field.value {
                    PrimitiveFieldValue::Inline(bytes) => Ok(FrozenValue::Inline(bytes.clone())),
                    PrimitiveFieldValue::Child(value) => primitive_value_to_frozen(ty, value),
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FrozenValue::Variant { tag: *tag, fields })
        }
        (
            Type::Array(element),
            PrimitiveValueBody::Sequence {
                element_schema,
                elements,
            },
        ) if element_schema == &element.schema_ref() => elements
            .iter()
            .map(|value| primitive_value_to_frozen(element, value))
            .collect::<Result<Vec<_>, _>>()
            .map(FrozenValue::DenseArray),
        _ => effect_fault("primitive value cannot be frozen as its invocation type"),
    }
}

fn effect_value_from_frozen(
    ty: &Type,
    frozen: FrozenValue,
) -> Result<EffectValue, Box<MachineError>> {
    match (&frozen, ty) {
        (FrozenValue::Inline(bytes), Type::Int | Type::Bool | Type::Check) => {
            let mut effect = effect_leaf(ty, bytes.clone());
            effect.frozen = Some(frozen);
            Ok(effect)
        }
        (FrozenValue::Opaque(bytes), Type::String | Type::Path) => {
            let mut effect = effect_leaf(ty, bytes.clone());
            effect.frozen = Some(frozen);
            Ok(effect)
        }
        (FrozenValue::Opaque(bytes), Type::Extern(ExternKind::Tree)) => {
            parse_ustar(bytes)
                .map_err(|_| effect_machine_error("frozen Tree was not plain ustar"))?;
            let canonical = canonical_archive_tree(bytes);
            let node = FramedNode::leaf(effect_schema(ty), canonical);
            Ok(EffectValue {
                identity: node.identity(),
                resident: bytes.clone(),
                frozen: Some(frozen),
                node: Some(node),
            })
        }
        (FrozenValue::Opaque(bytes), Type::Extern(_)) => {
            let mut effect = effect_leaf(ty, bytes.clone());
            effect.frozen = Some(frozen);
            Ok(effect)
        }
        (FrozenValue::Product(fields), Type::Record(record)) => {
            if fields.len() != record.fields.len() {
                return effect_fault("frozen product field count disagreed with schema");
            }
            let mut framed = Vec::with_capacity(fields.len());
            for (field, value) in record.fields.iter().zip(fields) {
                let effect = effect_value_from_frozen(&field.ty, value.clone())?;
                let framed_value = if matches!(field.ty, Type::Bool | Type::Int | Type::Check) {
                    FramedValue::Bytes(effect.resident)
                } else {
                    FramedValue::Optional(Some(effect.identity))
                };
                framed.push(FramedField {
                    schema: effect_schema(&field.ty),
                    value: framed_value,
                });
            }
            let node = FramedNode::Variant {
                schema: effect_schema(ty),
                tag: 0,
                fields: framed,
            };
            Ok(EffectValue {
                identity: node.identity(),
                resident: Vec::new(),
                frozen: Some(frozen),
                node: Some(node),
            })
        }
        (FrozenValue::DenseArray(elements), Type::Array(element)) => {
            let mut children = Vec::with_capacity(elements.len());
            for value in elements {
                children.push(effect_value_from_frozen(element, value.clone())?.identity);
            }
            let node = FramedNode::SeqChildren {
                schema: effect_schema(ty),
                element_schema: effect_schema(element),
                children,
            };
            Ok(EffectValue {
                identity: node.identity(),
                resident: Vec::new(),
                frozen: Some(frozen),
                node: Some(node),
            })
        }
        (FrozenValue::OrderedMap(rows), Type::Map { key, value }) => {
            let mut identities = Vec::with_capacity(rows.len());
            for (row_key, row_value) in rows {
                let key = effect_value_from_frozen(key, row_key.clone())?;
                let value = effect_value_from_frozen(value, row_value.clone())?;
                identities.push((key.identity, value.identity));
            }
            let node = FramedNode::OrderedMap {
                schema: effect_schema(ty),
                rows: identities,
            };
            Ok(EffectValue {
                identity: node.identity(),
                resident: Vec::new(),
                frozen: Some(frozen),
                node: Some(node),
            })
        }
        (FrozenValue::Variant { tag, fields }, Type::Enum(enumeration)) => {
            let variant = enumeration
                .variants
                .get(*tag as usize)
                .ok_or_else(|| effect_machine_error("frozen enum tag disagreed with schema"))?;
            let field_types = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(elements) => elements.iter().collect::<Vec<_>>(),
                VariantPayload::Record(fields) => {
                    fields.iter().map(|field| &field.ty).collect::<Vec<_>>()
                }
            };
            if fields.len() != field_types.len() {
                return effect_fault("frozen enum field count disagreed with schema");
            }
            let mut framed = Vec::with_capacity(fields.len());
            for (field_ty, value) in field_types.into_iter().zip(fields) {
                let effect = effect_value_from_frozen(field_ty, value.clone())?;
                let framed_value = if matches!(field_ty, Type::Bool | Type::Int | Type::Check) {
                    FramedValue::Bytes(effect.resident)
                } else {
                    FramedValue::Optional(Some(effect.identity))
                };
                framed.push(FramedField {
                    schema: effect_schema(field_ty),
                    value: framed_value,
                });
            }
            let node = FramedNode::Variant {
                schema: effect_schema(ty),
                tag: u64::from(*tag),
                fields: framed,
            };
            Ok(EffectValue {
                identity: node.identity(),
                resident: Vec::new(),
                frozen: Some(frozen),
                node: Some(node),
            })
        }
        _ => effect_fault("frozen value did not match target schema"),
    }
}

fn read_i64(bytes: &[u8]) -> Option<i64> {
    Some(i64::from_le_bytes(bytes.get(..8)?.try_into().ok()?))
}

fn effect_machine_error(detail: &'static str) -> Box<MachineError> {
    Box::new(MachineError::runtime(
        MachineOperation::Effect,
        RuntimeFault::EffectPlane { detail },
        None,
        None,
    ))
}

fn effect_fault<T>(detail: &'static str) -> Result<T, Box<MachineError>> {
    Err(effect_machine_error(detail))
}

fn split_tree_entry(bytes: &[u8]) -> Result<(&[u8], &[u8]), Box<MachineError>> {
    let prefix = b"tree-entry\0";
    let header = prefix
        .len()
        .checked_add(8)
        .ok_or_else(|| effect_machine_error("tree entry header overflow"))?;
    if !bytes.starts_with(prefix) || bytes.len() < header {
        return effect_fault("tree entry payload was malformed");
    }
    let length = u64::from_le_bytes(
        bytes[prefix.len()..header]
            .try_into()
            .expect("eight-byte tree entry length"),
    );
    let length =
        usize::try_from(length).map_err(|_| effect_machine_error("tree entry length overflow"))?;
    let tree_end = header
        .checked_add(length)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| effect_machine_error("tree entry payload was truncated"))?;
    Ok((&bytes[header..tree_end], &bytes[tree_end..]))
}

fn fixture_tree_name(bytes: &[u8]) -> Option<&[u8]> {
    let name = bytes.strip_prefix(b"fixture-tree\0")?;
    Some(name.split(|byte| *byte == 0).next().unwrap_or(name))
}

fn read_exec_stdout(
    stdout: impl Read,
    protocol: ExecProtocol,
    workspace: &Path,
    demand: DemandKey,
    sender: &std::sync::mpsc::Sender<DeliveredCompletion>,
) -> Result<Vec<u8>, String> {
    const READY_PREFIX: &[u8] = b"vix-ready\t";
    let mut reader = BufReader::new(stdout);
    let mut output = Vec::new();
    let mut line = Vec::new();
    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("read exec stdout: {error}"))?;
        if read == 0 {
            break;
        }
        if protocol == ExecProtocol::ProgressiveLinesV1 && line.starts_with(READY_PREFIX) {
            let mut path = &line[READY_PREFIX.len()..];
            if path.ends_with(b"\n") {
                path = &path[..path.len() - 1];
            }
            if path.ends_with(b"\r") {
                path = &path[..path.len() - 1];
            }
            let path = core::str::from_utf8(path)
                .map_err(|_| "progressive exec path was not UTF-8".to_owned())?;
            validate_exec_product_path(path)?;
            let bytes = std::fs::read(workspace.join(path)).map_err(|error| {
                format!("read progressive exec product `{path}` after readiness: {error}")
            });
            let _ = sender.send(DeliveredCompletion::ExecProgress {
                demand,
                product: bytes.map(|bytes| ExecProgress {
                    path: path.to_owned(),
                    bytes,
                }),
            });
        } else {
            output.extend_from_slice(&line);
        }
    }
    Ok(output)
}

fn validate_exec_product_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("progressive exec product path was empty".to_owned());
    }
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "progressive exec product `{}` was not a relative normal path",
            path.display()
        ));
    }
    Ok(())
}

fn archive_directory(root: &Path) -> Result<Vec<u8>, String> {
    fn collect(directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
        let mut entries = std::fs::read_dir(directory)
            .map_err(|error| {
                format!(
                    "read exec output directory `{}`: {error}",
                    directory.display()
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("read exec output entry: {error}"))?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| format!("inspect exec output `{}`: {error}", path.display()))?;
            if metadata.file_type().is_symlink() {
                return Err(format!(
                    "exec output symlink `{}` is not yet supported",
                    path.display()
                ));
            }
            if metadata.is_dir() {
                collect(&path, files)?;
            } else if metadata.is_file() {
                files.push(path);
            }
        }
        Ok(())
    }

    fn write_octal(dst: &mut [u8], value: u64) -> Result<(), String> {
        let width = dst
            .len()
            .checked_sub(1)
            .ok_or_else(|| "ustar octal field was empty".to_owned())?;
        let text = format!("{value:0width$o}\0");
        if text.len() != dst.len() {
            return Err(format!(
                "ustar value {value} overflowed {} bytes",
                dst.len()
            ));
        }
        dst.copy_from_slice(text.as_bytes());
        Ok(())
    }

    let mut files = Vec::new();
    collect(root, &mut files)?;
    files.sort();
    let mut archive = Vec::new();
    for file in files {
        let relative = file
            .strip_prefix(root)
            .map_err(|_| format!("exec output `{}` escaped its workspace", file.display()))?;
        let relative = relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if relative.len() > 100 {
            return Err(format!("exec output path `{relative}` exceeds ustar v1"));
        }
        let bytes = std::fs::read(&file)
            .map_err(|error| format!("read exec output `{}`: {error}", file.display()))?;
        let mut header = [0u8; 512];
        header[..relative.len()].copy_from_slice(relative.as_bytes());
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        write_octal(&mut header[124..136], bytes.len() as u64)?;
        header[136..148].copy_from_slice(b"00000000000\0");
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        let checksum = format!("{checksum:06o}\0 ");
        header[148..156].copy_from_slice(checksum.as_bytes());
        archive.extend_from_slice(&header);
        archive.extend_from_slice(&bytes);
        archive.resize(archive.len().div_ceil(512) * 512, 0);
    }
    archive.resize(archive.len() + 1024, 0);
    Ok(archive)
}

/// Canonical archive-tree identity material. It records entry kinds, paths,
/// modes relevant to the Tree model, and file/symlink payloads in path order;
/// the archive's block layout, padding, and original member order never enter.
fn canonical_archive_tree(bytes: &[u8]) -> Vec<u8> {
    let mut members = parse_ustar(bytes).expect("validated before canonical tree encoding");
    members.sort_by(|left, right| left.path().as_bytes().cmp(right.path().as_bytes()));
    let mut encoded = Vec::new();
    for member in members {
        match member {
            TarMember::File {
                path,
                bytes,
                executable,
            } => {
                encoded.push(0);
                frame_effect_tree_field(&mut encoded, path.as_bytes());
                encoded.push(u8::from(executable));
                frame_effect_tree_field(&mut encoded, &bytes);
            }
            TarMember::Dir { path } => {
                encoded.push(1);
                frame_effect_tree_field(&mut encoded, path.as_bytes());
            }
            TarMember::Symlink { path, target } => {
                encoded.push(2);
                frame_effect_tree_field(&mut encoded, path.as_bytes());
                frame_effect_tree_field(&mut encoded, target.as_bytes());
            }
        }
    }
    encoded
}

fn frame_effect_tree_field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    out.extend_from_slice(bytes);
}

fn invalid_realized_result(lowered: &LoweringArtifact, size: usize) -> TaskFault {
    TaskFault::InvalidResultShape {
        entry: FnId(0),
        region: lowered.executable().program().contract().functions[0].result,
        size,
    }
}

fn failure_context(
    failure: &FailureValue,
    lowered: &DemandExecution<'_>,
    attribution: &LoweringAttribution,
) -> Option<FailureContext> {
    // r[impl machine.error.failure-source-site-identity]
    match failure {
        FailureValue::IndexOutOfBounds { recipe, site, .. }
        | FailureValue::MissingKey { recipe, site }
        | FailureValue::DuplicateKey { recipe, site }
        | FailureValue::MissingDelimiter { recipe, site }
        | FailureValue::InvalidInteger { recipe, site }
        | FailureValue::IntegerOverflow { recipe, site }
        | FailureValue::DivisionByZero { recipe, site }
            if *recipe == lowered.recipe =>
        {
            let source = attribution.source_for_trace(*site)?;
            Some(FailureContext {
                function: source.function,
                node: source.node,
                span: source.span,
                demand_chain: vec![lowered.demand_key],
            })
        }
        // Effect-plane failures carry an effect recipe, never a lowered
        // island's; their context is attached where the effect evaluates.
        _ => None,
    }
}

/// Tier-1 exec plan identity: the normalized command. The v1 ratchet capability
/// packages' command grammar is fully positional, so the normalized plan is the
/// parsed argv itself, framed element by element.
///
/// r[impl machine.primitive.exec-identity]
/// r[impl machine.primitive.exec-plan-normalized]
fn exec_plan_recipe(argv: &[String]) -> RecipeId {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"vix.exec.plan.v1");
    bytes.extend_from_slice(&(argv.len() as u64).to_le_bytes());
    for argument in argv {
        bytes.extend_from_slice(&(argument.len() as u64).to_le_bytes());
        bytes.extend_from_slice(argument.as_bytes());
    }
    RecipeId::from_canonical_vir(&bytes)
}

enum DecodedResult {
    OkScalar(bool),
    /// A scalar `Int`/`Bool` value island published its exact result word — the
    /// demanded pure value, interned under its semantic schema. This is the
    /// wire-demand publication path: a hoisted invocation returns its scalar
    /// result to be memoized and observed, never a `Check` verdict.
    OkScalarValue(i64),
    OkValue,
    IndexOutOfBounds {
        site: u32,
        index: i64,
        length: i64,
    },
    MissingKey {
        site: u32,
    },
    DuplicateKey {
        site: u32,
    },
    MissingDelimiter {
        site: u32,
    },
    InvalidInteger {
        site: u32,
    },
    IntegerOverflow {
        site: u32,
    },
    IntDivisionByZero {
        site: u32,
    },
    ArrayMachine {
        site: u32,
        status: weavy::task::ArrayOpStatus,
    },
    OrderedMachine {
        site: u32,
        status: weavy::task::OrderedOpStatus,
    },
}

fn decode_result(
    task: &weavy::exec::ExecTask,
    lowered: &LoweringArtifact,
) -> Result<DecodedResult, Box<TaskFault>> {
    let Some(abi) = &lowered.array_outcome else {
        // A `Check` island's word is its pass/fail verdict. Scalar value
        // publications carry the demanded word directly; handle and structural
        // publications are realized through their verified result region without
        // manufacturing an array-outcome envelope.
        return Ok(if lowered.publishes_value {
            match lowered.output_type {
                Type::Int | Type::Bool => DecodedResult::OkScalarValue(task.result_i64()?),
                _ => DecodedResult::OkValue,
            }
        } else {
            DecodedResult::OkScalar(task.result_i64()? != 0)
        });
    };
    let result = task.result_structural()?;
    let selector = result.enum_selector()?;
    let selector = u32::try_from(selector).map_err(|_| {
        Box::new(TaskFault::InvalidResultShape {
            entry: FnId(0),
            region: lowered.executable().program().contract().functions[0].result,
            size: 0,
        })
    })?;
    if selector == abi.ok_variant {
        if lowered.publishes_value {
            return Ok(DecodedResult::OkValue);
        }
        return Ok(DecodedResult::OkScalar(
            result.enum_scalar_field(selector, 0)? != 0,
        ));
    }
    if selector == abi.index_out_of_bounds_variant {
        let site = u32::try_from(result.enum_scalar_field(selector, 0)?).map_err(|_| {
            Box::new(TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: lowered.executable().program().contract().functions[0].result,
                size: 0,
            })
        })?;
        return Ok(DecodedResult::IndexOutOfBounds {
            site,
            index: result.enum_scalar_field(selector, 1)?,
            length: result.enum_scalar_field(selector, 2)?,
        });
    }
    if selector == abi.array_machine_variant {
        let site = u32::try_from(result.enum_scalar_field(selector, 0)?).map_err(|_| {
            Box::new(TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: lowered.executable().program().contract().functions[0].result,
                size: 0,
            })
        })?;
        let raw_status = result.enum_scalar_field(selector, 1)?;
        let status = weavy::task::ArrayOpStatus::from_word(raw_status).ok_or(Box::new(
            TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: lowered.executable().program().contract().functions[0].result,
                size: 0,
            },
        ))?;
        return Ok(DecodedResult::ArrayMachine { site, status });
    }
    if selector == abi.ordered_machine_variant {
        let site = u32::try_from(result.enum_scalar_field(selector, 0)?).map_err(|_| {
            Box::new(TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: lowered.executable().program().contract().functions[0].result,
                size: 0,
            })
        })?;
        let raw_status = result.enum_scalar_field(selector, 1)?;
        let status = weavy::task::OrderedOpStatus::from_word(raw_status).ok_or(Box::new(
            TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: lowered.executable().program().contract().functions[0].result,
                size: 0,
            },
        ))?;
        return Ok(DecodedResult::OrderedMachine { site, status });
    }
    if selector == abi.missing_key_variant || selector == abi.duplicate_key_variant {
        let site = u32::try_from(result.enum_scalar_field(selector, 0)?).map_err(|_| {
            Box::new(TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: lowered.executable().program().contract().functions[0].result,
                size: 0,
            })
        })?;
        return Ok(if selector == abi.missing_key_variant {
            DecodedResult::MissingKey { site }
        } else {
            DecodedResult::DuplicateKey { site }
        });
    }
    if selector == abi.string_missing_delimiter_variant
        || selector == abi.string_invalid_integer_variant
        || selector == abi.string_integer_overflow_variant
    {
        let site = u32::try_from(result.enum_scalar_field(selector, 0)?).map_err(|_| {
            Box::new(TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: lowered.executable().program().contract().functions[0].result,
                size: 0,
            })
        })?;
        return Ok(if selector == abi.string_missing_delimiter_variant {
            DecodedResult::MissingDelimiter { site }
        } else if selector == abi.string_invalid_integer_variant {
            DecodedResult::InvalidInteger { site }
        } else {
            DecodedResult::IntegerOverflow { site }
        });
    }
    if selector == abi.int_division_by_zero_variant {
        let site = u32::try_from(result.enum_scalar_field(selector, 0)?).map_err(|_| {
            Box::new(TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: lowered.executable().program().contract().functions[0].result,
                size: 0,
            })
        })?;
        return Ok(DecodedResult::IntDivisionByZero { site });
    }
    Err(Box::new(TaskFault::InvalidResultShape {
        entry: FnId(0),
        region: lowered.executable().program().contract().functions[0].result,
        size: 0,
    }))
}

fn execution_facts(facts: weavy::exec::LaneFacts) -> ExecutionFacts {
    let selected = match facts.selected {
        LaneKind::Interpreter => ExecutionLaneFact::Interpreter,
        LaneKind::Native => ExecutionLaneFact::Native,
    };
    let fallback = facts.fallback.map(|fallback| match fallback {
        FallbackReason::NativeUnavailable => ExecutionFallbackFact::NativeUnavailable,
        FallbackReason::DisabledByEnvironment => ExecutionFallbackFact::DisabledByEnvironment,
        FallbackReason::DisabledByRequest => ExecutionFallbackFact::DisabledByRequest,
    });
    ExecutionFacts {
        selected,
        native_available: facts.native_available,
        native_compiled: facts.native_compiled,
        fallback,
    }
}

fn task_fault_site(fault: &TaskFault) -> Option<&FaultSite> {
    match fault {
        TaskFault::IndirectCalleeNegative { site, .. }
        | TaskFault::IndirectCalleeOutOfRange { site, .. }
        | TaskFault::IndirectCalleeContractMismatch { site, .. }
        | TaskFault::MissingIndirectCallFacts { site }
        | TaskFault::UnresidentCompareValueBytes { site, .. }
        | TaskFault::UnresidentStringConcatOperand { site, .. }
        | TaskFault::StringConcatAllocationFailed { site }
        | TaskFault::UnresidentByteProjectSource { site, .. }
        | TaskFault::ByteProjectionAllocationFailed { site }
        | TaskFault::IntToStringAllocationFailed { site }
        | TaskFault::UnresidentPathJoinOperand { site, .. }
        | TaskFault::PathJoinAllocationFailed { site }
        | TaskFault::PublicationAllocationFailed { site }
        | TaskFault::InvalidEnumSelector { site, .. }
        | TaskFault::EnumProjectionMismatch { site, .. }
        | TaskFault::InvalidArrayStatus { site, .. }
        | TaskFault::InvalidStringStatus { site, .. }
        | TaskFault::InvalidOrderedStatus { site, .. }
        | TaskFault::Environment { site, .. } => Some(site),
        TaskFault::PoisonedReDrive { original } | TaskFault::PoisonedResult { original } => {
            task_fault_site(original)
        }
        TaskFault::InvalidEntryFunction { .. }
        | TaskFault::InvalidEntryShape { .. }
        | TaskFault::InvalidEntryIndex { .. }
        | TaskFault::EntryKindMismatch { .. }
        | TaskFault::EntryMissing { .. }
        | TaskFault::EntryAlreadyInitialized { .. }
        | TaskFault::EntryWriteAfterDrive { .. }
        | TaskFault::EntryValueSize { .. }
        | TaskFault::InvalidResultShape { .. }
        | TaskFault::InvalidResultSelector { .. }
        | TaskFault::DriveTableLength { .. }
        | TaskFault::NativeFaultExit { .. }
        | TaskFault::InvalidFaultSite { .. }
        | TaskFault::ResultBeforeDone { .. }
        | TaskFault::PublicationIndexOutOfRange { .. }
        | TaskFault::DriveAfterDone => None,
    }
}

fn result_shape_attribution(
    fault: &TaskFault,
    output: Option<MachineAttribution>,
) -> Option<MachineAttribution> {
    match fault {
        TaskFault::InvalidResultShape { .. } | TaskFault::InvalidResultSelector { .. } => output,
        TaskFault::PoisonedResult { original } => result_shape_attribution(original, output),
        TaskFault::InvalidEntryFunction { .. }
        | TaskFault::InvalidEntryShape { .. }
        | TaskFault::InvalidEntryIndex { .. }
        | TaskFault::EntryKindMismatch { .. }
        | TaskFault::EntryMissing { .. }
        | TaskFault::EntryAlreadyInitialized { .. }
        | TaskFault::EntryWriteAfterDrive { .. }
        | TaskFault::EntryValueSize { .. }
        | TaskFault::DriveTableLength { .. }
        | TaskFault::IndirectCalleeNegative { .. }
        | TaskFault::IndirectCalleeOutOfRange { .. }
        | TaskFault::IndirectCalleeContractMismatch { .. }
        | TaskFault::MissingIndirectCallFacts { .. }
        | TaskFault::UnresidentCompareValueBytes { .. }
        | TaskFault::UnresidentStringConcatOperand { .. }
        | TaskFault::StringConcatAllocationFailed { .. }
        | TaskFault::UnresidentByteProjectSource { .. }
        | TaskFault::ByteProjectionAllocationFailed { .. }
        | TaskFault::IntToStringAllocationFailed { .. }
        | TaskFault::UnresidentPathJoinOperand { .. }
        | TaskFault::PathJoinAllocationFailed { .. }
        | TaskFault::PublicationAllocationFailed { .. }
        | TaskFault::PublicationIndexOutOfRange { .. }
        | TaskFault::InvalidEnumSelector { .. }
        | TaskFault::EnumProjectionMismatch { .. }
        | TaskFault::InvalidArrayStatus { .. }
        | TaskFault::InvalidStringStatus { .. }
        | TaskFault::InvalidOrderedStatus { .. }
        | TaskFault::Environment { .. }
        | TaskFault::NativeFaultExit { .. }
        | TaskFault::InvalidFaultSite { .. }
        | TaskFault::PoisonedReDrive { .. }
        | TaskFault::ResultBeforeDone { .. }
        | TaskFault::DriveAfterDone => None,
    }
}

fn task_fault_attribution(
    site: &FaultSite,
    lowered: &LoweringArtifact,
    attribution: &LoweringAttribution,
) -> Option<MachineAttribution> {
    let node = lowered.node_for_pc(site.function.0, site.pc as u32)?;
    let source = attribution.source_for_node(node)?;
    Some(MachineAttribution {
        function: source.function,
        node: source.node,
        span: source.span,
        weavy_function: Some(site.function),
        weavy_pc: Some(site.pc),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::Compiler;
    use crate::lowering::{LoweringCache, attribution_for};
    use crate::runtime::{
        DecodePrimitive, EventLog, FramedNode, MachineCause, Primitive, PrimitiveDescriptor,
        PrimitiveRegistry, TicketCompletionError,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{SyncSender, sync_channel};
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;
    use weavy::exec::{DriveTable, TaskFault};
    use weavy::task::{ArrayOpStatus, Op};
    use weavy::{Executable, ValueShapeRef};

    const ENUM_SOURCE: &str = r#"
enum Outcome {
    Ok(Bool),
    Err(String),
}

#[test]
fn fault_site() -> Stream<Check> {
    yield expect_eq(Outcome::Ok(true) == Outcome::Ok(true), true);
}
"#;

    const OUT_OF_BOUNDS_SOURCE: &str = r#"
#[test]
fn out_of_bounds() -> Stream<Check> {
    let values = [10, 20];
    yield expect_eq(values[7], 0);
}
"#;

    const MISSING_KEY_SOURCE: &str = r#"
#[test]
fn missing_key() -> Stream<Check> {
    let values: Map<String, Int> = %{};
    yield expect_eq(values.get("missing"), 0);
}
"#;

    const DUPLICATE_KEY_SOURCE: &str = r#"
#[test]
fn duplicate_key() -> Stream<Check> {
    let values = %{"present" => 1} + ("present", 2);
    yield expect_eq(values.len(), 0);
}
"#;

    const SCHEDULER_DECODE_SOURCE: &str = r#"
struct Row {
    name: String,
}

#[test]
fn scheduler_decode() -> Stream<Check> {
    let src = "{\"name\":\"mio\"}";
    let row: Row = json_decode(src);
    yield expect_eq(row.name, "mio");
}
"#;

    struct CountingDecode {
        descriptor: PrimitiveDescriptor,
        begins: Arc<AtomicUsize>,
    }

    impl Primitive for CountingDecode {
        fn descriptor(&self) -> &PrimitiveDescriptor {
            &self.descriptor
        }

        fn begin(&self, request: ValueId, ctx: EffectCtx) -> EffectTicket {
            self.begins.fetch_add(1, Ordering::AcqRel);
            DecodePrimitive::default().begin(request, ctx)
        }
    }

    struct DelayedDecode {
        descriptor: PrimitiveDescriptor,
        begins: Arc<AtomicUsize>,
        cancellations: Arc<AtomicUsize>,
        gate: Arc<(Mutex<bool>, Condvar)>,
        completed: SyncSender<(
            Result<(), TicketCompletionError>,
            super::super::PrimitivePublication,
        )>,
    }

    impl Primitive for DelayedDecode {
        fn descriptor(&self) -> &PrimitiveDescriptor {
            &self.descriptor
        }

        fn begin(&self, request: ValueId, ctx: EffectCtx) -> EffectTicket {
            self.begins.fetch_add(1, Ordering::AcqRel);
            let gate = self.gate.clone();
            let cancel_gate = self.gate.clone();
            let cancellations = self.cancellations.clone();
            let completed = self.completed.clone();
            let (ticket, completer) = ctx.ticket(move || {
                cancellations.fetch_add(1, Ordering::AcqRel);
                let (released, wake) = &*cancel_gate;
                *released.lock().expect("decode gate mutex poisoned") = true;
                wake.notify_all();
            });
            std::thread::spawn(move || {
                let (released, wake) = &*gate;
                let mut released = released.lock().expect("decode gate mutex poisoned");
                while !*released {
                    released = wake.wait(released).expect("decode gate mutex poisoned");
                }
                drop(released);
                let inner = DecodePrimitive::default().begin(request, ctx);
                let _subscription = inner.join(move |publication| {
                    let result = completer.complete(publication.clone());
                    completed
                        .send((result, publication))
                        .expect("scheduler test receives delayed completion");
                });
            });
            ticket
        }
    }

    fn decode_registry(primitive: Arc<dyn Primitive>) -> PrimitiveDispatcher {
        let mut registry = PrimitiveRegistry::default();
        registry
            .register(primitive)
            .expect("the scheduler test registers decode once");
        PrimitiveDispatcher::new(Arc::new(registry))
    }

    fn primitive_island(partitioned: &crate::vir::PartitionedTest) -> &Island {
        partitioned
            .values
            .iter()
            .map(|value| &value.island)
            .chain(partitioned.wire_islands.iter().map(|value| &value.island))
            .chain(partitioned.islands.iter())
            .chain(partitioned.generator.iter())
            .find(|island| {
                island
                    .nodes
                    .iter()
                    .chain(island.callees.iter().flat_map(|callee| &callee.nodes))
                    .any(|node| matches!(node.op, crate::vir::Op::InvokePrimitive { .. }))
            })
            .expect("scheduler decode has a registered-primitive island")
    }

    fn scheduler_decode_root(
        runtime: &mut Runtime<EventLog>,
        chaos: ChaosPolicy,
    ) -> Result<Evaluation, Box<MachineError>> {
        let module = Compiler::new()
            .compile(SCHEDULER_DECODE_SOURCE)
            .expect("scheduler decode source compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        let island = primitive_island(&partitioned);
        let attribution = attribution_for(island);
        let location = Location::for_test_island(&partitioned.name, island.id.0);
        let mut cache = LoweringCache::default();
        runtime.evaluate(
            island.id,
            &location,
            cache
                .get_or_lower_owned(island)
                .expect("scheduler decode source lowers"),
            &attribution,
            IslandInputs {
                arguments: Vec::new(),
                wires: Vec::new(),
            },
            chaos,
        )
    }

    fn submit_scheduler_decode(
        runtime: &mut Runtime<EventLog>,
    ) -> Result<DemandKey, Box<MachineError>> {
        let module = Compiler::new()
            .compile(SCHEDULER_DECODE_SOURCE)
            .expect("scheduler decode source compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        let island = primitive_island(&partitioned);
        let mut cache = LoweringCache::default();
        match runtime.submit_value(ValueRootRequest {
            island: island.id,
            location: Location::for_test_island(&partitioned.name, island.id.0),
            lowered: cache
                .get_or_lower_owned(island)
                .expect("scheduler decode source lowers"),
            attribution: Rc::new(attribution_for(island)),
            inputs: IslandInputs {
                arguments: Vec::new(),
                wires: Vec::new(),
            },
            chaos: ChaosPolicy::default(),
            realized_as: None,
        })? {
            RootSubmission::Pending(demand) => Ok(demand),
            RootSubmission::Ready(_) => panic!("fresh scheduler decode is pending"),
        }
    }

    #[test]
    fn kill_during_primitive_park_replays_and_joins_one_ticket() {
        let begins = Arc::new(AtomicUsize::new(0));
        let primitive = DecodePrimitive::default();
        let mut runtime = Runtime::new(EventLog::default());
        runtime.primitive_dispatcher = decode_registry(Arc::new(CountingDecode {
            descriptor: primitive.descriptor().clone(),
            begins: begins.clone(),
        }));

        let evaluation = scheduler_decode_root(
            &mut runtime,
            ChaosPolicy {
                kill_first_parked_primitive: true,
                ..ChaosPolicy::default()
            },
        )
        .expect("replayed decode completes");

        assert!(evaluation.passed, "the replayed frame publishes its check");
        assert_eq!(
            begins.load(Ordering::Acquire),
            1,
            "replay joins the demand-owned ticket instead of beginning again"
        );
        assert_eq!(runtime.counters.task_discards, 1);
        assert_eq!(runtime.counters.primitive_invocations, 1);
        assert_eq!(runtime.counters.effect_cancellations, 0);
        assert_eq!(runtime.primitive_dispatcher.in_flight(), 0);
        assert!(runtime.primitive_pending.is_empty());
    }

    #[test]
    fn last_waiter_abandonment_cancels_without_publication_and_ignores_late_delivery() {
        let begins = Arc::new(AtomicUsize::new(0));
        let cancellations = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let (completed_tx, completed_rx) = sync_channel(1);
        let primitive = DecodePrimitive::default();
        let mut runtime = Runtime::new(EventLog::default());
        runtime.primitive_dispatcher = decode_registry(Arc::new(DelayedDecode {
            descriptor: primitive.descriptor().clone(),
            begins: begins.clone(),
            cancellations: cancellations.clone(),
            gate,
            completed: completed_tx,
        }));
        let root = submit_scheduler_decode(&mut runtime).expect("decode root submits");

        let mut ctx = runtime.runnable.pop().expect("fresh decode is runnable");
        let request = match runtime
            .drive_context(&mut ctx)
            .expect("decode drives to its verified primitive yield")
        {
            DriveOutcome::YieldedPrimitive(request) => request,
            DriveOutcome::Parked(input) => panic!("decode parked on wire {input}"),
            DriveOutcome::Completed(_) => panic!("decode completed before its primitive"),
        };
        runtime
            .begin_primitive(ctx, request)
            .expect("decode frame parks on the delayed ticket");
        assert_eq!(runtime.primitive_pending.len(), 1);
        let primitive_demand = *runtime
            .primitive_pending
            .keys()
            .next()
            .expect("delayed primitive demand is pending");

        assert!(
            runtime
                .abandon_demand(root)
                .expect("abandoning a live root is valid")
        );
        let (completion, publication) = completed_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("cancelled worker attempts its late completion");
        assert_eq!(completion, Err(TicketCompletionError::Cancelled));
        assert_eq!(begins.load(Ordering::Acquire), 1);
        assert_eq!(cancellations.load(Ordering::Acquire), 1);
        assert_eq!(runtime.counters.effect_cancellations, 1);
        assert_eq!(runtime.primitive_dispatcher.in_flight(), 0);
        assert!(runtime.primitive_pending.is_empty());
        assert!(!runtime.demands.contains_key(&root));
        assert!(runtime.memo.is_empty(), "cancellation never memoizes");

        runtime
            .apply_completion(DeliveredCompletion::Primitive {
                demand: primitive_demand,
                publication,
            })
            .expect("a transport-raced late delivery is discarded");
        assert_eq!(runtime.counters.stale_completions_ignored, 1);
        assert!(runtime.memo.is_empty(), "late delivery cannot publish");
        assert!(runtime.root_results.is_empty());
    }

    #[derive(Clone, Copy)]
    enum ExpectedLanguageFailure {
        IndexOutOfBounds,
        MissingKey,
        DuplicateKey,
    }

    fn with_lowered(source: &str, inspect: impl FnOnce(&LoweringArtifact, &LoweringAttribution)) {
        let module = Compiler::new().compile(source).expect("source compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        let island = &partitioned.islands[0];
        let attribution = attribution_for(island);
        let mut cache = LoweringCache::default();
        let artifact = cache
            .get_or_lower(island)
            .expect("source lowers through verified executable");
        inspect(artifact, &attribution);
    }

    fn array_machine_result_artifact(
        artifact: &LoweringArtifact,
        status: ArrayOpStatus,
    ) -> LoweringArtifact {
        let mut program = artifact.program().clone();
        let contract = artifact.contract().clone();
        let code = &artifact.program().fns[0].code;
        let (construct_at, result_region, fields) = code
            .iter()
            .enumerate()
            .find_map(|(pc, op)| match op {
                Op::EnumConstruct {
                    dst,
                    variant: 2,
                    fields,
                } => Some((pc, *dst, fields.clone())),
                _ => None,
            })
            .expect("array lowering emits an ArrayMachine reconstruction");
        let site = match code.get(construct_at.checked_sub(2).expect("site constant precedes")) {
            Some(Op::ConstI64 { value, .. }) => *value,
            op => panic!("array machine site uses a static scalar constant: {op:?}"),
        };
        let field_offset = |field: usize| {
            let region = fields
                .get(field)
                .expect("array machine field exists")
                .source;
            contract.functions[0].frame.regions[region.0 as usize].offset
        };
        let result = contract.functions[0].result;
        let result_region_contract = &contract.functions[0].frame.regions[result.0 as usize];
        let result_size = u32::try_from(
            result_region_contract
                .shape
                .checked_byte_len()
                .expect("declared outcome size fits"),
        )
        .expect("declared outcome size is a bytecode size");
        program.fns[0].code = vec![
            Op::ConstI64 {
                dst: field_offset(0),
                value: site,
            },
            Op::ConstI64 {
                dst: field_offset(1),
                value: status as i64,
            },
            Op::EnumConstruct {
                dst: result_region,
                variant: 2,
                fields,
            },
            Op::Ret {
                src: result_region_contract.offset,
                size: result_size,
            },
        ];
        let verified = program
            .verify(contract)
            .expect("the declared ArrayMachine result remains verifier-admitted");
        artifact.with_test_verified_executable(Executable::new(verified))
    }

    #[test]
    fn poisoned_fault_site_maps_through_cached_pcs_and_fresh_spans() {
        with_lowered(ENUM_SOURCE, |artifact, attribution| {
            let pc = artifact.program().fns[0]
                .code
                .iter()
                .position(|op| matches!(op, Op::EnumIsVariant { .. }))
                .expect("enum equality emits checked selector validation");
            let site = FaultSite {
                function: FnId(0),
                pc,
                op: Box::new(artifact.program().fns[0].code[pc].clone()),
                call: None,
            };
            let fault = TaskFault::PoisonedResult {
                original: Box::new(TaskFault::InvalidEnumSelector {
                    site,
                    value_shape: ValueShapeRef(0),
                    expected: vec![0, 1],
                    actual: 9,
                }),
            };
            let site = task_fault_site(&fault)
                .expect("nested poison retains the fault site")
                .clone();
            let mapped = task_fault_attribution(&site, artifact, attribution)
                .expect("fault site maps through lowering pc ownership");
            let error = MachineError::task(
                MachineOperation::Drive,
                fault,
                Some(mapped.clone()),
                artifact.demand_key,
            );
            assert!(matches!(
                error.cause,
                MachineCause::Task(fault) if matches!(*fault, TaskFault::PoisonedResult { .. })
            ));

            let shifted = format!("\n\n{ENUM_SOURCE}");
            let shifted_module = Compiler::new()
                .compile(&shifted)
                .expect("shifted source compiles");
            let shifted_partitioned = shifted_module.partition_test(&shifted_module.tests[0]);
            let shifted_attribution = attribution_for(&shifted_partitioned.islands[0]);
            let shifted_mapped = task_fault_attribution(&site, artifact, &shifted_attribution)
                .expect("same cached pc uses fresh source attribution");
            assert_ne!(mapped.span, shifted_mapped.span);
        });
    }

    #[test]
    fn machine_fault_marks_task_and_demand_machine_failed_without_a_memo() {
        with_lowered(ENUM_SOURCE, |artifact, _| {
            let mut runtime = Runtime::new(EventLog::default());
            runtime.demands.insert(
                artifact.demand_key,
                DemandRecord {
                    key: artifact.demand_key,
                    state: DemandState::Queued,
                    result: None,
                },
            );
            let task = runtime.spawn_task(artifact.demand_key);
            let error = MachineError::runtime(
                MachineOperation::Drive,
                RuntimeFault::PureIslandYielded,
                None,
                Some(artifact.demand_key),
            );
            let returned =
                runtime.terminate_machine_fault(task, artifact.demand_key, error.clone());
            assert_eq!(returned, error);
            assert_eq!(runtime.tasks[&task].state, TaskState::Failed);
            assert_eq!(
                runtime.demands[&artifact.demand_key].state,
                DemandState::MachineFailed
            );
            assert!(runtime.memo.is_empty());
            assert!(runtime.sink.events().iter().any(|event| matches!(
                event.kind,
                EventKind::MachineFailed {
                    task: failed_task,
                    key,
                    operation: MachineOperation::Drive,
                } if failed_task == task && key == artifact.demand_key
            )));
        });
    }

    #[test]
    fn no_site_task_fault_keeps_its_demand_without_source_attribution() {
        with_lowered(ENUM_SOURCE, |artifact, attribution| {
            let runtime = Runtime::new(EventLog::default());
            let invocation = DemandExecution::new(artifact, Vec::new());
            let error = runtime.task_fault(
                MachineOperation::Drive,
                TaskFault::DriveTableLength {
                    table: DriveTable::Ready,
                    expected: 1,
                    actual: 0,
                },
                &invocation,
                attribution,
                None,
            );
            assert_eq!(error.attribution, None);
            assert_eq!(error.demand_chain, [artifact.demand_key]);
            assert!(matches!(
                error.cause,
                MachineCause::Task(fault) if matches!(*fault, TaskFault::DriveTableLength { .. })
            ));
        });
    }

    #[test]
    fn result_shape_fault_alone_uses_the_output_attribution() {
        with_lowered(ENUM_SOURCE, |artifact, attribution| {
            let runtime = Runtime::new(EventLog::default());
            let invocation = DemandExecution::new(artifact, Vec::new());
            let output = runtime
                .output_attribution(artifact, attribution)
                .expect("root return has output source attribution");
            let fault = TaskFault::InvalidResultShape {
                entry: FnId(0),
                region: weavy::RegionId(0),
                size: 8,
            };
            let fallback = result_shape_attribution(&fault, Some(output.clone()));
            let error = runtime.task_fault(
                MachineOperation::Result,
                fault,
                &invocation,
                attribution,
                fallback,
            );
            assert_eq!(error.attribution, Some(output));
        });
    }

    #[test]
    // r[verify machine.error.failure-source-site-identity]
    fn language_failure_memo_hit_rebuilds_current_attribution_without_reexecution() {
        for (source, expected) in [
            (
                OUT_OF_BOUNDS_SOURCE,
                ExpectedLanguageFailure::IndexOutOfBounds,
            ),
            (MISSING_KEY_SOURCE, ExpectedLanguageFailure::MissingKey),
            (DUPLICATE_KEY_SOURCE, ExpectedLanguageFailure::DuplicateKey),
        ] {
            assert_language_failure_memo_hit(source, expected);
        }
    }

    fn assert_language_failure_memo_hit(source: &str, expected: ExpectedLanguageFailure) {
        let module = Compiler::new().compile(source).expect("source compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        let island = &partitioned.islands[0];
        let first_attribution = attribution_for(island);
        let location = Location::for_test_island(&partitioned.name, island.id.0);
        let mut cache = LoweringCache::default();
        let mut runtime = Runtime::new(EventLog::default());

        let (first, demand_key) = {
            let artifact = cache
                .get_or_lower_owned(island)
                .expect("first compilation lowers through the verified executable");
            let demand_key = artifact.demand_key;
            let evaluation = runtime
                .evaluate(
                    island.id,
                    &location,
                    artifact,
                    &first_attribution,
                    IslandInputs {
                        arguments: Vec::new(),
                        wires: Vec::new(),
                    },
                    ChaosPolicy::default(),
                )
                .expect("first demand becomes a typed language failure");
            (evaluation, demand_key)
        };
        let first_failure = first.failure.clone().expect("outcome is recorded");
        let first_context = first
            .failure_context
            .clone()
            .expect("first report resolves the indexing source");
        let first_site = expected_failure_site(&first_failure, expected);
        assert_eq!(
            first_context.span,
            first_attribution
                .source_for_trace(first_site)
                .expect("failure site is a source trace")
                .span
        );

        let shifted_source = format!("\n\n{source}");
        let shifted_module = Compiler::new()
            .compile(&shifted_source)
            .expect("shifted source compiles");
        let shifted_partitioned = shifted_module.partition_test(&shifted_module.tests[0]);
        let shifted_island = &shifted_partitioned.islands[0];
        let shifted_attribution = attribution_for(shifted_island);
        assert_eq!(shifted_island.id, island.id);

        let second = {
            let artifact = cache
                .get_or_lower_owned(shifted_island)
                .expect("span-only recompilation reuses the verified artifact");
            runtime
                .evaluate(
                    shifted_island.id,
                    &location,
                    artifact,
                    &shifted_attribution,
                    IslandInputs {
                        arguments: Vec::new(),
                        wires: Vec::new(),
                    },
                    ChaosPolicy::default(),
                )
                .expect("second demand is an exact memo hit")
        };
        let second_context = second
            .failure_context
            .as_ref()
            .expect("memo report resolves its current source");

        assert_eq!(second.memo, MemoVerdict::Exact);
        assert_eq!(first.identity, second.identity);
        assert_eq!(first.failure, second.failure);
        assert_eq!(
            second_context.span,
            shifted_attribution
                .source_for_trace(first_site)
                .expect("stable site resolves through the shifted attribution")
                .span
        );
        assert_ne!(first_context.span, second_context.span);
        assert_eq!(second_context.demand_chain, [demand_key]);
        assert_eq!(runtime.counters().task_spawns, 1);
        assert_eq!(runtime.counters().memo_misses, 1);
        assert_eq!(runtime.counters().memo_hits_exact, 1);
        assert_eq!(
            runtime
                .sink()
                .events()
                .iter()
                .filter(|event| matches!(event.kind, EventKind::TaskSpawned { .. }))
                .count(),
            1
        );
        assert_eq!(
            runtime
                .sink()
                .events()
                .iter()
                .filter(|event| matches!(event.kind, EventKind::LanguageFailed { .. }))
                .count(),
            1
        );
    }

    fn expected_failure_site(failure: &FailureValue, expected: ExpectedLanguageFailure) -> u32 {
        match (expected, failure) {
            (
                ExpectedLanguageFailure::IndexOutOfBounds,
                FailureValue::IndexOutOfBounds { site, .. },
            )
            | (ExpectedLanguageFailure::MissingKey, FailureValue::MissingKey { site, .. })
            | (ExpectedLanguageFailure::DuplicateKey, FailureValue::DuplicateKey { site, .. }) => {
                *site
            }
            _ => panic!("language failure kind does not match the production source: {failure:?}"),
        }
    }

    #[test]
    fn verified_array_machine_result_is_never_a_language_failure_or_memo() {
        with_lowered(OUT_OF_BOUNDS_SOURCE, |artifact, attribution| {
            let artifact = array_machine_result_artifact(artifact, ArrayOpStatus::InvalidHandle);
            let mut runtime = Runtime::new(EventLog::default());
            let location = Location::for_test_island("out_of_bounds", 0);
            let error = runtime
                .evaluate(
                    IslandId(0),
                    &location,
                    Rc::new(artifact),
                    attribution,
                    IslandInputs {
                        arguments: Vec::new(),
                        wires: Vec::new(),
                    },
                    ChaosPolicy::default(),
                )
                .expect_err("non-OutOfRange status is a machine error");

            assert!(matches!(
                error.cause,
                MachineCause::Runtime(RuntimeFault::ArrayMachineStatus {
                    status: ArrayOpStatus::InvalidHandle,
                    ..
                })
            ));
            assert!(runtime.tasks().all(|task| task.state == TaskState::Failed));
            assert!(
                runtime
                    .demands()
                    .all(|demand| demand.state == DemandState::MachineFailed)
            );
            assert!(runtime.memo.is_empty());
            assert!(
                runtime
                    .store()
                    .inspect()
                    .all(|entry| entry.failure().is_none())
            );
            assert!(
                !runtime
                    .sink()
                    .events()
                    .iter()
                    .any(|event| matches!(event.kind, EventKind::LanguageFailed { .. }))
            );
            assert!(runtime.sink().events().iter().any(|event| matches!(
                event.kind,
                EventKind::MachineFailed {
                    operation: MachineOperation::Result,
                    ..
                }
            )));
        });
    }

    const PASSING_CHECK_SOURCE: &str = r#"
#[test]
fn passing() -> Stream<Check> {
    yield expect_eq(1 + 1, 2);
}
"#;

    #[test]
    // r[verify machine.identity.framed-encoding]
    fn realized_check_identity_is_the_framed_leaf_identity() {
        let module = Compiler::new()
            .compile(PASSING_CHECK_SOURCE)
            .expect("source compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        let island = &partitioned.islands[0];
        let attribution = attribution_for(island);
        let location = Location::for_test_island(&partitioned.name, island.id.0);
        let mut cache = LoweringCache::default();
        let mut runtime = Runtime::new(EventLog::default());
        let artifact = cache
            .get_or_lower_owned(island)
            .expect("source lowers through the verified executable");
        let evaluation = runtime
            .evaluate(
                island.id,
                &location,
                artifact,
                &attribution,
                IslandInputs {
                    arguments: Vec::new(),
                    wires: Vec::new(),
                },
                ChaosPolicy::default(),
            )
            .expect("passing check evaluates to a realized value");

        assert!(evaluation.passed, "1 + 1 == 2 is a passing check");
        assert!(evaluation.failure.is_none());

        // The production realized-scalar path routes through the closed writer:
        // its identity is exactly the framed scalar-leaf identity, computed here
        // independently of the store.
        let expected = FramedNode::leaf(Type::Check.schema_ref(), vec![u8::from(true)]).identity();
        assert_eq!(
            evaluation.identity, expected,
            "realized check identity is the framed leaf identity from the closed writer"
        );

        // And the store carries that same entry-carried identity as a load.
        let entry = runtime
            .store()
            .entry(evaluation.handle)
            .expect("realized value is resident");
        assert_eq!(entry.identity, expected);
    }
}
