use std::io::Read as _;

use vix::vir::{ExternKind, Type};

use crate::rt::{
    ArgRoleDecl, BlobBytes, BlobHandle, EffectCtx, EffectTicket, Primitive, PrimitiveDecl,
    PrimitiveMachineError, PrimitiveMemoPolicy, ValueId,
};

/// The typed request of the `blob-gunzip` primitive: the compressed `Blob`.
///
/// `BlobBytes` carries its own `#[facet(vix::wire_extern = "Blob")]`, so the vix
/// `Type` is read off this shape and the whole contract — request, result, id —
/// lives here beside the implementation. `vix-core` holds nothing of it:
/// `Blob.gunzip` is a primitive-backed method on the fully-open rail
/// (`binding::MethodLowering::Primitive`), with no dedicated op in core at all
/// (issue 2520).
#[derive(facet::Facet)]
pub struct BlobGunzipRequest {
    pub blob: BlobBytes,
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
pub fn blob_gunzip_primitive_id() -> crate::rt::PrimitiveId {
    BLOB_GUNZIP_DECL.id()
}

/// The registration declaration. `Hermetic` because the result is a function of
/// the request alone — the request's identity folds in the blob's content, so the
/// memo is keyed by exactly what the answer depends on, with no external
/// observation to witness.
pub const BLOB_GUNZIP_DECL: PrimitiveDecl = PrimitiveDecl {
    namespace: "vix.machine",
    name: "blob-gunzip",
    id_name: "blob-gunzip",
    version: 1,
    memo_policy: PrimitiveMemoPolicy::Hermetic,
    protocol_version: 1,
    failure_schema_name: "BlobGunzipFailure",
    capabilities: &[],
    args: &[ArgRoleDecl::Value],
};

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
/// `decode` already is — a *pragmatic* effect, not an authority crossing.
///
/// **Generalization, when a second algorithm appears:** the shape this wants
/// eventually is `blob.decompress(Compression::Gzip)`, with the algorithm as a
/// request field the way `decode(document, Format)` takes its format — one
/// operation, new variants without new primitives. That needs a `Compression`
/// enum in the stdlib; it is not worth the surface until something actually
/// serves zstd.
pub struct BlobGunzipPrimitive;

impl<Ctx> Primitive<Ctx> for BlobGunzipPrimitive {
    type Request = BlobGunzipRequest;
    type Response = BlobHandle;
    type Deps = ();

    const DECL: PrimitiveDecl = BLOB_GUNZIP_DECL;

    fn begin(&self, req: BlobGunzipRequest, ctx: EffectCtx, _deps: ()) -> EffectTicket<BlobHandle> {
        let (ticket, completer) = EffectTicket::<BlobHandle>::pair(&ctx, || {});
        std::thread::spawn(move || {
            let _ = match serve(&req.blob.0, &ctx) {
                Ok(value) => completer.complete_ok(&ctx, BlobHandle(value)),
                Err(error) => completer.complete_err(&ctx, error),
            };
        });
        ticket
    }
}

/// Decompress the request's blob and intern the result as an ordinary `Blob`.
///
/// No `EffectCtx::read` is needed: a Blob argument arrives as its resident bytes,
/// and those bytes are already folded into the request identity that keys the
/// memo. There is nothing external to witness.
fn serve(compressed: &[u8], ctx: &EffectCtx) -> Result<ValueId, PrimitiveMachineError> {
    let plain = gunzip(compressed)?;
    ctx.intern(&Type::Extern(ExternKind::Blob).schema_ref(), &plain)
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
