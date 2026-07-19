use sha2::{Digest as _, Sha256};

use crate::schema::SchemaRef;
use crate::vir::{ExternKind, Type};

use super::{
    ArgRoleDecl, Digest, EffectCtx, EffectTicket, PrimitiveCompletion, PrimitiveDecl,
    PrimitiveMachineError, PrimitiveMemoPolicy, PrimitivePublication, ReadProjection, Receipt,
    ResponseDecl, Primitive, ValueId,
};
// Only the test-only hand parsers (the `decode_primitive_value` oracle) walk the
// wire `PrimitiveValue` structurally now; production `begin` decodes instead.
#[cfg(test)]
use super::{PrimitiveField, PrimitiveFieldValue, PrimitiveValue, PrimitiveValueBody};

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct UpstreamDigest(pub [u8; 32]);

/// A registry capability handle. Wire-side this is `Type::Extern(Registry)`; it
/// wraps a [`ValueId`] like [`BlobId`], but is a distinct newtype so the derived
/// schema walker (`Type::from_facet`) can tell the two wire meanings apart —
/// distinct meanings, distinct types.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct RegistryHandle(pub ValueId);

/// A pinned Blob target identity. This is not a resident value but a *reference*
/// to one, so it decomposes structurally into a `ValueId`'s `{schema, content}`:
/// the schema is an `Extern(Schema)` store value and the content is the digest
/// wire-encoded as a hex `String` (see [`Type::from_facet`]'s leaf overrides).
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct BlobId {
    pub schema: SchemaRef,
    pub content: Digest,
}

impl BlobId {
    pub fn new(value: ValueId) -> Result<Self, PrimitiveMachineError> {
        if value.schema != Type::Extern(ExternKind::Blob).schema_ref() {
            return Err(PrimitiveMachineError::InvalidRequest { request: value });
        }
        Ok(Self {
            schema: value.schema,
            content: value.content,
        })
    }

    #[must_use]
    pub fn id(&self) -> ValueId {
        ValueId {
            schema: self.schema.clone(),
            content: self.content,
        }
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct OriginHint {
    pub capability: RegistryHandle,
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
pub fn pinned_fetch_primitive_id() -> super::PrimitiveId {
    super::PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "pinned-fetch".to_owned(),
        version: 1,
    }
}

pub struct PinnedFetchPrimitive;

impl<Ctx> Primitive<Ctx> for PinnedFetchPrimitive {
    type Request = PinnedFetchRequest;
    type Deps = ();

    const DECL: PrimitiveDecl = PrimitiveDecl {
        namespace: "vix.machine",
        name: "fetch",
        id_name: "pinned-fetch",
        version: 1,
        memo_policy: PrimitiveMemoPolicy::Pinned,
        protocol_version: 1,
        response: ResponseDecl::Extern(ExternKind::Blob),
        failure_schema_name: "PinnedFetchFailure",
        capabilities: &[ExternKind::Registry],
        args: &[ArgRoleDecl::Value],
    };

    fn begin(&self, req: PinnedFetchRequest, ctx: EffectCtx, _deps: ()) -> EffectTicket {
        let (ticket, completer) = ctx.ticket(|| {});
        std::thread::spawn(move || {
            let completion = serve(req.pin, &ctx)
                .map(PrimitiveCompletion::Ok)
                .unwrap_or_else(PrimitiveCompletion::MachineError);
            let publication = ctx.finish(completion).unwrap_or_else(|error| PrimitivePublication {
                completion: PrimitiveCompletion::MachineError(error),
                receipt: Receipt {
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

fn serve(pin: PinnedBlobRef, ctx: &EffectCtx) -> Result<ValueId, PrimitiveMachineError> {
    let target = pin.value.id();

    if let Ok(stored) = ctx.read(&target, ReadProjection::Whole) {
        verify_upstream(&stored.bytes, pin.upstream.as_ref())?;
        return Ok(stored.identity);
    }

    let mut last_error = None;
    if let Some(candidate) = ctx.persisted_candidate(&target)? {
        if candidate.claimed != target {
            last_error = Some(PrimitiveMachineError::CorruptCandidate {
                source: candidate.claimed,
            });
        } else {
            let observed = blob_identity(&candidate.bytes);
            if observed != target {
                last_error = Some(PrimitiveMachineError::CorruptCandidate { source: observed });
            } else {
                verify_upstream(&candidate.bytes, pin.upstream.as_ref())?;
                return admit(&candidate.bytes, &target, ctx);
            }
        }
    }

    for origin in &pin.origins {
        match ctx.origin_candidate(&origin.capability.0, &origin.coordinate, &target) {
            Ok(bytes) => {
                verify_upstream(&bytes, pin.upstream.as_ref())?;
                let admitted = admit(&bytes, &target, ctx)?;
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

// `#[cfg(test)]`: since `begin` decodes via `decode_primitive_value`, these hand
// parsers survive only as the oracle for `primitive_value_decode`'s tests, which
// assert that decoder agrees with them on the same wire `PrimitiveValue` for the
// real request types. Test-only, so they don't dead-code-warn in a normal build.
#[cfg(test)]
pub(crate) fn parse_request(
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

#[cfg(test)]
pub(crate) fn parse_blob_id(
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

#[cfg(test)]
pub(crate) fn parse_origins(value: &PrimitiveValue) -> Result<Vec<OriginHint>, PrimitiveMachineError> {
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
            let capability = RegistryHandle(child(capability)?.identity());
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

#[cfg(test)]
pub(crate) fn parse_upstream(
    value: &PrimitiveValue,
) -> Result<Option<UpstreamDigest>, PrimitiveMachineError> {
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
        detail: "pinned fetch request disagrees with its declared schema".to_owned(),
    }
}

#[cfg(test)]
mod schema_snapshot {
    //! Wire-identity gate for the type-schema-unification PR.
    //!
    //! `PrimitiveDescriptor.request_schema = SchemaPattern::exact(&…type().schema_ref())`,
    //! so dispatch matching and wire identity depend on the *exact* schema bytes
    //! of each request/hint/ref type. These snapshots are the canonical
    //! `SchemaRef::Display` (see `schema.rs`) of the five types captured against
    //! the hand-written constructors; deriving them from the `Facet` shapes must
    //! keep every one byte-identical.

    use super::{BlobId, OriginHint, PinnedBlobRef, PinnedFetchRequest};
    use crate::runtime::ObserveRequest;
    use crate::vir::Type;

    // Captured against the hand-written `*_type()` constructors before they were
    // deleted; the derived `Type::from_facet` types must match byte-for-byte.
    const BLOB_ID: &str = "e5904d597f3968d0";
    const ORIGIN_HINT: &str = "ecd3ac5ba264e915";
    const PINNED_BLOB_REF: &str = "a077429ce555df22";
    const PINNED_FETCH_REQUEST: &str = "053ce66d21abed59";
    const OBSERVE_REQUEST: &str = "d0e09706fdc08ace";

    #[test]
    fn request_schemas_are_byte_identical() {
        assert_eq!(Type::from_facet::<BlobId>().schema_ref().to_string(), BLOB_ID);
        assert_eq!(
            Type::from_facet::<OriginHint>().schema_ref().to_string(),
            ORIGIN_HINT
        );
        assert_eq!(
            Type::from_facet::<PinnedBlobRef>().schema_ref().to_string(),
            PINNED_BLOB_REF
        );
        assert_eq!(
            Type::from_facet::<PinnedFetchRequest>()
                .schema_ref()
                .to_string(),
            PINNED_FETCH_REQUEST
        );
        assert_eq!(
            Type::from_facet::<ObserveRequest>()
                .schema_ref()
                .to_string(),
            OBSERVE_REQUEST
        );
    }
}
