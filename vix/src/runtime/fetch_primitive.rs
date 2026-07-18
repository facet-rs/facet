use sha2::{Digest as _, Sha256};

use crate::schema::{SchemaPattern, SchemaRef};
use crate::vir::{ExternKind, RecordField, RecordType, Type};

use super::{
    Digest, EffectCtx, EffectTicket, Primitive, PrimitiveCompletion, PrimitiveDescriptor,
    PrimitiveField, PrimitiveFieldValue, PrimitiveMachineError, PrimitiveMemoPolicy,
    PrimitiveValue, PrimitiveValueBody, ReadProjection, ValueId,
};

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct UpstreamDigest(pub [u8; 32]);

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct BlobId(pub ValueId);

impl BlobId {
    pub fn new(value: ValueId) -> Result<Self, PrimitiveMachineError> {
        if value.schema != Type::Extern(ExternKind::Blob).schema_ref() {
            return Err(PrimitiveMachineError::InvalidRequest { request: value });
        }
        Ok(Self(value))
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct OriginHint {
    pub capability: ValueId,
    pub coordinate: String,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PinnedBlobRef {
    pub value: BlobId,
    pub origins: Vec<OriginHint>,
    pub upstream: Option<UpstreamDigest>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PinnedFetchRequest {
    pub pin: PinnedBlobRef,
}

#[must_use]
pub fn blob_id_type() -> Type {
    Type::Record(RecordType::new(
        "BlobId",
        vec![
            RecordField {
                name: "schema".to_owned(),
                ty: Type::Extern(ExternKind::Schema),
            },
            RecordField {
                name: "content".to_owned(),
                ty: Type::String,
            },
        ],
    ))
}

#[must_use]
pub fn origin_hint_type() -> Type {
    Type::Record(RecordType::new(
        "OriginHint",
        vec![
            RecordField {
                name: "capability".to_owned(),
                ty: Type::Extern(ExternKind::Registry),
            },
            RecordField {
                name: "coordinate".to_owned(),
                ty: Type::String,
            },
        ],
    ))
}

#[must_use]
pub fn pinned_blob_ref_type() -> Type {
    Type::Record(RecordType::new(
        "PinnedBlobRef",
        vec![
            RecordField {
                name: "value".to_owned(),
                ty: blob_id_type(),
            },
            RecordField {
                name: "origins".to_owned(),
                ty: Type::array(origin_hint_type()),
            },
            RecordField {
                name: "upstream".to_owned(),
                ty: Type::option(Type::String),
            },
        ],
    ))
}

#[must_use]
pub fn pinned_fetch_request_type() -> Type {
    Type::Record(RecordType::new(
        "PinnedFetchRequest",
        vec![RecordField {
            name: "pin".to_owned(),
            ty: pinned_blob_ref_type(),
        }],
    ))
}

#[must_use]
pub fn pinned_fetch_primitive_id() -> super::PrimitiveId {
    super::PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "pinned-fetch".to_owned(),
        version: 1,
    }
}

pub struct PinnedFetchPrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for PinnedFetchPrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: pinned_fetch_primitive_id(),
                request_schema: SchemaPattern::exact(&pinned_fetch_request_type().schema_ref()),
                response_schema: SchemaPattern::exact(&Type::Extern(ExternKind::Blob).schema_ref()),
                failure_schema: SchemaPattern::Var {
                    name: "PinnedFetchFailure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Pinned,
                protocol_version: 1,
                capability_schemas: vec![SchemaPattern::exact(
                    &Type::Extern(ExternKind::Registry).schema_ref(),
                )],
            },
        }
    }
}

impl<Ctx> Primitive<Ctx> for PinnedFetchPrimitive {
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
    let request = ctx.read(request, ReadProjection::Whole)?;
    let pin = parse_request(request.value, request.identity)?;

    if let Ok(stored) = ctx.read(&pin.value.0, ReadProjection::Whole) {
        verify_upstream(&stored.bytes, pin.upstream.as_ref())?;
        return Ok(stored.identity);
    }

    let mut last_error = None;
    if let Some(candidate) = ctx.persisted_candidate(&pin.value.0)? {
        if candidate.claimed != pin.value.0 {
            last_error = Some(PrimitiveMachineError::CorruptCandidate {
                source: candidate.claimed,
            });
        } else {
            let observed = blob_identity(&candidate.bytes);
            if observed != pin.value.0 {
                last_error = Some(PrimitiveMachineError::CorruptCandidate { source: observed });
            } else {
                verify_upstream(&candidate.bytes, pin.upstream.as_ref())?;
                return admit(&candidate.bytes, &pin.value.0, ctx);
            }
        }
    }

    for origin in &pin.origins {
        match ctx.origin_candidate(&origin.capability, &origin.coordinate, &pin.value.0) {
            Ok(bytes) => {
                verify_upstream(&bytes, pin.upstream.as_ref())?;
                let admitted = admit(&bytes, &pin.value.0, ctx)?;
                ctx.persist_value(&admitted, &bytes)?;
                return Ok(admitted);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(
        last_error.unwrap_or_else(|| PrimitiveMachineError::Unavailable {
            detail: "pinned Blob has no admissible serving source".to_owned(),
        }),
    )
}

fn admit(
    bytes: &[u8],
    expected: &ValueId,
    ctx: &EffectCtx,
) -> Result<ValueId, PrimitiveMachineError> {
    let admitted = ctx.intern(&Type::Extern(ExternKind::Blob).schema_ref(), bytes)?;
    if &admitted != expected {
        return Err(PrimitiveMachineError::CorruptCandidate { source: admitted });
    }
    Ok(admitted)
}

fn blob_identity(bytes: &[u8]) -> ValueId {
    super::FramedNode::leaf(Type::Extern(ExternKind::Blob).schema_ref(), bytes.to_vec()).identity()
}

fn verify_upstream(
    bytes: &[u8],
    expected: Option<&UpstreamDigest>,
) -> Result<(), PrimitiveMachineError> {
    let Some(expected) = expected else {
        return Ok(());
    };
    let observed: [u8; 32] = Sha256::digest(bytes).into();
    if observed != expected.0 {
        return Err(PrimitiveMachineError::PolicyRejected {
            detail: "Blob body contradicts its upstream digest".to_owned(),
        });
    }
    Ok(())
}

fn parse_request(
    request: PrimitiveValue,
    request_id: ValueId,
) -> Result<PinnedBlobRef, PrimitiveMachineError> {
    let PrimitiveValueBody::Product(request_fields) = request.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let [pin] = request_fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let pin = child(pin)?;
    let PrimitiveValueBody::Product(fields) = &pin.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let [value, origins, upstream] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let value = parse_blob_id(child(value)?, &request_id)?;
    let origins = parse_origins(child(origins)?)?;
    let upstream = parse_upstream(child(upstream)?)?;
    Ok(PinnedBlobRef {
        value,
        origins,
        upstream,
    })
}

fn parse_blob_id(
    value: &PrimitiveValue,
    request: &ValueId,
) -> Result<BlobId, PrimitiveMachineError> {
    let PrimitiveValueBody::Product(fields) = &value.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        });
    };
    let [schema, content] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        });
    };
    let schema = SchemaRef::from_canonical_bytes(bytes(child(schema)?)?).map_err(|_| {
        PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        }
    })?;
    let content = core::str::from_utf8(bytes(child(content)?)?)
        .ok()
        .and_then(|text| hex::decode(text).ok())
        .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
        .ok_or_else(|| PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        })?;
    BlobId::new(ValueId {
        schema,
        content: Digest(content),
    })
}

fn parse_origins(value: &PrimitiveValue) -> Result<Vec<OriginHint>, PrimitiveMachineError> {
    let PrimitiveValueBody::Sequence { elements, .. } = &value.body else {
        return Err(invalid_value());
    };
    elements
        .iter()
        .map(|origin| {
            let PrimitiveValueBody::Product(fields) = &origin.body else {
                return Err(invalid_value());
            };
            let [capability, coordinate] = fields.as_slice() else {
                return Err(invalid_value());
            };
            let capability = child(capability)?.identity();
            let coordinate = core::str::from_utf8(bytes(child(coordinate)?)?)
                .map_err(|_| invalid_value())?
                .to_owned();
            Ok(OriginHint {
                capability,
                coordinate,
            })
        })
        .collect()
}

fn parse_upstream(value: &PrimitiveValue) -> Result<Option<UpstreamDigest>, PrimitiveMachineError> {
    let PrimitiveValueBody::Variant { tag, fields } = &value.body else {
        return Err(invalid_value());
    };
    match (*tag, fields.as_slice()) {
        (0, [digest]) => {
            let digest = core::str::from_utf8(bytes(child(digest)?)?)
                .ok()
                .and_then(|text| hex::decode(text).ok())
                .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
                .ok_or_else(invalid_value)?;
            Ok(Some(UpstreamDigest(digest)))
        }
        (1, []) => Ok(None),
        _ => Err(invalid_value()),
    }
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
        detail: "pinned fetch request disagrees with its declared schema".to_owned(),
    }
}
