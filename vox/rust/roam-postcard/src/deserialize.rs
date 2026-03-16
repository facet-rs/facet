use facet::Facet;
use facet_core::{Def, ScalarType, StructKind, Type, UserType};
use facet_reflect::Partial;
use roam_schema::SchemaRegistry;

use crate::decode::{self, Cursor};
use crate::error::DeserializeError;
use crate::plan::{FieldOp, TranslationPlan, build_identity_plan};

/// Deserialize postcard bytes into `T` using a translation plan.
pub fn from_slice<T: Facet<'static>>(
    input: &[u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<T, DeserializeError> {
    let partial =
        Partial::alloc_owned::<T>().map_err(|e| DeserializeError::ReflectError(e.to_string()))?;
    let mut cursor = Cursor::new(input);
    let partial = deserialize_value(partial, &mut cursor, plan, registry)?;
    let heap_value = partial
        .build()
        .map_err(|e| DeserializeError::ReflectError(e.to_string()))?;
    heap_value
        .materialize()
        .map_err(|e| DeserializeError::ReflectError(e.to_string()))
}

/// Deserialize postcard bytes into `T` using identity plan (same types both sides).
pub fn from_slice_identity<T: Facet<'static>>(input: &[u8]) -> Result<T, DeserializeError> {
    let plan = build_identity_plan(T::SHAPE);
    let registry = SchemaRegistry::new();
    from_slice(input, &plan, &registry)
}

/// Core deserialization: read postcard bytes into a Partial using the plan.
fn deserialize_value<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let shape = partial.shape();
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());

    // Transparent wrappers
    if shape.is_transparent() {
        let partial = partial.begin_inner().map_err(re)?;
        let partial = deserialize_value(partial, cursor, plan, registry)?;
        return partial.end().map_err(re);
    }

    // Scalars — leaf types, no plan needed
    if let Some(scalar_type) = shape.scalar_type() {
        return deserialize_scalar(partial, cursor, scalar_type);
    }

    // Def-based types before user types
    match shape.def {
        Def::Option(_) => return deserialize_option(partial, cursor, plan, registry),
        Def::List(list_def) => {
            if list_def.t().is_type::<u8>() {
                return deserialize_byte_list(partial, cursor);
            }
            return deserialize_list(partial, cursor, registry);
        }
        Def::Array(array_def) => return deserialize_array(partial, cursor, array_def.n, registry),
        Def::Slice(_) => return deserialize_list(partial, cursor, registry),
        Def::Map(_) => return deserialize_map(partial, cursor, registry),
        Def::Set(_) => return deserialize_set(partial, cursor, registry),
        Def::Pointer(_) => return deserialize_pointer(partial, cursor, plan, registry),
        _ => {}
    }

    // User types: struct/enum — plan-driven
    match shape.ty {
        Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
            StructKind::Struct | StructKind::TupleStruct | StructKind::Tuple => {
                deserialize_struct_planned(partial, cursor, plan, registry)
            }
            StructKind::Unit => Ok(partial),
        },
        Type::User(UserType::Enum(_)) => deserialize_enum_planned(partial, cursor, plan, registry),
        _ => Err(DeserializeError::UnsupportedType(format!("{}", shape))),
    }
}

/// Struct deserialization — always plan-driven.
fn deserialize_struct_planned<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let mut partial = partial;

    for op in &plan.field_ops {
        match op {
            FieldOp::Read { local_index } => {
                partial = partial.begin_nth_field(*local_index).map_err(re)?;
                // Use nested plan if available, otherwise identity
                if let Some(nested_plan) = plan.nested.get(local_index) {
                    partial = deserialize_value(partial, cursor, nested_plan, registry)?;
                } else {
                    let field_plan = build_identity_plan(partial.shape());
                    partial = deserialize_value(partial, cursor, &field_plan, registry)?;
                }
                partial = partial.end().map_err(re)?;
            }
            FieldOp::Skip { type_id } => {
                let kind = registry.get(type_id).map(|s| &s.kind).ok_or_else(|| {
                    DeserializeError::Custom(format!("schema not found for skip: {type_id:?}"))
                })?;
                decode::skip_value(cursor, kind, registry)?;
            }
        }
    }

    // Missing local fields get defaults via partial.build()
    Ok(partial)
}

/// Enum deserialization — plan-driven.
fn deserialize_enum_planned<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());

    let remote_disc = cursor.read_varint()? as usize;

    if let Some(enum_plan) = &plan.enum_plan {
        let local_idx = enum_plan
            .variant_map
            .get(remote_disc)
            .copied()
            .flatten()
            .ok_or(DeserializeError::UnknownVariant {
                remote_index: remote_disc,
            })?;

        let mut partial = partial.select_nth_variant(local_idx).map_err(re)?;

        // Deserialize variant fields using variant plan if available
        if let Some(variant_plan) = enum_plan.variant_plans.get(&remote_disc) {
            for op in &variant_plan.field_ops {
                match op {
                    FieldOp::Read { local_index } => {
                        partial = partial.begin_nth_field(*local_index).map_err(re)?;
                        let field_plan = build_identity_plan(partial.shape());
                        partial = deserialize_value(partial, cursor, &field_plan, registry)?;
                        partial = partial.end().map_err(re)?;
                    }
                    FieldOp::Skip { type_id } => {
                        let kind = registry.get(type_id).map(|s| &s.kind).ok_or_else(|| {
                            DeserializeError::Custom(format!(
                                "schema not found for skip: {type_id:?}"
                            ))
                        })?;
                        decode::skip_value(cursor, kind, registry)?;
                    }
                }
            }
        } else {
            // No variant plan — read fields by local shape order (identity)
            let variant = partial.shape();
            match variant.ty {
                Type::User(UserType::Struct(struct_type)) => {
                    for i in 0..struct_type.fields.len() {
                        partial = partial.begin_nth_field(i).map_err(re)?;
                        let field_plan = build_identity_plan(partial.shape());
                        partial = deserialize_value(partial, cursor, &field_plan, registry)?;
                        partial = partial.end().map_err(re)?;
                    }
                }
                _ => {}
            }
        }

        Ok(partial)
    } else {
        // No enum plan — use local shape directly
        let shape = partial.shape();
        let enum_type = match shape.ty {
            Type::User(UserType::Enum(e)) => e,
            _ => return Err(DeserializeError::UnsupportedType("expected enum".into())),
        };

        if remote_disc >= enum_type.variants.len() {
            return Err(DeserializeError::InvalidEnumDiscriminant {
                pos: cursor.pos(),
                index: remote_disc as u64,
                variant_count: enum_type.variants.len(),
            });
        }

        let variant = &enum_type.variants[remote_disc];
        let field_count = variant.data.fields.len();

        let mut partial = partial.select_nth_variant(remote_disc).map_err(re)?;
        for i in 0..field_count {
            partial = partial.begin_nth_field(i).map_err(re)?;
            let field_plan = build_identity_plan(partial.shape());
            partial = deserialize_value(partial, cursor, &field_plan, registry)?;
            partial = partial.end().map_err(re)?;
        }
        Ok(partial)
    }
}

fn deserialize_scalar<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    scalar_type: ScalarType,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    match scalar_type {
        ScalarType::Unit => partial.set(()).map_err(re),
        ScalarType::Bool => {
            let b = cursor.read_byte()?;
            match b {
                0x00 => partial.set(false).map_err(re),
                0x01 => partial.set(true).map_err(re),
                other => Err(DeserializeError::InvalidBool {
                    pos: cursor.pos() - 1,
                    got: other,
                }),
            }
        }
        ScalarType::Char => {
            let s = cursor.read_str()?;
            let c = s
                .chars()
                .next()
                .ok_or_else(|| DeserializeError::Custom("empty string for char".into()))?;
            partial.set(c).map_err(re)
        }
        ScalarType::U8 => {
            let v = cursor.read_byte()?;
            partial.set(v).map_err(re)
        }
        ScalarType::U16 => {
            let v = cursor.read_varint()? as u16;
            partial.set(v).map_err(re)
        }
        ScalarType::U32 => {
            let v = cursor.read_varint()? as u32;
            partial.set(v).map_err(re)
        }
        ScalarType::U64 => {
            let v = cursor.read_varint()?;
            partial.set(v).map_err(re)
        }
        ScalarType::U128 => {
            let v = cursor.read_varint_u128()?;
            partial.set(v).map_err(re)
        }
        ScalarType::USize => {
            let v = cursor.read_varint()? as usize;
            partial.set(v).map_err(re)
        }
        ScalarType::I8 => {
            let v = cursor.read_byte()? as i8;
            partial.set(v).map_err(re)
        }
        ScalarType::I16 => {
            let v = cursor.read_signed_varint()? as i16;
            partial.set(v).map_err(re)
        }
        ScalarType::I32 => {
            let v = cursor.read_signed_varint()? as i32;
            partial.set(v).map_err(re)
        }
        ScalarType::I64 => {
            let v = cursor.read_signed_varint()?;
            partial.set(v).map_err(re)
        }
        ScalarType::I128 => {
            let v = cursor.read_signed_varint_i128()?;
            partial.set(v).map_err(re)
        }
        ScalarType::ISize => {
            let v = cursor.read_signed_varint()? as isize;
            partial.set(v).map_err(re)
        }
        ScalarType::F32 => {
            let bytes = cursor.read_bytes(4)?;
            let v = f32::from_le_bytes(bytes.try_into().unwrap());
            partial.set(v).map_err(re)
        }
        ScalarType::F64 => {
            let bytes = cursor.read_bytes(8)?;
            let v = f64::from_le_bytes(bytes.try_into().unwrap());
            partial.set(v).map_err(re)
        }
        ScalarType::String => {
            let s = cursor.read_str()?;
            partial.set(s.to_owned()).map_err(re)
        }
        ScalarType::Str => {
            let s = cursor.read_str()?;
            partial.set(s.to_owned()).map_err(re)
        }
        ScalarType::CowStr => {
            let s = cursor.read_str()?;
            partial
                .set(std::borrow::Cow::<'static, str>::Owned(s.to_owned()))
                .map_err(re)
        }
        _ => Err(DeserializeError::UnsupportedType(format!(
            "scalar {scalar_type:?}"
        ))),
    }
}

fn deserialize_option<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let tag = cursor.read_byte()?;
    match tag {
        0x00 => Ok(partial), // None — Partial leaves it as default None
        0x01 => {
            let partial = partial.begin_some().map_err(re)?;
            let partial = deserialize_value(partial, cursor, plan, registry)?;
            partial.end().map_err(re)
        }
        other => Err(DeserializeError::InvalidOptionTag {
            pos: cursor.pos() - 1,
            got: other,
        }),
    }
}

fn deserialize_byte_list<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let bytes = cursor.read_byte_slice()?;
    partial.set(bytes.to_vec()).map_err(re)
}

fn deserialize_list<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let len = cursor.read_varint()? as usize;
    let mut partial = partial.init_list_with_capacity(len).map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_list_item().map_err(re)?;
        let item_plan = build_identity_plan(partial.shape());
        partial = deserialize_value(partial, cursor, &item_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_array<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    n: usize,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    // No length prefix for fixed-size arrays
    let mut partial = partial;
    for i in 0..n {
        partial = partial.begin_nth_field(i).map_err(re)?;
        let item_plan = build_identity_plan(partial.shape());
        partial = deserialize_value(partial, cursor, &item_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_map<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let len = cursor.read_varint()? as usize;
    let mut partial = partial.init_map().map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_key().map_err(re)?;
        let key_plan = build_identity_plan(partial.shape());
        partial = deserialize_value(partial, cursor, &key_plan, registry)?;
        partial = partial.end().map_err(re)?;

        partial = partial.begin_value().map_err(re)?;
        let val_plan = build_identity_plan(partial.shape());
        partial = deserialize_value(partial, cursor, &val_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_set<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let len = cursor.read_varint()? as usize;
    let mut partial = partial.init_set().map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_set_item().map_err(re)?;
        let item_plan = build_identity_plan(partial.shape());
        partial = deserialize_value(partial, cursor, &item_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_pointer<'facet>(
    partial: Partial<'facet, false>,
    cursor: &mut Cursor<'_>,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, false>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let partial = partial.begin_smart_ptr().map_err(re)?;
    let partial = deserialize_value(partial, cursor, plan, registry)?;
    partial.end().map_err(re)
}
