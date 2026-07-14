//! Production-path ratchet runner: source -> generated AST -> VIR -> Weavy.

use std::collections::{BTreeMap, BTreeSet};

use crate::compiler::{Compiler, CompilerConfig};
use crate::diagnostic::Diagnostics;
use crate::lowering::{
    LoweringArtifact, LoweringAttribution, LoweringCache, LoweringCacheCounters, LoweringError,
    attribution_for,
};
use crate::runtime::{
    ChaosPolicy, Counters, DemandState, Evaluation, Event, EventKind, EventLog, FailureContext,
    FailureValue, FramedNode, GeneratorOutcome, IslandInputs, Location, MachineError, MemoVerdict,
    PersistentRuntimeJournal, PersistentRuntimeJournalError, PersistentRuntimeJournalLoadReport,
    PersistentRuntimeState, RealizedWireDemand, Runtime, SchemaId, SnapshotCapture,
    SnapshotOutcome, TaskState, ValueId, WireDemand,
};
use crate::vir::{
    DescribedWire, FunctionId, Island, IslandId, Module, Op, PartitionedRecipe, PartitionedValue,
    TraceCheck, ValueIslandId, WireArg, WireSelector,
};

/// The user functions named by a test's described-wire trace checks. A bundled
/// invocation observation is emitted only for these — the observation is bounded
/// to the selected observers, never every call.
fn selected_wire_functions(partitioned: &crate::vir::PartitionedTest) -> BTreeSet<FunctionId> {
    partitioned
        .sites
        .iter()
        .filter_map(|site| match &site.recipe {
            PartitionedRecipe::Trace(
                TraceCheck::Demanded { wire }
                | TraceCheck::NeverDemanded { wire }
                | TraceCheck::DemandedOnce { wire },
            ) => Some(wire.function),
            _ => None,
        })
        .collect()
}

/// The resolved canonical preimage of a deferred trace check's binding-level
/// described wire: the structural fingerprint of the let-bound invocation node
/// in the authored test graph. `None` for every other trace shape. Resolution
/// reads the authored graph only — nothing is demanded, lowered, or interned.
fn binding_preimage(
    module: &Module,
    test_function: FunctionId,
    trace: &TraceCheck,
) -> Option<String> {
    let (TraceCheck::Demanded { wire }
    | TraceCheck::NeverDemanded { wire }
    | TraceCheck::DemandedOnce { wire }) = trace
    else {
        return None;
    };
    let WireSelector::Binding(node) = wire.selector else {
        return None;
    };
    Some(module.invocation_preimage(test_function, node))
}

/// Record one realized demand per distinct executed invocation preimage that a
/// described-wire observer selects, without adding a scheduler edge. The cost
/// model may fuse a mapped element or bundle a single-consumer pure call into a
/// direct `WeavyOp::Call`; this reads the executed island's demand-independent
/// structure and retains the exact canonical preimage — the invocation's
/// structural fingerprint in the authored graph, plus framed argument
/// identities when every argument is a closed literal — so `demanded_once`
/// distinguishes `costly(1)` from `costly(2)` and a binding selector matches a
/// composite-argument invocation. Only unconditional calls (never a
/// control-region member) are observed, so an untaken arm's invocation is never
/// fabricated; equal preimages share one entry, including with a realization
/// the memo path already recorded.
fn observe_bundled_invocations(
    runtime: &mut Runtime<EventLog>,
    module: &Module,
    test_function: FunctionId,
    island: &Island,
    selected: &BTreeSet<FunctionId>,
    seen: &mut BTreeSet<String>,
) {
    let mut controlled = BTreeSet::new();
    for node in &island.nodes {
        match &node.op {
            Op::If {
                consequent,
                alternative,
            } => {
                controlled.extend(consequent.nodes.iter().copied());
                controlled.extend(alternative.nodes.iter().copied());
            }
            Op::Match { arms } => {
                for arm in arms {
                    controlled.extend(arm.nodes.iter().copied());
                }
            }
            Op::OrderedMatch { arms, fallback } => {
                for arm in arms {
                    controlled.extend(arm.condition.nodes.iter().copied());
                    controlled.extend(arm.body.nodes.iter().copied());
                }
                controlled.extend(fallback.nodes.iter().copied());
            }
            _ => {}
        }
    }
    let authored = &module.functions[test_function.0 as usize];
    for node in &island.nodes {
        let Op::Call(function) = node.op else {
            continue;
        };
        if controlled.contains(&node.id) || !selected.contains(&function) {
            continue;
        }
        // The preimage is read from the authored graph, never the island cut,
        // so shared-value and wire substitutions cannot distort it.
        let preimage = module.invocation_preimage(test_function, node.id);
        if seen.contains(&preimage)
            || runtime
                .realized_wire_demands()
                .iter()
                .any(|demand| demand.preimage == preimage)
        {
            continue;
        }
        let mut literals = Vec::with_capacity(node.inputs.len());
        let mut literal = true;
        for input in &node.inputs {
            match authored.nodes[input.0 as usize].op {
                Op::Int(value) => literals.push(WireArg::Int(value)),
                Op::Bool(value) => literals.push(WireArg::Bool(value)),
                _ => {
                    literal = false;
                    break;
                }
            }
        }
        let arguments = literal.then(|| literals.iter().map(wire_arg_identity).collect::<Vec<_>>());
        seen.insert(preimage.clone());
        runtime.record_wire_demand(function, arguments, preimage);
    }
}

/// Owned backing for one island's flat wire-demand tree: everything a
/// [`WireDemand`] borrows, kept alive alongside the island evaluation it feeds.
/// A leaf argument island has no realized value inputs and no nested wires, so
/// each wire's `arguments`/`wires` are empty; deeper argument graphs are built
/// by the same lazy seam once their rungs are reached.
struct FlatWires<'a> {
    islands: Vec<IslandId>,
    locations: Vec<Location>,
    attributions: Vec<LoweringAttribution>,
    functions: Vec<FunctionId>,
    demand_args: Vec<Option<Vec<ValueId>>>,
    preimages: Vec<String>,
    artifacts: Vec<&'a LoweringArtifact>,
}

impl<'a> FlatWires<'a> {
    fn demands(&'a self) -> Vec<WireDemand<'a>> {
        self.islands
            .iter()
            .enumerate()
            .map(|(index, &island)| WireDemand {
                island,
                location: &self.locations[index],
                lowered: self.artifacts[index],
                attribution: &self.attributions[index],
                arguments: &[],
                wires: &[],
                function: self.functions[index],
                demand_arguments: self.demand_args[index].as_deref(),
                preimage: &self.preimages[index],
            })
            .collect()
    }
}

/// Build the flat wire-demand backing for an island's `wire_inputs`. Every named
/// argument island was lowered up front, so this only looks it up and pins its
/// cost-model location — one location per representative wire island, so
/// structurally equal awaits share one memo cell and realize once.
fn flat_wires<'a>(
    cache: &'a LoweringCache,
    module: &Module,
    wire_lookup: &BTreeMap<ValueIslandId, &PartitionedValue>,
    wire_inputs: &[ValueIslandId],
    test_name: &str,
    source_revision: Option<&str>,
) -> FlatWires<'a> {
    let mut backing = FlatWires {
        islands: Vec::with_capacity(wire_inputs.len()),
        locations: Vec::with_capacity(wire_inputs.len()),
        attributions: Vec::with_capacity(wire_inputs.len()),
        functions: Vec::with_capacity(wire_inputs.len()),
        demand_args: Vec::with_capacity(wire_inputs.len()),
        preimages: Vec::with_capacity(wire_inputs.len()),
        artifacts: Vec::with_capacity(wire_inputs.len()),
    };
    for value in wire_inputs {
        let wire = wire_lookup
            .get(value)
            .expect("a wire input names a partitioned argument island");
        assert!(
            wire.island.value_inputs.is_empty() && wire.island.wire_inputs.is_empty(),
            "argument island with nested inputs awaits the general wire seam",
        );
        let artifact = cache
            .lowered(&wire.island)
            .expect("argument island was lowered before execution");
        backing.islands.push(wire.island.id);
        backing.locations.push(scoped_location(
            Location::for_test_value(test_name, &format!("wire-{}", value.stable_segment())),
            source_revision,
        ));
        backing.attributions.push(attribution_for(&wire.island));
        let provenance = wire.wire.as_ref();
        backing.functions.push(
            provenance
                .map(|provenance| provenance.function)
                .unwrap_or(wire.island.function),
        );
        backing.demand_args.push(provenance.and_then(|provenance| {
            provenance
                .arguments
                .as_ref()
                .map(|arguments| arguments.iter().map(wire_arg_identity).collect())
        }));
        backing
            .preimages
            .push(module.invocation_preimage(wire.id.function, wire.id.node));
        backing.artifacts.push(artifact);
    }
    backing
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    Diagnostics(Diagnostics),
    Machine(Box<MachineError>),
    /// A generator published a provenance selector that is not a representable
    /// static site index.
    MalformedSiteKey {
        test: String,
        site: u64,
    },
    /// A generator published a selector that does not name a static yield site
    /// of the test.
    UnknownSiteKey {
        test: String,
        site: u64,
    },
    /// A generator published the same site more than once. The zero-dynamic-key
    /// base case admits at most one occurrence per site; repeated multiplicity
    /// requires the dynamic-key tail (055-059).
    DuplicateSiteKey {
        test: String,
        site: u32,
    },
    /// A generator's scrutinee control language-failed before deciding a branch.
    /// This stays on the typed language plane, carrying the failure value and its
    /// source context; it is never reclassified as a machine invariant.
    GeneratorLanguageFailure {
        test: String,
        /// Boxed to keep `RunError` small: the failure value dwarfs every other
        /// variant, and boxing it keeps the common `Result<_, RunError>` cheap.
        failure: Box<FailureValue>,
        context: Option<FailureContext>,
    },
    PersistentRuntime(Box<PersistentRuntimeJournalError>),
}

/// The stable provenance key of a published check: the yield site's selector
/// plus the canonical runtime identities of its dynamic iteration keys. The
/// dynamic tail is empty in the zero-dynamic-key base case and is the extension
/// point for keyed dynamic iteration (055-059); each future key is a framed
/// [`ValueId`], never a handle integer or ABI word. Completed check elements are
/// keyed by this, never by publication arrival order.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProvenanceKey {
    pub site: u32,
    pub dynamic_keys: Vec<ValueId>,
}

impl ProvenanceKey {
    #[must_use]
    fn site(site: u32) -> Self {
        Self {
            site,
            dynamic_keys: Vec::new(),
        }
    }
}

impl From<Diagnostics> for RunError {
    fn from(diagnostics: Diagnostics) -> Self {
        Self::Diagnostics(diagnostics)
    }
}

impl From<LoweringError> for RunError {
    fn from(error: LoweringError) -> Self {
        match error {
            LoweringError::Diagnostics(diagnostics) => Self::Diagnostics(diagnostics),
            LoweringError::Machine(error) => Self::Machine(error),
        }
    }
}

impl From<MachineError> for RunError {
    fn from(error: MachineError) -> Self {
        Self::Machine(Box::new(error))
    }
}

impl From<Box<MachineError>> for RunError {
    fn from(error: Box<MachineError>) -> Self {
        Self::Machine(error)
    }
}

impl From<PersistentRuntimeJournalError> for RunError {
    fn from(error: PersistentRuntimeJournalError) -> Self {
        Self::PersistentRuntime(Box::new(error))
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct CheckRun {
    /// The stable yield-provenance key of this check. Plain/chaos agreement and
    /// multiplicity are decided by this key, not by publication arrival order.
    pub provenance: ProvenanceKey,
    /// The evaluated value identity of a value check. Absent for a trace check,
    /// which produces no store value.
    pub identity: Option<ValueId>,
    /// Ordinary demand arguments by semantic identity. Shared publications
    /// appear here exactly as they do in the demand preimage.
    pub arguments: Vec<ValueId>,
    pub passed: bool,
    pub failure: Option<FailureValue>,
    pub failure_context: Option<FailureContext>,
    /// Detail for a *failed* trace check. Deliberately absent when a trace check
    /// passes, so a passing trace check's report carries only its provenance and
    /// verdict and stays byte-identical across the plain and chaos lanes even
    /// though the observed counter differs between them.
    pub trace_failure: Option<TraceFailure>,
    /// The structural rendering captured by an `expect_snapshot` value check.
    /// Absent for every other check kind. Byte-identical across lanes.
    pub snapshot: Option<SnapshotCapture>,
}

/// Why a trace check went red: the descriptor and the value observed in the
/// frozen completed-run snapshot. Only present on a failing trace check.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct TraceFailure {
    pub check: TraceCheck,
    pub observed: u64,
}

/// The frozen completed-run snapshot a trace check is evaluated against. It is
/// captured once, after every selected value check completes, and the trace
/// checks read it without demanding any operand, issuing any scheduler request,
/// or interning anything — so a trace check never counts its own reporting work
/// against the very quantities it inspects.
#[derive(Clone, Debug)]
struct TraceSnapshot {
    scheduler_requests: u64,
    memo_entries: u64,
    memo_hits: u64,
    store_interns: u64,
    effect_spawns: u64,
    value_island_spawns: u64,
    successful_aggregate_freezes: u64,
    active_molten_selections: u64,
    forced_copy_selections: u64,
    framed_bytes: u64,
    peak_molten_bytes: u64,
    peak_molten_nodes: u64,
    /// Fetch effects actually performed during the run.
    fetches_performed: u64,
    /// The union of external read projections across every demand receipt.
    /// Reads go through the recording fixture-store accessors, so this set is
    /// complete by construction; `never_read` is absence in this set.
    reads: BTreeSet<String>,
    function_calls: BTreeMap<FunctionId, u64>,
    /// One entry per realized wire demand (a computation the memo path actually
    /// ran). Repeated identical `recipe + argument` demands memoize to a single
    /// realization, so a call-site selector observes at most one entry; distinct
    /// arguments contribute distinct entries. This is the frozen log the
    /// described-wire trace checks read; it retains only the callee identity,
    /// argument identities, and canonical preimage a trace descriptor can
    /// select on.
    wire_demands: Vec<RealizedWireDemand>,
}

/// The canonical identity of one described-wire scalar argument. Computed the
/// same way an evaluated scalar value interns, so a described literal selects
/// the exact realized argument identity without demanding anything.
fn wire_arg_identity(arg: &WireArg) -> ValueId {
    let (ty_name, bytes) = match arg {
        WireArg::Int(value) => ("Int", value.to_le_bytes().to_vec()),
        WireArg::Bool(value) => ("Bool", i64::from(*value).to_le_bytes().to_vec()),
        WireArg::FixtureTree(name) => {
            let mut bytes = b"fixture-tree\0".to_vec();
            bytes.extend(name.as_bytes());
            ("Tree", bytes)
        }
    };
    FramedNode::leaf(
        SchemaId::named(&format!("vix.semantic.v1:{ty_name}")),
        bytes,
    )
    .identity()
}

/// An at-most trace comparison: the observed counter and whether it stays
/// within the surface literal bound. Compared in i128 so an observed counter
/// cannot wrap past the contract.
fn at_most(observed: u64, bound: i64) -> (u64, bool) {
    (observed, i128::from(observed) <= i128::from(bound))
}

impl TraceSnapshot {
    /// Count the realized demands that match a described wire. A name-level
    /// selector matches every realization of the callee; a call-site selector
    /// matches only the exact described argument identities; a binding selector
    /// matches the invocation's canonical preimage, resolved from the authored
    /// graph when the check was deferred. The described literals are resolved
    /// to identities here — never demanded or counted.
    fn wire_matches(&self, wire: &DescribedWire, binding: Option<&str>) -> u64 {
        self.wire_demands
            .iter()
            .filter(|demand| match &wire.selector {
                WireSelector::Name => demand.function == wire.function,
                WireSelector::CallSite(arguments) => {
                    let described: Vec<ValueId> = arguments.iter().map(wire_arg_identity).collect();
                    demand.function == wire.function
                        && demand.arguments.as_deref() == Some(&described[..])
                }
                WireSelector::Binding(_) => {
                    binding.is_some_and(|preimage| demand.preimage == preimage)
                }
            })
            .count() as u64
    }

    /// Evaluate one trace check against the frozen snapshot. `binding` is the
    /// resolved canonical preimage of a binding-level described wire, absent
    /// for every other check shape.
    fn evaluate(
        &self,
        provenance: ProvenanceKey,
        check: TraceCheck,
        binding: Option<&str>,
    ) -> CheckRun {
        let (observed, passed) = match &check {
            TraceCheck::SchedulerRequestsAtMost { bound } => {
                at_most(self.scheduler_requests, *bound)
            }
            TraceCheck::MemoEntriesAtMost { bound } => at_most(self.memo_entries, *bound),
            TraceCheck::MemoHitsAtLeast { bound } => {
                let observed = self.memo_hits;
                (observed, i128::from(observed) >= i128::from(*bound))
            }
            TraceCheck::StoreInternsAtMost { bound } => at_most(self.store_interns, *bound),
            TraceCheck::ValueIslandSpawnsAtMost { bound } => {
                at_most(self.value_island_spawns, *bound)
            }
            TraceCheck::SuccessfulAggregateFreezesAtMost { bound } => {
                at_most(self.successful_aggregate_freezes, *bound)
            }
            TraceCheck::ActiveMoltenSelectionsAtMost { bound } => {
                at_most(self.active_molten_selections, *bound)
            }
            TraceCheck::ForcedCopySelectionsAtMost { bound } => {
                at_most(self.forced_copy_selections, *bound)
            }
            TraceCheck::FramedBytesAtMost { bound } => at_most(self.framed_bytes, *bound),
            TraceCheck::PeakMoltenBytesAtMost { bound } => at_most(self.peak_molten_bytes, *bound),
            TraceCheck::PeakMoltenNodesAtMost { bound } => at_most(self.peak_molten_nodes, *bound),
            TraceCheck::FunctionCallsExactly { function, times } => {
                let observed = self.function_calls.get(function).copied().unwrap_or(0);
                (observed, i128::from(observed) == i128::from(*times))
            }
            TraceCheck::Demanded { wire } => {
                let observed = self.wire_matches(wire, binding);
                (observed, observed >= 1)
            }
            TraceCheck::NeverDemanded { wire } => {
                let observed = self.wire_matches(wire, binding);
                (observed, observed == 0)
            }
            TraceCheck::DemandedOnce { wire } => {
                let observed = self.wire_matches(wire, binding);
                (observed, observed == 1)
            }
            TraceCheck::RanProcesses { count } => {
                let observed = self.effect_spawns;
                (observed, i128::from(observed) == i128::from(*count))
            }
            TraceCheck::Read { path } => {
                let observed = u64::from(self.reads.contains(path));
                (observed, observed == 1)
            }
            TraceCheck::NeverRead { path } => {
                let observed = u64::from(self.reads.contains(path));
                (observed, observed == 0)
            }
            TraceCheck::Fetched { times } => {
                let observed = self.fetches_performed;
                (observed, i128::from(observed) == i128::from(*times))
            }
        };
        CheckRun {
            provenance,
            identity: None,
            arguments: Vec::new(),
            passed,
            failure: None,
            failure_context: None,
            trace_failure: (!passed).then_some(TraceFailure { check, observed }),
            snapshot: None,
        }
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct SuiteRun {
    pub checks: Vec<CheckRun>,
    pub values: Vec<ValuePublicationRun>,
    pub counters: Counters,
    pub events: Vec<Event>,
    pub receipt_count: u64,
    pub all_demands_ready: bool,
    pub all_tasks_terminal: bool,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ValuePublicationRun {
    pub provenance: ValueIslandId,
    pub identity: ValueId,
    pub failure: Option<FailureValue>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct RatchetReport {
    pub warnings: Diagnostics,
    pub plain: SuiteRun,
    pub chaos: SuiteRun,
    pub lowering_cache: LoweringCacheCounters,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct RerunAuditReport {
    pub warnings: Diagnostics,
    pub first: SuiteRun,
    pub second: SuiteRun,
    pub nondeterministic: bool,
    pub lowering_cache: LoweringCacheCounters,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PersistenceAuditReport {
    pub warnings: Diagnostics,
    pub first: SuiteRun,
    pub second: SuiteRun,
    pub load: PersistentRuntimeJournalLoadReport,
    pub journal_bytes: u64,
    pub nondeterministic: bool,
    pub lowering_cache: LoweringCacheCounters,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct SourceRevisionAuditReport {
    pub first_warnings: Diagnostics,
    pub second_warnings: Diagnostics,
    pub first: SuiteRun,
    pub second: SuiteRun,
    pub load: PersistentRuntimeJournalLoadReport,
    pub journal_bytes: u64,
    pub nondeterministic: bool,
    pub lowering_cache: LoweringCacheCounters,
}

impl SuiteRun {
    /// The completed check family keyed by provenance. Publication arrival order
    /// is a live schedule artifact; agreement is decided over this key→outcome
    /// map, not over the append-order vector.
    #[must_use]
    pub fn check_family(&self) -> BTreeMap<ProvenanceKey, &CheckRun> {
        self.checks
            .iter()
            .map(|check| (check.provenance.clone(), check))
            .collect()
    }

    #[must_use]
    pub fn value_family(&self) -> BTreeMap<ValueIslandId, &ValuePublicationRun> {
        self.values
            .iter()
            .map(|value| (value.provenance, value))
            .collect()
    }
}

impl RatchetReport {
    #[must_use]
    pub fn agrees(&self) -> bool {
        self.plain.check_family() == self.chaos.check_family()
            && self.plain.value_family() == self.chaos.value_family()
    }

    #[must_use]
    pub fn passed(&self) -> bool {
        self.agrees()
            && self.plain.checks.iter().all(|check| check.passed)
            && self.chaos.checks.iter().all(|check| check.passed)
            && self.plain.all_demands_ready
            && self.chaos.all_demands_ready
            && self.plain.all_tasks_terminal
            && self.chaos.all_tasks_terminal
    }
}

/// A source that is parsed, checked, lowered, verified, and natively compiled,
/// ready to execute. Every island the two lanes will demand is already lowered —
/// and therefore JIT-compiled, since [`crate::lowering::LoweringArtifact`] holds
/// an eagerly-compiled [`weavy::exec::Executable`] — and cached, so
/// [`PreparedRun::execute`] performs no compilation and does only the asymptotic
/// evaluation a budget is meant to gate.
///
/// This is the readiness boundary the outer budget runner measures against:
/// preparation is a fixed, O(1) compiler/JIT cost that is not the tested
/// program's work, so it is completed *before* the wall clock starts.
pub struct PreparedRun {
    compilation: crate::compiler::Compilation,
    cache: LoweringCache,
}

/// Execution lifecycle boundaries exposed to the outer budget runner. Each
/// `Runtime` owns store, memo, demand, task, and event-log state; observing
/// these points distinguishes fixed lane scaffolding from growth retained while
/// a lane executes.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExecutionPhase {
    PlainRuntimeReady,
    PlainCompleted,
    ChaosRuntimeReady,
    ChaosCompleted,
}

/// The harness snapshot oracle: the expected structural renderings a run is
/// checked against, keyed by test identity + stable snapshot name. It is the
/// authority a snapshot `Check` delegates to — `evaluate_snapshot_site` sets the
/// check's verdict from `expected == rendered`, so a changed rendering (or a
/// missing golden) makes the check, and hence [`RatchetReport::passed`], false.
///
/// Goldens are supplied through this generic API. A future `vx test` disk loader
/// is an explicit adapter seam: it populates this registry from on-disk snapshot
/// artifacts (and an `--update` mode writes back the `rendered` captures a run
/// produces). This type deliberately defines no disk format.
#[derive(Clone, Debug, Default)]
pub struct SnapshotExpectations {
    entries: BTreeMap<String, BTreeMap<String, String>>,
}

impl SnapshotExpectations {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the expected rendering for `test`'s snapshot named `name`.
    #[must_use]
    pub fn with(mut self, test: &str, name: &str, rendered: &str) -> Self {
        self.entries
            .entry(test.to_owned())
            .or_default()
            .insert(name.to_owned(), rendered.to_owned());
        self
    }

    fn expected(&self, test: &str, name: &str) -> Option<&str> {
        self.entries.get(test)?.get(name).map(String::as_str)
    }
}

/// r[impl machine.scheduler.chaos-kill-oracle]
/// r[impl machine.scheduler.replay-is-semantics]
pub fn run_source(source: &str) -> Result<RatchetReport, RunError> {
    run_source_with_config(source, CompilerConfig::default())
}

/// Run a root source alongside named library modules (the `//! uses:` harness
/// directive of rungs 106–110): identical to [`run_source`] in every lane
/// aspect, with imports resolved against the presented module set.
pub fn run_source_with_modules(
    source: &str,
    modules: &[crate::modules::ModuleSource<'_>],
) -> Result<RatchetReport, RunError> {
    prepare_source_with_modules(source, modules)?.execute()
}

/// The readiness-boundary form of [`run_source_with_modules`].
pub fn prepare_source_with_modules(
    source: &str,
    modules: &[crate::modules::ModuleSource<'_>],
) -> Result<PreparedRun, RunError> {
    prepare_modules_with_cache(
        source,
        modules,
        CompilerConfig::default(),
        LoweringCache::default(),
    )
}

/// Run every declared test twice against a snapshot oracle. Snapshot checks
/// compare their rendering to `expectations`; every other check is unaffected.
pub fn run_source_with_snapshots(
    source: &str,
    expectations: &SnapshotExpectations,
) -> Result<RatchetReport, RunError> {
    prepare_source_with_config(source, CompilerConfig::default())?
        .execute_with_snapshots(expectations)
}

/// Run every declared test twice against a snapshot oracle through an explicitly
/// selected execution lane. This composes the typed per-executable lane seam with
/// snapshot verdicts without mutating the process environment.
pub fn run_source_with_snapshots_and_lane(
    source: &str,
    expectations: &SnapshotExpectations,
    lane: weavy::exec::LaneRequest,
) -> Result<RatchetReport, RunError> {
    prepare_source_with_lane(source, lane)?.execute_with_snapshots(expectations)
}

/// Run through the production scheduler while retaining every interior Weavy
/// source mark. This is an explicit diagnostic lane: ordinary [`run_source`]
/// uses bounded Production tracing and preserves only structural task events.
pub fn run_source_innards(source: &str) -> Result<RatchetReport, RunError> {
    prepare_source_with_cache(source, CompilerConfig::default(), LoweringCache::innards())?
        .execute()
}
/// Run every declared test twice under explicit shape-selection configuration.
/// The forced-copy molten differential compiles the same source with
/// `force_molten_copy` set and proves the produced value identities match the
/// default molten run.
pub fn run_source_with_config(
    source: &str,
    config: CompilerConfig,
) -> Result<RatchetReport, RunError> {
    prepare_source_with_config(source, config)?.execute()
}

/// Run every declared test twice through an explicitly selected execution lane.
///
/// This is the non-environment lane seam behind the cross-lane differential: the
/// [`weavy::exec::LaneRequest`] is threaded per-executable through the lowering
/// cache, so a native and an interpreter run can be produced in one process
/// without mutating the global `WEAVY_JIT` variable (which would race sibling
/// tests under a parallel runner). Every other aspect — Production trace mode,
/// the plain/chaos discipline, provenance-keyed families — is identical to
/// [`run_source`], so the two lanes are compared on the exact production shape.
///
/// Lane *authority* still lives in Weavy ([`weavy::exec::LaneRequest`] /
/// `Executable::with_lane`, `r[machine.execution.weavy-owns-mode]`): this is a
/// forwarding seam used only by the cross-lane certificate, not a machine-side
/// lane selector — the production path stays on
/// [`weavy::exec::LaneRequest::Auto`].
pub fn run_source_with_lane(
    source: &str,
    lane: weavy::exec::LaneRequest,
) -> Result<RatchetReport, RunError> {
    prepare_source_with_lane(source, lane)?.execute()
}

pub fn run_source_rerun_audit(source: &str) -> Result<RerunAuditReport, RunError> {
    prepare_source_with_config(source, CompilerConfig::default())?.execute_rerun_audit()
}

pub fn run_source_rerun_audit_with_lane(
    source: &str,
    lane: weavy::exec::LaneRequest,
) -> Result<RerunAuditReport, RunError> {
    prepare_source_with_lane(source, lane)?.execute_rerun_audit()
}

pub fn run_source_revision_audit(
    first_source: &str,
    second_source: &str,
) -> Result<SourceRevisionAuditReport, RunError> {
    run_source_revision_audit_with_lane(first_source, second_source, weavy::exec::LaneRequest::Auto)
}

pub fn run_source_revision_audit_with_lane(
    first_source: &str,
    second_source: &str,
    lane: weavy::exec::LaneRequest,
) -> Result<SourceRevisionAuditReport, RunError> {
    let config = CompilerConfig {
        force_scalar_call_boundaries: true,
        ..CompilerConfig::default()
    };
    let mut first_prepared =
        prepare_source_with_cache(first_source, config, LoweringCache::for_lane(lane))?;
    let first_revision = source_revision_id(first_source);
    let second_revision = source_revision_id(second_source);
    let first_warnings = first_prepared.compilation.warnings.clone();
    let mut state = PersistentRuntimeState::default();
    let first = run_lane(
        &first_prepared.compilation.module,
        &mut first_prepared.cache,
        ChaosPolicy::default(),
        ExecutionPhase::PlainRuntimeReady,
        ExecutionPhase::PlainCompleted,
        &SnapshotExpectations::new(),
        &mut |_| {},
        false,
        false,
        None,
        Some(&mut state),
        None,
        None,
        Some(&first_revision),
    )?;
    let journal = state.to_journal();
    let journal_json = journal.to_json()?;
    let loaded_journal = PersistentRuntimeJournal::from_json(&journal_json)?;
    let journal_json = loaded_journal.to_json()?;
    let loaded_journal = PersistentRuntimeJournal::from_json(&journal_json)?;
    let journal_bytes = journal_json.len() as u64;

    let mut second_prepared =
        prepare_source_with_cache(second_source, config, first_prepared.cache)?;
    let second_warnings = second_prepared.compilation.warnings.clone();
    let mut second_state = PersistentRuntimeState::default();
    let mut load = PersistentRuntimeJournalLoadReport::default();
    let second = run_lane(
        &second_prepared.compilation.module,
        &mut second_prepared.cache,
        ChaosPolicy::default(),
        ExecutionPhase::PlainRuntimeReady,
        ExecutionPhase::PlainCompleted,
        &SnapshotExpectations::new(),
        &mut |_| {},
        true,
        true,
        None,
        Some(&mut second_state),
        Some(&loaded_journal),
        Some(&mut load),
        Some(&second_revision),
    )?;
    let first_value_checks = first
        .checks
        .iter()
        .filter(|check| check.identity.is_some())
        .collect::<Vec<_>>();
    let second_value_checks = second
        .checks
        .iter()
        .filter(|check| check.identity.is_some())
        .collect::<Vec<_>>();
    let nondeterministic =
        first.value_family() != second.value_family() || first_value_checks != second_value_checks;
    Ok(SourceRevisionAuditReport {
        first_warnings,
        second_warnings,
        first,
        second,
        load,
        journal_bytes,
        nondeterministic,
        lowering_cache: second_prepared.cache.counters(),
    })
}

fn source_revision_id(source: &str) -> String {
    blake3::hash(source.as_bytes()).to_hex().to_string()
}

/// The readiness-boundary form of [`run_source_with_lane`]: compile, lower, and
/// verify every demanded island through the requested lane without running a
/// test.
pub fn prepare_source_with_lane(
    source: &str,
    lane: weavy::exec::LaneRequest,
) -> Result<PreparedRun, RunError> {
    prepare_source_with_cache(
        source,
        CompilerConfig::default(),
        LoweringCache::for_lane(lane),
    )
}

/// Parse, check, lower, verify, and natively compile `source` without running
/// any test. When this returns `Ok`, all compilation is complete and the
/// returned [`PreparedRun`] is ready to execute with no further compilation.
///
/// r[impl machine.scheduler.replay-is-semantics]
pub fn prepare_source(source: &str) -> Result<PreparedRun, RunError> {
    prepare_source_with_config(source, CompilerConfig::default())
}

/// The configurable form of [`prepare_source`]. This keeps shape-selection
/// experiments on the same readiness boundary as the ordinary production path.
pub fn prepare_source_with_config(
    source: &str,
    config: CompilerConfig,
) -> Result<PreparedRun, RunError> {
    prepare_source_with_cache(source, config, LoweringCache::default())
}

fn prepare_source_with_cache(
    source: &str,
    config: CompilerConfig,
    cache: LoweringCache,
) -> Result<PreparedRun, RunError> {
    prepare_modules_with_cache(source, &[], config, cache)
}

fn prepare_modules_with_cache(
    source: &str,
    modules: &[crate::modules::ModuleSource<'_>],
    config: CompilerConfig,
    mut cache: LoweringCache,
) -> Result<PreparedRun, RunError> {
    let compilation = Compiler::with_config(config).compile_with_modules(source, modules)?;

    // Lower every island the lanes will demand so its native code is compiled
    // and cached now, before execution. `get_or_lower` keys on canonical recipe
    // content, so these exact entries are reused as cache hits during execution.
    for test in &compilation.module.tests {
        // Every value-check island. A trace site reads the frozen counter
        // snapshot and lowers nothing, so it is skipped here.
        let partitioned = compilation.module.try_partition_test(test)?;
        for value in &partitioned.values {
            if value.island.purpose != crate::vir::IslandPurpose::Effect {
                cache.get_or_lower(&value.island)?;
            }
        }
        // Argument islands demanded lazily through force-on-park are compiled now
        // so a park resolves through a warm cache hit, never a compilation.
        for wire in &partitioned.wire_islands {
            cache.get_or_lower(&wire.island)?;
        }
        if let Some(generator) = &partitioned.generator {
            cache.get_or_lower(generator)?;
        }
        for site in &partitioned.sites {
            match &site.recipe {
                PartitionedRecipe::Value { island }
                | PartitionedRecipe::Snapshot { island, .. } => {
                    cache.get_or_lower(&partitioned.islands[*island])?;
                }
                PartitionedRecipe::Trace(_) => {}
            }
        }
    }

    Ok(PreparedRun { compilation, cache })
}

impl PreparedRun {
    /// Run every declared test twice over the warm cache. The chaos lane discards
    /// the first running task at an edge safepoint and must publish the same
    /// identities. No compilation happens here: every `get_or_lower` is a hit.
    ///
    /// r[impl machine.scheduler.chaos-kill-oracle]
    /// r[impl machine.scheduler.replay-is-semantics]
    pub fn execute(self) -> Result<RatchetReport, RunError> {
        self.execute_inner(&SnapshotExpectations::new(), |_| {})
    }

    /// Execute with a snapshot oracle: snapshot checks compare their rendering to
    /// `expectations` and go red on mismatch/missing; other checks are unaffected.
    pub fn execute_with_snapshots(
        self,
        expectations: &SnapshotExpectations,
    ) -> Result<RatchetReport, RunError> {
        self.execute_inner(expectations, |_| {})
    }

    /// Execute with lifecycle observations made while each lane's runtime is
    /// still live. This is intentionally a production-path seam: observers see
    /// no per-iteration callbacks and cannot hide retained execution state.
    pub fn execute_with_observer(
        self,
        observe: impl FnMut(ExecutionPhase),
    ) -> Result<RatchetReport, RunError> {
        self.execute_inner(&SnapshotExpectations::new(), observe)
    }

    pub fn execute_rerun_audit(mut self) -> Result<RerunAuditReport, RunError> {
        let mut state = PersistentRuntimeState::default();
        let first = run_lane(
            &self.compilation.module,
            &mut self.cache,
            ChaosPolicy::default(),
            ExecutionPhase::PlainRuntimeReady,
            ExecutionPhase::PlainCompleted,
            &SnapshotExpectations::new(),
            &mut |_| {},
            false,
            false,
            None,
            Some(&mut state),
            None,
            None,
            None,
        )?;
        let mut second_state = PersistentRuntimeState::default();
        let second = run_lane(
            &self.compilation.module,
            &mut self.cache,
            ChaosPolicy::default(),
            ExecutionPhase::PlainRuntimeReady,
            ExecutionPhase::PlainCompleted,
            &SnapshotExpectations::new(),
            &mut |_| {},
            true,
            true,
            Some(state),
            Some(&mut second_state),
            None,
            None,
            None,
        )?;
        let first_value_checks = first
            .checks
            .iter()
            .filter(|check| check.identity.is_some())
            .collect::<Vec<_>>();
        let second_value_checks = second
            .checks
            .iter()
            .filter(|check| check.identity.is_some())
            .collect::<Vec<_>>();
        let nondeterministic = first.value_family() != second.value_family()
            || first_value_checks != second_value_checks;
        Ok(RerunAuditReport {
            warnings: self.compilation.warnings,
            first,
            second,
            nondeterministic,
            lowering_cache: self.cache.counters(),
        })
    }

    pub fn execute_persistence_audit(self) -> Result<PersistenceAuditReport, RunError> {
        self.execute_persistence_audit_with_journal_mutator(|journal| journal)
    }

    pub fn execute_persistence_audit_with_journal_mutator(
        mut self,
        mutate: impl FnOnce(PersistentRuntimeJournal) -> PersistentRuntimeJournal,
    ) -> Result<PersistenceAuditReport, RunError> {
        let mut state = PersistentRuntimeState::default();
        let first = run_lane(
            &self.compilation.module,
            &mut self.cache,
            ChaosPolicy::default(),
            ExecutionPhase::PlainRuntimeReady,
            ExecutionPhase::PlainCompleted,
            &SnapshotExpectations::new(),
            &mut |_| {},
            false,
            false,
            None,
            Some(&mut state),
            None,
            None,
            None,
        )?;
        let journal = state.to_journal();
        let journal_json = journal.to_json()?;
        let loaded_journal = PersistentRuntimeJournal::from_json(&journal_json)?;
        let loaded_journal = mutate(loaded_journal);
        let journal_json = loaded_journal.to_json()?;
        let loaded_journal = PersistentRuntimeJournal::from_json(&journal_json)?;
        let journal_bytes = journal_json.len() as u64;
        let mut second_state = PersistentRuntimeState::default();
        let mut load = PersistentRuntimeJournalLoadReport::default();
        let second = run_lane(
            &self.compilation.module,
            &mut self.cache,
            ChaosPolicy::default(),
            ExecutionPhase::PlainRuntimeReady,
            ExecutionPhase::PlainCompleted,
            &SnapshotExpectations::new(),
            &mut |_| {},
            true,
            true,
            None,
            Some(&mut second_state),
            Some(&loaded_journal),
            Some(&mut load),
            None,
        )?;
        let first_value_checks = first
            .checks
            .iter()
            .filter(|check| check.identity.is_some())
            .collect::<Vec<_>>();
        let second_value_checks = second
            .checks
            .iter()
            .filter(|check| check.identity.is_some())
            .collect::<Vec<_>>();
        let nondeterministic = first.value_family() != second.value_family()
            || first_value_checks != second_value_checks;
        Ok(PersistenceAuditReport {
            warnings: self.compilation.warnings,
            first,
            second,
            load,
            journal_bytes,
            nondeterministic,
            lowering_cache: self.cache.counters(),
        })
    }

    fn execute_inner(
        mut self,
        expectations: &SnapshotExpectations,
        mut observe: impl FnMut(ExecutionPhase),
    ) -> Result<RatchetReport, RunError> {
        let plain = run_lane(
            &self.compilation.module,
            &mut self.cache,
            ChaosPolicy::default(),
            ExecutionPhase::PlainRuntimeReady,
            ExecutionPhase::PlainCompleted,
            expectations,
            &mut observe,
            true,
            false,
            None,
            None,
            None,
            None,
            None,
        )?;
        let chaos = run_lane(
            &self.compilation.module,
            &mut self.cache,
            ChaosPolicy {
                kill_first_running_task: true,
            },
            ExecutionPhase::ChaosRuntimeReady,
            ExecutionPhase::ChaosCompleted,
            expectations,
            &mut observe,
            true,
            false,
            None,
            None,
            None,
            None,
            None,
        )?;
        Ok(RatchetReport {
            warnings: self.compilation.warnings,
            plain,
            chaos,
            lowering_cache: self.cache.counters(),
        })
    }
}

/// The per-test context an evaluated value-check site reads: the test name for
/// its cost-model location, the argument-island lookup backing its wires, and the
/// shared publications available as value inputs.
struct SiteContext<'a> {
    test_name: &'a str,
    source_revision: Option<&'a str>,
    module: &'a Module,
    wire_lookup: &'a BTreeMap<ValueIslandId, &'a PartitionedValue>,
    published_values: &'a BTreeMap<ValueIslandId, Evaluation>,
}

fn scoped_location(location: Location, source_revision: Option<&str>) -> Location {
    source_revision.map_or(location.clone(), |revision| {
        location.with_source_revision(revision)
    })
}

/// Evaluate one value-check island as an ordinary pure demand and record its
/// provenance-keyed outcome. Provenance is the site's stable `YieldSiteId`, and
/// with no dynamic keys the demand location is byte-identical to the historical
/// flat check location.
fn evaluate_value_site(
    runtime: &mut Runtime<EventLog>,
    cache: &mut LoweringCache,
    context: &SiteContext<'_>,
    island: &crate::vir::Island,
    site: u32,
    chaos: ChaosPolicy,
) -> Result<CheckRun, RunError> {
    // The island was lowered up front; re-lower to guarantee the cache entry,
    // then hold every borrow immutably so the wire-demand tree can be built.
    cache.get_or_lower(island)?;
    let lowered = cache
        .lowered(island)
        .expect("value-check island is lowered");
    let attribution = attribution_for(island);
    let provenance = ProvenanceKey::site(site);
    let location = scoped_location(
        Location::for_test_provenance(context.test_name, site, &provenance.dynamic_keys),
        context.source_revision,
    );
    let arguments = island
        .value_inputs
        .iter()
        .map(|value| {
            context
                .published_values
                .get(value)
                .cloned()
                .expect("partitioned value input was published")
        })
        .collect::<Vec<_>>();
    // Each `AwaitWire` in this island forces its argument island lazily through
    // the memo path; an untaken control region never parks, so it never demands.
    let wires_backing = flat_wires(
        cache,
        context.module,
        context.wire_lookup,
        &island.wire_inputs,
        context.test_name,
        context.source_revision,
    );
    let wires = wires_backing.demands();
    let evaluation: Evaluation = runtime.evaluate(
        island.id,
        &location,
        lowered,
        &attribution,
        IslandInputs {
            arguments: &arguments,
            wires: &wires,
        },
        chaos,
    )?;
    let argument_identities = arguments.iter().map(|argument| argument.identity).collect();
    Ok(CheckRun {
        provenance,
        identity: Some(evaluation.identity),
        arguments: argument_identities,
        passed: evaluation.passed,
        failure: evaluation.failure,
        failure_context: evaluation.failure_context,
        trace_failure: None,
        snapshot: None,
    })
}

/// Evaluate one `expect_snapshot` site: demand its value-publishing island,
/// render the published value structurally, and decide the check's verdict
/// against the harness snapshot oracle. The value publication is an ordinary
/// demand, so plain/chaos and native/interpreter lanes agree on the same
/// identity and the same rendering; the verdict is a function of (name, actual,
/// expected) alone, so it is lane-stable too.
///
/// The check passes only when the rendering equals the oracle's golden for this
/// test + name. A mismatch, a missing golden, a duplicate name within the test,
/// or a render fault each produces a red `CheckRun` carrying typed context — none
/// aborts the run.
#[allow(clippy::too_many_arguments)]
fn evaluate_snapshot_site(
    runtime: &mut Runtime<EventLog>,
    cache: &mut LoweringCache,
    context: &SiteContext<'_>,
    island: &crate::vir::Island,
    site: u32,
    name: &str,
    oracle: &SnapshotExpectations,
    seen_names: &mut BTreeSet<String>,
    chaos: ChaosPolicy,
) -> Result<CheckRun, RunError> {
    cache.get_or_lower(island)?;
    let lowered = cache.lowered(island).expect("snapshot island is lowered");
    let output_type = lowered.output_type.clone();
    let attribution = attribution_for(island);
    let provenance = ProvenanceKey::site(site);
    let location = scoped_location(
        Location::for_test_provenance(context.test_name, site, &provenance.dynamic_keys),
        context.source_revision,
    );
    let arguments = island
        .value_inputs
        .iter()
        .map(|value| {
            context
                .published_values
                .get(value)
                .cloned()
                .expect("partitioned value input was published")
        })
        .collect::<Vec<_>>();
    let wires_backing = flat_wires(
        cache,
        context.module,
        context.wire_lookup,
        &island.wire_inputs,
        context.test_name,
        context.source_revision,
    );
    let wires = wires_backing.demands();
    let evaluation: Evaluation = runtime.evaluate(
        island.id,
        &location,
        lowered,
        &attribution,
        IslandInputs {
            arguments: &arguments,
            wires: &wires,
        },
        chaos,
    )?;
    let argument_identities = arguments.iter().map(|argument| argument.identity).collect();
    // If the value publication itself language-failed, there is nothing to
    // render; surface that as the check failure with its own attribution.
    if !evaluation.passed || evaluation.failure.is_some() {
        return Ok(CheckRun {
            provenance,
            identity: Some(evaluation.identity),
            arguments: argument_identities,
            passed: false,
            failure: evaluation.failure,
            failure_context: evaluation.failure_context,
            trace_failure: None,
            snapshot: None,
        });
    }
    // A second use of a name in the same test run is a duplicate, regardless of
    // what it renders to; the oracle keys by name and cannot hold two goldens.
    let (rendered, outcome) = if !seen_names.insert(name.to_owned()) {
        let rendered = runtime
            .render_snapshot(evaluation.handle, &output_type)
            .unwrap_or_default();
        (rendered, SnapshotOutcome::DuplicateName)
    } else {
        match runtime.render_snapshot(evaluation.handle, &output_type) {
            Err(detail) => (String::new(), SnapshotOutcome::RenderFault { detail }),
            Ok(rendered) => match oracle.expected(context.test_name, name) {
                None => (rendered, SnapshotOutcome::MissingExpected),
                Some(expected) if expected == rendered => (rendered, SnapshotOutcome::Matched),
                Some(expected) => (
                    rendered,
                    SnapshotOutcome::Mismatch {
                        expected: expected.to_owned(),
                    },
                ),
            },
        }
    };
    let capture = SnapshotCapture {
        name: name.to_owned(),
        rendered,
        outcome,
    };
    let passed = capture.passed();
    Ok(CheckRun {
        provenance,
        identity: Some(evaluation.identity),
        arguments: argument_identities,
        passed,
        failure: None,
        failure_context: None,
        trace_failure: None,
        snapshot: Some(capture),
    })
}

#[allow(clippy::too_many_arguments)]
fn run_lane(
    module: &crate::vir::Module,
    cache: &mut LoweringCache,
    chaos: ChaosPolicy,
    ready_phase: ExecutionPhase,
    completed_phase: ExecutionPhase,
    expectations: &SnapshotExpectations,
    observe: &mut impl FnMut(ExecutionPhase),
    evaluate_trace_checks: bool,
    use_rerun_overlays: bool,
    persistent_in: Option<PersistentRuntimeState>,
    persistent_out: Option<&mut PersistentRuntimeState>,
    persistent_journal_in: Option<&PersistentRuntimeJournal>,
    persistent_journal_report: Option<&mut PersistentRuntimeJournalLoadReport>,
    source_revision: Option<&str>,
) -> Result<SuiteRun, RunError> {
    let mut journal_load_report = None;
    let mut runtime = if let Some(journal) = persistent_journal_in {
        let (runtime, report) =
            Runtime::with_persistent_journal_values(EventLog::default(), journal)?;
        journal_load_report = Some(report);
        runtime
    } else if let Some(state) = persistent_in {
        Runtime::with_persistent_state(EventLog::default(), state)
    } else {
        Runtime::new(EventLog::default())
    };
    runtime.set_authoritative_rerun_audit(use_rerun_overlays);
    observe(ready_phase);
    let mut checks = Vec::new();
    let mut values = Vec::new();
    // Trace checks are deferred until every selected value check completes; they
    // are evaluated once, together, against the frozen completed-run snapshot.
    // Each deferred trace carries the resolved canonical preimage of its
    // binding-level described wire (when it has one), read from the authored
    // graph at deferral time — nothing is demanded to resolve it.
    let mut deferred_traces: Vec<(ProvenanceKey, TraceCheck, Option<String>)> = Vec::new();
    // Distinct executed invocation preimages already observed for a described-wire
    // observer, so equal preimages share one realized-demand entry.
    let mut observed_invocations: BTreeSet<String> = BTreeSet::new();
    let mut kill_available = chaos.kill_first_running_task;
    let mut journal_claims_loaded = false;

    for test in &module.tests {
        runtime.set_fixture_rerun_overlay(
            use_rerun_overlays
                .then(|| test.metadata.rerun_with.clone())
                .flatten(),
        );
        if let Some(journal) = persistent_journal_in
            && !journal_claims_loaded
        {
            if let Some(report) = journal_load_report.as_mut() {
                runtime.load_persistent_journal_claims(journal, report);
            }
            journal_claims_loaded = true;
        }
        let partitioned = module.try_partition_test(test)?;
        let selected = selected_wire_functions(&partitioned);
        let mut evaluated_islands: Vec<usize> = Vec::new();
        // Snapshot names already emitted in this test; a repeat is a duplicate.
        let mut seen_snapshot_names: BTreeSet<String> = BTreeSet::new();
        let wire_lookup: BTreeMap<ValueIslandId, &PartitionedValue> = partitioned
            .wire_islands
            .iter()
            .map(|wire| (wire.id, wire))
            .collect();
        let mut published_values = BTreeMap::new();
        for capability in &partitioned.capabilities {
            let evaluation = runtime.publish_capability(&capability.ty, &capability.name);
            published_values.insert(capability.id, evaluation);
        }
        for value in &partitioned.values {
            let arguments = value
                .island
                .value_inputs
                .iter()
                .map(|input| {
                    published_values
                        .get(input)
                        .cloned()
                        .expect("value islands are ordered after their dependencies")
                })
                .collect::<Vec<_>>();
            let chaos = ChaosPolicy {
                kill_first_running_task: kill_available,
            };
            let evaluation = if value.island.purpose == crate::vir::IslandPurpose::Effect {
                if matches!(
                    value.island.effect_output().map(|node| &node.op),
                    Some(crate::vir::Op::Exec { .. })
                ) {
                    let capability = arguments
                        .first()
                        .cloned()
                        .expect("exec effect island has its declared capability input");
                    runtime.evaluate_exec(
                        &value.island,
                        &scoped_location(
                            Location::for_test_value(&partitioned.name, &value.id.stable_segment()),
                            source_revision,
                        ),
                        &capability,
                        chaos,
                    )?
                } else {
                    let fingerprint = value
                        .effect_fingerprint
                        .as_deref()
                        .expect("effect island carries a structural fingerprint");
                    runtime.evaluate_effect(
                        value.island.id,
                        &scoped_location(
                            Location::for_test_effect(&partitioned.name, fingerprint),
                            source_revision,
                        ),
                        fingerprint,
                        &value.island,
                        &arguments,
                        chaos,
                    )?
                }
            } else {
                let lowered = cache.get_or_lower(&value.island)?;
                let attribution = attribution_for(&value.island);
                runtime.evaluate(
                    value.island.id,
                    &scoped_location(
                        Location::for_test_value(&partitioned.name, &value.id.stable_segment()),
                        source_revision,
                    ),
                    lowered,
                    &attribution,
                    IslandInputs {
                        arguments: &arguments,
                        wires: &[],
                    },
                    chaos,
                )?
            };
            kill_available = false;
            // A hoisted invocation island that actually computed (a memo miss)
            // records one realized demand for its described invocation. A
            // memoized re-demand of the same recipe+argument is a hit and adds
            // nothing, so repeated identical wires memoize to a single
            // realization.
            if let Some(wire) = &value.wire
                && evaluation.memo == MemoVerdict::Miss
            {
                let arguments = wire
                    .arguments
                    .as_ref()
                    .map(|arguments| arguments.iter().map(wire_arg_identity).collect::<Vec<_>>());
                runtime.record_wire_demand(
                    wire.function,
                    arguments,
                    module.invocation_preimage(value.id.function, value.id.node),
                );
            }
            values.push(ValuePublicationRun {
                provenance: value.id,
                identity: evaluation.identity,
                failure: evaluation.failure.clone(),
            });
            published_values.insert(value.id, evaluation);
        }
        for catch in &partitioned.catches {
            let operand = published_values
                .get(&catch.operand)
                .cloned()
                .expect("catch operand was published before its result");
            let evaluation = runtime.publish_catch(&catch.result_type, &operand)?;
            values.push(ValuePublicationRun {
                provenance: catch.id,
                identity: evaluation.identity,
                failure: None,
            });
            published_values.insert(catch.id, evaluation);
        }
        // A branch-dependent generator runs its real Match/If control through one
        // verified generator task that publishes only the taken sites; each taken
        // descriptor then becomes an ordinary pure check demand. A flat generator
        // keeps the historical unconditional path with byte-identical behaviour.
        let taken = if test.generator.has_conditional_sites() {
            let generator = partitioned
                .generator
                .as_ref()
                .expect("conditional test has a partitioned generator");
            let lowered = cache.get_or_lower(generator)?;
            let attribution = attribution_for(generator);
            let arguments = generator
                .value_inputs
                .iter()
                .map(|value| {
                    published_values
                        .get(value)
                        .cloned()
                        .expect("generator shared value was published")
                })
                .collect::<Vec<_>>();
            let outcome = runtime.drive_generator(
                generator.id,
                lowered,
                &attribution,
                &arguments,
                ChaosPolicy {
                    kill_first_running_task: kill_available,
                },
            )?;
            kill_available = false;
            let published = match outcome {
                GeneratorOutcome::Sites(published) => published,
                GeneratorOutcome::LanguageFailure { failure, context } => {
                    return Err(RunError::GeneratorLanguageFailure {
                        test: test.name.clone(),
                        failure,
                        context,
                    });
                }
            };
            let mut seen = BTreeSet::new();
            let mut sites = Vec::with_capacity(published.len());
            for raw in published {
                let site = u32::try_from(raw).map_err(|_| RunError::MalformedSiteKey {
                    test: test.name.clone(),
                    site: raw,
                })?;
                if !seen.insert(site) {
                    return Err(RunError::DuplicateSiteKey {
                        test: test.name.clone(),
                        site,
                    });
                }
                sites.push((site, raw));
            }
            Some(sites)
        } else {
            None
        };

        match taken {
            // Taken generator sites, in publication order. Each resolves to its
            // recipe by stable YieldSiteId — never by island-vector ordinal —
            // so a value site becomes an ordinary pure demand and a trace site
            // is deferred to the post-run snapshot.
            Some(sites) => {
                for (site, raw) in sites {
                    let partitioned_site = partitioned
                        .sites
                        .iter()
                        .find(|candidate| candidate.id.0 == site)
                        .ok_or(RunError::UnknownSiteKey {
                            test: test.name.clone(),
                            site: raw,
                        })?;
                    match &partitioned_site.recipe {
                        PartitionedRecipe::Value { island } => {
                            let misses_before = runtime.counters().memo_misses;
                            let check = evaluate_value_site(
                                &mut runtime,
                                cache,
                                &SiteContext {
                                    test_name: &partitioned.name,
                                    source_revision,
                                    module,
                                    wire_lookup: &wire_lookup,
                                    published_values: &published_values,
                                },
                                &partitioned.islands[*island],
                                site,
                                ChaosPolicy::default(),
                            )?;
                            if runtime.counters().memo_misses > misses_before {
                                evaluated_islands.push(*island);
                            }
                            checks.push(check);
                        }
                        PartitionedRecipe::Snapshot { island, name } => {
                            let misses_before = runtime.counters().memo_misses;
                            let check = evaluate_snapshot_site(
                                &mut runtime,
                                cache,
                                &SiteContext {
                                    test_name: &partitioned.name,
                                    source_revision,
                                    module,
                                    wire_lookup: &wire_lookup,
                                    published_values: &published_values,
                                },
                                &partitioned.islands[*island],
                                site,
                                name,
                                expectations,
                                &mut seen_snapshot_names,
                                ChaosPolicy::default(),
                            )?;
                            if runtime.counters().memo_misses > misses_before {
                                evaluated_islands.push(*island);
                            }
                            checks.push(check);
                        }
                        PartitionedRecipe::Trace(trace) => {
                            if evaluate_trace_checks {
                                deferred_traces.push((
                                    ProvenanceKey::site(site),
                                    trace.clone(),
                                    binding_preimage(module, test.function, trace),
                                ));
                            }
                        }
                    }
                }
            }
            // Flat generator: every top-level site publishes unconditionally.
            // Its stable YieldSiteId is the provenance selector, independent of
            // how value and trace sites interleave.
            None => {
                for partitioned_site in &partitioned.sites {
                    let site = partitioned_site.id.0;
                    match &partitioned_site.recipe {
                        PartitionedRecipe::Value { island } => {
                            let misses_before = runtime.counters().memo_misses;
                            let check = evaluate_value_site(
                                &mut runtime,
                                cache,
                                &SiteContext {
                                    test_name: &partitioned.name,
                                    source_revision,
                                    module,
                                    wire_lookup: &wire_lookup,
                                    published_values: &published_values,
                                },
                                &partitioned.islands[*island],
                                site,
                                ChaosPolicy {
                                    kill_first_running_task: kill_available,
                                },
                            )?;
                            if runtime.counters().memo_misses > misses_before {
                                evaluated_islands.push(*island);
                            }
                            checks.push(check);
                            kill_available = false;
                        }
                        PartitionedRecipe::Snapshot { island, name } => {
                            let misses_before = runtime.counters().memo_misses;
                            let check = evaluate_snapshot_site(
                                &mut runtime,
                                cache,
                                &SiteContext {
                                    test_name: &partitioned.name,
                                    source_revision,
                                    module,
                                    wire_lookup: &wire_lookup,
                                    published_values: &published_values,
                                },
                                &partitioned.islands[*island],
                                site,
                                name,
                                expectations,
                                &mut seen_snapshot_names,
                                ChaosPolicy {
                                    kill_first_running_task: kill_available,
                                },
                            )?;
                            if runtime.counters().memo_misses > misses_before {
                                evaluated_islands.push(*island);
                            }
                            checks.push(check);
                            kill_available = false;
                        }
                        PartitionedRecipe::Trace(trace) => {
                            if evaluate_trace_checks {
                                deferred_traces.push((
                                    ProvenanceKey::site(site),
                                    trace.clone(),
                                    binding_preimage(module, test.function, trace),
                                ));
                            }
                        }
                    }
                }
            }
        }

        // Bounded Production observation: for every evaluated check island, retain
        // one realized-demand entry per distinct executed invocation preimage a
        // described-wire observer selects. This never adds a scheduler edge and
        // never changes partitioning, execution, memoization, or results.
        if !selected.is_empty() {
            for island in evaluated_islands {
                observe_bundled_invocations(
                    &mut runtime,
                    module,
                    test.function,
                    &partitioned.islands[island],
                    &selected,
                    &mut observed_invocations,
                );
            }
        }
    }

    // Freeze the completed-run snapshot after every selected value check, then
    // evaluate the deferred trace checks against it. This reads counters only;
    // it mutates no runtime state, so a trace check never adds to the scheduler
    // requests, memo entries, or store interns it inspects.
    let counters = runtime.counters();
    let mut function_calls = BTreeMap::new();
    for event in runtime.sink().events() {
        if let EventKind::WeavyFrameEntered { function, .. } = event.kind {
            *function_calls.entry(function).or_insert(0) += 1;
        }
    }
    let snapshot = TraceSnapshot {
        scheduler_requests: counters.scheduler_requests,
        memo_entries: runtime.memo_entries(),
        memo_hits: counters.memo_hits_exact
            + counters.memo_hits_projection
            + counters.memo_hits_semantic,
        store_interns: counters.store_interns,
        effect_spawns: counters.effect_spawns,
        value_island_spawns: counters.value_island_spawns,
        successful_aggregate_freezes: counters.successful_aggregate_freezes,
        active_molten_selections: counters.active_molten_selections,
        forced_copy_selections: counters.forced_copy_selections,
        framed_bytes: counters.framed_bytes,
        peak_molten_bytes: counters.peak_molten_bytes,
        peak_molten_nodes: counters.peak_molten_nodes,
        fetches_performed: counters.fetches_performed,
        reads: runtime
            .receipts()
            .flat_map(|receipt| receipt.reads.iter())
            .map(|read| read.projection.clone())
            .collect(),
        function_calls,
        wire_demands: runtime.realized_wire_demands().to_vec(),
    };
    for (provenance, trace, binding) in deferred_traces {
        checks.push(snapshot.evaluate(provenance, trace, binding.as_deref()));
    }

    let receipt_count = runtime.receipts().count() as u64;
    let all_demands_ready = runtime
        .demands()
        .all(|demand| matches!(demand.state, DemandState::Ready | DemandState::Failed));
    let all_tasks_terminal = runtime
        .tasks()
        .all(|task| matches!(task.state, TaskState::Completed | TaskState::Discarded));
    observe(completed_phase);
    let (sink, state) = runtime.into_sink_and_persistent_state();
    if let Some(out) = persistent_out {
        *out = state;
    }
    if let (Some(report_out), Some(report)) = (persistent_journal_report, journal_load_report) {
        *report_out = report;
    }
    let events = sink.into_events();
    Ok(SuiteRun {
        checks,
        values,
        counters,
        events,
        receipt_count,
        all_demands_ready,
        all_tasks_terminal,
    })
}
