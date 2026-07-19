use crate::schema::SchemaPattern;
use crate::vir::{ExternKind, RecordField, RecordType, Type};

use super::{
    EffectCtx, EffectTicket, RawPrimitive, PrimitiveCompletion, PrimitiveDescriptor, PrimitiveField,
    PrimitiveFieldValue, PrimitiveMachineError, PrimitiveMemoPolicy, PrimitiveValue,
    PrimitiveValueBody, ReadProjection, ValueId, fixture_tree_name,
};

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
pub fn tree_read_primitive_id() -> super::PrimitiveId {
    super::PrimitiveId {
        namespace: "vix.machine".to_owned(),
        name: "tree-read".to_owned(),
        version: 1,
    }
}

pub struct TreeReadPrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for TreeReadPrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: tree_read_primitive_id(),
                request_schema: SchemaPattern::exact(&tree_read_request_type().schema_ref()),
                response_schema: SchemaPattern::exact(&Type::String.schema_ref()),
                failure_schema: SchemaPattern::Var {
                    name: "TreeReadFailure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Observed,
                protocol_version: 1,
                capability_schemas: vec![SchemaPattern::exact(
                    &Type::Extern(ExternKind::Tree).schema_ref(),
                )],
            },
        }
    }
}

impl<Ctx> RawPrimitive<Ctx> for TreeReadPrimitive {
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
    let (tree, path) = parse_request(request.value, request.identity)?;
    let tree_id = tree.identity();
    let tree_bytes = bytes(&tree)?;
    let path = core::str::from_utf8(bytes(&path)?)
        .map_err(|_| invalid_value("tree-read path was not UTF-8"))?;
    let projection = if let Some(name) = fixture_tree_name(tree_bytes) {
        let name = core::str::from_utf8(name)
            .map_err(|_| invalid_value("fixture tree name was not UTF-8"))?;
        format!("{name}/{path}")
    } else {
        path.to_owned()
    };
    let witnessed = ctx.read(&tree_id, ReadProjection::TreePath { path: projection })?;
    let admitted = ctx.intern(&Type::String.schema_ref(), &witnessed.bytes)?;
    if admitted != witnessed.identity {
        return Err(PrimitiveMachineError::CorruptCandidate { source: admitted });
    }
    Ok(admitted)
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
    let [tree, path] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request_id,
        });
    };
    Ok((child(tree)?.clone(), child(path)?.clone()))
}

fn child(field: &PrimitiveField) -> Result<&PrimitiveValue, PrimitiveMachineError> {
    let PrimitiveFieldValue::Child(value) = &field.value else {
        return Err(invalid_value(
            "tree-read request field disagrees with its declared schema",
        ));
    };
    Ok(value)
}

fn bytes(value: &PrimitiveValue) -> Result<&[u8], PrimitiveMachineError> {
    let PrimitiveValueBody::Bytes(bytes) = &value.body else {
        return Err(invalid_value(
            "tree-read request value was not resident bytes",
        ));
    };
    Ok(bytes)
}

fn invalid_value(detail: &str) -> PrimitiveMachineError {
    PrimitiveMachineError::AuthorityViolation {
        detail: detail.to_owned(),
    }
}
