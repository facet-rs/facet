use facet::Facet;
use facet_core::{Def, ScalarType, StructKind, Type, UserType};
use facet_reflect::Partial;
use roam_schema::SchemaRegistry;

use crate::decode::{self, Cursor};
use crate::error::DeserializeError;
use crate::plan::{FieldOp, TranslationPlan, build_identity_plan};

/// Deserialize postcard bytes into owned `T` (identity plan, same types both sides).
pub fn from_slice<T: Facet<'static>>(input: &[u8]) -> Result<T, DeserializeError> {
    let plan = build_identity_plan(T::SHAPE);
    let registry = SchemaRegistry::new();
    from_slice_with_plan(input, &plan, &registry)
}

/// Deserialize postcard bytes into owned `T` using a translation plan.
pub fn from_slice_with_plan<T: Facet<'static>>(
    input: &[u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<T, DeserializeError> {
    let partial =
        Partial::alloc_owned::<T>().map_err(|e| DeserializeError::ReflectError(e.to_string()))?;
    let mut cursor = Cursor::new(input);
    let partial = deserialize_value::<false>(partial, &mut cursor, plan, registry)?;
    let heap_value = partial
        .build()
        .map_err(|e| DeserializeError::ReflectError(e.to_string()))?;
    heap_value
        .materialize()
        .map_err(|e| DeserializeError::ReflectError(e.to_string()))
}

/// Deserialize postcard bytes into borrowed `T` (identity plan, same types both sides).
/// The returned value may borrow from `input`.
pub fn from_slice_borrowed<'input, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    let plan = build_identity_plan(T::SHAPE);
    let registry = SchemaRegistry::new();
    from_slice_borrowed_with_plan(input, &plan, &registry)
}

/// Deserialize postcard bytes into borrowed `T` using a translation plan.
pub fn from_slice_borrowed_with_plan<'input, 'facet, T: Facet<'facet>>(
    input: &'input [u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<T, DeserializeError>
where
    'input: 'facet,
{
    let partial =
        Partial::alloc::<T>().map_err(|e| DeserializeError::ReflectError(e.to_string()))?;
    let mut cursor = Cursor::new(input);
    let partial = deserialize_value::<true>(partial, &mut cursor, plan, registry)?;
    let heap_value = partial
        .build()
        .map_err(|e| DeserializeError::ReflectError(e.to_string()))?;
    heap_value
        .materialize()
        .map_err(|e| DeserializeError::ReflectError(e.to_string()))
}

/// Deserialize postcard bytes into an existing Partial (for in-place deserialization).
pub fn deserialize_into<'input, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    input: &'input [u8],
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError>
where
    'input: 'facet,
{
    let mut cursor = Cursor::new(input);
    deserialize_value::<BORROW>(partial, &mut cursor, plan, registry)
}

/// Core deserialization: read postcard bytes into a Partial using the plan.
///
/// The cursor lifetime `'de` must outlive `'facet` so borrowed strings
/// can be stored in the Partial.
fn deserialize_value<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    deserialize_value_inner::<BORROW>(partial, cursor, plan, registry, false)
}

fn deserialize_value_inner<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
    is_trailing: bool,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let shape = partial.shape();
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());

    // Handle opaque adapters (e.g. Payload).
    if let Some(adapter) = shape.opaque_adapter {
        let bytes = if is_trailing {
            // Trailing opaque fields consume all remaining bytes (no length prefix).
            cursor.read_bytes(cursor.remaining())?
        } else {
            // Non-trailing opaque fields are length-prefixed.
            cursor.read_byte_slice()?
        };
        let deser_fn = adapter.deserialize;
        let input = facet::OpaqueDeserialize::Borrowed(bytes);
        #[allow(unsafe_code)]
        let partial = unsafe {
            partial.set_from_function(move |target_ptr| {
                (deser_fn)(input, target_ptr).map(|_| ()).map_err(|e| {
                    facet_reflect::ReflectErrorKind::InvariantViolation {
                        invariant: Box::leak(
                            format!("opaque adapter deserialize failed: {e}").into_boxed_str(),
                        ),
                    }
                })
            })
        }
        .map_err(re)?;
        return Ok(partial);
    }

    // Transparent wrappers
    if shape.is_transparent() {
        let partial = partial.begin_inner().map_err(re)?;
        let partial = deserialize_value::<BORROW>(partial, cursor, plan, registry)?;
        return partial.end().map_err(re);
    }

    // Scalars
    if let Some(scalar_type) = shape.scalar_type() {
        return deserialize_scalar::<BORROW>(partial, cursor, scalar_type);
    }

    // Def-based types
    match shape.def {
        Def::Option(_) => {
            return deserialize_option::<BORROW>(partial, cursor, plan, registry);
        }
        Def::Result(_) => {
            return deserialize_result::<BORROW>(partial, cursor, plan, registry);
        }
        Def::List(list_def) => {
            if list_def.t().is_type::<u8>() {
                return deserialize_byte_list(partial, cursor);
            }
            return deserialize_list::<BORROW>(partial, cursor, registry);
        }
        Def::Array(array_def) => {
            return deserialize_array::<BORROW>(partial, cursor, array_def.n, registry);
        }
        Def::Slice(_) => return deserialize_list::<BORROW>(partial, cursor, registry),
        Def::Map(_) => return deserialize_map::<BORROW>(partial, cursor, registry),
        Def::Set(_) => return deserialize_set::<BORROW>(partial, cursor, registry),
        Def::Pointer(_) => {
            return deserialize_pointer::<BORROW>(partial, cursor, plan, registry);
        }
        _ => {}
    }

    // User types
    match shape.ty {
        Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
            StructKind::Struct | StructKind::TupleStruct | StructKind::Tuple => {
                deserialize_struct_planned::<BORROW>(partial, cursor, plan, registry)
            }
            StructKind::Unit => Ok(partial),
        },
        Type::User(UserType::Enum(_)) => {
            deserialize_enum_planned::<BORROW>(partial, cursor, plan, registry)
        }
        _ => Err(DeserializeError::UnsupportedType(format!("{}", shape))),
    }
}

fn deserialize_struct_planned<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());

    // Get the struct fields for trailing attribute checks.
    let struct_fields = match partial.shape().ty {
        Type::User(UserType::Struct(s)) => s.fields,
        _ => &[],
    };

    let mut partial = partial;

    for op in &plan.field_ops {
        match op {
            FieldOp::Read { local_index } => {
                let trailing = struct_fields
                    .get(*local_index)
                    .is_some_and(|f| f.has_builtin_attr("trailing"));
                partial = partial.begin_nth_field(*local_index).map_err(re)?;
                if let Some(nested_plan) = plan.nested.get(local_index) {
                    partial = deserialize_value_inner::<BORROW>(
                        partial,
                        cursor,
                        nested_plan,
                        registry,
                        trailing,
                    )?;
                } else {
                    let field_plan = build_identity_plan(partial.shape());
                    partial = deserialize_value_inner::<BORROW>(
                        partial,
                        cursor,
                        &field_plan,
                        registry,
                        trailing,
                    )?;
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

    Ok(partial)
}

fn deserialize_enum_planned<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let remote_disc = cursor.read_varint()? as usize;

    if let Some(enum_plan) = &plan.enum_plan {
        let local_idx = enum_plan
            .variant_map
            .get(remote_disc)
            .copied()
            .flatten()
            // r[impl schema.errors.unknown-variant-runtime]
            .ok_or(DeserializeError::UnknownVariant {
                remote_index: remote_disc,
            })?;

        let mut partial = partial.select_nth_variant(local_idx).map_err(re)?;

        // Get variant field metadata for trailing checks.
        let variant_fields = match partial.shape().ty {
            Type::User(UserType::Struct(s)) => s.fields,
            _ => &[],
        };

        if let Some(variant_plan) = enum_plan.variant_plans.get(&remote_disc) {
            for op in &variant_plan.field_ops {
                match op {
                    FieldOp::Read { local_index } => {
                        let trailing = variant_fields
                            .get(*local_index)
                            .is_some_and(|f| f.has_builtin_attr("trailing"));
                        partial = partial.begin_nth_field(*local_index).map_err(re)?;
                        let field_plan = build_identity_plan(partial.shape());
                        partial = deserialize_value_inner::<BORROW>(
                            partial,
                            cursor,
                            &field_plan,
                            registry,
                            trailing,
                        )?;
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
            for i in 0..variant_fields.len() {
                let trailing = variant_fields[i].has_builtin_attr("trailing");
                partial = partial.begin_nth_field(i).map_err(re)?;
                let field_plan = build_identity_plan(partial.shape());
                partial = deserialize_value_inner::<BORROW>(
                    partial,
                    cursor,
                    &field_plan,
                    registry,
                    trailing,
                )?;
                partial = partial.end().map_err(re)?;
            }
        }

        Ok(partial)
    } else {
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
            let trailing = variant.data.fields[i].has_builtin_attr("trailing");
            partial = partial.begin_nth_field(i).map_err(re)?;
            let field_plan = build_identity_plan(partial.shape());
            partial = deserialize_value_inner::<BORROW>(
                partial,
                cursor,
                &field_plan,
                registry,
                trailing,
            )?;
            partial = partial.end().map_err(re)?;
        }
        Ok(partial)
    }
}

fn deserialize_scalar<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    scalar_type: ScalarType,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
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
            // SAFETY: The caller of from_slice_borrowed guarantees 'input: 'facet,
            // so the cursor's borrowed data outlives the Partial. For from_slice (owned),
            // ScalarType::Str never appears because 'static types use String, not &str.
            let s = cursor.read_str()?;
            #[allow(unsafe_code)]
            let s: &'facet str = unsafe { std::mem::transmute(s) };
            partial.set(s).map_err(re)
        }
        ScalarType::CowStr => {
            let s = cursor.read_str()?;
            #[allow(unsafe_code)]
            let s: &'facet str = unsafe { std::mem::transmute(s) };
            partial.set(std::borrow::Cow::Borrowed(s)).map_err(re)
        }
        _ => Err(DeserializeError::UnsupportedType(format!(
            "scalar {scalar_type:?}"
        ))),
    }
}

fn deserialize_option<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    _plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let tag = cursor.read_byte()?;
    match tag {
        0x00 => Ok(partial),
        0x01 => {
            let partial = partial.begin_some().map_err(re)?;
            let inner_plan = build_identity_plan(partial.shape());
            let partial = deserialize_value::<BORROW>(partial, cursor, &inner_plan, registry)?;
            partial.end().map_err(re)
        }
        other => Err(DeserializeError::InvalidOptionTag {
            pos: cursor.pos() - 1,
            got: other,
        }),
    }
}

fn deserialize_result<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    _plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let variant_index = cursor.read_varint()? as usize;
    match variant_index {
        0 => {
            let partial = partial.begin_ok().map_err(re)?;
            let inner_plan = build_identity_plan(partial.shape());
            let partial = deserialize_value::<BORROW>(partial, cursor, &inner_plan, registry)?;
            partial.end().map_err(re)
        }
        1 => {
            let partial = partial.begin_err().map_err(re)?;
            let inner_plan = build_identity_plan(partial.shape());
            let partial = deserialize_value::<BORROW>(partial, cursor, &inner_plan, registry)?;
            partial.end().map_err(re)
        }
        other => Err(DeserializeError::UnknownVariant {
            remote_index: other,
        }),
    }
}

fn deserialize_byte_list<'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'_>,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let bytes = cursor.read_byte_slice()?;
    partial.set(bytes.to_vec()).map_err(re)
}

fn deserialize_list<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let len = cursor.read_varint()? as usize;
    let mut partial = partial.init_list_with_capacity(len).map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_list_item().map_err(re)?;
        let item_plan = build_identity_plan(partial.shape());
        partial = deserialize_value::<BORROW>(partial, cursor, &item_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_array<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    n: usize,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let mut partial = partial;
    for i in 0..n {
        partial = partial.begin_nth_field(i).map_err(re)?;
        let item_plan = build_identity_plan(partial.shape());
        partial = deserialize_value::<BORROW>(partial, cursor, &item_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_map<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let len = cursor.read_varint()? as usize;
    let mut partial = partial.init_map().map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_key().map_err(re)?;
        let key_plan = build_identity_plan(partial.shape());
        partial = deserialize_value::<BORROW>(partial, cursor, &key_plan, registry)?;
        partial = partial.end().map_err(re)?;

        partial = partial.begin_value().map_err(re)?;
        let val_plan = build_identity_plan(partial.shape());
        partial = deserialize_value::<BORROW>(partial, cursor, &val_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_set<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let len = cursor.read_varint()? as usize;
    let mut partial = partial.init_set().map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_set_item().map_err(re)?;
        let item_plan = build_identity_plan(partial.shape());
        partial = deserialize_value::<BORROW>(partial, cursor, &item_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_pointer<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    _plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let shape = partial.shape();

    // Special case: &[u8] — borrowed byte slice reference.
    // We can't use begin_smart_ptr() for plain references. Instead, read
    // the bytes and set the fat pointer directly.
    if let Def::Pointer(ptr_def) = shape.def {
        if let Some(facet_core::KnownPointer::SharedReference) = ptr_def.known {
            if let Some(pointee) = ptr_def.pointee() {
                if let Def::Slice(slice_def) = pointee.def {
                    if slice_def.t().is_type::<u8>() {
                        let bytes = cursor.read_byte_slice()?;
                        // SAFETY: from_slice_borrowed guarantees 'input: 'facet,
                        // so borrowing from the cursor is valid.
                        #[allow(unsafe_code)]
                        let bytes: &'facet [u8] = unsafe { std::mem::transmute(bytes) };
                        return partial.set(bytes).map_err(re);
                    }
                }
            }
        }
    }

    let partial = partial.begin_smart_ptr().map_err(re)?;
    let inner_plan = build_identity_plan(partial.shape());
    let partial = deserialize_value::<BORROW>(partial, cursor, &inner_plan, registry)?;
    partial.end().map_err(re)
}
