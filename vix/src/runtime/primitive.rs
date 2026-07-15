use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::schema::SchemaRef;

use super::{DemandKey, ReadObservation, ReadProjection, ReadWitness, Receipt, ValueId};

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrimitiveId {
    pub namespace: String,
    pub name: String,
    pub version: u32,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveMemoPolicy {
    Hermetic,
    Pinned,
    Observed,
    Volatile,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveDescriptor {
    pub id: PrimitiveId,
    pub request_schema: SchemaRef,
    pub response_schema: SchemaRef,
    pub failure_schema: SchemaRef,
    pub memo_policy: PrimitiveMemoPolicy,
    pub protocol_version: u32,
    /// Minimal declared capability types. FV-E3 enriches these into semantic
    /// admissibility constraints; concrete capabilities are always request
    /// values referenced by `ValueId`.
    pub capability_schemas: Vec<SchemaRef>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveMachineError {
    Unavailable { detail: String },
    Cancelled,
    Exhausted { detail: String },
    PolicyRejected { detail: String },
    CorruptCandidate { source: ValueId },
    RefreshConflict { current: ValueId },
    InvalidRequest { request: ValueId },
    AuthorityViolation { detail: String },
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveCompletion {
    Ok(ValueId),
    Failed(ValueId),
    MachineError(PrimitiveMachineError),
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveEvent {
    pub schema: SchemaRef,
    pub value: ValueId,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct JournalObservation {
    pub schema: SchemaRef,
    pub value: ValueId,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ProgressivePublication {
    pub projection: ReadProjection,
    pub value: ValueId,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PrimitivePublication {
    pub completion: PrimitiveCompletion,
    pub receipt: Receipt,
    pub journal: Vec<JournalObservation>,
    pub progressive: Vec<ProgressivePublication>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessedValue {
    pub identity: ValueId,
    pub bytes: Vec<u8>,
    pub observation: ReadObservation,
}

pub trait EffectAuthority: Send + Sync {
    fn read(
        &self,
        source: &ValueId,
        projection: &ReadProjection,
    ) -> Result<WitnessedValue, PrimitiveMachineError>;

    fn intern(&self, schema: &SchemaRef, bytes: &[u8]) -> Result<ValueId, PrimitiveMachineError>;

    fn emit(&self, event: PrimitiveEvent) -> Result<(), PrimitiveMachineError>;

    fn mint_mount_grant(&self, request: &ValueId) -> Result<ValueId, PrimitiveMachineError>;
}

#[derive(Clone)]
pub struct EffectCtx {
    demand: DemandKey,
    authority: Arc<dyn EffectAuthority>,
    transaction: Arc<Mutex<EffectTransaction>>,
}

#[derive(Default)]
struct EffectTransaction {
    reads: Vec<ReadWitness>,
    journal: Vec<JournalObservation>,
    progressive: Vec<ProgressivePublication>,
    completed: bool,
}

impl EffectCtx {
    #[must_use]
    pub fn new(demand: DemandKey, authority: Arc<dyn EffectAuthority>) -> Self {
        Self {
            demand,
            authority,
            transaction: Arc::new(Mutex::new(EffectTransaction::default())),
        }
    }

    pub fn read(
        &self,
        source: &ValueId,
        projection: ReadProjection,
    ) -> Result<WitnessedValue, PrimitiveMachineError> {
        let witnessed = self.authority.read(source, &projection)?;
        self.transaction
            .lock()
            .expect("effect transaction mutex poisoned")
            .reads
            .push(ReadWitness {
                source: source.clone(),
                projection,
                observation: witnessed.observation.clone(),
            });
        Ok(witnessed)
    }

    pub fn intern(
        &self,
        schema: &SchemaRef,
        bytes: &[u8],
    ) -> Result<ValueId, PrimitiveMachineError> {
        self.authority.intern(schema, bytes)
    }

    pub fn emit(&self, event: PrimitiveEvent) -> Result<(), PrimitiveMachineError> {
        self.authority.emit(event)
    }

    pub fn mint_mount_grant(&self, request: &ValueId) -> Result<ValueId, PrimitiveMachineError> {
        self.authority.mint_mount_grant(request)
    }

    pub fn observe(&self, observation: JournalObservation) {
        self.transaction
            .lock()
            .expect("effect transaction mutex poisoned")
            .journal
            .push(observation);
    }

    pub fn publish_progress(&self, publication: ProgressivePublication) {
        self.transaction
            .lock()
            .expect("effect transaction mutex poisoned")
            .progressive
            .push(publication);
    }

    pub fn finish(
        &self,
        completion: PrimitiveCompletion,
    ) -> Result<PrimitivePublication, PrimitiveMachineError> {
        let mut transaction = self
            .transaction
            .lock()
            .expect("effect transaction mutex poisoned");
        if transaction.completed {
            return Err(PrimitiveMachineError::AuthorityViolation {
                detail: "primitive attempted more than one completion transaction".to_owned(),
            });
        }
        transaction.completed = true;
        Ok(PrimitivePublication {
            completion,
            receipt: Receipt {
                demand: self.demand,
                reads: std::mem::take(&mut transaction.reads),
            },
            journal: std::mem::take(&mut transaction.journal),
            progressive: std::mem::take(&mut transaction.progressive),
        })
    }

    pub fn ticket(
        &self,
        cancel: impl FnOnce() + Send + 'static,
    ) -> (EffectTicket, EffectCompleter) {
        EffectTicket::pair(self.demand, cancel)
    }
}

pub trait Primitive: Send + Sync {
    fn descriptor(&self) -> &PrimitiveDescriptor;
    fn begin(&self, request: ValueId, ctx: EffectCtx) -> EffectTicket;
}

type TicketWaiter = Box<dyn FnOnce(PrimitivePublication) + Send + 'static>;

struct TicketState {
    outcome: Option<PrimitivePublication>,
    waiters: BTreeMap<u64, TicketWaiter>,
    next_waiter: u64,
    lease_generation: u64,
    cancelled: bool,
    cancel: Option<Box<dyn FnOnce() + Send + 'static>>,
}

struct TicketShared {
    demand: DemandKey,
    state: Mutex<TicketState>,
}

#[derive(Clone)]
pub struct EffectTicket {
    shared: Arc<TicketShared>,
}

pub struct EffectCompleter {
    shared: Arc<TicketShared>,
}

pub struct TicketSubscription {
    shared: Arc<TicketShared>,
    waiter: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketCompletionError {
    AlreadyCompleted,
    Cancelled,
}

impl EffectTicket {
    fn pair(demand: DemandKey, cancel: impl FnOnce() + Send + 'static) -> (Self, EffectCompleter) {
        let shared = Arc::new(TicketShared {
            demand,
            state: Mutex::new(TicketState {
                outcome: None,
                waiters: BTreeMap::new(),
                next_waiter: 0,
                lease_generation: 0,
                cancelled: false,
                cancel: Some(Box::new(cancel)),
            }),
        });
        (
            Self {
                shared: shared.clone(),
            },
            EffectCompleter { shared },
        )
    }

    #[must_use]
    pub fn demand(&self) -> DemandKey {
        self.shared.demand
    }

    pub fn renew_lease(&self) -> u64 {
        let mut state = self.shared.state.lock().expect("ticket mutex poisoned");
        state.lease_generation = state.lease_generation.wrapping_add(1);
        state.lease_generation
    }

    pub fn join(
        &self,
        waiter: impl FnOnce(PrimitivePublication) + Send + 'static,
    ) -> TicketSubscription {
        let mut waiter = Some(Box::new(waiter) as TicketWaiter);
        let mut state = self.shared.state.lock().expect("ticket mutex poisoned");
        if let Some(outcome) = state.outcome.clone() {
            drop(state);
            waiter.take().expect("waiter exists")(outcome);
            return TicketSubscription {
                shared: self.shared.clone(),
                waiter: None,
            };
        }
        if state.cancelled {
            return TicketSubscription {
                shared: self.shared.clone(),
                waiter: None,
            };
        }
        let id = state.next_waiter;
        state.next_waiter = state.next_waiter.wrapping_add(1);
        state
            .waiters
            .insert(id, waiter.take().expect("waiter exists"));
        TicketSubscription {
            shared: self.shared.clone(),
            waiter: Some(id),
        }
    }

    #[must_use]
    pub fn outcome(&self) -> Option<PrimitivePublication> {
        self.shared
            .state
            .lock()
            .expect("ticket mutex poisoned")
            .outcome
            .clone()
    }

    pub fn cancel_demand(&self) -> bool {
        let cancel = {
            let mut state = self.shared.state.lock().expect("ticket mutex poisoned");
            if state.cancelled || state.outcome.is_some() {
                return false;
            }
            state.cancelled = true;
            state.waiters.clear();
            state.cancel.take()
        };
        if let Some(cancel) = cancel {
            cancel();
        }
        true
    }
}

impl EffectCompleter {
    pub fn complete(self, outcome: PrimitivePublication) -> Result<(), TicketCompletionError> {
        let waiters = {
            let mut state = self.shared.state.lock().expect("ticket mutex poisoned");
            if state.cancelled {
                return Err(TicketCompletionError::Cancelled);
            }
            if state.outcome.is_some() {
                return Err(TicketCompletionError::AlreadyCompleted);
            }
            state.outcome = Some(outcome.clone());
            std::mem::take(&mut state.waiters)
        };
        for (_, waiter) in waiters {
            waiter(outcome.clone());
        }
        Ok(())
    }
}

impl Drop for TicketSubscription {
    fn drop(&mut self) {
        if let Some(waiter) = self.waiter.take() {
            self.shared
                .state
                .lock()
                .expect("ticket mutex poisoned")
                .waiters
                .remove(&waiter);
        }
    }
}

#[derive(Default)]
pub struct PrimitiveRegistry {
    primitives: BTreeMap<PrimitiveId, Arc<dyn Primitive>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimitiveRegistrationError {
    Duplicate(PrimitiveId),
}

impl PrimitiveRegistry {
    pub fn register(
        &mut self,
        primitive: Arc<dyn Primitive>,
    ) -> Result<(), PrimitiveRegistrationError> {
        let id = primitive.descriptor().id.clone();
        if self.primitives.insert(id.clone(), primitive).is_some() {
            return Err(PrimitiveRegistrationError::Duplicate(id));
        }
        Ok(())
    }

    #[must_use]
    pub fn descriptor(&self, id: &PrimitiveId) -> Option<&PrimitiveDescriptor> {
        self.primitives
            .get(id)
            .map(|primitive| primitive.descriptor())
    }

    pub fn begin(
        &self,
        id: &PrimitiveId,
        request: ValueId,
        ctx: EffectCtx,
    ) -> Result<EffectTicket, PrimitiveDispatchError> {
        let primitive = self
            .primitives
            .get(id)
            .ok_or_else(|| PrimitiveDispatchError::Unregistered(id.clone()))?;
        if request.schema != primitive.descriptor().request_schema {
            return Err(PrimitiveDispatchError::RequestSchema {
                primitive: id.clone(),
                expected: primitive.descriptor().request_schema.clone(),
                found: request.schema,
            });
        }
        Ok(primitive.begin(request, ctx))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimitiveDispatchError {
    Unregistered(PrimitiveId),
    RequestSchema {
        primitive: PrimitiveId,
        expected: SchemaRef,
        found: SchemaRef,
    },
}
