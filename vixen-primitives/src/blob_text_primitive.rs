//! `blob-text` — lossless UTF-8 decoding of a `Blob`, as an EXPLICIT
//! projection.
//!
//! `machine.primitive.exec-outcome` settles that a stream's completed value is
//! a Blob and that *text decoding is an explicit projection, not machine
//! vocabulary*. This primitive is that projection: `Blob.try_text() ->
//! Result<String, DecodeError>`. Decoding is LOSSLESS — invalid UTF-8 is a
//! typed `DecodeError` value, never a replacement character — which is a
//! semantic improvement over the retired lossy line-map outcome shape: bytes
//! that were not text can no longer masquerade as text.
//!
//! The stdlib `text`/`lines` wrappers (house pattern of the decode aliases)
//! ride this primitive: `text` unwraps or raises the typed error, `lines`
//! frames the decoded text — line framing as stdlib, exactly where the spec
//! puts it.

use vix::schema::SchemaPattern;
use vix::vir::{RESULT_ERR_VARIANT, RESULT_OK_VARIANT, Type, decode_error_type};

use crate::rt::{
    EffectCtx, PrimitiveCompletion, PrimitiveDescriptor, PrimitiveField, PrimitiveFieldValue,
    PrimitiveId, PrimitiveMachineError, PrimitiveMemoPolicy, PrimitiveValue, PrimitiveValueBody,
    RawEffectTicket, RawPrimitive, ReadProjection, ValueId,
};

/// The request record `Blob.try_text()` lowers to: just the receiver.
#[must_use]
pub fn blob_text_request_type() -> Type {
    Type::Record(vix::vir::RecordType::new(
        "BlobTextRequest",
        vec![vix::vir::RecordField {
            name: "blob".to_owned(),
            ty: Type::Extern(vix::vir::ExternKind::Blob),
        }],
    ))
}

/// `Result<String, DecodeError>` — the decode either yields the text or a
/// typed error naming where the bytes stopped being UTF-8.
#[must_use]
pub fn blob_text_result_type() -> Type {
    Type::result(Type::String, decode_error_type())
}

#[must_use]
pub fn blob_text_primitive_id() -> PrimitiveId {
    PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "blob-text".to_owned(),
        version: 1,
    }
}

/// `Blob.try_text() -> Result<String, DecodeError>`. `Hermetic`: the result is
/// a pure function of the request value, whose identity folds in the blob's
/// content — same discipline as `blob-len`.
pub struct BlobTextPrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for BlobTextPrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: blob_text_primitive_id(),
                request_schema: SchemaPattern::exact(&blob_text_request_type().schema_ref()),
                response_schema: SchemaPattern::exact(&blob_text_result_type().schema_ref()),
                failure_schema: SchemaPattern::Var {
                    name: "BlobTextFailure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Hermetic,
                protocol_version: 1,
                capability_schemas: Vec::new(),
            },
        }
    }
}

impl<Ctx> RawPrimitive<Ctx> for BlobTextPrimitive {
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
    let request = ctx.read(request, ReadProjection::Whole)?;
    let bytes = blob_field(request.value, request.identity)?;
    let result_ty = blob_text_result_type();
    let value = match core::str::from_utf8(&bytes) {
        Ok(text) => result_variant(
            &result_ty,
            RESULT_OK_VARIANT,
            Type::String.schema_ref(),
            PrimitiveValue::bytes(Type::String.schema_ref(), text.as_bytes().to_vec()),
        ),
        Err(error) => {
            // A typed decode error, not a lossy replacement: the offsets say
            // exactly where the bytes stopped being UTF-8.
            let error_ty = decode_error_type();
            let Type::Record(error_record) = &error_ty else {
                unreachable!("DecodeError is a record type");
            };
            let field = |index: usize, bytes: Vec<u8>| PrimitiveField {
                schema: error_record.fields[index].ty.schema_ref(),
                value: PrimitiveFieldValue::Child(Box::new(PrimitiveValue::bytes(
                    error_record.fields[index].ty.schema_ref(),
                    bytes,
                ))),
            };
            let payload = PrimitiveValue {
                schema: error_ty.schema_ref(),
                body: PrimitiveValueBody::Product(vec![
                    field(0, b"expected UTF-8 text".to_vec()),
                    field(1, b"$bytes".to_vec()),
                    field(2, (error.valid_up_to() as i64).to_le_bytes().to_vec()),
                    field(3, (bytes.len() as i64).to_le_bytes().to_vec()),
                ]),
            };
            result_variant(
                &result_ty,
                RESULT_ERR_VARIANT,
                error_ty.schema_ref(),
                payload,
            )
        }
    };
    ctx.intern_value(value)
}

fn result_variant(
    result_ty: &Type,
    tag: u32,
    payload_schema: vix::schema::SchemaRef,
    payload: PrimitiveValue,
) -> PrimitiveValue {
    PrimitiveValue {
        schema: result_ty.schema_ref(),
        body: PrimitiveValueBody::Variant {
            tag,
            fields: vec![PrimitiveField {
                schema: payload_schema,
                value: PrimitiveFieldValue::Child(Box::new(payload)),
            }],
        },
    }
}

/// Extract the resident bytes of the request's single `Blob` field — the same
/// walk `blob-len` performs.
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
                    detail: "blob-text request field was not resident Blob bytes".to_owned(),
                });
            }
        },
    };
    Ok(bytes)
}
