use std::collections::BTreeMap;

use weavy::task::{FnId, Task, TaskEvent as WeavyTaskEvent, TaskStep};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::lowering::{
    LoweringArtifact, LoweringAttribution, OUTCOME_INDEX_OUT_OF_RANGE, OUTCOME_MALFORMED_ARRAY,
    OUTCOME_OK,
};
use crate::support::Span;
use crate::vir::IslandId;

use super::FrameSlot;
use super::identity::{
    DemandKey, DemandPreimage, Location, LocationId, SchemaId, ValueBody, ValueId,
};
use super::model::{
    DemandRecord, DemandState, MemoVerdict, Receipt, TaskId, TaskRecord, TaskState,
};
use super::observe::{Counters, Event, EventKind, EventSink, SafePointClass};
use super::store::{Handle, Interned, Store, StoreEntry};

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
    ) -> Result<Evaluation, Diagnostics> {
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
                .ok_or_else(|| runtime_invariant("memo handle missing from store"))?
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

            let mut task = Task::spawn(&lowered.program, FnId(0));
            for &(slot, handle) in &constants {
                if !self.store.write_task_handle(&mut task, slot, handle) {
                    return Err(runtime_invariant(
                        "constant handle missing while initializing task frame",
                    ));
                }
            }
            let step = self.store.with_value_memories(|value_memories| {
                task.run_hosted_with_value_memories(
                    &lowered.program,
                    &mut [],
                    &[],
                    &mut [],
                    value_memories,
                )
            });
            match step {
                TaskStep::Done => {}
                TaskStep::Yielded => {
                    return Err(runtime_invariant(
                        "pure island unexpectedly yielded to the host",
                    ));
                }
                TaskStep::Parked { .. } => {
                    self.transition_task(task_id, TaskState::Parked)?;
                    return Err(runtime_invariant("pure island unexpectedly parked"));
                }
            }
            for event in &task.trace {
                self.emit_weavy(task_id, *event, attribution)?;
            }
            let passed = match self.island_outcome(&task, lowered, attribution) {
                Ok(passed) => passed,
                Err(failure) => {
                    self.transition_task(task_id, TaskState::Completed)?;
                    self.transition_demand(lowered.demand_key, DemandState::Failed)?;
                    return Err(failure);
                }
            };
            let interned = self.store.intern_realized(
                SchemaId::named("vix.Check.v1"),
                ValueBody::flat(&[u8::from(passed)]),
            );
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

    /// Read the island's return region. A fault-ABI island returns a status
    /// word beside its value; a nonzero status is a typed demand failure
    /// attributed to the source construct that raised it.
    ///
    /// r[impl lang.collection.array-index]
    fn island_outcome(
        &self,
        task: &Task,
        lowered: &LoweringArtifact,
        attribution: &LoweringAttribution,
    ) -> Result<bool, Diagnostics> {
        if !lowered.fault_abi {
            return Ok(task.result_i64() != 0);
        }
        let word = |index: usize| -> Result<i64, Diagnostics> {
            let start = index * 8;
            task.result
                .get(start..start + 8)
                .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
                .map(i64::from_le_bytes)
                .ok_or_else(|| runtime_invariant("island outcome region is truncated"))
        };
        match word(0)? {
            OUTCOME_OK => Ok(word(3)? != 0),
            OUTCOME_INDEX_OUT_OF_RANGE => Err(Diagnostics::one(Diagnostic {
                code: DiagnosticCode::IndexOutOfRange,
                primary: fault_span(task, attribution),
                labels: Vec::new(),
                payload: DiagnosticPayload::IndexOutOfRange {
                    index: word(1)?,
                    length: word(2)?,
                },
            })),
            OUTCOME_MALFORMED_ARRAY => Err(runtime_invariant(
                "store-backed array payload does not answer its own length",
            )),
            _ => Err(runtime_invariant("island published an unknown outcome status")),
        }
    }

    fn materialize_constants(&mut self, lowered: &LoweringArtifact) -> Vec<(FrameSlot, Handle)> {
        lowered
            .constants
            .iter()
            .map(|constant| {
                let interned = self.store.intern_realized(
                    constant.schema,
                    ValueBody::new(&constant.identity_preimage, &constant.memory),
                );
                self.observe_interned(interned);
                (constant.slot, interned.handle)
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

    fn transition_demand(&mut self, key: DemandKey, to: DemandState) -> Result<(), Diagnostics> {
        let from = self
            .demands
            .get(&key)
            .ok_or_else(|| runtime_invariant("demand transition without a demand record"))?
            .state;
        self.demands.get_mut(&key).expect("checked above").state = to;
        self.emit(EventKind::DemandTransition { key, from, to });
        Ok(())
    }

    fn transition_task(&mut self, id: TaskId, to: TaskState) -> Result<(), Diagnostics> {
        let from = self
            .tasks
            .get(&id)
            .ok_or_else(|| runtime_invariant("task transition without a task record"))?
            .state;
        self.tasks.get_mut(&id).expect("checked above").state = to;
        self.emit(EventKind::TaskTransition { task: id, from, to });
        Ok(())
    }

    fn emit_weavy(
        &mut self,
        task: TaskId,
        event: WeavyTaskEvent,
        attribution: &LoweringAttribution,
    ) -> Result<(), Diagnostics> {
        let kind = match event {
            WeavyTaskEvent::FrameEntered(function) => EventKind::WeavyFrameEntered {
                task,
                function: attribution
                    .function_for_frame(function.0)
                    .ok_or_else(|| runtime_invariant("Weavy frame has no Vix attribution"))?,
            },
            WeavyTaskEvent::FrameExited(function) => EventKind::WeavyFrameExited {
                task,
                function: attribution
                    .function_for_frame(function.0)
                    .ok_or_else(|| runtime_invariant("Weavy frame has no Vix attribution"))?,
            },
            WeavyTaskEvent::Parked { input } => EventKind::WeavyParked { task, input },
            WeavyTaskEvent::Resumed => EventKind::WeavyResumed { task },
            WeavyTaskEvent::Mark(id) => {
                let source = attribution
                    .source_for_trace(id)
                    .ok_or_else(|| runtime_invariant("Weavy trace has no Vix attribution"))?;
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

/// A faulting island abandons execution at the raising node, so the last trace
/// mark it emitted is that node. Attribution turns it back into a source span.
fn fault_span(task: &Task, attribution: &LoweringAttribution) -> Span {
    task.trace
        .iter()
        .rev()
        .find_map(|event| match event {
            WeavyTaskEvent::Mark(id) => attribution.source_for_trace(*id),
            _ => None,
        })
        .map_or(Span { start: 0, end: 0 }, |entry| entry.span)
}

fn runtime_invariant(detail: &str) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::RuntimeInvariant,
        primary: Span { start: 0, end: 0 },
        labels: Vec::new(),
        payload: DiagnosticPayload::Invariant {
            detail: detail.to_owned(),
        },
    })
}
