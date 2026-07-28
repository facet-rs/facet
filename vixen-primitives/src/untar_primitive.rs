use vix::binding::TREE;
use vix::runtime::canonical_resident_tree;
use vix::vir::{ExternKind, Type};

use crate::rt::{
    ArgRoleDecl, BlobBytes, EffectCtx, EffectTicket, Primitive, PrimitiveDecl,
    PrimitiveMachineError, PrimitiveMemoPolicy, ResponseValue, ValueId,
};

/// The typed request of the `untar` primitive: the archive `Blob`.
///
/// `BlobBytes` carries its own `#[facet(vix::wire_extern = "Blob")]`, so the vix
/// `Type` is read off this shape and the whole contract — request, result, id —
/// lives here beside the implementation, exactly as `blob-gunzip` does.
#[derive(facet::Facet)]
pub struct UntarRequest {
    pub archive: BlobBytes,
}

/// A `Tree` **response**: an already-interned identity.
///
/// The embedder-handle counterpart to `vix-core`'s `BlobHandle`. Its
/// `#[facet(vix::wire_extern = "…")]` names a *host* extern, which is the whole
/// point of that seam being generic — `vixen` introduces a host-typed handle
/// without `vix-core` learning the name (issue 2520).
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[facet(vix::wire_extern = "Tree")]
pub struct TreeHandle(pub ValueId);

impl ResponseValue for TreeHandle {
    fn into_value(self) -> ValueId {
        self.0
    }
}

#[must_use]
pub fn untar_request_type() -> Type {
    Type::from_facet::<UntarRequest>()
}

#[must_use]
pub fn untar_result_type() -> Type {
    Type::Extern(ExternKind::Host(TREE))
}

#[must_use]
pub fn untar_primitive_id() -> crate::rt::PrimitiveId {
    UNTAR_DECL.id()
}

/// The registration declaration. `Hermetic` because the result is a function of
/// the request alone — the request's identity folds in the archive's content, so
/// the memo is keyed by exactly what the answer depends on, with no external
/// observation to witness.
pub const UNTAR_DECL: PrimitiveDecl = PrimitiveDecl {
    namespace: "vix.machine",
    name: "untar",
    id_name: "untar",
    version: 1,
    memo_policy: PrimitiveMemoPolicy::Hermetic,
    protocol_version: 1,
    failure_schema_name: "UntarFailure",
    capabilities: &[],
    args: &[ArgRoleDecl::Value],
};

/// `untar(blob) -> Tree` — expand an archive into the semantic tree it describes.
///
/// **Why this is a primitive and not a machine op.** It was `Op::Untar` until
/// now, one of the four ops that kept the vixen domain resident in the
/// scheduler (issue 2597). Nothing about it needs to be there: it crosses no
/// authority boundary, spawns nothing, and observes nothing outside its request
/// — it is pure bytes→Tree, the same shape `blob-gunzip` already proved. What
/// kept it in core was that `vix-core` owned the archive reader; what moves it
/// out is that the reader is now reachable as ordinary library surface.
///
/// **Why the result is interned as canonical bytes.** A `Tree`'s identity is
/// derived from the semantic value (`Tree::encode_canonical`), never from the
/// bytes it arrived in, so untarring an archive and untarring a carrier of the
/// same tree yield one identity. `Op::Untar` could set identity and resident
/// bytes independently — it built the store entry itself. A primitive cannot:
/// `EffectCtx::intern` derives the identity from the bytes it is handed. So the
/// bytes handed over *are* the canonical form, which is exactly the identity the
/// op produced, and `tree_from_resident` reads that form back. No identity
/// changes and no new authority is minted, which is the point — a primitive that
/// could name an identity unrelated to its bytes would be a hole in the rail.
pub struct UntarPrimitive;

impl<Ctx> Primitive<Ctx> for UntarPrimitive {
    type Request = UntarRequest;
    type Response = TreeHandle;
    type Deps = ();

    const DECL: PrimitiveDecl = UNTAR_DECL;

    fn begin(&self, req: UntarRequest, ctx: EffectCtx, _deps: ()) -> EffectTicket<TreeHandle> {
        let (ticket, completer) = EffectTicket::<TreeHandle>::pair(&ctx, || {});
        std::thread::spawn(move || {
            let _ = match serve(&req.archive.0, &ctx) {
                Ok(value) => completer.complete_ok(&ctx, TreeHandle(value)),
                Err(error) => completer.complete_err(&ctx, error),
            };
        });
        ticket
    }
}

/// Read the archive and intern the tree it describes.
///
/// No `EffectCtx::read` is needed: a Blob argument arrives as its resident
/// bytes, and those bytes are already folded into the request identity that keys
/// the memo. There is nothing external to witness.
fn serve(archive: &[u8], ctx: &EffectCtx) -> Result<ValueId, PrimitiveMachineError> {
    let canonical = canonical_resident_tree(archive).map_err(|error| {
        PrimitiveMachineError::AuthorityViolation {
            detail: format!("Blob does not describe a tree: {error}"),
        }
    })?;
    ctx.intern(&untar_result_type().schema_ref(), &canonical)
}
