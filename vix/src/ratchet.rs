//! Production-path ratchet runner: source -> generated AST -> VIR -> Weavy.

use crate::compiler::Compiler;
use crate::diagnostic::Diagnostics;
use crate::lowering::{LoweringCache, LoweringCacheCounters, LoweringError, attribution_for};
use crate::runtime::{
    ChaosPolicy, Counters, DemandState, Evaluation, Event, EventLog, FailureContext, FailureValue,
    Location, MachineError, Runtime, TaskState, ValueId,
};
use crate::vir::{GeneratorBody, GeneratorStep, IslandId, Test};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    Diagnostics(Diagnostics),
    Machine(Box<MachineError>),
    /// A test compiled into a generator whose yield sites are branch-dependent
    /// (owned by a taken control arm). The static runner can only publish a flat
    /// list of unconditional top-level checks; folding conditional codata into
    /// demand-driven descriptors is a later runtime checkpoint. This is the
    /// explicit typed seam, not a silent partial run.
    UnsupportedGenerator {
        test: String,
    },
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

impl RatchetReport {
    #[must_use]
    pub fn agrees(&self) -> bool {
        self.plain.checks == self.chaos.checks
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
        run_generator_body(
            module,
            test,
            &test.generator,
            cache,
            &mut runtime,
            &mut checks,
            &mut kill_available,
            &mut 0,
        )?;
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

fn run_generator_body(
    module: &crate::vir::Module,
    test: &Test,
    body: &GeneratorBody,
    cache: &mut LoweringCache,
    runtime: &mut Runtime<EventLog>,
    checks: &mut Vec<CheckRun>,
    kill_available: &mut bool,
    next_island: &mut u32,
) -> Result<(), RunError> {
    for step in &body.steps {
        match step {
            GeneratorStep::Yield(site) => {
                let island = module.partition_test_output(test, site.check, IslandId(*next_island));
                *next_island += 1;
                let evaluation =
                    evaluate_generator_island(&island, test, cache, runtime, kill_available)?;
                checks.push(CheckRun {
                    identity: evaluation.identity,
                    passed: evaluation.passed,
                    failure: evaluation.failure,
                    failure_context: evaluation.failure_context,
                });
            }
            GeneratorStep::Match { arms, .. } => {
                let mut selected = None;
                for arm in arms {
                    let island =
                        module.partition_test_output(test, arm.condition, IslandId(*next_island));
                    *next_island += 1;
                    let evaluation =
                        evaluate_generator_island(&island, test, cache, runtime, kill_available)?;
                    if evaluation.passed {
                        if selected.replace(&arm.body).is_some() {
                            return Err(RunError::Diagnostics(
                                crate::diagnostic::Diagnostics::one(
                                    crate::diagnostic::Diagnostic::unsupported(
                                        test.generator.steps.first().map_or(
                                            crate::support::Span { start: 0, end: 0 },
                                            |_| crate::support::Span { start: 0, end: 0 },
                                        ),
                                        "generator match selected more than one arm",
                                    ),
                                ),
                            ));
                        }
                    }
                }
                let selected = selected.ok_or_else(|| {
                    RunError::Diagnostics(crate::diagnostic::Diagnostics::one(
                        crate::diagnostic::Diagnostic::unsupported(
                            crate::support::Span { start: 0, end: 0 },
                            "generator match selected no arm",
                        ),
                    ))
                })?;
                run_generator_body(
                    module,
                    test,
                    selected,
                    cache,
                    runtime,
                    checks,
                    kill_available,
                    next_island,
                )?;
            }
            GeneratorStep::If {
                condition,
                consequent,
                alternative,
            } => {
                let island = module.partition_test_output(test, *condition, IslandId(*next_island));
                *next_island += 1;
                let evaluation =
                    evaluate_generator_island(&island, test, cache, runtime, kill_available)?;
                let selected = if evaluation.passed {
                    consequent
                } else {
                    alternative
                };
                run_generator_body(
                    module,
                    test,
                    selected,
                    cache,
                    runtime,
                    checks,
                    kill_available,
                    next_island,
                )?;
            }
        }
    }
    Ok(())
}

fn evaluate_generator_island(
    island: &crate::vir::Island,
    test: &Test,
    cache: &mut LoweringCache,
    runtime: &mut Runtime<EventLog>,
    kill_available: &mut bool,
) -> Result<Evaluation, RunError> {
    let lowered = cache.get_or_lower(island)?;
    let attribution = attribution_for(island);
    let location = Location::for_test_island(&test.name, island.id.0);
    let evaluation = runtime.evaluate(
        island.id,
        &location,
        lowered,
        &attribution,
        ChaosPolicy {
            kill_first_running_task: *kill_available,
        },
    )?;
    *kill_available = false;
    Ok(evaluation)
}
