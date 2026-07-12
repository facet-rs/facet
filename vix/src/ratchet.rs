//! Production-path ratchet runner: source -> generated AST -> VIR -> Weavy.

use std::collections::{BTreeMap, BTreeSet};

use crate::compiler::{Compiler, CompilerConfig};
use crate::diagnostic::Diagnostics;
use crate::lowering::{LoweringCache, LoweringCacheCounters, LoweringError, attribution_for};
use crate::runtime::{
    ChaosPolicy, Counters, DemandState, Evaluation, Event, EventKind, EventLog, FailureContext,
    FailureValue, FramedNode, GeneratorOutcome, Location, MachineError, MemoVerdict, Runtime,
    SchemaId, TaskState, ValueId,
};
use crate::vir::{DescribedWire, FunctionId, PartitionedRecipe, TraceCheck, ValueIslandId, WireArg};

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
    store_interns: u64,
    value_island_spawns: u64,
    successful_aggregate_freezes: u64,
    active_molten_selections: u64,
    forced_copy_selections: u64,
    framed_bytes: u64,
    peak_molten_bytes: u64,
    peak_molten_nodes: u64,
    function_calls: BTreeMap<FunctionId, u64>,
    /// One entry per realized wire demand (a computation the memo path actually
    /// ran). Repeated identical `recipe + argument` demands memoize to a single
    /// realization, so a call-site selector observes at most one entry; distinct
    /// arguments contribute distinct entries. This is the frozen log the
    /// described-wire trace checks read; it retains only the callee identity and
    /// argument identities a trace descriptor can select on.
    wire_demands: Vec<RealizedWireDemand>,
}

/// One realized invocation recorded for described-wire observation: which user
/// function was demanded and with which canonical argument identities. Recorded
/// only when a wire demand actually computes (a memo miss that ran), so the log
/// counts realizations, never re-demands of an already-memoized key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RealizedWireDemand {
    pub function: FunctionId,
    pub arguments: Vec<ValueId>,
}

/// The canonical identity of one described-wire scalar argument. Computed the
/// same way an evaluated scalar value interns, so a described literal selects
/// the exact realized argument identity without demanding anything.
fn wire_arg_identity(arg: &WireArg) -> ValueId {
    let (ty_name, bytes) = match arg {
        WireArg::Int(value) => ("Int", value.to_le_bytes().to_vec()),
        WireArg::Bool(value) => ("Bool", i64::from(*value).to_le_bytes().to_vec()),
    };
    FramedNode::leaf(SchemaId::named(&format!("vix.semantic.v1:{ty_name}")), bytes).identity()
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
    /// matches only the exact described argument identities. The described
    /// literals are resolved to identities here — never demanded or counted.
    fn wire_matches(&self, wire: &DescribedWire) -> u64 {
        let described: Vec<ValueId> = wire.arguments.iter().map(wire_arg_identity).collect();
        self.wire_demands
            .iter()
            .filter(|demand| {
                demand.function == wire.function
                    && (wire.name_level || demand.arguments == described)
            })
            .count() as u64
    }

    /// Evaluate one trace check against the frozen snapshot.
    fn evaluate(&self, provenance: ProvenanceKey, check: TraceCheck) -> CheckRun {
        let (observed, passed) = match &check {
            TraceCheck::SchedulerRequestsAtMost { bound } => at_most(self.scheduler_requests, *bound),
            TraceCheck::MemoEntriesAtMost { bound } => at_most(self.memo_entries, *bound),
            TraceCheck::StoreInternsAtMost { bound } => at_most(self.store_interns, *bound),
            TraceCheck::ValueIslandSpawnsAtMost { bound } => at_most(self.value_island_spawns, *bound),
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
                let observed = self.wire_matches(wire);
                (observed, observed >= 1)
            }
            TraceCheck::NeverDemanded { wire } => {
                let observed = self.wire_matches(wire);
                (observed, observed == 0)
            }
            TraceCheck::DemandedOnce { wire } => {
                let observed = self.wire_matches(wire);
                (observed, observed == 1)
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

/// r[impl machine.scheduler.chaos-kill-oracle]
/// r[impl machine.scheduler.replay-is-semantics]
pub fn run_source(source: &str) -> Result<RatchetReport, RunError> {
    run_source_with_config(source, CompilerConfig::default())
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
    mut cache: LoweringCache,
) -> Result<PreparedRun, RunError> {
    let compilation = Compiler::with_config(config).compile(source)?;

    // Lower every island the lanes will demand so its native code is compiled
    // and cached now, before execution. `get_or_lower` keys on canonical recipe
    // content, so these exact entries are reused as cache hits during execution.
    for test in &compilation.module.tests {
        // A conditional generator runs as its own verified task island.
        if test.generator.has_conditional_sites() {
            let generator = compilation.module.generator_task_island(test)?;
            cache.get_or_lower(&generator)?;
        }
        // Every value-check island. A trace site reads the frozen counter
        // snapshot and lowers nothing, so it is skipped here.
        let partitioned = compilation.module.try_partition_test(test)?;
        for value in &partitioned.values {
            cache.get_or_lower(&value.island)?;
        }
        for site in &partitioned.sites {
            if let PartitionedRecipe::Value { island } = &site.recipe {
                cache.get_or_lower(&partitioned.islands[*island])?;
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
        self.execute_with_observer(|_| {})
    }

    /// Execute with lifecycle observations made while each lane's runtime is
    /// still live. This is intentionally a production-path seam: observers see
    /// no per-iteration callbacks and cannot hide retained execution state.
    pub fn execute_with_observer(
        mut self,
        mut observe: impl FnMut(ExecutionPhase),
    ) -> Result<RatchetReport, RunError> {
        let plain = run_lane(
            &self.compilation.module,
            &mut self.cache,
            ChaosPolicy::default(),
            ExecutionPhase::PlainRuntimeReady,
            ExecutionPhase::PlainCompleted,
            &mut observe,
        )?;
        let chaos = run_lane(
            &self.compilation.module,
            &mut self.cache,
            ChaosPolicy {
                kill_first_running_task: true,
            },
            ExecutionPhase::ChaosRuntimeReady,
            ExecutionPhase::ChaosCompleted,
            &mut observe,
        )?;
        Ok(RatchetReport {
            warnings: self.compilation.warnings,
            plain,
            chaos,
            lowering_cache: self.cache.counters(),
        })
    }
}

/// Evaluate one value-check island as an ordinary pure demand and record its
/// provenance-keyed outcome. Provenance is the site's stable `YieldSiteId`, and
/// with no dynamic keys the demand location is byte-identical to the historical
/// flat check location.
fn evaluate_value_site(
    runtime: &mut Runtime<EventLog>,
    cache: &mut LoweringCache,
    test_name: &str,
    island: &crate::vir::Island,
    published_values: &BTreeMap<ValueIslandId, Evaluation>,
    site: u32,
    chaos: ChaosPolicy,
) -> Result<CheckRun, RunError> {
    let lowered = cache.get_or_lower(island)?;
    let attribution = attribution_for(island);
    let provenance = ProvenanceKey::site(site);
    let location = Location::for_test_provenance(test_name, site, &provenance.dynamic_keys);
    let arguments = island
        .value_inputs
        .iter()
        .map(|value| {
            published_values
                .get(value)
                .cloned()
                .expect("partitioned value input was published")
        })
        .collect::<Vec<_>>();
    // Wire inputs are demanded pure values this check consumes; each is already
    // published by its callee island. Supply their scalar results as ready
    // awaited words so an unconditional wire await resumes without parking.
    let awaited = island
        .wire_inputs
        .iter()
        .map(|value| {
            let evaluation = published_values
                .get(value)
                .expect("wire input was published before its consumer");
            runtime
                .scalar_word(evaluation.handle)
                .expect("a wire callee publishes a scalar result word")
        })
        .collect::<Vec<_>>();
    let evaluation: Evaluation = runtime.evaluate(
        island.id,
        &location,
        lowered,
        &attribution,
        &arguments,
        &awaited,
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
    })
}

fn run_lane(
    module: &crate::vir::Module,
    cache: &mut LoweringCache,
    chaos: ChaosPolicy,
    ready_phase: ExecutionPhase,
    completed_phase: ExecutionPhase,
    observe: &mut impl FnMut(ExecutionPhase),
) -> Result<SuiteRun, RunError> {
    let mut runtime = Runtime::new(EventLog::default());
    observe(ready_phase);
    let mut checks = Vec::new();
    let mut values = Vec::new();
    // Trace checks are deferred until every selected value check completes; they
    // are evaluated once, together, against the frozen completed-run snapshot.
    let mut deferred_traces: Vec<(ProvenanceKey, TraceCheck)> = Vec::new();
    let mut kill_available = chaos.kill_first_running_task;

    for test in &module.tests {
        // A branch-dependent generator runs its real Match/If control through one
        // verified generator task that publishes only the taken sites; each taken
        // descriptor then becomes an ordinary pure check demand. A flat generator
        // keeps the historical unconditional path with byte-identical behaviour.
        let taken = if test.generator.has_conditional_sites() {
            let generator = module.generator_task_island(test)?;
            let lowered = cache.get_or_lower(&generator)?;
            let attribution = attribution_for(&generator);
            let outcome = runtime.drive_generator(
                generator.id,
                lowered,
                &attribution,
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

        let partitioned = module.try_partition_test(test)?;
        let mut published_values = BTreeMap::new();
        for value in &partitioned.values {
            let lowered = cache.get_or_lower(&value.island)?;
            let attribution = attribution_for(&value.island);
            let location = Location::for_test_value(&partitioned.name, &value.id.stable_segment());
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
            let evaluation = runtime.evaluate(
                value.island.id,
                &location,
                lowered,
                &attribution,
                &arguments,
                &[],
                ChaosPolicy {
                    kill_first_running_task: kill_available,
                },
            )?;
            kill_available = false;
            // A hoisted wire island that actually computed (a memo miss) records
            // one realized demand for its described invocation. A memoized
            // re-demand of the same recipe+argument is a hit and adds nothing, so
            // repeated identical wires memoize to a single realization.
            if let Some(wire) = &value.wire
                && evaluation.memo == MemoVerdict::Miss
            {
                let arguments = wire.arguments.iter().map(wire_arg_identity).collect();
                runtime.record_wire_demand(wire.function, arguments);
            }
            values.push(ValuePublicationRun {
                provenance: value.id,
                identity: evaluation.identity,
                failure: evaluation.failure.clone(),
            });
            published_values.insert(value.id, evaluation);
        }
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
                            checks.push(evaluate_value_site(
                                &mut runtime,
                                cache,
                                &partitioned.name,
                                &partitioned.islands[*island],
                                &published_values,
                                site,
                                ChaosPolicy::default(),
                            )?);
                        }
                        PartitionedRecipe::Trace(trace) => {
                            deferred_traces.push((ProvenanceKey::site(site), trace.clone()));
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
                            checks.push(evaluate_value_site(
                                &mut runtime,
                                cache,
                                &partitioned.name,
                                &partitioned.islands[*island],
                                &published_values,
                                site,
                                ChaosPolicy {
                                    kill_first_running_task: kill_available,
                                },
                            )?);
                            kill_available = false;
                        }
                        PartitionedRecipe::Trace(trace) => {
                            deferred_traces.push((ProvenanceKey::site(site), trace.clone()));
                        }
                    }
                }
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
        store_interns: counters.store_interns,
        value_island_spawns: counters.value_island_spawns,
        successful_aggregate_freezes: counters.successful_aggregate_freezes,
        active_molten_selections: counters.active_molten_selections,
        forced_copy_selections: counters.forced_copy_selections,
        framed_bytes: counters.framed_bytes,
        peak_molten_bytes: counters.peak_molten_bytes,
        peak_molten_nodes: counters.peak_molten_nodes,
        function_calls,
        wire_demands: runtime
            .realized_wire_demands()
            .iter()
            .map(|(function, arguments)| RealizedWireDemand {
                function: *function,
                arguments: arguments.clone(),
            })
            .collect(),
    };
    for (provenance, trace) in deferred_traces {
        checks.push(snapshot.evaluate(provenance, trace));
    }

    let receipt_count = runtime.receipts().count() as u64;
    let all_demands_ready = runtime
        .demands()
        .all(|demand| demand.state == DemandState::Ready);
    let all_tasks_terminal = runtime
        .tasks()
        .all(|task| matches!(task.state, TaskState::Completed | TaskState::Discarded));
    observe(completed_phase);
    let events = runtime.into_sink().into_events();
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
