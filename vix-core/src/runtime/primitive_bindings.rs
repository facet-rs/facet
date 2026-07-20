//! Primitive *binding descriptors*: the compiler/binder's view of the builtin
//! primitives — their [`PrimitiveId`]s, request types, and the surface-nameable
//! wire types (`OriginHint`, `PinnedBlobRef`, …) a program references.
//!
//! This is the language side of the primitive boundary and lives in the core
//! language crate: lowering binds calls against these ids and types. The
//! matching `RawPrimitive` *implementations* (`PinnedFetchPrimitive`,
//! `ObservePrimitive`, `TreeReadPrimitive`, …) live in the `vixen` runtime that
//! embeds the language — the descriptors here are all it needs to compile a
//! program that calls them.

use crate::schema::SchemaRef;
use crate::vir::{ExternKind, RecordField, RecordType, Type};

use super::{Digest, PrimitiveId, PrimitiveMachineError, ValueId};

// ---- fetch / blob ---------------------------------------------------------

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct UpstreamDigest(pub [u8; 32]);

/// A registry capability handle. Wire-side this is `Type::Extern(Registry)`; it
/// wraps a [`ValueId`] like [`BlobId`], but is a distinct newtype so the derived
/// schema walker (`Type::from_facet`) can tell the two wire meanings apart —
/// distinct meanings, distinct types.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct RegistryHandle(pub ValueId);

/// A served Blob handle: the typed response of `fetch`/`observe`. Wire-side this
/// is `Type::Extern(Blob)` (see the `vir` leaf override), and it already *is* an
/// interned [`ValueId`] — completing with it never re-encodes or re-interns. The
/// `ResponseValue` impl lives with the primitive machinery in the runtime crate.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct BlobHandle(pub ValueId);

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
pub fn pinned_fetch_primitive_id() -> PrimitiveId {
    PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "pinned-fetch".to_owned(),
        version: 1,
    }
}

// ---- observe --------------------------------------------------------------

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
pub fn observe_primitive_id() -> PrimitiveId {
    PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "observe".to_owned(),
        version: 1,
    }
}

// ---- tree-read ------------------------------------------------------------

#[must_use]
pub fn tree_read_request_type() -> Type {
    Type::Record(RecordType::new(
        "TreeReadRequest",
        vec![
            RecordField {
                name: "tree".to_owned(),
                ty: Type::Extern(ExternKind::Tree),
            },
            RecordField {
                name: "path".to_owned(),
                ty: Type::Path,
            },
        ],
    ))
}

#[must_use]
pub fn tree_read_primitive_id() -> PrimitiveId {
    PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "tree-read".to_owned(),
        version: 1,
    }
}
