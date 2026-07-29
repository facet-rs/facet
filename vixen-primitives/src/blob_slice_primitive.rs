//! `blob-slice` — the byte-range projection of a `Blob`, settled side.
//!
//! `Blob.take(len)` lowers to this primitive's request
//! (`vix::runtime::blob_slice_request_type`, contract in core beside
//! `tree-read`'s, for the same reason: the progressive machinery names it).
//! On a settled Blob this hermetic implementation serves the slice; on a
//! still-running effect's stream field the invocation is `EFFECT`-marked and
//! the scheduler serves the range from the stream's published frontier
//! instead — this code never runs for the progressive path, exactly as
//! `TreeReadPrimitive` never runs for an in-flight tree projection.
//!
//! A range the blob does not cover is a loud typed machine error, mirroring
//! the scheduler's beyond-frontier fault: bytes are never fabricated and a
//! short stream never silently truncates a demanded range.
//!
//! r[impl machine.primitive.exec-outcome]

use vix::schema::SchemaPattern;
use vix::vir::{ExternKind, Type};

use crate::rt::{
    EffectCtx, PrimitiveCompletion, PrimitiveDescriptor, PrimitiveFieldValue, PrimitiveMachineError,
    PrimitiveMemoPolicy, PrimitiveValue, PrimitiveValueBody, RawEffectTicket, RawPrimitive,
    ReadProjection, ValueId, blob_slice_primitive_id, blob_slice_request_type,
};

pub struct BlobSlicePrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for BlobSlicePrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: blob_slice_primitive_id(),
                request_schema: SchemaPattern::exact(&blob_slice_request_type().schema_ref()),
                response_schema: SchemaPattern::exact(
                    &Type::Extern(ExternKind::Blob).schema_ref(),
                ),
                failure_schema: SchemaPattern::Var {
                    name: "BlobSliceFailure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Hermetic,
                protocol_version: 1,
                capability_schemas: Vec::new(),
            },
        }
    }
}

impl<Ctx> RawPrimitive<Ctx> for BlobSlicePrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx, _app: &Ctx) -> RawEffectTicket {
        let (ticket, completer) = ctx.ticket(|| {});
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
        ticket
    }
}

fn execute(request: &ValueId, ctx: &EffectCtx) -> Result<ValueId, PrimitiveMachineError> {
    let request_value = ctx.read(request, ReadProjection::Whole)?;
    let invalid = |detail: String| PrimitiveMachineError::AuthorityViolation { detail };
    let PrimitiveValueBody::Product(fields) = &request_value.value.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        });
    };
    let [blob, start, end] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        });
    };
    let bytes = match &blob.value {
        PrimitiveFieldValue::Inline(bytes) => bytes.clone(),
        PrimitiveFieldValue::Child(value) => match &value.body {
            PrimitiveValueBody::Bytes(bytes) => bytes.clone(),
            _ => {
                return Err(invalid(
                    "blob-slice request field was not resident Blob bytes".to_owned(),
                ));
            }
        },
    };
    let bound = |field: &crate::rt::PrimitiveField, name: &str| -> Result<usize, _> {
        let raw = match &field.value {
            PrimitiveFieldValue::Inline(bytes) => bytes.clone(),
            PrimitiveFieldValue::Child(value) => value.resident_bytes().to_vec(),
        };
        let raw: [u8; 8] = raw
            .as_slice()
            .try_into()
            .map_err(|_| invalid(format!("blob-slice `{name}` was not an Int")))?;
        usize::try_from(i64::from_le_bytes(raw))
            .map_err(|_| invalid(format!("blob-slice `{name}` was negative")))
    };
    let start = bound(start, "start")?;
    let end = bound(end, "end")?;
    if start > end || end > bytes.len() {
        return Err(invalid(format!(
            "blob-slice range [{start}, {end}) exceeds the {}-byte blob",
            bytes.len()
        )));
    }
    ctx.intern_value(PrimitiveValue::bytes(
        Type::Extern(ExternKind::Blob).schema_ref(),
        bytes[start..end].to_vec(),
    ))
}
