use crate::vir::{ExternKind, Type};

use super::{
    ArgRoleDecl, BlobHandle, EffectCtx, EffectTicket, ObserveCoordinate, ObservedClaim, OriginHint,
    Primitive, PrimitiveDecl, PrimitiveMachineError, PrimitiveMemoPolicy, SelectorDecl,
    SelectorVariantDecl, ValueId,
};
// Only the test-only hand parser (the `decode_primitive_value` oracle) walks the
// wire `PrimitiveValue` structurally now; production `begin` decodes instead.
#[cfg(test)]
use super::{PrimitiveField, PrimitiveFieldValue, PrimitiveValue, PrimitiveValueBody};

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
pub struct ObservePrimitive;

/// The `Mode` selector `observe`'s second argument folds to its `refresh` flag:
/// `Mode::Observe` → `false`, `Mode::Refresh` → `true`.
const MODE_SELECTOR: SelectorDecl = SelectorDecl {
    enum_name: "Mode",
    noun: "observe mode",
    variants: &[
        SelectorVariantDecl {
            variant: "Observe",
            flag: false,
        },
        SelectorVariantDecl {
            variant: "Refresh",
            flag: true,
        },
    ],
};

impl<Ctx> Primitive<Ctx> for ObservePrimitive {
    type Request = ObserveRequest;
    type Response = BlobHandle;
    type Deps = ();

    const DECL: PrimitiveDecl = PrimitiveDecl {
        namespace: "vix.machine",
        name: "observe",
        id_name: "observe",
        version: 1,
        memo_policy: PrimitiveMemoPolicy::Observed,
        protocol_version: 1,
        failure_schema_name: "ObserveFailure",
        capabilities: &[ExternKind::Registry],
        args: &[ArgRoleDecl::Value, ArgRoleDecl::Selector(MODE_SELECTOR)],
    };

    fn begin(&self, req: ObserveRequest, ctx: EffectCtx, _deps: ()) -> EffectTicket<BlobHandle> {
        let (ticket, completer) = EffectTicket::<BlobHandle>::pair(&ctx, || {});
        std::thread::spawn(move || {
            let _ = match serve(req, &ctx) {
                Ok(value) => completer.complete_ok(&ctx, BlobHandle(value)),
                Err(error) => completer.complete_err(&ctx, error),
            };
        });
        ticket
    }
}

fn serve(request: ObserveRequest, ctx: &EffectCtx) -> Result<ValueId, PrimitiveMachineError> {
    let coordinate = ObserveCoordinate {
        capability: request.origin.capability.0,
        coordinate: request.origin.coordinate,
    };
    let refresh = request.refresh;

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

// `#[cfg(test)]`: since `begin` decodes via `decode_primitive_value`, this hand
// parser survives only as the oracle for `primitive_value_decode`'s tests, which
// assert that decoder agrees with it on the same wire `PrimitiveValue` for the
// real `ObserveRequest`. Test-only, so it doesn't dead-code-warn in a normal build.
#[cfg(test)]
pub(crate) fn parse_request(
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

#[cfg(test)]
fn inline_i64(field: &PrimitiveField) -> Result<i64, PrimitiveMachineError> {
    let PrimitiveFieldValue::Inline(bytes) = &field.value else {
        return Err(invalid_value());
    };
    Ok(i64::from_le_bytes(
        bytes.as_slice().try_into().map_err(|_| invalid_value())?,
    ))
}

#[cfg(test)]
fn child(field: &PrimitiveField) -> Result<&PrimitiveValue, PrimitiveMachineError> {
    let PrimitiveFieldValue::Child(value) = &field.value else {
        return Err(invalid_value());
    };
    Ok(value)
}

#[cfg(test)]
fn bytes(value: &PrimitiveValue) -> Result<&[u8], PrimitiveMachineError> {
    let PrimitiveValueBody::Bytes(bytes) = &value.body else {
        return Err(invalid_value());
    };
    Ok(bytes)
}

#[cfg(test)]
fn invalid_value() -> PrimitiveMachineError {
    PrimitiveMachineError::AuthorityViolation {
        detail: "observe request disagrees with its declared schema".to_owned(),
    }
}
