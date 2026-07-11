use std::collections::BTreeMap;

use weavy::exec::{FallbackReason, FaultSite, LaneKind, TaskFault};
use weavy::task::{FnId, TaskEvent as WeavyTaskEvent, TaskStep};

use crate::lowering::{LoweringArtifact, LoweringAttribution};
use crate::vir::IslandId;

use super::identity::{DemandKey, DemandPreimage, Location, LocationId, SchemaId, ValueId};
use super::model::{
    DemandRecord, DemandState, FailureContext, FailureValue, MemoVerdict, Receipt, TaskId,
    TaskRecord, TaskState,
};
use super::observe::{
    Counters, Event, EventKind, EventSink, ExecutionFacts, ExecutionFallbackFact,
    ExecutionLaneFact, SafePointClass,
};
use super::store::{Handle, Interned, Store, StoreEntry};
use super::{MachineAttribution, MachineError, MachineOperation, RuntimeFault};

#[derive(Clone, Debug)]
struct MemoEntry {
    location: Location,
    key: DemandKey,
    preimage: DemandPreimage,
    result: Handle,
    receipt: Option<Receipt>,
}

#[derive(facet::Facet, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChaosPolicy {
    pub kill_first_running_task: bool,
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
    demands: BTreeMap<DemandKey, DemandRecord>,
    tasks: BTreeMap<TaskId, TaskRecord>,
    counters: Counters,
    next_task: u64,
}

impl<S: EventSink> Runtime<S> {
    #[must_use]
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            sequence: 0,
            store: Store::default(),
            memo: BTreeMap::new(),
            demands: BTreeMap::new(),
            tasks: BTreeMap::new(),
            counters: Counters::default(),
            next_task: 0,
        }
    }

    pub fn evaluate(
        &mut self,
        island: IslandId,
        location: &Location,
        lowered: &LoweringArtifact,
        attribution: &LoweringAttribution,
        chaos: ChaosPolicy,
    ) -> Result<Evaluation, Box<MachineError>> {
        self.emit(EventKind::Demanded {
            key: lowered.demand_key,
        });

        if let Some(entry) = self.memo.get(&location.id)
            && entry.location == *location
            && entry.key == lowered.demand_key
            && entry.preimage == lowered.demand_preimage
        {
            let handle = entry.result;
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
                .identity;
            let passed = failure.is_none()
                && self
                    .store
                    .entry(handle)
                    .and_then(StoreEntry::resident_bytes)
                    .is_some_and(|bytes| bytes == [1]);
            self.counters.memo_hits_exact += 1;
            self.emit(EventKind::Memo {
                location: location.id,
                verdict: MemoVerdict::Exact,
                verified: 0,
            });
            return Ok(Evaluation {
                handle,
                identity,
                passed,
                memo: MemoVerdict::Exact,
                failure_context: failure
                    .as_ref()
                    .and_then(|failure| failure_context(failure, lowered, attribution)),
                failure,
            });
        }

        self.counters.memo_misses += 1;
        self.emit(EventKind::Memo {
            location: location.id,
            verdict: MemoVerdict::Miss,
            verified: 0,
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

        let constants = self.materialize_constants(lowered);
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

            let mut task = match lowered.executable().spawn(FnId(0)) {
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
            for (constant, handle) in lowered.constants.iter().zip(constants) {
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
            let step = match self.store.with_value_memories(|value_memories| {
                task.drive_hosted_with_value_memories(&mut [], &[], &mut [], value_memories)
                    .map_err(Box::new)
            }) {
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
            };
            match step {
                TaskStep::Done => {}
                TaskStep::Yielded => {
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
            let passed = match decode_result(&task, lowered) {
                Ok(DecodedResult::Ok(passed)) => passed,
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
                    self.observe_interned(interned);
                    self.memo.insert(
                        location.id,
                        MemoEntry {
                            location: location.clone(),
                            key: lowered.demand_key,
                            preimage: lowered.demand_preimage.clone(),
                            result: interned.handle,
                            receipt: None,
                        },
                    );
                    if let Some(demand) = self.demands.get_mut(&lowered.demand_key) {
                        demand.result = Some(interned.handle);
                    }
                    self.transition_task(task_id, TaskState::Completed)?;
                    self.transition_demand(lowered.demand_key, DemandState::Failed)?;
                    self.emit(EventKind::LanguageFailed {
                        task: task_id,
                        key: lowered.demand_key,
                        failure: failure.clone(),
                    });
                    return Ok(Evaluation {
                        handle: interned.handle,
                        identity: interned.identity,
                        passed: false,
                        memo: MemoVerdict::Miss,
                        failure: Some(failure),
                        failure_context: report_context,
                    });
                }
                Ok(DecodedResult::MissingKey { site }) => {
                    let failure = FailureValue::MissingKey {
                        recipe: lowered.recipe,
                        site,
                    };
                    return self.complete_language_failure(
                        task_id,
                        location,
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
                    return self.complete_language_failure(
                        task_id,
                        location,
                        lowered,
                        attribution,
                        failure,
                    );
                }
                Ok(DecodedResult::MissingDelimiter { site }) => {
                    return self.complete_language_failure(
                        task_id,
                        location,
                        lowered,
                        attribution,
                        FailureValue::MissingDelimiter {
                            recipe: lowered.recipe,
                            site,
                        },
                    );
                }
                Ok(DecodedResult::InvalidInteger { site }) => {
                    return self.complete_language_failure(
                        task_id,
                        location,
                        lowered,
                        attribution,
                        FailureValue::InvalidInteger {
                            recipe: lowered.recipe,
                            site,
                        },
                    );
                }
                Ok(DecodedResult::IntegerOverflow { site }) => {
                    return self.complete_language_failure(
                        task_id,
                        location,
                        lowered,
                        attribution,
                        FailureValue::IntegerOverflow {
                            recipe: lowered.recipe,
                            site,
                        },
                    );
                }
                Err(fault) => {
                    let fallback = result_shape_attribution(
                        &fault,
                        self.output_attribution(lowered, attribution),
                    );
                    let error = self.task_fault(
                        MachineOperation::Result,
                        *fault,
                        lowered,
                        attribution,
                        fallback,
                    );
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
            };
            let interned = self
                .store
                .intern_realized(SchemaId::named("vix.Check.v1"), &[u8::from(passed)]);
            self.observe_interned(interned);

            self.memo.insert(
                location.id,
                MemoEntry {
                    location: location.clone(),
                    key: lowered.demand_key,
                    preimage: lowered.demand_preimage.clone(),
                    result: interned.handle,
                    receipt: None,
                },
            );
            if let Some(demand) = self.demands.get_mut(&lowered.demand_key) {
                demand.result = Some(interned.handle);
            }
            self.transition_task(task_id, TaskState::Completed)?;
            self.transition_demand(lowered.demand_key, DemandState::Ready)?;
            self.emit(EventKind::Completed {
                key: lowered.demand_key,
                identity: interned.identity,
            });
            return Ok(Evaluation {
                handle: interned.handle,
                identity: interned.identity,
                passed,
                memo: MemoVerdict::Miss,
                failure: None,
                failure_context: None,
            });
        }
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
        chaos: ChaosPolicy,
    ) -> Result<GeneratorOutcome, Box<MachineError>> {
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
        let constants = self.materialize_constants(lowered);
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

            let mut task = match lowered.executable().spawn(FnId(0)) {
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
            let step = match self.store.with_value_memories(|value_memories| {
                task.drive_hosted_with_value_memories(&mut [], &[], &mut [], value_memories)
                    .map_err(Box::new)
            }) {
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
            };
            match step {
                TaskStep::Done => {}
                TaskStep::Yielded => {
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
                Ok(DecodedResult::Ok(_)) => {
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
        lowered: &LoweringArtifact,
        attribution: &LoweringAttribution,
        failure: FailureValue,
    ) -> Result<GeneratorOutcome, Box<MachineError>> {
        let context = failure_context(&failure, lowered, attribution);
        let interned = self.store.intern_failure(failure.clone(), &[]);
        self.observe_interned(interned);
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
                    .intern_realized(constant.store_schema, &constant.bytes);
                self.observe_interned(interned);
                interned.handle
            })
            .collect()
    }

    fn observe_interned(&mut self, interned: Interned) {
        self.counters.bytes_hashed += interned.bytes_hashed;
        if interned.deduped {
            self.counters.store_dedups += 1;
        } else {
            self.counters.store_interns += 1;
        }
        self.emit(EventKind::StoreAlloc {
            identity: interned.identity,
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
        lowered: &LoweringArtifact,
        attribution: &LoweringAttribution,
        failure: FailureValue,
    ) -> Result<Evaluation, Box<MachineError>> {
        let report_context = failure_context(&failure, lowered, attribution);
        let interned = self.store.intern_failure(failure.clone(), &[]);
        self.observe_interned(interned);
        self.memo.insert(
            location.id,
            MemoEntry {
                location: location.clone(),
                key: lowered.demand_key,
                preimage: lowered.demand_preimage.clone(),
                result: interned.handle,
                receipt: None,
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
        lowered: &LoweringArtifact,
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

    pub fn demands(&self) -> impl Iterator<Item = &DemandRecord> {
        self.demands.values()
    }

    pub fn tasks(&self) -> impl Iterator<Item = &TaskRecord> {
        self.tasks.values()
    }

    pub fn receipts(&self) -> impl Iterator<Item = &Receipt> {
        self.memo
            .values()
            .filter_map(|entry| entry.receipt.as_ref())
    }

    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }

    #[must_use]
    pub fn sink(&self) -> &S {
        &self.sink
    }

    #[must_use]
    pub fn into_sink(self) -> S {
        self.sink
    }
}

fn failure_context(
    failure: &FailureValue,
    lowered: &LoweringArtifact,
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
        FailureValue::IndexOutOfBounds { .. }
        | FailureValue::MissingKey { .. }
        | FailureValue::DuplicateKey { .. }
        | FailureValue::MissingDelimiter { .. }
        | FailureValue::InvalidInteger { .. }
        | FailureValue::IntegerOverflow { .. } => None,
    }
}

enum DecodedResult {
    Ok(bool),
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
    task: &weavy::exec::ExecTask<'_>,
    lowered: &LoweringArtifact,
) -> Result<DecodedResult, Box<TaskFault>> {
    let Some(abi) = &lowered.array_outcome else {
        return Ok(task
            .result_i64()
            .map(|result| DecodedResult::Ok(result != 0))?);
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
        return Ok(DecodedResult::Ok(
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
        | TaskFault::UnresidentPathJoinOperand { site, .. }
        | TaskFault::PathJoinAllocationFailed { site }
        | TaskFault::PublicationAllocationFailed { site }
        | TaskFault::InvalidEnumSelector { site, .. }
        | TaskFault::EnumProjectionMismatch { site, .. }
        | TaskFault::InvalidArrayStatus { site, .. }
        | TaskFault::InvalidStringStatus { site, .. }
        | TaskFault::InvalidOrderedStatus { site, .. } => Some(site),
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
        | TaskFault::UnresidentPathJoinOperand { .. }
        | TaskFault::PathJoinAllocationFailed { .. }
        | TaskFault::PublicationAllocationFailed { .. }
        | TaskFault::PublicationIndexOutOfRange { .. }
        | TaskFault::InvalidEnumSelector { .. }
        | TaskFault::EnumProjectionMismatch { .. }
        | TaskFault::InvalidArrayStatus { .. }
        | TaskFault::InvalidStringStatus { .. }
        | TaskFault::InvalidOrderedStatus { .. }
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
    use crate::runtime::{EventLog, MachineCause};
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
            let error = runtime.task_fault(
                MachineOperation::Drive,
                TaskFault::DriveTableLength {
                    table: DriveTable::Ready,
                    expected: 1,
                    actual: 0,
                },
                artifact,
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
                artifact,
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
                .get_or_lower(island)
                .expect("first compilation lowers through the verified executable");
            let demand_key = artifact.demand_key;
            let evaluation = runtime
                .evaluate(
                    island.id,
                    &location,
                    artifact,
                    &first_attribution,
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
                .get_or_lower(shifted_island)
                .expect("span-only recompilation reuses the verified artifact");
            runtime
                .evaluate(
                    shifted_island.id,
                    &location,
                    artifact,
                    &shifted_attribution,
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
                    &artifact,
                    attribution,
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
}
