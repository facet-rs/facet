use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use vix::runtime::{
    EffectAuthority, EffectCtx, FramedNode, JournalObservation, PrimitiveCompletion,
    PrimitiveDescriptor, PrimitiveDispatchError, PrimitiveDispatcher, PrimitiveEvent, PrimitiveId,
    PrimitiveMachineError, PrimitiveMemoPolicy, PrimitiveRegistry, RawEffectTicket, RawPrimitive,
    ReadObservation, ReadProjection, TicketCompletionError, ValueId, WitnessedValue,
};
use vix::schema::SchemaRef;
use vix::vir::Type;

#[derive(Default)]
struct MemoryAuthority {
    bodies: Mutex<BTreeMap<ValueId, Vec<u8>>>,
    events: Mutex<Vec<PrimitiveEvent>>,
}

impl MemoryAuthority {
    fn insert(&self, schema: SchemaRef, bytes: &[u8]) -> ValueId {
        let identity = FramedNode::leaf(schema, bytes.to_vec()).identity();
        self.bodies
            .lock()
            .expect("body mutex poisoned")
            .insert(identity.clone(), bytes.to_vec());
        identity
    }
}

impl EffectAuthority for MemoryAuthority {
    fn read(
        &self,
        source: &ValueId,
        _projection: &ReadProjection,
    ) -> Result<WitnessedValue, PrimitiveMachineError> {
        let bytes = self
            .bodies
            .lock()
            .expect("body mutex poisoned")
            .get(source)
            .cloned()
            .ok_or_else(|| PrimitiveMachineError::Unavailable {
                detail: "body absent".to_owned(),
            })?;
        Ok(WitnessedValue {
            identity: source.clone(),
            value: vix::runtime::PrimitiveValue::bytes(source.schema.clone(), bytes.clone()),
            bytes,
            observation: ReadObservation::Value(source.clone()),
        })
    }

    fn intern(&self, schema: &SchemaRef, bytes: &[u8]) -> Result<ValueId, PrimitiveMachineError> {
        Ok(self.insert(schema.clone(), bytes))
    }

    fn emit(&self, event: PrimitiveEvent) -> Result<(), PrimitiveMachineError> {
        self.events
            .lock()
            .expect("event mutex poisoned")
            .push(event);
        Ok(())
    }

    fn mint_mount_grant(&self, request: &ValueId) -> Result<ValueId, PrimitiveMachineError> {
        Ok(request.clone())
    }
}

struct EchoPrimitive {
    descriptor: PrimitiveDescriptor,
    begins: AtomicUsize,
}

impl<Ctx> RawPrimitive<Ctx> for EchoPrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx, _app: &Ctx) -> RawEffectTicket {
        self.begins.fetch_add(1, Ordering::Relaxed);
        let (ticket, completer) = ctx.ticket(|| {});
        let publication = ctx
            .read(&request, ReadProjection::Whole)
            .and_then(|value| ctx.intern(&Type::String.schema_ref(), &value.bytes))
            .map(PrimitiveCompletion::Ok)
            .and_then(|completion| ctx.finish(completion));
        let publication = match publication {
            Ok(publication) => publication,
            Err(error) => ctx
                .finish(PrimitiveCompletion::MachineError(error))
                .expect("the error is the first completion transaction"),
        };
        completer
            .complete(publication)
            .expect("fresh ticket accepts one completion");
        ticket
    }
}

fn descriptor() -> PrimitiveDescriptor {
    PrimitiveDescriptor {
        id: PrimitiveId {
            namespace: "vix.test".to_owned(),
            name: "echo".to_owned(),
            version: 1,
        },
        request_schema: vix::schema::SchemaPattern::exact(&Type::String.schema_ref()),
        response_schema: vix::schema::SchemaPattern::exact(&Type::String.schema_ref()),
        failure_schema: vix::schema::SchemaPattern::exact(&Type::String.schema_ref()),
        memo_policy: PrimitiveMemoPolicy::Hermetic,
        protocol_version: 1,
        capability_schemas: Vec::new(),
    }
}

#[test]
fn registered_dispatch_records_reads_without_an_opt_in_receipt_call() {
    let authority = Arc::new(MemoryAuthority::default());
    let request = authority.insert(Type::String.schema_ref(), b"hello");
    let demand = vix::runtime::DemandKey::from_preimage(&vix::runtime::DemandPreimage {
        closure: vix::runtime::RecipeId::from_canonical_vir(b"primitive-foundation"),
        arguments: vec![request.clone()],
    });
    let ctx = EffectCtx::new(demand, authority);

    let primitive = Arc::new(EchoPrimitive {
        descriptor: descriptor(),
        begins: AtomicUsize::new(0),
    });
    let mut registry = PrimitiveRegistry::default();
    registry
        .register(primitive.clone())
        .expect("primitive registers once");
    let ticket = registry
        .begin(&primitive.descriptor.id, request.clone(), ctx, &())
        .expect("generic descriptor lookup dispatches");
    let publication = ticket.outcome().expect("immediate primitive completed");

    assert!(matches!(publication.completion, PrimitiveCompletion::Ok(_)));
    assert_eq!(publication.receipt.demand, demand);
    assert_eq!(publication.receipt.reads.len(), 1);
    assert_eq!(publication.receipt.reads[0].source, request);
    assert_eq!(
        publication.receipt.reads[0].projection,
        ReadProjection::Whole
    );
    assert!(publication.journal.is_empty());
}

#[test]
fn request_schema_is_checked_before_primitive_code_runs() {
    let authority = Arc::new(MemoryAuthority::default());
    let wrong = authority.insert(Type::Int.schema_ref(), &1_i64.to_le_bytes());
    let demand = vix::runtime::DemandKey::from_preimage(&vix::runtime::DemandPreimage {
        closure: vix::runtime::RecipeId::from_canonical_vir(b"wrong-schema"),
        arguments: vec![wrong.clone()],
    });
    let primitive = Arc::new(EchoPrimitive {
        descriptor: descriptor(),
        begins: AtomicUsize::new(0),
    });
    let mut registry = PrimitiveRegistry::default();
    registry
        .register(primitive.clone())
        .expect("primitive registers");

    assert!(matches!(
        registry.begin(
            &primitive.descriptor.id,
            wrong,
            EffectCtx::new(demand, authority),
            &(),
        ),
        Err(error) if matches!(*error, PrimitiveDispatchError::RequestSchema { .. })
    ));
}

#[test]
fn duplicate_running_demand_joins_one_registered_primitive_ticket() {
    let authority = Arc::new(MemoryAuthority::default());
    let request = authority.insert(Type::String.schema_ref(), b"same request");
    let demand = vix::runtime::DemandKey::from_preimage(&vix::runtime::DemandPreimage {
        closure: vix::runtime::RecipeId::from_canonical_vir(b"dispatch-dedup"),
        arguments: vec![request.clone()],
    });
    let primitive = Arc::new(EchoPrimitive {
        descriptor: descriptor(),
        begins: AtomicUsize::new(0),
    });
    let mut registry = PrimitiveRegistry::default();
    registry
        .register(primitive.clone())
        .expect("primitive registers");
    let dispatcher = PrimitiveDispatcher::new(Arc::new(registry));

    let first = dispatcher
        .begin_or_join(
            &primitive.descriptor.id,
            request.clone(),
            EffectCtx::new(demand, authority.clone()),
            &(),
        )
        .expect("first demand begins");
    let second = dispatcher
        .begin_or_join(
            &primitive.descriptor.id,
            request,
            EffectCtx::new(demand, authority),
            &(),
        )
        .expect("duplicate demand joins");

    assert_eq!(primitive.begins.load(Ordering::Relaxed), 1);
    assert_eq!(first.demand(), second.demand());
    assert_eq!(first.outcome(), second.outcome());
    assert_eq!(dispatcher.in_flight(), 1);
    assert!(dispatcher.retire(demand).is_some());
}

#[test]
fn killed_requester_drops_only_its_subscription_and_replay_joins_the_ticket() {
    let authority = Arc::new(MemoryAuthority::default());
    let request = authority.insert(Type::String.schema_ref(), b"request");
    let demand = vix::runtime::DemandKey::from_preimage(&vix::runtime::DemandPreimage {
        closure: vix::runtime::RecipeId::from_canonical_vir(b"join"),
        arguments: vec![request.clone()],
    });
    let ctx = EffectCtx::new(demand, authority);
    let (ticket, completer) = ctx.ticket(|| {});

    let killed = ticket.join(|_| panic!("killed task must not be resumed"));
    drop(killed);

    let resumed = Arc::new(Mutex::new(Vec::new()));
    let resumed_by_waiter = resumed.clone();
    let _replay = ticket.join(move |publication| {
        resumed_by_waiter
            .lock()
            .expect("resume mutex poisoned")
            .push(publication);
    });
    let publication = ctx
        .finish(PrimitiveCompletion::Ok(request))
        .expect("one completion transaction");
    completer
        .complete(publication)
        .expect("replay joins the original ticket");

    assert_eq!(resumed.lock().expect("resume mutex poisoned").len(), 1);
    assert!(ticket.outcome().is_some());
}

#[test]
fn explicit_demand_cancellation_prevents_late_publication() {
    let authority = Arc::new(MemoryAuthority::default());
    let request = authority.insert(Type::String.schema_ref(), b"request");
    let demand = vix::runtime::DemandKey::from_preimage(&vix::runtime::DemandPreimage {
        closure: vix::runtime::RecipeId::from_canonical_vir(b"cancel"),
        arguments: vec![request.clone()],
    });
    let ctx = EffectCtx::new(demand, authority);
    let cancelled = Arc::new(Mutex::new(0_u64));
    let cancelled_by_backend = cancelled.clone();
    let (ticket, completer) = ctx.ticket(move || {
        *cancelled_by_backend.lock().expect("cancel mutex poisoned") += 1;
    });

    assert!(ticket.cancel_demand());
    let publication = ctx
        .finish(PrimitiveCompletion::Ok(request))
        .expect("completion transaction can be built but is unreachable");
    assert_eq!(
        completer.complete(publication),
        Err(TicketCompletionError::Cancelled),
    );
    assert!(ticket.outcome().is_none());
    assert_eq!(*cancelled.lock().expect("cancel mutex poisoned"), 1);
}

#[test]
fn journal_observations_are_part_of_completion_not_the_event_sink() {
    let authority = Arc::new(MemoryAuthority::default());
    let value = authority.insert(Type::String.schema_ref(), b"observed");
    let demand = vix::runtime::DemandKey::from_preimage(&vix::runtime::DemandPreimage {
        closure: vix::runtime::RecipeId::from_canonical_vir(b"journal"),
        arguments: vec![value.clone()],
    });
    let ctx = EffectCtx::new(demand, authority);
    ctx.observe(JournalObservation {
        schema: Type::String.schema_ref(),
        value: value.clone(),
    });
    let publication = ctx
        .finish(PrimitiveCompletion::Ok(value))
        .expect("one completion transaction");
    assert_eq!(publication.journal.len(), 1);
}
