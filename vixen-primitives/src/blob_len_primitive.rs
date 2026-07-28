use vix::schema::SchemaPattern;
use vix::vir::Type;

use crate::rt::{
    BlobHandle, EffectCtx, PrimitiveCompletion, PrimitiveDescriptor, PrimitiveFieldValue,
    PrimitiveId, PrimitiveMachineError, PrimitiveMemoPolicy, PrimitiveValue, PrimitiveValueBody,
    RawEffectTicket, RawPrimitive, ReadProjection, ValueId,
};

/// The wire request of the `blob-len` primitive: just the `Blob` whose byte
/// length is wanted. The vix `Type` is read off this facet shape — `BlobHandle`
/// carries its own `#[facet(vix::wire_extern = "Blob")]` annotation — so the whole
/// contract (request, result, id) lives beside the implementation and `vix-core`
/// holds nothing of it. `Blob.len` is a primitive-backed method
/// (`binding::MethodLowering::Primitive`), reached through the generic
/// request-record rail, so there is no bespoke `Op::BlobLen` in core (issue 2528).
#[derive(facet::Facet)]
pub struct BlobLenRequest {
    pub blob: BlobHandle,
}

#[must_use]
pub fn blob_len_request_type() -> Type {
    Type::from_facet::<BlobLenRequest>()
}

#[must_use]
pub fn blob_len_result_type() -> Type {
    Type::Int
}

#[must_use]
pub fn blob_len_primitive_id() -> PrimitiveId {
    PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "blob-len".to_owned(),
        version: 1,
    }
}

/// `Blob.len() -> Int` — the byte length of a `Blob`. A `Blob` is resident bytes
/// (`Primitive::Bytes`), so the length is a pure function of the request value:
/// the invocation carries the blob and the primitive counts its bytes. `Hermetic`
/// because the result depends only on the (immutable) request, with no external
/// observation — the memo is keyed by the request identity, which folds in the
/// blob's content.
pub struct BlobLenPrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for BlobLenPrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: blob_len_primitive_id(),
                request_schema: SchemaPattern::exact(&blob_len_request_type().schema_ref()),
                response_schema: SchemaPattern::exact(&blob_len_result_type().schema_ref()),
                failure_schema: SchemaPattern::Var {
                    name: "BlobLenFailure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Hermetic,
                protocol_version: 1,
                capability_schemas: Vec::new(),
            },
        }
    }
}

impl<Ctx> RawPrimitive<Ctx> for BlobLenPrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx, _app: &Ctx) -> RawEffectTicket {
        let (ticket, completer) = ctx.ticket(|| {});
        std::thread::spawn(move || {
            let completion = execute(&request, &ctx)
                .map(PrimitiveCompletion::Ok)
                .unwrap_or_else(PrimitiveCompletion::MachineError);
            let publication =
                ctx.finish(completion)
                    .unwrap_or_else(|error| crate::rt::PrimitivePublication {
                        completion: PrimitiveCompletion::MachineError(error),
                        receipt: crate::rt::Receipt {
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
    let request = ctx.read(request, ReadProjection::Whole)?;
    let blob = blob_field(request.value, request.identity)?;
    let len = i64::try_from(blob.len()).map_err(|_| PrimitiveMachineError::AuthorityViolation {
        detail: "Blob length did not fit Int".to_owned(),
    })?;
    ctx.intern_value(PrimitiveValue::bytes(
        Type::Int.schema_ref(),
        len.to_le_bytes().to_vec(),
    ))
}

/// Extract the resident bytes of the request's single `Blob` field. A `Blob` is a
/// bytes primitive, so once the request is read whole its blob field is resident
/// bytes — an empty blob is a legitimate zero-length read.
fn blob_field(
    request: PrimitiveValue,
    request_id: ValueId,
) -> Result<Vec<u8>, PrimitiveMachineError> {
    let PrimitiveValueBody::Product(fields) = request.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let [blob] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let bytes = match &blob.value {
        PrimitiveFieldValue::Inline(bytes) => bytes.clone(),
        PrimitiveFieldValue::Child(value) => match &value.body {
            PrimitiveValueBody::Bytes(bytes) => bytes.clone(),
            PrimitiveValueBody::Product(_)
            | PrimitiveValueBody::Sequence { .. }
            | PrimitiveValueBody::Variant { .. }
            | PrimitiveValueBody::OrderedMap(_) => {
                return Err(PrimitiveMachineError::AuthorityViolation {
                    detail: "blob-len request field was not resident Blob bytes".to_owned(),
                });
            }
        },
    };
    Ok(bytes)
}
