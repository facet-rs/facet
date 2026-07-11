use std::collections::BTreeMap;

use weavy::exec::{FallbackReason, FaultSite, LaneKind, TaskFault};
use weavy::task::{FnId, TaskEvent as WeavyTaskEvent, TaskStep};

use crate::lowering::{LoweringArtifact, LoweringAttribution};
use crate::vir::IslandId;

use super::identity::{DemandKey, DemandPreimage, Location, LocationId, SchemaId, ValueId};
use super::model::{
    DemandRecord, DemandState, MemoVerdict, Receipt, TaskId, TaskRecord, TaskState,
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

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Evaluation {
    pub handle: Handle,
    pub identity: ValueId,
    pub passed: bool,
    pub memo: MemoVerdict,
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
    ) -> Result<Evaluation, MachineError> {
        self.emit(EventKind::Demanded {
            key: lowered.demand_key,
        });

        if let Some(entry) = self.memo.get(&location.id)
            && entry.location == *location
            && entry.key == lowered.demand_key
            && entry.preimage == lowered.demand_preimage
        {
            let handle = entry.result;
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
            let passed = self
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
                    return Err(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )?);
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
                        return Err(self.terminate_machine_fault(
                            task_id,
                            lowered.demand_key,
                            error,
                        )?);
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
                    return Err(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )?);
                }
            }
            let step = match self.store.with_value_memories(|value_memories| {
                task.drive_hosted_with_value_memories(&mut [], &[], &mut [], value_memories)
            }) {
                Ok(step) => step,
                Err(fault) => {
                    let error = self.task_fault(
                        MachineOperation::Drive,
                        fault,
                        lowered,
                        attribution,
                        self.root_attribution(lowered, attribution),
                    );
                    return Err(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )?);
                }
            };
            match step {
                TaskStep::Done => {}
                TaskStep::Yielded => {
                    let error = MachineError::runtime(
                        MachineOperation::Drive,
                        RuntimeFault::PureIslandYielded,
                        self.root_attribution(lowered, attribution),
                        Some(lowered.demand_key),
                    );
                    return Err(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )?);
                }
                TaskStep::Parked { input } => {
                    let error = MachineError::runtime(
                        MachineOperation::Drive,
                        RuntimeFault::PureIslandParked { input },
                        self.root_attribution(lowered, attribution),
                        Some(lowered.demand_key),
                    );
                    return Err(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )?);
                }
            }
            for event in task.trace() {
                if let Err(error) =
                    self.emit_weavy(task_id, *event, attribution, lowered.demand_key)
                {
                    return Err(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )?);
                }
            }
            let passed = match task.result_i64() {
                Ok(result) => result != 0,
                Err(fault) => {
                    let error = self.task_fault(
                        MachineOperation::Result,
                        fault,
                        lowered,
                        attribution,
                        self.root_attribution(lowered, attribution),
                    );
                    return Err(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )?);
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
            });
        }
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

    fn transition_demand(&mut self, key: DemandKey, to: DemandState) -> Result<(), MachineError> {
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

    fn transition_task(&mut self, id: TaskId, to: TaskState) -> Result<(), MachineError> {
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
    ) -> Result<(), MachineError> {
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
    ) -> Result<MachineError, MachineError> {
        self.transition_task(task, TaskState::Failed)?;
        self.transition_demand(demand, DemandState::Failed)?;
        self.emit(EventKind::MachineFailed {
            task,
            key: demand,
            operation: error.operation,
        });
        Ok(error)
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

    fn root_attribution(
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
        | TaskFault::InvalidEnumSelector { site, .. }
        | TaskFault::EnumProjectionMismatch { site, .. } => Some(site),
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
        | TaskFault::DriveTableLength { .. }
        | TaskFault::NativeFaultExit { .. }
        | TaskFault::InvalidFaultSite { .. }
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
    use weavy::ValueShapeRef;
    use weavy::exec::TaskFault;
    use weavy::task::Op;

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
                op: artifact.program().fns[0].code[pc].clone(),
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
                MachineCause::Task(TaskFault::PoisonedResult { .. })
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
    fn machine_fault_marks_task_and_demand_failed_without_a_memo() {
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
            let returned = runtime
                .terminate_machine_fault(task, artifact.demand_key, error.clone())
                .expect("recorded task and demand transition to Failed");
            assert_eq!(returned, error);
            assert_eq!(runtime.tasks[&task].state, TaskState::Failed);
            assert_eq!(
                runtime.demands[&artifact.demand_key].state,
                DemandState::Failed
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
}
