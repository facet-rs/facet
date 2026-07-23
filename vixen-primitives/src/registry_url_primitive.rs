use vix::schema::SchemaPattern;
use vix::vir::{ExternKind, OPTION_NONE_VARIANT, OPTION_SOME_VARIANT, Type};

use crate::rt::{
    BlobId, EffectCtx, OriginHint, PinnedBlobRef, PrimitiveCompletion, PrimitiveDescriptor,
    PrimitiveField, PrimitiveFieldValue, PrimitiveMachineError, PrimitiveMemoPolicy, PrimitiveValue,
    PrimitiveValueBody, RawEffectTicket, RawPrimitive, ReadProjection, ValueId,
    registry_url_primitive_id, registry_url_request_type,
};

/// `Registry.url(name) -> PinnedBlobRef` — resolve an artifact name against the
/// offline harness registry manifest into a pinned Blob reference (provenance URL
/// plus the REQUIRED content hash). The manifest lookup is vixen domain logic, so
/// it lives here rather than in the language core; the manifest bytes reach the
/// primitive through `EffectCtx::read(registry, RegistryManifest)`, exactly as
/// `tree-read` reaches fixture file bytes through a `TreePath` read.
pub struct RegistryUrlPrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for RegistryUrlPrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: registry_url_primitive_id(),
                request_schema: SchemaPattern::exact(&registry_url_request_type().schema_ref()),
                response_schema: SchemaPattern::exact(
                    &Type::from_facet::<PinnedBlobRef>().schema_ref(),
                ),
                failure_schema: SchemaPattern::Var {
                    name: "RegistryUrlFailure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Observed,
                protocol_version: 1,
                capability_schemas: vec![SchemaPattern::exact(
                    &Type::Extern(ExternKind::Registry).schema_ref(),
                )],
            },
        }
    }
}

impl<Ctx> RawPrimitive<Ctx> for RegistryUrlPrimitive {
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
    let (registry, name) = parse_request(request.value, request.identity)?;
    let registry_id = registry.identity();
    let name = core::str::from_utf8(bytes(&name)?)
        .map_err(|_| invalid_value("registry artifact name was not UTF-8"))?
        .to_owned();

    let witnessed = ctx.read(&registry_id, ReadProjection::RegistryManifest)?;
    let manifest = core::str::from_utf8(&witnessed.bytes)
        .map_err(|_| invalid_value("registry manifest was not UTF-8"))?;
    let (url, hash, upstream) = resolve(manifest, &name).ok_or_else(|| {
        PrimitiveMachineError::Unavailable {
            detail: format!("fixture registry artifact {name} is absent"),
        }
    })?;

    ctx.intern_value(build_pin(&registry, url, hash, upstream))
}

/// Find the manifest row for `name`, yielding `(url, hash, optional upstream)`.
/// The manifest is whitespace-columnar: `artifact url hash [upstream]`.
fn resolve(manifest: &str, name: &str) -> Option<(String, String, Option<String>)> {
    manifest.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let artifact = fields.next()?;
        let url = fields.next()?;
        let hash = fields.next()?;
        let upstream = fields.next().map(str::to_owned);
        (artifact == name).then(|| (url.to_owned(), hash.to_owned(), upstream))
    })
}

/// Assemble the `PinnedBlobRef` result value. This mirrors the wire shape the
/// retired `Op::RegistryUrl` machine op built by hand, so a resolved pin keeps
/// its byte-for-byte value identity: a `BlobId` (the `Blob` schema plus the hex
/// content hash), a single `OriginHint` (the registry capability plus the
/// provenance URL), and the optional upstream digest.
fn build_pin(
    registry: &PrimitiveValue,
    url: String,
    hash: String,
    upstream: Option<String>,
) -> PrimitiveValue {
    let blob_schema = Type::Extern(ExternKind::Blob).schema_ref();
    let blob_id = PrimitiveValue {
        schema: Type::from_facet::<BlobId>().schema_ref(),
        body: PrimitiveValueBody::Product(vec![
            child(PrimitiveValue::bytes(
                Type::Extern(ExternKind::Schema).schema_ref(),
                blob_schema.canonical_bytes(),
            )),
            child(PrimitiveValue::bytes(
                Type::String.schema_ref(),
                hash.into_bytes(),
            )),
        ]),
    };
    let origin = PrimitiveValue {
        schema: Type::from_facet::<OriginHint>().schema_ref(),
        body: PrimitiveValueBody::Product(vec![
            child(registry.clone()),
            child(PrimitiveValue::bytes(
                Type::String.schema_ref(),
                url.into_bytes(),
            )),
        ]),
    };
    PrimitiveValue {
        schema: Type::from_facet::<PinnedBlobRef>().schema_ref(),
        body: PrimitiveValueBody::Product(vec![
            child(blob_id),
            child(PrimitiveValue {
                schema: Type::array(Type::from_facet::<OriginHint>()).schema_ref(),
                body: PrimitiveValueBody::Sequence {
                    element_schema: Type::from_facet::<OriginHint>().schema_ref(),
                    elements: vec![origin],
                },
            }),
            child(PrimitiveValue {
                schema: Type::option(Type::String).schema_ref(),
                body: upstream.map_or_else(
                    || PrimitiveValueBody::Variant {
                        tag: OPTION_NONE_VARIANT,
                        fields: Vec::new(),
                    },
                    |upstream| PrimitiveValueBody::Variant {
                        tag: OPTION_SOME_VARIANT,
                        fields: vec![child(PrimitiveValue::bytes(
                            Type::String.schema_ref(),
                            upstream.into_bytes(),
                        ))],
                    },
                ),
            }),
        ]),
    }
}

fn parse_request(
    request: PrimitiveValue,
    request_id: ValueId,
) -> Result<(PrimitiveValue, PrimitiveValue), PrimitiveMachineError> {
    let PrimitiveValueBody::Product(fields) = request.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    let [registry, name] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    Ok((field_child(registry)?.clone(), field_child(name)?.clone()))
}

fn child(value: PrimitiveValue) -> PrimitiveField {
    PrimitiveField {
        schema: value.schema.clone(),
        value: PrimitiveFieldValue::Child(Box::new(value)),
    }
}

fn field_child(field: &PrimitiveField) -> Result<&PrimitiveValue, PrimitiveMachineError> {
    let PrimitiveFieldValue::Child(value) = &field.value else {
        return Err(invalid_value(
            "registry-url request field disagrees with its declared schema",
        ));
    };
    Ok(value)
}

fn bytes(value: &PrimitiveValue) -> Result<&[u8], PrimitiveMachineError> {
    let PrimitiveValueBody::Bytes(bytes) = &value.body else {
        return Err(invalid_value(
            "registry-url request value was not resident bytes",
        ));
    };
    Ok(bytes)
}

fn invalid_value(detail: &str) -> PrimitiveMachineError {
    PrimitiveMachineError::AuthorityViolation {
        detail: detail.to_owned(),
    }
}
