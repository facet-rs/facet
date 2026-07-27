use std::io::Read as _;

use vix::schema::SchemaPattern;
use vix::vir::{ExternKind, Type};

use crate::rt::{
    BlobHandle, EffectCtx, PrimitiveCompletion, PrimitiveDescriptor, PrimitiveFieldValue,
    PrimitiveId, PrimitiveMachineError, PrimitiveMemoPolicy, PrimitiveValue, PrimitiveValueBody,
    RawEffectTicket, RawPrimitive, ReadProjection, ValueId,
};

/// The wire request of the `blob-gunzip` primitive: the compressed `Blob`. Like
/// `blob-len`, this rides the fully-open rail — a primitive-backed method whose
/// whole contract (request, result, id) is declared here beside its
/// implementation, with no dedicated op in `vix-core` at all (issue 2520).
#[derive(facet::Facet)]
pub struct BlobGunzipRequest {
    pub blob: BlobHandle,
}

#[must_use]
pub fn blob_gunzip_request_type() -> Type {
    Type::from_facet::<BlobGunzipRequest>()
}

#[must_use]
pub fn blob_gunzip_result_type() -> Type {
    Type::Extern(ExternKind::Blob)
}

#[must_use]
pub fn blob_gunzip_primitive_id() -> PrimitiveId {
    PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "blob-gunzip".to_owned(),
        version: 1,
    }
}

/// `Blob.gunzip() -> Blob` — decompress a gzip member.
///
/// **Why this exists at all:** a crates.io `.crate` is a *gzipped* tar, and
/// `untar` parses plain ustar. Until now the only archive the system had ever
/// unpacked was an uncompressed fixture that merely had a `.crate` name, so the
/// whole fetch→extract leg worked exclusively against a shape real registries do
/// not serve. `untar(blob.gunzip())` is the composition that reads a real one.
///
/// **Why a separate operation rather than teaching `untar` to sniff:**
/// decompression and archive parsing are two transforms, and fusing them would
/// make the container format invisible in the recipe — a program would no longer
/// say what it is unpacking, and `.tar.zst` would arrive as a silent third
/// behaviour of one name. Composition keeps each step nameable and cacheable on
/// its own.
///
/// **Why a primitive and not pure vix:** by the classification rule this is pure
/// work and belongs in the VIX layer, but vix cannot manipulate bytes at all yet,
/// so the implementation must be Rust. It is the same deliberate exception
/// `decode` already is — a *pragmatic* effect, not an authority crossing. It is
/// `Hermetic` accordingly: the result is a function of the request alone, whose
/// identity folds in the blob's content.
///
/// **Generalization, when a second algorithm appears:** the shape this wants
/// eventually is `blob.decompress(Compression::Gzip)`, with the algorithm as a
/// request field the way `decode(document, Format)` takes its format — one
/// operation, new variants without new primitives. That needs a `Compression`
/// enum in the stdlib; it is not worth the surface until something actually
/// serves zstd.
pub struct BlobGunzipPrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for BlobGunzipPrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: blob_gunzip_primitive_id(),
                request_schema: SchemaPattern::exact(&blob_gunzip_request_type().schema_ref()),
                response_schema: SchemaPattern::exact(&blob_gunzip_result_type().schema_ref()),
                failure_schema: SchemaPattern::Var {
                    name: "BlobGunzipFailure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Hermetic,
                protocol_version: 1,
                capability_schemas: Vec::new(),
            },
        }
    }
}

impl<Ctx> RawPrimitive<Ctx> for BlobGunzipPrimitive {
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
    let compressed = blob_field(request.value, request.identity)?;
    let plain = gunzip(&compressed)?;
    ctx.intern_value(PrimitiveValue::bytes(
        Type::Extern(ExternKind::Blob).schema_ref(),
        plain,
    ))
}

/// Decompress one gzip member.
///
/// A truncated or corrupt member is a typed failure rather than a partial Blob:
/// admitting the bytes decoded so far would let a torn download become a value
/// with an identity, and every consumer downstream would then be caching a
/// truncation.
fn gunzip(compressed: &[u8]) -> Result<Vec<u8>, PrimitiveMachineError> {
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(compressed)
        .read_to_end(&mut plain)
        .map_err(|error| PrimitiveMachineError::AuthorityViolation {
            detail: format!("Blob is not a readable gzip member: {error}"),
        })?;
    Ok(plain)
}

/// Extract the resident bytes of the request's single `Blob` field, exactly as
/// `blob-len` does — a `Blob` is a bytes primitive, so once the request is read
/// whole its blob field is resident bytes.
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
    match &blob.value {
        PrimitiveFieldValue::Inline(bytes) => Ok(bytes.clone()),
        PrimitiveFieldValue::Child(value) => match &value.body {
            PrimitiveValueBody::Bytes(bytes) => Ok(bytes.clone()),
            PrimitiveValueBody::Product(_)
            | PrimitiveValueBody::Sequence { .. }
            | PrimitiveValueBody::Variant { .. } => {
                Err(PrimitiveMachineError::AuthorityViolation {
                    detail: "blob-gunzip request field was not resident Blob bytes".to_owned(),
                })
            }
        },
    }
}
