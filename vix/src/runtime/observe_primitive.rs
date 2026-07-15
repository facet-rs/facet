use crate::schema::SchemaPattern;
use crate::vir::{ExternKind, RecordField, RecordType, Type};

use super::{
    EffectCtx, EffectTicket, ObserveCoordinate, ObservedClaim, Primitive, PrimitiveCompletion,
    PrimitiveDescriptor, PrimitiveField, PrimitiveFieldValue, PrimitiveMachineError,
    PrimitiveMemoPolicy, PrimitiveValue, PrimitiveValueBody, ValueId, origin_hint_type,
};

#[must_use]
pub fn observe_request_type() -> Type {
    Type::Record(RecordType::new(
        "ObserveRequest",
        vec![RecordField {
            name: "origin".to_owned(),
            ty: origin_hint_type(),
        }],
    ))
}

#[must_use]
pub fn observe_primitive_id() -> super::PrimitiveId {
    super::PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "observe".to_owned(),
        version: 1,
    }
}

/// The generic `observe` primitive (`machine.primitive.effect-set-v1`). Unlike
/// `fetch`, an observation does not carry the result identity in its request:
/// the coordinate is read, the arriving bytes name themselves, and that observed
/// identity is pinned into the receipt and appended to the coordinate's
/// append-only claim history (`machine.primitive.fetch-is-pinned`,
/// `machine.persistence.four-lifetimes`). Its memo policy is therefore
/// `Observed`: the identity becomes known through a receipted observation.
pub struct ObservePrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for ObservePrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: observe_primitive_id(),
                request_schema: SchemaPattern::exact(&observe_request_type().schema_ref()),
                response_schema: SchemaPattern::exact(&Type::Extern(ExternKind::Blob).schema_ref()),
                failure_schema: SchemaPattern::Var {
                    name: "ObserveFailure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Observed,
                protocol_version: 1,
                capability_schemas: vec![SchemaPattern::exact(
                    &Type::Extern(ExternKind::Registry).schema_ref(),
                )],
            },
        }
    }
}

impl Primitive for ObservePrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx) -> EffectTicket {
        let (ticket, completer) = ctx.ticket(|| {});
        std::thread::spawn(move || {
            let completion = execute(&request, &ctx)
                .map(PrimitiveCompletion::Ok)
                .unwrap_or_else(PrimitiveCompletion::MachineError);
            let publication =
                ctx.finish(completion)
                    .unwrap_or_else(|error| super::PrimitivePublication {
                        completion: PrimitiveCompletion::MachineError(error),
                        receipt: super::Receipt {
                            demand: ctx.demand(),
                            reads: Vec::new(),
                        },
                        journal: Vec::new(),
                        progressive: Vec::new(),
                    });
            let _ = completer.complete(publication);
        });
        ticket
    }
}

fn execute(request: &ValueId, ctx: &EffectCtx) -> Result<ValueId, PrimitiveMachineError> {
    let request = ctx.read(request, super::ReadProjection::Whole)?;
    let coordinate = parse_request(request.value, request.identity)?;

    let blob_schema = Type::Extern(ExternKind::Blob).schema_ref();
    let (bytes, observed) =
        ctx.observe_origin(&coordinate.capability, &coordinate.coordinate, &blob_schema)?;

    let admitted = ctx.intern(&blob_schema, &bytes)?;
    if admitted != observed {
        return Err(PrimitiveMachineError::CorruptCandidate { source: admitted });
    }
    ctx.persist_value(&admitted, &bytes)?;
    ctx.append_claim(&ObservedClaim {
        coordinate,
        observed: admitted.clone(),
    })?;
    Ok(admitted)
}

fn parse_request(
    request: PrimitiveValue,
    request_id: ValueId,
) -> Result<ObserveCoordinate, PrimitiveMachineError> {
    let PrimitiveValueBody::Product(fields) = request.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let [origin] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let origin = child(origin)?;
    let PrimitiveValueBody::Product(origin_fields) = &origin.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let [capability, coordinate] = origin_fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let capability = child(capability)?.identity();
    let coordinate = core::str::from_utf8(bytes(child(coordinate)?)?)
        .map_err(|_| invalid_value())?
        .to_owned();
    Ok(ObserveCoordinate {
        capability,
        coordinate,
    })
}

fn child(field: &PrimitiveField) -> Result<&PrimitiveValue, PrimitiveMachineError> {
    let PrimitiveFieldValue::Child(value) = &field.value else {
        return Err(invalid_value());
    };
    Ok(value)
}

fn bytes(value: &PrimitiveValue) -> Result<&[u8], PrimitiveMachineError> {
    let PrimitiveValueBody::Bytes(bytes) = &value.body else {
        return Err(invalid_value());
    };
    Ok(bytes)
}

fn invalid_value() -> PrimitiveMachineError {
    PrimitiveMachineError::AuthorityViolation {
        detail: "observe request disagrees with its declared schema".to_owned(),
    }
}
