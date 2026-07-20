use vix::decode::{self, DecodeError, DecodedValue};
use vix::schema::{SchemaPattern, SchemaRef};
use vix::vir::{
    DecodeFormat, RESULT_ERR_VARIANT, RESULT_OK_VARIANT, Type, VariantPayload, decode_error_type,
    decode_primitive_id, decode_request_type,
};

use crate::rt::{
    EffectCtx, PrimitiveCompletion, PrimitiveDescriptor, PrimitiveField, PrimitiveFieldValue,
    PrimitiveMachineError, PrimitiveMemoPolicy, PrimitiveValue, PrimitiveValueBody,
    RawEffectTicket, RawPrimitive, ReadProjection, ValueId,
};

pub struct DecodePrimitive {
    descriptor: PrimitiveDescriptor,
}

impl Default for DecodePrimitive {
    fn default() -> Self {
        Self {
            descriptor: PrimitiveDescriptor {
                id: decode_primitive_id(),
                request_schema: SchemaPattern::exact(&decode_request_type().schema_ref()),
                response_schema: SchemaPattern::Var {
                    name: "Response".to_owned(),
                },
                failure_schema: SchemaPattern::Var {
                    name: "Failure".to_owned(),
                },
                memo_policy: PrimitiveMemoPolicy::Hermetic,
                protocol_version: 1,
                capability_schemas: Vec::new(),
            },
        }
    }
}

impl<Ctx> RawPrimitive<Ctx> for DecodePrimitive {
    fn descriptor(&self) -> &PrimitiveDescriptor {
        &self.descriptor
    }

    fn begin(&self, request: ValueId, ctx: EffectCtx, _app: &Ctx) -> RawEffectTicket {
        let (ticket, completer) = ctx.ticket(|| {});
        let completion = decode_request(&request, &ctx)
            .and_then(|value| ctx.intern_value(value))
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
        ticket
    }
}

fn decode_request(
    request: &ValueId,
    ctx: &EffectCtx,
) -> Result<PrimitiveValue, PrimitiveMachineError> {
    let witnessed = ctx.read(request, ReadProjection::Whole)?;
    let PrimitiveValueBody::Product(fields) = witnessed.value.body else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        });
    };
    let [format, target, fallible, document] = fields.as_slice() else {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        });
    };
    let format = match inline_i64(format)? {
        0 => DecodeFormat::Json,
        1 => DecodeFormat::Toml,
        _ => {
            return Err(PrimitiveMachineError::InvalidRequest {
                request: request.clone(),
            });
        }
    };
    let target_ref = SchemaRef::from_canonical_bytes(child_bytes(target)?).map_err(|_| {
        PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        }
    })?;
    let target = ctx.type_for_schema(&target_ref)?;
    if target.schema_ref() != target_ref {
        return Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        });
    }
    let fallible = inline_i64(fallible)? != 0;
    let document = core::str::from_utf8(child_bytes(document)?).map_err(|_| {
        PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        }
    })?;

    match decode::decode(format, document, &target) {
        Ok(decoded) => {
            let decoded = decoded_value(&target, &decoded)?;
            if fallible {
                Ok(result_value(
                    Type::result(target, decode_error_type()),
                    RESULT_OK_VARIANT,
                    decoded,
                ))
            } else {
                Ok(decoded)
            }
        }
        Err(error) if fallible => Ok(result_value(
            Type::result(target, decode_error_type()),
            RESULT_ERR_VARIANT,
            decode_error_value(&error),
        )),
        Err(_) => Err(PrimitiveMachineError::InvalidRequest {
            request: request.clone(),
        }),
    }
}

fn inline_i64(field: &PrimitiveField) -> Result<i64, PrimitiveMachineError> {
    let PrimitiveFieldValue::Inline(bytes) = &field.value else {
        return Err(invalid_field());
    };
    Ok(i64::from_le_bytes(
        bytes.as_slice().try_into().map_err(|_| invalid_field())?,
    ))
}

fn child_bytes(field: &PrimitiveField) -> Result<&[u8], PrimitiveMachineError> {
    let PrimitiveFieldValue::Child(value) = &field.value else {
        return Err(invalid_field());
    };
    let PrimitiveValueBody::Bytes(bytes) = &value.body else {
        return Err(invalid_field());
    };
    Ok(bytes)
}

fn invalid_field() -> PrimitiveMachineError {
    PrimitiveMachineError::AuthorityViolation {
        detail: "registered decode request field disagrees with its schema".to_owned(),
    }
}

fn result_value(result: Type, tag: u32, payload: PrimitiveValue) -> PrimitiveValue {
    PrimitiveValue {
        schema: result.schema_ref(),
        body: PrimitiveValueBody::Variant {
            tag,
            fields: vec![child_field(payload)],
        },
    }
}

fn decoded_value(ty: &Type, value: &DecodedValue) -> Result<PrimitiveValue, PrimitiveMachineError> {
    match (ty, value) {
        (Type::Int, DecodedValue::Int(value)) => Ok(PrimitiveValue::bytes(
            ty.schema_ref(),
            value.to_le_bytes().to_vec(),
        )),
        (Type::Bool, DecodedValue::Bool(value)) => Ok(PrimitiveValue::bytes(
            ty.schema_ref(),
            i64::from(*value).to_le_bytes().to_vec(),
        )),
        (Type::String, DecodedValue::Str(value)) => Ok(PrimitiveValue::bytes(
            ty.schema_ref(),
            value.as_bytes().to_vec(),
        )),
        (Type::Record(record), DecodedValue::Record(values))
            if record.fields.len() == values.len() =>
        {
            let fields = record
                .fields
                .iter()
                .zip(values)
                .map(|(field, value)| decoded_value(&field.ty, value).map(field_value))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PrimitiveValue {
                schema: ty.schema_ref(),
                body: PrimitiveValueBody::Product(fields),
            })
        }
        (_, DecodedValue::OptionSome(value)) if ty.option_inner().is_some() => Ok(result_value(
            ty.clone(),
            0,
            decoded_value(ty.option_inner().expect("guarded option"), value)?,
        )),
        (_, DecodedValue::OptionNone) if ty.option_inner().is_some() => Ok(PrimitiveValue {
            schema: ty.schema_ref(),
            body: PrimitiveValueBody::Variant {
                tag: 1,
                fields: Vec::new(),
            },
        }),
        (Type::Enum(enumeration), DecodedValue::Variant { index, fields }) => {
            let variant = enumeration
                .variants
                .get(*index as usize)
                .ok_or_else(invalid_field)?;
            let types = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(types) => types.iter().collect(),
                VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
            };
            if types.len() != fields.len() {
                return Err(invalid_field());
            }
            let fields = types
                .into_iter()
                .zip(fields)
                .map(|(ty, value)| decoded_value(ty, value).map(field_value))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PrimitiveValue {
                schema: ty.schema_ref(),
                body: PrimitiveValueBody::Variant {
                    tag: *index,
                    fields,
                },
            })
        }
        _ => Err(invalid_field()),
    }
}

fn decode_error_value(error: &DecodeError) -> PrimitiveValue {
    let kind = PrimitiveValue::bytes(
        Type::String.schema_ref(),
        error.kind.label().as_bytes().to_vec(),
    );
    let path = PrimitiveValue::bytes(
        Type::String.schema_ref(),
        error.path_names().join(".").into_bytes(),
    );
    let offset = error.span.map_or(-1, |span| i64::from(span.offset));
    let len = error.span.map_or(-1, |span| i64::from(span.len));
    PrimitiveValue {
        schema: decode_error_type().schema_ref(),
        body: PrimitiveValueBody::Product(vec![
            child_field(kind),
            child_field(path),
            inline_field(Type::Int.schema_ref(), offset.to_le_bytes().to_vec()),
            inline_field(Type::Int.schema_ref(), len.to_le_bytes().to_vec()),
        ]),
    }
}

fn field_value(value: PrimitiveValue) -> PrimitiveField {
    let inline = value.schema == Type::Int.schema_ref() || value.schema == Type::Bool.schema_ref();
    if inline {
        let PrimitiveValue {
            schema,
            body: PrimitiveValueBody::Bytes(bytes),
        } = value
        else {
            unreachable!("guarded bytes")
        };
        inline_field(schema, bytes)
    } else {
        child_field(value)
    }
}

fn inline_field(schema: SchemaRef, bytes: Vec<u8>) -> PrimitiveField {
    PrimitiveField {
        schema,
        value: PrimitiveFieldValue::Inline(bytes),
    }
}

fn child_field(value: PrimitiveValue) -> PrimitiveField {
    PrimitiveField {
        schema: value.schema.clone(),
        value: PrimitiveFieldValue::Child(Box::new(value)),
    }
}
