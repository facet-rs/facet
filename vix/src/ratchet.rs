//! Production-path ratchet runner: source -> generated AST -> VIR -> Weavy.

use std::collections::{BTreeMap, BTreeSet};

use crate::compiler::Compiler;
use crate::diagnostic::Diagnostics;
use crate::lowering::{LoweringCache, LoweringCacheCounters, LoweringError, attribution_for};
use crate::runtime::{
    ChaosPolicy, Counters, DemandState, Evaluation, Event, EventLog, FailureContext, FailureValue,
    GeneratorOutcome, Location, MachineError, Runtime, TaskState, ValueId,
};

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
        failure: FailureValue,
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
    pub identity: ValueId,
    pub passed: bool,
    pub failure: Option<FailureValue>,
    pub failure_context: Option<FailureContext>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct SuiteRun {
    pub checks: Vec<CheckRun>,
    pub counters: Counters,
    pub events: Vec<Event>,
    pub receipt_count: u64,
    pub all_demands_ready: bool,
    pub all_tasks_terminal: bool,
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
}

impl RatchetReport {
    #[must_use]
    pub fn agrees(&self) -> bool {
        self.plain.check_family() == self.chaos.check_family()
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

/// Run every declared test twice. The chaos lane discards the first running
/// task at an edge safepoint and must publish the same identities.
///
/// r[impl machine.scheduler.chaos-kill-oracle]
/// r[impl machine.scheduler.replay-is-semantics]
pub fn run_source(source: &str) -> Result<RatchetReport, RunError> {
    let compilation = Compiler::new().compile(source)?;

    let mut cache = LoweringCache::default();

    let plain = run_lane(&compilation.module, &mut cache, ChaosPolicy::default())?;
    let chaos = run_lane(
        &compilation.module,
        &mut cache,
        ChaosPolicy {
            kill_first_running_task: true,
        },
    )?;
    Ok(RatchetReport {
        warnings: compilation.warnings,
        plain,
        chaos,
        lowering_cache: cache.counters(),
    })
}

fn run_lane(
    module: &crate::vir::Module,
    cache: &mut LoweringCache,
    chaos: ChaosPolicy,
) -> Result<SuiteRun, RunError> {
    let mut runtime = Runtime::new(EventLog::default());
    let mut checks = Vec::new();
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

        let partitioned = module.partition_test(test);
        match taken {
            // Taken generator sites, in publication order, evaluated as ordinary
            // pure check demands and keyed by provenance.
            Some(sites) => {
                for (site, raw) in sites {
                    let island = partitioned.islands.get(site as usize).ok_or_else(|| {
                        RunError::UnknownSiteKey {
                            test: test.name.clone(),
                            site: raw,
                        }
                    })?;
                    let lowered = cache.get_or_lower(island)?;
                    let attribution = attribution_for(island);
                    // Zero-dynamic-key base case: the empty dynamic tail makes this
                    // location byte-identical to the flat check location.
                    let provenance = ProvenanceKey::site(site);
                    let location = Location::for_test_provenance(
                        &partitioned.name,
                        site,
                        &provenance.dynamic_keys,
                    );
                    let evaluation: Evaluation = runtime.evaluate(
                        island.id,
                        &location,
                        lowered,
                        &attribution,
                        ChaosPolicy::default(),
                    )?;
                    checks.push(CheckRun {
                        provenance,
                        identity: evaluation.identity,
                        passed: evaluation.passed,
                        failure: evaluation.failure,
                        failure_context: evaluation.failure_context,
                    });
                }
            }
            // Flat generator: every top-level site publishes unconditionally, so
            // the island index is its provenance selector.
            None => {
                for island in &partitioned.islands {
                    let lowered = cache.get_or_lower(island)?;
                    let attribution = attribution_for(island);
                    let location = Location::for_test_island(&partitioned.name, island.id.0);
                    let evaluation: Evaluation = runtime.evaluate(
                        island.id,
                        &location,
                        lowered,
                        &attribution,
                        ChaosPolicy {
                            kill_first_running_task: kill_available,
                        },
                    )?;
                    kill_available = false;
                    checks.push(CheckRun {
                        provenance: ProvenanceKey::site(island.id.0),
                        identity: evaluation.identity,
                        passed: evaluation.passed,
                        failure: evaluation.failure,
                        failure_context: evaluation.failure_context,
                    });
                }
            }
        }
    }

    let counters = runtime.counters();
    let receipt_count = runtime.receipts().count() as u64;
    let all_demands_ready = runtime
        .demands()
        .all(|demand| demand.state == DemandState::Ready);
    let all_tasks_terminal = runtime
        .tasks()
        .all(|task| matches!(task.state, TaskState::Completed | TaskState::Discarded));
    let events = runtime.into_sink().into_events();
    Ok(SuiteRun {
        checks,
        counters,
        events,
        receipt_count,
        all_demands_ready,
        all_tasks_terminal,
    })
}
