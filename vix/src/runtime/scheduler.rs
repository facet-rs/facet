use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::ops::Deref;

use weavy::exec::{
    FallbackReason, FaultSite, LaneKind, ResolvedTaskValue, StoreHandle, TaskFault,
    TaskStructuralValue, TaskValueResolver,
};
use weavy::task::{FnId, HostFn, TaskEvent as WeavyTaskEvent, TaskStep};

use crate::decode::{self, DecodedValue};
use crate::lowering::{
    DocumentParseCall, LoweringArtifact, LoweringAttribution, ValueInputBinding,
};
use crate::vir::{FunctionId, IslandId, Type, VariantPayload};

use super::identity::{DemandKey, DemandPreimage, Location, LocationId, SchemaId, ValueId};
use super::identity::{FramedField, FramedNode, FramedValue};
use super::model::{
    DemandRecord, DemandState, FailureContext, FailureValue, MemoVerdict, ReadWitness, Receipt,
    TaskId, TaskRecord, TaskState,
};
use super::observe::{
    Counters, Event, EventKind, EventSink, ExecutionFacts, ExecutionFallbackFact,
    ExecutionLaneFact, SafePointClass,
};
use super::store::{FrozenValue, Handle, Interned, Store, StoreEntry};
use super::{MachineAttribution, MachineError, MachineOperation, RuntimeFault};

#[derive(Clone, Debug)]
struct MemoEntry {
    location: Location,
    key: DemandKey,
    preimage: DemandPreimage,
    result: Handle,
    receipt: Option<Receipt>,
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

/// The single generic document host table entry records its request and yields.
/// It does not parse, own a store, or choose a schema: those are scheduler-owned
/// responsibilities performed after Weavy has returned the suspended frame.
#[derive(Clone, Copy, Debug)]
struct DocumentHostRequest {
    plan: usize,
    input: i64,
}

#[derive(Default)]
struct DocumentHostQueue {
    requests: Vec<DocumentHostRequest>,
    fault: Option<String>,
}

impl DocumentHostQueue {
    fn call(&mut self, frame: &mut [u8]) {
        let plan = super::FrameSlot::for_word(0)
            .and_then(|slot| slot.read(frame))
            .and_then(|value| usize::try_from(value).ok());
        let input = super::FrameSlot::for_word(1).and_then(|slot| slot.read(frame));
        match (plan, input) {
            (Some(plan), Some(input)) => self.requests.push(DocumentHostRequest { plan, input }),
            _ => self.fault = Some("invalid document host ABI header".to_owned()),
        }
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChaosPolicy {
    pub kill_first_running_task: bool,
}

/// The inputs an island evaluation consumes: its pre-published shared value
/// arguments (already realized), and its demand wires (unresolved). A wire is
/// resolved lazily — only when the task actually parks on it — through the
/// canonical `DemandPreimage`/memo path, never pre-resolved.
#[derive(Clone, Copy)]
pub struct IslandInputs<'a> {
    pub arguments: &'a [Evaluation],
    pub wires: &'a [WireDemand<'a>],
}

/// One demand wire an island may force: the canonical argument demand the
/// scheduler evaluates through the existing memo machinery when the consuming
/// task parks on the wire's `AwaitWire` input. It carries everything needed to
/// evaluate that argument island — its recipe artifact, cost-model location,
/// realized arguments, and its own nested wires — plus the callee identity used
/// to record the realized dependency. A wire is never evaluated unless the task
/// parks on it.
#[derive(Clone, Copy)]
pub struct WireDemand<'a> {
    pub island: IslandId,
    pub location: &'a Location,
    pub lowered: &'a LoweringArtifact,
    pub attribution: &'a LoweringAttribution,
    pub arguments: &'a [Evaluation],
    pub wires: &'a [WireDemand<'a>],
    pub function: FunctionId,
    /// The canonical scalar argument identities of this invocation, recorded in
    /// the realized-demand log when the wire actually computes (a memo miss).
    /// Empty for a zero-argument callee. A memoized re-force adds no entry, so
    /// the log counts one realization per distinct demand identity.
    pub demand_arguments: &'a [ValueId],
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
    /// One entry per realized wire demand — a callee invocation the memo path
    /// actually computed (a miss that ran), recorded as its callee function and
    /// canonical argument identities. Memoized re-demands add no entry, so this
    /// log counts realizations. It backs the described-wire trace checks and
    /// retains only the callee/argument selectors a descriptor can name.
    wire_demands: Vec<(FunctionId, Vec<ValueId>)>,
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
            wire_demands: Vec::new(),
        }
    }

    /// The frozen log of realized wire demands: each callee invocation the memo
    /// path computed, by callee function and canonical argument identities.
    #[must_use]
    pub fn realized_wire_demands(&self) -> &[(FunctionId, Vec<ValueId>)] {
        &self.wire_demands
    }

    /// Record one realized wire demand — a callee invocation the memo path
    /// actually computed. The runner calls this only on a memo miss, so a
    /// memoized re-demand of the same recipe+argument adds no entry.
    pub fn record_wire_demand(&mut self, function: FunctionId, arguments: Vec<ValueId>) {
        self.wire_demands.push((function, arguments));
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

    pub fn evaluate(
        &mut self,
        island: IslandId,
        location: &Location,
        lowered: &LoweringArtifact,
        attribution: &LoweringAttribution,
        inputs: IslandInputs<'_>,
        chaos: ChaosPolicy,
    ) -> Result<Evaluation, Box<MachineError>> {
        let IslandInputs { arguments, wires } = inputs;
        let invocation = DemandExecution::new(
            lowered,
            arguments.iter().map(|argument| argument.identity).collect(),
        );
        let lowered = &invocation;
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

        // A wire that forces a demand already Running on the stack is a cyclic
        // demand. The demand state machine detects it here — before the record is
        // re-queued — as a typed fault, so a re-entrant wire never recurses
        // forever.
        if self
            .demands
            .get(&lowered.demand_key)
            .is_some_and(|record| record.state == DemandState::Running)
        {
            return Err(Box::new(MachineError::runtime(
                MachineOperation::Drive,
                RuntimeFault::ReentrantDemand {
                    key: lowered.demand_key,
                },
                self.output_attribution(lowered.artifact, attribution),
                Some(lowered.demand_key),
            )));
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

        if let Some(argument) = arguments.iter().find(|argument| argument.failure.is_some()) {
            let failure = argument.failure.clone().expect("selected failed argument");
            self.memo.insert(
                location.id,
                MemoEntry {
                    location: location.clone(),
                    key: lowered.demand_key,
                    preimage: lowered.demand_preimage.clone(),
                    result: argument.handle,
                    receipt: None,
                },
            );
            if let Some(demand) = self.demands.get_mut(&lowered.demand_key) {
                demand.result = Some(argument.handle);
            }
            self.transition_demand(lowered.demand_key, DemandState::Failed)?;
            return Ok(Evaluation {
                handle: argument.handle,
                identity: argument.identity,
                passed: false,
                memo: MemoVerdict::Miss,
                failure: Some(failure),
                failure_context: self.output_attribution(lowered.artifact, attribution).map(
                    |source| FailureContext {
                        function: source.function,
                        node: source.node,
                        span: source.span,
                        demand_chain: vec![lowered.demand_key],
                    },
                ),
            });
        }

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

        let constants = self.materialize_constants(lowered.artifact);
        let mut kill_armed = chaos.kill_first_running_task;
        loop {
            self.counters.scheduler_requests += 1;
            let task_id = self.spawn_task(lowered.demand_key);
            if matches!(
                lowered.output_type,
                Type::Array(_) | Type::Map { .. } | Type::Set(_) | Type::Enum(_)
            ) {
                self.counters.value_island_spawns += 1;
            }
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
            for (binding, argument) in lowered.value_inputs.iter().zip(arguments) {
                if binding.store_schema != argument.identity.schema {
                    let error = MachineError::runtime(
                        MachineOperation::EntryBinding,
                        RuntimeFault::ValueInputSchemaMismatch,
                        None,
                        Some(lowered.demand_key),
                    );
                    return Err(Box::new(self.terminate_machine_fault(
                        task_id,
                        lowered.demand_key,
                        error,
                    )));
                }
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
                    let Some(handle) = self.store.weavy_handle(argument.handle) else {
                        let error = MachineError::runtime(
                            MachineOperation::EntryBinding,
                            RuntimeFault::MissingValueInputStoreHandle,
                            None,
                            Some(lowered.demand_key),
                        );
                        return Err(Box::new(self.terminate_machine_fault(
                            task_id,
                            lowered.demand_key,
                            error,
                        )));
                    };
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
            // Demand wires start unresolved. The task drives until it parks on a
            // wire it actually consumes; only then does the scheduler evaluate
            // that wire's canonical argument demand through the memo path and
            // resume the SAME task. A wire the task never parks on — an untaken
            // branch's argument — is never evaluated, entered, or failed.
            let mut ready = vec![false; wires.len()];
            let mut awaited = vec![0i64; wires.len()];
            let mut document_reads = Vec::new();
            loop {
                let mut document_host_queue = DocumentHostQueue::default();
                let mut document_host = |frame: &mut [u8]| document_host_queue.call(frame);
                let step = match self.store.with_value_memory_overrides(
                    &value_memory_overrides,
                    |value_memories| {
                        let mut hosts: Vec<HostFn<'_>> = if lowered.document_parse_calls.is_empty()
                        {
                            Vec::new()
                        } else {
                            vec![&mut document_host]
                        };
                        task.drive_hosted_with_value_memories(
                            &mut ready,
                            &awaited,
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
                };
                drop(document_host);
                match step {
                    TaskStep::Done => break,
                    TaskStep::Yielded => {
                        let request = match document_host_queue.requests.as_slice() {
                            [request] if document_host_queue.fault.is_none() => *request,
                            _ => {
                                let error = MachineError::runtime(
                                    MachineOperation::Drive,
                                    RuntimeFault::DocumentParseHost {
                                        detail: document_host_queue.fault.unwrap_or_else(|| {
                                            "document host yielded without exactly one request"
                                                .to_owned()
                                        }),
                                    },
                                    None,
                                    Some(lowered.demand_key),
                                );
                                return Err(Box::new(self.terminate_machine_fault(
                                    task_id,
                                    lowered.demand_key,
                                    error,
                                )));
                            }
                        };
                        let Some(plan) = lowered.document_parse_calls.get(request.plan) else {
                            let error = MachineError::runtime(
                                MachineOperation::Drive,
                                RuntimeFault::DocumentParseHost {
                                    detail:
                                        "document host plan is absent from the lowered artifact"
                                            .to_owned(),
                                },
                                None,
                                Some(lowered.demand_key),
                            );
                            return Err(Box::new(self.terminate_machine_fault(
                                task_id,
                                lowered.demand_key,
                                error,
                            )));
                        };
                        if let Err(detail) = self.complete_document_parse(
                            &mut task,
                            plan,
                            request,
                            &mut document_reads,
                        ) {
                            let error = MachineError::runtime(
                                MachineOperation::Drive,
                                RuntimeFault::DocumentParseHost { detail },
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
                    TaskStep::Parked { input } => {
                        // The task has fully returned control — its frame arena is
                        // the owned suspended state, so every task/store/value-
                        // memory borrow is released here. Resolve the wire it parked
                        // on through the canonical DemandPreimage/memo state
                        // machine, then resume the same task.
                        let index = input as usize;
                        let Some(wire) = wires.get(index) else {
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
                        };
                        self.emit(EventKind::WeavyParked {
                            task: task_id,
                            input,
                        });
                        let resolved = self.evaluate(
                            wire.island,
                            wire.location,
                            wire.lowered,
                            wire.attribution,
                            IslandInputs {
                                arguments: wire.arguments,
                                wires: wire.wires,
                            },
                            ChaosPolicy::default(),
                        )?;
                        // A wire that actually computed (a memo miss that ran)
                        // records one realized demand for its described invocation;
                        // a memoized re-force is a hit and records nothing, so
                        // structurally equal forces observe a single realization.
                        if resolved.memo == MemoVerdict::Miss {
                            self.record_wire_demand(wire.function, wire.demand_arguments.to_vec());
                        }
                        if let Some(failure) = resolved.failure {
                            // A demanded argument failed on the language plane;
                            // propagate the typed failure with its authored source
                            // site to the parent demand.
                            self.memo.insert(
                                location.id,
                                MemoEntry {
                                    location: location.clone(),
                                    key: lowered.demand_key,
                                    preimage: lowered.demand_preimage.clone(),
                                    result: resolved.handle,
                                    receipt: None,
                                },
                            );
                            if let Some(demand) = self.demands.get_mut(&lowered.demand_key) {
                                demand.result = Some(resolved.handle);
                            }
                            self.transition_demand(lowered.demand_key, DemandState::Failed)?;
                            return Ok(Evaluation {
                                handle: resolved.handle,
                                identity: resolved.identity,
                                passed: false,
                                memo: MemoVerdict::Miss,
                                failure: Some(failure),
                                failure_context: resolved.failure_context,
                            });
                        }
                        let word = self.scalar_word(resolved.handle).ok_or_else(|| {
                            Box::new(MachineError::runtime(
                                MachineOperation::Drive,
                                RuntimeFault::PureIslandParked { input },
                                None,
                                Some(lowered.demand_key),
                            ))
                        })?;
                        awaited[index] = word;
                        ready[index] = true;
                        self.emit(EventKind::WeavyResumed { task: task_id });
                    }
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
                        .intern_realized(semantic_schema_id(&lowered.output_type), bytes);
                    self.observe_interned(interned);
                    self.memo.insert(
                        location.id,
                        MemoEntry {
                            location: location.clone(),
                            key: lowered.demand_key,
                            preimage: lowered.demand_preimage.clone(),
                            result: interned.handle,
                            receipt: document_receipt(lowered.demand_key, &document_reads),
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
                        passed: true,
                        memo: MemoVerdict::Miss,
                        failure: None,
                        failure_context: None,
                    });
                }
                Ok(DecodedResult::OkValue) => {
                    let realized = match realize_value(&task, lowered.artifact, &self.store) {
                        Ok(realized) => realized,
                        Err(fault) => {
                            let error = self.task_fault(
                                MachineOperation::Result,
                                fault,
                                lowered,
                                attribution,
                                self.output_attribution(lowered.artifact, attribution),
                            );
                            return Err(Box::new(self.terminate_machine_fault(
                                task_id,
                                lowered.demand_key,
                                error,
                            )));
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
                    self.observe_interned(interned);
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
                            key: lowered.demand_key,
                            preimage: lowered.demand_preimage.clone(),
                            result: interned.handle,
                            receipt: document_receipt(lowered.demand_key, &document_reads),
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
                        passed: true,
                        memo: MemoVerdict::Miss,
                        failure: None,
                        failure_context: None,
                    });
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
                Ok(DecodedResult::IntDivisionByZero { site }) => {
                    return self.complete_language_failure(
                        task_id,
                        location,
                        lowered,
                        attribution,
                        FailureValue::DivisionByZero {
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
                    receipt: document_receipt(lowered.demand_key, &document_reads),
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
        arguments: &[Evaluation],
        chaos: ChaosPolicy,
    ) -> Result<GeneratorOutcome, Box<MachineError>> {
        let invocation = DemandExecution::new(
            lowered,
            arguments.iter().map(|argument| argument.identity).collect(),
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
            for (binding, argument) in lowered.value_inputs.iter().zip(arguments) {
                if binding.store_schema != argument.identity.schema {
                    return Err(Box::new(MachineError::runtime(
                        MachineOperation::EntryBinding,
                        RuntimeFault::ValueInputSchemaMismatch,
                        None,
                        Some(lowered.demand_key),
                    )));
                }
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
            let step = match self.store.with_value_memory_overrides(
                &value_memory_overrides,
                |value_memories| {
                    task.drive_hosted_with_value_memories(&mut [], &[], &mut [], value_memories)
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

    fn complete_document_parse(
        &mut self,
        task: &mut weavy::exec::ExecTask<'_>,
        plan: &DocumentParseCall,
        request: DocumentHostRequest,
        reads: &mut Vec<ReadWitness>,
    ) -> Result<(), String> {
        if plan.target_schema != semantic_schema_id(&plan.target) {
            return Err(
                "document host target schema does not match its declared target type".to_owned(),
            );
        }
        let handle = StoreHandle::new(
            usize::try_from(request.input)
                .map_err(|_| "document host input is not a store handle".to_owned())?,
        )
        .ok_or_else(|| "document host input store handle is invalid".to_owned())?;
        let entry = self
            .store
            .entry_by_weavy_handle(handle)
            .ok_or_else(|| "document host input store entry is absent".to_owned())?;
        let input = std::str::from_utf8(
            entry
                .resident_bytes()
                .ok_or_else(|| "document host input is not resident".to_owned())?,
        )
        .map_err(|_| "document host input is not UTF-8 String data".to_owned())?
        .to_owned();
        reads.push(ReadWitness {
            source: entry.identity,
            projection: "typed-doc-parse".to_owned(),
        });

        let result = decode::decode(plan.format, &input, &plan.target);
        let mut interned = Vec::new();
        zero_host_region(task, plan.output)?;
        match result {
            Ok(value) => {
                write_host_word(task, plan.output, 0, 0)?;
                let mut cursor = 1;
                materialize_decoded_value(
                    task,
                    plan.output,
                    &plan.target,
                    &value,
                    &mut cursor,
                    &mut self.store,
                    &mut interned,
                )?;
            }
            Err(error) => {
                write_host_word(task, plan.output, 0, 1)?;
                let error_ty = runtime_decode_error_type();
                let mut cursor = 1;
                materialize_decode_error(
                    task,
                    plan.output,
                    &error_ty,
                    &error,
                    &mut cursor,
                    &mut self.store,
                    &mut interned,
                )?;
            }
        }
        for interned in interned {
            self.observe_interned(interned);
        }
        self.counters.document_parse_host_calls += 1;
        Ok(())
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
        lowered: &DemandExecution<'_>,
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
            .filter_map(|entry| entry.receipt.as_ref())
    }

    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
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
        let referent = deref_frozen(store, *id)?;
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
                .handle_for_identity(*id)
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
        .handle_for_identity(id)
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
    task: &weavy::exec::ExecTask<'_>,
    lowered: &LoweringArtifact,
    store: &Store,
) -> Result<RealizedValue, TaskFault> {
    task.with_result_resolver(|result, resolver| {
        let selector = u32::try_from(result.enum_selector()?)
            .map_err(|_| invalid_realized_result(lowered, 0))?;
        let value = result.as_value().enum_field(selector, 0)?;
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
                schema: semantic_schema_id(collection_ty),
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
                schema: semantic_schema_id(collection_ty),
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
                FramedNode::leaf(semantic_schema_id(ty), bytes.clone()),
                FrozenValue::Inline(bytes),
                8,
            ))
        }
        Type::String | Type::Path => {
            let resolved = resolver
                .resolve(value.value_ref()?)
                .ok_or_else(|| invalid_realized_result(lowered, 0))?;
            match resolved {
                ResolvedTaskValue::Store(handle) => {
                    let entry = store
                        .entry_by_weavy_handle(handle)
                        .ok_or_else(|| invalid_realized_result(lowered, 0))?;
                    Ok((
                        FramedNode::Reference(entry.identity),
                        FrozenValue::Reference(entry.identity),
                        0,
                    ))
                }
                ResolvedTaskValue::TaskMolten(bytes) => Ok((
                    FramedNode::leaf(semantic_schema_id(ty), bytes.to_vec()),
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
                    let node = FramedNode::Reference(entry.identity);
                    Ok((node, FrozenValue::Reference(entry.identity), 0))
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
            schema: semantic_schema_id(field_ty),
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
            schema: semantic_schema_id(ty),
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
                FramedNode::leaf(semantic_schema_id(ty), bytes.to_vec()),
                bytes.to_vec(),
                bytes.len(),
            )),
            _ => Err(invalid_realized_result(lowered, bytes.len())),
        },
        ResolvedTaskValue::Store(handle) => {
            let entry = store
                .entry_by_weavy_handle(handle)
                .ok_or_else(|| invalid_realized_result(lowered, 0))?;
            let resident = entry
                .resident_bytes()
                .ok_or_else(|| invalid_realized_result(lowered, 0))?
                .to_vec();
            // A root that is already store-backed needs no freeze. Nested store
            // references are handled by `realize_array` through their ValueId.
            Err(invalid_realized_result(lowered, resident.len()))
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
    let array_schema = semantic_schema_id(&Type::Array(Box::new(element.clone())));
    let element_schema = semantic_schema_id(element);
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
        Type::String | Type::Path | Type::Array(_) | Type::Map { .. } | Type::Set(_) => true,
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
        (FrozenValue::Reference(identity), _) => {
            let schema = publication_schema(binding, ty)?;
            let handle = store
                .handle_for_identity(*identity)
                .and_then(|handle| store.weavy_handle(handle))
                .ok_or(())?;
            Ok(weavy::exec::FrozenValue::Store { schema, handle })
        }
        (FrozenValue::Opaque(bytes), Type::String | Type::Path) => {
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
        Type::String | Type::Path | Type::Array(_) | Type::Map { .. } | Type::Set(_) => {
            Ok(FrozenInline {
                bytes: vec![0; 8],
                references: vec![(0, frozen_to_weavy(frozen, ty, binding, store)?)],
            })
        }
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

fn semantic_schema_id(ty: &Type) -> SchemaId {
    SchemaId::named(&format!("vix.semantic.v1:{}", ty.name()))
}

fn runtime_decode_error_type() -> Type {
    Type::Record(crate::vir::RecordType {
        name: "DecodeError".to_owned(),
        fields: vec![
            crate::vir::RecordField {
                name: "kind".to_owned(),
                ty: Type::String,
            },
            crate::vir::RecordField {
                name: "path".to_owned(),
                ty: Type::String,
            },
            crate::vir::RecordField {
                name: "document_offset".to_owned(),
                ty: Type::Int,
            },
            crate::vir::RecordField {
                name: "document_len".to_owned(),
                ty: Type::Int,
            },
        ],
    })
}

fn document_receipt(demand: DemandKey, reads: &[ReadWitness]) -> Option<Receipt> {
    (!reads.is_empty()).then(|| Receipt {
        demand,
        reads: reads.to_vec(),
    })
}

fn zero_host_region(
    task: &mut weavy::exec::ExecTask<'_>,
    region: super::FrameRegion,
) -> Result<(), String> {
    for index in 0..region.words().as_usize() {
        write_host_word(task, region, index, 0)?;
    }
    Ok(())
}

fn write_host_word(
    task: &mut weavy::exec::ExecTask<'_>,
    region: super::FrameRegion,
    index: usize,
    value: i64,
) -> Result<(), String> {
    let slot = region
        .word(index)
        .ok_or_else(|| "document host wrote outside its typed result region".to_owned())?;
    task.write_host_word(slot.byte_offset(), value)
        .map_err(|fault| format!("document host frame materialization failed: {fault:?}"))
}

fn materialize_decoded_value(
    task: &mut weavy::exec::ExecTask<'_>,
    region: super::FrameRegion,
    ty: &Type,
    value: &DecodedValue,
    cursor: &mut usize,
    store: &mut Store,
    interned: &mut Vec<Interned>,
) -> Result<(), String> {
    match (ty, value) {
        (Type::Int, DecodedValue::Int(value)) => {
            write_host_word(task, region, *cursor, *value)?;
            *cursor += 1;
        }
        (Type::Bool, DecodedValue::Bool(value)) => {
            write_host_word(task, region, *cursor, i64::from(*value))?;
            *cursor += 1;
        }
        (Type::String, DecodedValue::Str(value)) => {
            let allocated = store.intern_realized(semantic_schema_id(ty), value.as_bytes());
            let handle = store
                .weavy_handle(allocated.handle)
                .ok_or_else(|| "document host allocated a missing String handle".to_owned())?;
            write_host_word(task, region, *cursor, handle.as_i64())?;
            interned.push(allocated);
            *cursor += 1;
        }
        (Type::Record(record), DecodedValue::Record(values)) => {
            if record.fields.len() != values.len() {
                return Err("decoded record field count disagrees with its schema".to_owned());
            }
            for (field, value) in record.fields.iter().zip(values) {
                materialize_decoded_value(task, region, &field.ty, value, cursor, store, interned)?;
            }
        }
        (Type::Enum(enumeration), DecodedValue::OptionSome(value))
            if ty.option_inner().is_some() =>
        {
            write_host_word(task, region, *cursor, 0)?;
            *cursor += 1;
            let inner = ty.option_inner().expect("guarded option inner");
            materialize_decoded_value(task, region, inner, value, cursor, store, interned)?;
            let _ = enumeration;
        }
        (Type::Enum(_), DecodedValue::OptionNone) if ty.option_inner().is_some() => {
            write_host_word(task, region, *cursor, 1)?;
            *cursor += 1;
        }
        (Type::Enum(enumeration), DecodedValue::Variant { index, fields }) => {
            let variant = enumeration
                .variants
                .get(*index as usize)
                .ok_or_else(|| "decoded enum variant is outside its schema".to_owned())?;
            write_host_word(task, region, *cursor, i64::from(*index))?;
            *cursor += 1;
            let types: Vec<&Type> = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(fields) => fields.iter().collect(),
                VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
            };
            if types.len() != fields.len() {
                return Err("decoded enum payload count disagrees with its schema".to_owned());
            }
            for (ty, value) in types.into_iter().zip(fields) {
                materialize_decoded_value(task, region, ty, value, cursor, store, interned)?;
            }
        }
        _ => {
            return Err(format!(
                "decoded value does not match target schema {}",
                ty.name()
            ));
        }
    }
    Ok(())
}

fn materialize_decode_error(
    task: &mut weavy::exec::ExecTask<'_>,
    region: super::FrameRegion,
    ty: &Type,
    error: &decode::DecodeError,
    cursor: &mut usize,
    store: &mut Store,
    interned: &mut Vec<Interned>,
) -> Result<(), String> {
    let Type::Record(record) = ty else {
        return Err("decode error schema is not a record".to_owned());
    };
    let kind = DecodedValue::Str(error.kind.label().to_owned());
    let path = DecodedValue::Str(error.path_names().join("."));
    let offset = DecodedValue::Int(error.span.map_or(-1, |span| i64::from(span.offset)));
    let len = DecodedValue::Int(error.span.map_or(-1, |span| i64::from(span.len)));
    for (field, value) in record.fields.iter().zip([kind, path, offset, len]) {
        materialize_decoded_value(task, region, &field.ty, &value, cursor, store, interned)?;
    }
    Ok(())
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
        FailureValue::IndexOutOfBounds { .. }
        | FailureValue::MissingKey { .. }
        | FailureValue::DuplicateKey { .. }
        | FailureValue::MissingDelimiter { .. }
        | FailureValue::InvalidInteger { .. }
        | FailureValue::IntegerOverflow { .. }
        | FailureValue::DivisionByZero { .. } => None,
    }
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
    task: &weavy::exec::ExecTask<'_>,
    lowered: &LoweringArtifact,
) -> Result<DecodedResult, Box<TaskFault>> {
    let Some(abi) = &lowered.array_outcome else {
        // A `Check` island's word is its pass/fail verdict; a scalar `Int`/`Bool`
        // value island's word is the demanded value itself.
        return Ok(match lowered.output_type {
            Type::Int | Type::Bool => DecodedResult::OkScalarValue(task.result_i64()?),
            _ => DecodedResult::OkScalar(task.result_i64()? != 0),
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
    use crate::runtime::{EventLog, FramedNode, MachineCause};
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
                .get_or_lower(island)
                .expect("first compilation lowers through the verified executable");
            let demand_key = artifact.demand_key;
            let evaluation = runtime
                .evaluate(
                    island.id,
                    &location,
                    artifact,
                    &first_attribution,
                    IslandInputs {
                        arguments: &[],
                        wires: &[],
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
                .get_or_lower(shifted_island)
                .expect("span-only recompilation reuses the verified artifact");
            runtime
                .evaluate(
                    shifted_island.id,
                    &location,
                    artifact,
                    &shifted_attribution,
                    IslandInputs {
                        arguments: &[],
                        wires: &[],
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
                    &artifact,
                    attribution,
                    IslandInputs {
                        arguments: &[],
                        wires: &[],
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
            .get_or_lower(island)
            .expect("source lowers through the verified executable");
        let evaluation = runtime
            .evaluate(
                island.id,
                &location,
                artifact,
                &attribution,
                IslandInputs {
                    arguments: &[],
                    wires: &[],
                },
                ChaosPolicy::default(),
            )
            .expect("passing check evaluates to a realized value");

        assert!(evaluation.passed, "1 + 1 == 2 is a passing check");
        assert!(evaluation.failure.is_none());

        // The production realized-scalar path routes through the closed writer:
        // its identity is exactly the framed scalar-leaf identity, computed here
        // independently of the store.
        let expected =
            FramedNode::leaf(SchemaId::named("vix.Check.v1"), vec![u8::from(true)]).identity();
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
