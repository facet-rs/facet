use crate::schema::SchemaPattern;
use crate::vir::{ExternKind, Type};

use super::{
    EffectCtx, EffectTicket, ObserveCoordinate, ObservedClaim, OriginHint, Primitive,
    PrimitiveCompletion, PrimitiveDescriptor, PrimitiveField, PrimitiveFieldValue,
    PrimitiveMachineError, PrimitiveMemoPolicy, PrimitiveValue, PrimitiveValueBody, ValueId,
};

/// The `observe` request shape. There is no other Rust spelling of this struct —
/// it is authored here so the derived `Type::from_facet::<ObserveRequest>()` is
/// the single source for both `RequestShape.request_ty` and the descriptor's
/// `request_schema`.
///
/// `refresh == false` = observe (memoized by demand like any effect result);
/// `refresh == true` = refresh, a distinct demand that forces a fresh receipted
/// observation past the within-run memo and appends a new head under optimistic
/// concurrency.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ObserveRequest {
    pub origin: OriginHint,
    pub refresh: bool,
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
                request_schema: SchemaPattern::exact(
                    &Type::from_facet::<ObserveRequest>().schema_ref(),
                ),
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

impl<Ctx> Primitive<Ctx> for ObservePrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx, _app: &Ctx) -> EffectTicket {
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
    let (coordinate, refresh) = parse_request(request.value, request.identity)?;

    // Optimistic concurrency for refresh: sample the head before reading the
    // origin, and reject the append if a concurrent observer advanced the head
    // while this refresh was in flight (`RefreshConflict`). A plain observe does
    // not gate on the head; it appends its receipted observation unconditionally.
    let expected_head = if refresh {
        Some(ctx.claim_head(&coordinate)?)
    } else {
        None
    };

    let blob_schema = Type::Extern(ExternKind::Blob).schema_ref();
    let (bytes, observed) =
        ctx.observe_origin(&coordinate.capability, &coordinate.coordinate, &blob_schema)?;

    let admitted = ctx.intern(&blob_schema, &bytes)?;
    if admitted != observed {
        return Err(PrimitiveMachineError::CorruptCandidate { source: admitted });
    }
    ctx.persist_value(&admitted, &bytes)?;

    if let Some(expected) = expected_head {
        let current = ctx.claim_head(&coordinate)?;
        if current != expected {
            let current = current
                .map(|claim| claim.observed)
                .unwrap_or_else(|| admitted.clone());
            return Err(PrimitiveMachineError::RefreshConflict { current });
        }
    }

    ctx.append_claim(&ObservedClaim {
        coordinate,
        observed: admitted.clone(),
    })?;
    Ok(admitted)
}

fn parse_request(
    request: PrimitiveValue,
    request_id: ValueId,
) -> Result<(ObserveCoordinate, bool), PrimitiveMachineError> {
    let PrimitiveValueBody::Product(fields) = request.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let [origin, refresh] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let refresh = inline_i64(refresh)? != 0;
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
    Ok((
        ObserveCoordinate {
            capability,
            coordinate,
        },
        refresh,
    ))
}

fn inline_i64(field: &PrimitiveField) -> Result<i64, PrimitiveMachineError> {
    let PrimitiveFieldValue::Inline(bytes) = &field.value else {
        return Err(invalid_value());
    };
    Ok(i64::from_le_bytes(
        bytes.as_slice().try_into().map_err(|_| invalid_value())?,
    ))
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
