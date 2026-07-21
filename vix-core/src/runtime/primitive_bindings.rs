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

use crate::schema::{SchemaPattern, SchemaRef};
use crate::vir::{ExternKind, RecordField, RecordType, Type};

use super::{
    ArgRole, Digest, PrimitiveDescriptor, PrimitiveId, PrimitiveMachineError, PrimitiveMemoPolicy,
    RequestShape, Selector, SelectorVariant, ValueId,
};

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

// ---- surface-binding decls (the language's view of a primitive's call shape) --
//
// A primitive's *surface contract* — the prelude name it binds, the request
// record it lowers to, its capabilities and memo policy — is language-level
// metadata the compiler needs to lower a call, independent of the primitive's
// implementation. It is authored here as const data; the `vixen` runtime's
// typed authoring layer (`Primitive`/`TypedAdapter`) reuses the same decls and
// [`synth_descriptor`]/[`synth_shape`] to build the runtime descriptor, so the
// two never diverge.

/// One accepted variant of a selector argument and the boolean flag it folds
/// into the request record — the const-friendly mirror of [`SelectorVariant`].
pub struct SelectorVariantDecl {
    pub variant: &'static str,
    pub flag: bool,
}

/// A selector argument declared as const data — the mirror of [`Selector`].
pub struct SelectorDecl {
    pub enum_name: &'static str,
    pub noun: &'static str,
    pub variants: &'static [SelectorVariantDecl],
}

/// The role a surface argument plays, declared as const data. `Value` carries no
/// type: its expected [`Type`] is the *i-th* field of `Type::from_facet::<Request>()`,
/// zipped in order, so the request struct is the single source of the arg types.
pub enum ArgRoleDecl {
    Value,
    Selector(SelectorDecl),
}

/// Everything a registered primitive's surface contract *is*, as const data.
/// Consumed to synthesize the [`PrimitiveDescriptor`] and [`RequestShape`];
/// nothing here is heap-allocated and no [`Type`] is embedded (the types come
/// from `Type::from_facet::<Request>()`).
pub struct PrimitiveDecl {
    pub namespace: &'static str,
    /// The primitive's surface binding name in the prelude (`RawPrimitive::surface_name`).
    pub name: &'static str,
    /// The registered [`PrimitiveId`] name. Usually equal to `name`, but the two
    /// diverge where the surface spelling differs from the machine id — e.g.
    /// `fetch` (surface) is registered as `pinned-fetch` (id).
    pub id_name: &'static str,
    pub version: u32,
    pub memo_policy: PrimitiveMemoPolicy,
    pub protocol_version: u32,
    pub failure_schema_name: &'static str,
    /// The primitive's *curated* capability extern kinds — declared here, never
    /// derived from the request tree.
    pub capabilities: &'static [ExternKind],
    pub args: &'static [ArgRoleDecl],
}

impl PrimitiveDecl {
    #[must_use]
    pub fn id(&self) -> PrimitiveId {
        PrimitiveId {
            namespace: self.namespace.to_owned(),
            name: self.id_name.to_owned(),
            version: self.version,
        }
    }
}

/// Synthesize the runtime descriptor from a decl and its request/response types.
#[must_use]
pub fn synth_descriptor(
    decl: &PrimitiveDecl,
    request_ty: &Type,
    response_ty: &Type,
) -> PrimitiveDescriptor {
    PrimitiveDescriptor {
        id: decl.id(),
        request_schema: SchemaPattern::exact(&request_ty.schema_ref()),
        response_schema: SchemaPattern::exact(&response_ty.schema_ref()),
        failure_schema: SchemaPattern::Var {
            name: decl.failure_schema_name.to_owned(),
        },
        memo_policy: decl.memo_policy,
        protocol_version: decl.protocol_version,
        capability_schemas: decl
            .capabilities
            .iter()
            .map(|kind| SchemaPattern::exact(&Type::Extern(*kind).schema_ref()))
            .collect(),
    }
}

/// Synthesize the surface [`RequestShape`] the compiler lowers a call through.
#[must_use]
pub fn synth_shape(decl: &PrimitiveDecl, request_ty: Type, response_ty: Type) -> RequestShape {
    let fields = record_fields(&request_ty);
    let args = decl
        .args
        .iter()
        .zip(fields)
        .map(|(arg, field)| match arg {
            ArgRoleDecl::Value => ArgRole::Value {
                expected: field.ty.clone(),
            },
            ArgRoleDecl::Selector(selector) => ArgRole::Selector(Selector {
                enum_name: selector.enum_name.to_owned(),
                noun: selector.noun.to_owned(),
                variants: selector
                    .variants
                    .iter()
                    .map(|variant| SelectorVariant {
                        variant: variant.variant.to_owned(),
                        flag: variant.flag,
                    })
                    .collect(),
            }),
        })
        .collect();
    RequestShape {
        args,
        result: response_ty,
        primitive: decl.id(),
        request_ty,
    }
}

/// The record fields a request type contributes, in order. A request is always a
/// record (one field per surface argument); anything else contributes none.
fn record_fields(request_ty: &Type) -> &[RecordField] {
    match request_ty {
        Type::Record(record) => &record.fields,
        _ => &[],
    }
}

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

/// The `fetch` primitive's surface contract (registered id `pinned-fetch`).
pub const FETCH_DECL: PrimitiveDecl = PrimitiveDecl {
    namespace: "vix.machine",
    name: "fetch",
    id_name: "pinned-fetch",
    version: 1,
    memo_policy: PrimitiveMemoPolicy::Pinned,
    protocol_version: 1,
    failure_schema_name: "PinnedFetchFailure",
    capabilities: &[ExternKind::Registry],
    args: &[ArgRoleDecl::Value],
};

/// The `observe` primitive's surface contract.
pub const OBSERVE_DECL: PrimitiveDecl = PrimitiveDecl {
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

/// A primitive's surface projection: the source name it binds, its id, and the
/// request shape the compiler lowers its call through. This is the language's
/// knowledge of a primitive's contract, not its host implementation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveSurface {
    pub surface_name: &'static str,
    pub id: PrimitiveId,
    pub shape: RequestShape,
}

/// Backward-compatible name for the statically bundled subset of primitive
/// surfaces. Custom and bundled primitives share the same contract type.
pub type BuiltinPrimitiveSurface = PrimitiveSurface;

/// The builtin primitives that project a prelude free-function surface: `fetch`
/// and `observe`. `tree-read` binds only as a `.text()` method (no surface name),
/// and `decode`/`try_decode` share one hand-registered id — neither appears here.
#[must_use]
pub fn builtin_primitive_surfaces() -> Vec<BuiltinPrimitiveSurface> {
    fn surface<Request: facet::Facet<'static>, Response: facet::Facet<'static>>(
        decl: &PrimitiveDecl,
    ) -> PrimitiveSurface {
        let request_ty = Type::from_facet::<Request>();
        let response_ty = Type::from_facet::<Response>();
        PrimitiveSurface {
            surface_name: decl.name,
            id: decl.id(),
            shape: synth_shape(decl, request_ty, response_ty),
        }
    }
    vec![
        surface::<PinnedFetchRequest, BlobHandle>(&FETCH_DECL),
        surface::<ObserveRequest, BlobHandle>(&OBSERVE_DECL),
    ]
}
