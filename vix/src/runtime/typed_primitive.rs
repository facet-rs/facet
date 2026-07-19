//! The "declare a primitive in one block" authoring layer.
//!
//! A [`Primitive`] states everything a registered primitive is as *data* —
//! its identity, memo policy, response/failure schemas, curated capabilities,
//! and the surface-argument roles — plus one typed `begin` that receives its
//! already-decoded `Request` and the exact slice of the embedder `Ctx` it needs.
//! No hand-written [`PrimitiveDescriptor`], no per-primitive wire parser, no
//! bespoke [`RequestShape`] `match`: [`TypedAdapter`] synthesizes the descriptor
//! and shape once from the [`PrimitiveDecl`] and `Type::from_facet::<Request>()`,
//! and bridges the untyped `RawPrimitive<Ctx>` surface onto the typed one by reading
//! + [`decode_primitive_value`]-decoding the wire request before dispatch.
//!
//! The synthesis is required to be byte-identical to the descriptors/shapes the
//! migrated primitives (`fetch`, `observe`) hand-wrote before this layer — see
//! the `milestone` tests below, which reconstruct the pre-migration values and
//! assert equality.

use crate::schema::SchemaPattern;
use crate::vir::{ExternKind, RecordField, Type};

use super::{
    decode_primitive_value, ArgRole, EffectCtx, EffectTicket, FromRef, RawPrimitive,
    PrimitiveCompletion, PrimitiveDescriptor, PrimitiveId, PrimitiveMachineError,
    PrimitiveMemoPolicy, PrimitivePublication, ReadProjection, Receipt, RequestShape, Selector,
    SelectorVariant, ValueId,
};

/// One accepted variant of a selector argument and the boolean flag it folds
/// into the request record — the const-friendly mirror of [`SelectorVariant`].
pub struct SelectorVariantDecl {
    pub variant: &'static str,
    pub flag: bool,
}

/// A selector argument declared as const data — the mirror of [`Selector`]. The
/// accepted enum, its variants, and the diagnostic noun live here rather than in
/// a bespoke Rust reader per primitive.
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

/// How a primitive's response schema is declared: a concrete extern kind
/// (`Extern(Blob)` for fetch/observe) or a schema variable (a generic result).
pub enum ResponseDecl {
    Extern(ExternKind),
    Var(&'static str),
}

/// Everything a registered primitive *is*, as const data. Consumed once by
/// [`TypedAdapter::new`] to synthesize the [`PrimitiveDescriptor`] and
/// [`RequestShape`]; nothing here is heap-allocated and no [`Type`] is embedded
/// (the types come from `Type::from_facet::<Request>()`).
pub struct PrimitiveDecl {
    pub namespace: &'static str,
    /// The primitive's surface binding name in the prelude (`RawPrimitive::surface_name`).
    pub name: &'static str,
    /// The registered [`PrimitiveId`] name. Usually equal to `name`, but the
    /// two diverge where the surface spelling differs from the machine id — e.g.
    /// `fetch` (surface) is registered as `pinned-fetch` (id). Kept distinct so
    /// the synthesized descriptor id stays byte-identical to the hand-written one.
    pub id_name: &'static str,
    pub version: u32,
    pub memo_policy: PrimitiveMemoPolicy,
    pub protocol_version: u32,
    pub response: ResponseDecl,
    pub failure_schema_name: &'static str,
    /// The primitive's *curated* capability extern kinds — declared here, never
    /// derived from the request tree.
    pub capabilities: &'static [ExternKind],
    pub args: &'static [ArgRoleDecl],
}

impl PrimitiveDecl {
    #[must_use]
    fn id(&self) -> PrimitiveId {
        PrimitiveId {
            namespace: self.namespace.to_owned(),
            name: self.id_name.to_owned(),
            version: self.version,
        }
    }
}

/// A primitive declared in one block: its shape data (`DECL`), its typed request
/// and dependency-slice associated types, and one typed `begin`. The blanket
/// [`RawPrimitive`] bridge lives on [`TypedAdapter`], not here, because
/// `RawPrimitive::descriptor` returns `&PrimitiveDescriptor` and so needs an owner.
pub trait Primitive<Ctx>: Send + Sync {
    type Request: facet::Facet<'static>;
    type Deps: FromRef<Ctx>;
    const DECL: PrimitiveDecl;

    /// Serve an already-decoded request with the projected dependency slice.
    /// The wire read + decode happened in [`TypedAdapter`]; a decode failure
    /// never reaches here (it is completed synchronously by the adapter).
    fn begin(&self, req: Self::Request, ctx: EffectCtx, deps: Self::Deps) -> EffectTicket;
}

/// Bridges a [`Primitive`] onto the untyped [`RawPrimitive`] surface. Owns the
/// descriptor and shape it synthesizes once (so `descriptor()` can return `&`),
/// and, on `begin`, reads + decodes the wire request into `P::Request` before
/// handing off to the typed impl.
pub struct TypedAdapter<P> {
    inner: P,
    descriptor: PrimitiveDescriptor,
    shape: RequestShape,
}

impl<P> TypedAdapter<P> {
    #[must_use]
    pub fn new<Ctx>(inner: P) -> Self
    where
        P: Primitive<Ctx>,
    {
        let decl = <P as Primitive<Ctx>>::DECL;
        let request_ty = Type::from_facet::<P::Request>();
        let descriptor = synth_descriptor(&decl, &request_ty);
        let shape = synth_shape(&decl, request_ty);
        Self {
            inner,
            descriptor,
            shape,
        }
    }
}

fn response_pattern(response: &ResponseDecl) -> SchemaPattern {
    match response {
        ResponseDecl::Extern(kind) => SchemaPattern::exact(&Type::Extern(*kind).schema_ref()),
        ResponseDecl::Var(name) => SchemaPattern::Var {
            name: (*name).to_owned(),
        },
    }
}

/// The concrete result [`Type`] a `RequestShape` carries. Only a concrete
/// `Extern` response has one; a schema-variable response is generic and cannot
/// yet be expressed as a shape (no migrated primitive uses one).
fn response_result_ty(response: &ResponseDecl) -> Type {
    match response {
        ResponseDecl::Extern(kind) => Type::Extern(*kind),
        ResponseDecl::Var(name) => {
            panic!("a Var-response primitive (`{name}`) cannot express a RequestShape result type")
        }
    }
}

fn synth_descriptor(decl: &PrimitiveDecl, request_ty: &Type) -> PrimitiveDescriptor {
    PrimitiveDescriptor {
        id: decl.id(),
        request_schema: SchemaPattern::exact(&request_ty.schema_ref()),
        response_schema: response_pattern(&decl.response),
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

fn synth_shape(decl: &PrimitiveDecl, request_ty: Type) -> RequestShape {
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
        result: response_result_ty(&decl.response),
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

impl<Ctx, P> RawPrimitive<Ctx> for TypedAdapter<P>
where
    P: Primitive<Ctx>,
    P::Deps: FromRef<Ctx>,
{
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn surface_name(&self) -> Option<&'static str> {
        Some(<P as Primitive<Ctx>>::DECL.name)
    }

    fn request_shape(&self) -> Option<RequestShape> {
        Some(self.shape.clone())
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx, app: &Ctx) -> EffectTicket {
        // Read the wire request the way the hand-parsers did — this records the
        // read witness/receipt into the shared transaction, which the typed
        // `begin` (and its worker thread) then extend.
        let witnessed = match ctx.read(&request, ReadProjection::Whole) {
            Ok(witnessed) => witnessed,
            Err(error) => return complete_with_error(&ctx, error),
        };
        let req = match decode_primitive_value::<P::Request>(&witnessed.value) {
            Ok(req) => req,
            Err(error) => return complete_with_error(&ctx, error),
        };
        let deps = <P::Deps as FromRef<Ctx>>::from_ref(app);
        self.inner.begin(req, ctx, deps)
    }
}

/// Complete a demand synchronously with a machine error — the decode-failure
/// path, mirroring the `unwrap_or_else`/`finish` error shape the fetch/observe
/// `begin` bodies use when their own work fails.
fn complete_with_error(ctx: &EffectCtx, error: PrimitiveMachineError) -> EffectTicket {
    let (ticket, completer) = ctx.ticket(|| {});
    let publication = ctx
        .finish(PrimitiveCompletion::MachineError(error))
        .unwrap_or_else(|error| PrimitivePublication {
            completion: PrimitiveCompletion::MachineError(error),
            receipt: Receipt {
                demand: ctx.demand(),
                reads: Vec::new(),
            },
            journal: Vec::new(),
            progressive: Vec::new(),
        });
    let _ = completer.complete(publication);
    ticket
}

#[cfg(test)]
mod milestone {
    //! Byte-identity gate for the capstone: the descriptor and request-shape the
    //! [`TypedAdapter`] synthesizes for `fetch`/`observe` MUST equal what those
    //! primitives hand-wrote before this layer landed. The `old_*` builders below
    //! reconstruct the pre-migration literals verbatim; a mismatch is a synthesis
    //! bug, not a stale constant.

    use super::*;
    use crate::runtime::{
        observe_primitive_id, pinned_fetch_primitive_id, ObservePrimitive, ObserveRequest,
        OriginHint, PinnedBlobRef, PinnedFetchPrimitive, PinnedFetchRequest,
    };

    fn old_fetch_descriptor() -> PrimitiveDescriptor {
        PrimitiveDescriptor {
            id: pinned_fetch_primitive_id(),
            request_schema: SchemaPattern::exact(&Type::from_facet::<PinnedFetchRequest>().schema_ref()),
            response_schema: SchemaPattern::exact(&Type::Extern(ExternKind::Blob).schema_ref()),
            failure_schema: SchemaPattern::Var {
                name: "PinnedFetchFailure".to_owned(),
            },
            memo_policy: PrimitiveMemoPolicy::Pinned,
            protocol_version: 1,
            capability_schemas: vec![SchemaPattern::exact(
                &Type::Extern(ExternKind::Registry).schema_ref(),
            )],
        }
    }

    fn old_fetch_shape() -> RequestShape {
        RequestShape {
            args: vec![ArgRole::Value {
                expected: Type::from_facet::<PinnedBlobRef>(),
            }],
            request_ty: Type::from_facet::<PinnedFetchRequest>(),
            result: Type::Extern(ExternKind::Blob),
            primitive: pinned_fetch_primitive_id(),
        }
    }

    fn old_observe_descriptor() -> PrimitiveDescriptor {
        PrimitiveDescriptor {
            id: observe_primitive_id(),
            request_schema: SchemaPattern::exact(&Type::from_facet::<ObserveRequest>().schema_ref()),
            response_schema: SchemaPattern::exact(&Type::Extern(ExternKind::Blob).schema_ref()),
            failure_schema: SchemaPattern::Var {
                name: "ObserveFailure".to_owned(),
            },
            memo_policy: PrimitiveMemoPolicy::Observed,
            protocol_version: 1,
            capability_schemas: vec![SchemaPattern::exact(
                &Type::Extern(ExternKind::Registry).schema_ref(),
            )],
        }
    }

    fn old_observe_shape() -> RequestShape {
        RequestShape {
            args: vec![
                ArgRole::Value {
                    expected: Type::from_facet::<OriginHint>(),
                },
                ArgRole::Selector(Selector {
                    enum_name: "Mode".to_owned(),
                    noun: "observe mode".to_owned(),
                    variants: vec![
                        SelectorVariant {
                            variant: "Observe".to_owned(),
                            flag: false,
                        },
                        SelectorVariant {
                            variant: "Refresh".to_owned(),
                            flag: true,
                        },
                    ],
                }),
            ],
            request_ty: Type::from_facet::<ObserveRequest>(),
            result: Type::Extern(ExternKind::Blob),
            primitive: observe_primitive_id(),
        }
    }

    #[test]
    fn fetch_adapter_is_byte_identical_to_the_hand_written_primitive() {
        let adapter = TypedAdapter::new::<()>(PinnedFetchPrimitive);
        assert_eq!(*RawPrimitive::<()>::descriptor(&adapter), old_fetch_descriptor());
        assert_eq!(
            RawPrimitive::<()>::request_shape(&adapter),
            Some(old_fetch_shape())
        );
        assert_eq!(RawPrimitive::<()>::surface_name(&adapter), Some("fetch"));
    }

    #[test]
    fn observe_adapter_is_byte_identical_to_the_hand_written_primitive() {
        let adapter = TypedAdapter::new::<()>(ObservePrimitive);
        assert_eq!(
            *RawPrimitive::<()>::descriptor(&adapter),
            old_observe_descriptor()
        );
        assert_eq!(
            RawPrimitive::<()>::request_shape(&adapter),
            Some(old_observe_shape())
        );
        assert_eq!(RawPrimitive::<()>::surface_name(&adapter), Some("observe"));
    }
}
