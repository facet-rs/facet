use std::collections::HashMap;

use facet::Facet;
use facet_core::{Def, ScalarType, StructKind, Type, UserType};
use facet_reflect::Partial;
use vox_schema::SchemaRegistry;

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
    let shape = partial.shape();
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());

    // r[impl zerocopy.framing.value.opaque.length-prefix]
    // Handle opaque adapters (e.g. Payload). u32le length-prefixed.
    if let Some(adapter) = shape.opaque_adapter {
        let bytes = cursor.read_opaque_bytes()?;
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

    // Proxy types (e.g. Rx<T>, Tx<T> with #[facet(proxy = ())])
    if let Some(proxy_def) = shape.proxy {
        let proxy_shape = proxy_def.shape;

        // First, serialize the proxy value from the cursor into a temp buffer
        {
            let proxy_layout = proxy_shape
                .layout
                .sized_layout()
                .map_err(|_| DeserializeError::ReflectError("proxy type must be sized".into()))?;

            let proxy_uninit = facet_core::alloc_for_layout(proxy_layout);
            #[allow(unsafe_code)]
            let proxy_partial = unsafe { Partial::from_raw_with_shape(proxy_uninit, proxy_shape) }
                .map_err(|e| DeserializeError::ReflectError(e.to_string()))?;
            let proxy_plan = build_identity_plan(proxy_shape);
            let proxy_partial =
                deserialize_value::<BORROW>(proxy_partial, cursor, &proxy_plan, registry)?;
            proxy_partial
                .finish_in_place()
                .map_err(|e| DeserializeError::ReflectError(e.to_string()))?;

            // Now convert_in: proxy → target using set_from_function
            let convert_in = proxy_def.convert_in;
            #[allow(unsafe_code)]
            let proxy_ptr = unsafe { proxy_uninit.assume_init() };
            #[allow(unsafe_code)]
            let partial = unsafe {
                partial.set_from_function(move |target_uninit| {
                    (convert_in)(proxy_ptr.as_const(), target_uninit)
                        .map(|_| ())
                        .map_err(|e| facet_reflect::ReflectErrorKind::InvariantViolation {
                            invariant: Box::leak(
                                format!("proxy convert_in failed: {e}").into_boxed_str(),
                            ),
                        })
                })
            }
            .map_err(re)?;

            // Deallocate the proxy memory. convert_in consumed the value
            // via ptr::read, so we must NOT call drop_in_place (double-free).
            #[allow(unsafe_code)]
            unsafe {
                facet_core::dealloc_for_layout(proxy_ptr, proxy_layout);
            }

            return Ok(partial);
        };
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
            let inner_plan = match plan {
                TranslationPlan::Option { inner } => inner.as_ref(),
                _ => &TranslationPlan::Identity,
            };
            return deserialize_option::<BORROW>(partial, cursor, inner_plan, registry);
        }
        Def::Result(_) => {
            return deserialize_result::<BORROW>(partial, cursor, plan, registry);
        }
        Def::List(list_def) => {
            if list_def.t().is_type::<u8>() {
                return deserialize_byte_list(partial, cursor);
            }
            let element_plan = match plan {
                TranslationPlan::List { element } => element.as_ref(),
                _ => &TranslationPlan::Identity,
            };
            return deserialize_list::<BORROW>(partial, cursor, element_plan, registry);
        }
        Def::Array(array_def) => {
            let element_plan = match plan {
                TranslationPlan::Array { element } => element.as_ref(),
                _ => &TranslationPlan::Identity,
            };
            return deserialize_array::<BORROW>(
                partial,
                cursor,
                array_def.n,
                element_plan,
                registry,
            );
        }
        Def::Slice(_) => {
            let element_plan = match plan {
                TranslationPlan::List { element } => element.as_ref(),
                _ => &TranslationPlan::Identity,
            };
            return deserialize_list::<BORROW>(partial, cursor, element_plan, registry);
        }
        Def::Map(_) => {
            let (key_plan, value_plan) = match plan {
                TranslationPlan::Map { key, value } => (key.as_ref(), value.as_ref()),
                _ => (&TranslationPlan::Identity, &TranslationPlan::Identity),
            };
            return deserialize_map::<BORROW>(partial, cursor, key_plan, value_plan, registry);
        }
        Def::Set(_) => {
            let element_plan = match plan {
                TranslationPlan::List { element } => element.as_ref(),
                _ => &TranslationPlan::Identity,
            };
            return deserialize_set::<BORROW>(partial, cursor, element_plan, registry);
        }
        Def::Pointer(_) => {
            let pointee_plan = match plan {
                TranslationPlan::Pointer { pointee } => pointee.as_ref(),
                _ => &TranslationPlan::Identity,
            };
            return deserialize_pointer::<BORROW>(partial, cursor, pointee_plan, registry);
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

    let (field_ops, nested) = match plan {
        TranslationPlan::Struct { field_ops, nested }
        | TranslationPlan::Tuple { field_ops, nested } => (field_ops.as_slice(), nested),
        TranslationPlan::Identity => {
            let identity_plan = build_identity_plan(partial.shape());
            return deserialize_struct_planned::<BORROW>(partial, cursor, &identity_plan, registry);
        }
        _ => {
            return Err(DeserializeError::Custom(format!(
                "expected Struct/Tuple/Identity plan, got {plan:?}"
            )));
        }
    };

    let mut partial = partial;

    for op in field_ops {
        match op {
            FieldOp::Read { local_index } => {
                partial = partial.begin_nth_field(*local_index).map_err(re)?;
                if let Some(nested_plan) = nested.get(local_index) {
                    partial = deserialize_value::<BORROW>(partial, cursor, nested_plan, registry)?;
                } else {
                    let field_plan = build_identity_plan(partial.shape());
                    partial = deserialize_value::<BORROW>(partial, cursor, &field_plan, registry)?;
                }
                partial = partial.end().map_err(re)?;
            }
            FieldOp::Skip { type_ref } => {
                let kind = type_ref.resolve_kind(registry).ok_or_else(|| {
                    DeserializeError::Custom(format!("schema not found for skip: {type_ref:?}"))
                })?;
                decode::skip_value(cursor, &kind, registry)?;
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

    match plan {
        TranslationPlan::Enum {
            variant_map,
            variant_plans,
            nested,
        } => {
            let local_idx = variant_map
                .get(remote_disc)
                .copied()
                .flatten()
                // r[impl schema.errors.unknown-variant-runtime]
                .ok_or(DeserializeError::UnknownVariant {
                    remote_index: remote_disc,
                })?;
            // Get variant field metadata BEFORE selecting the variant,
            // because after select_nth_variant the shape is still the enum,
            // not a struct.
            let variant_fields = match partial.shape().ty {
                Type::User(UserType::Enum(e)) => e
                    .variants
                    .get(local_idx)
                    .map(|v| v.data.fields)
                    .unwrap_or(&[]),
                _ => &[],
            };

            let mut partial = partial.select_nth_variant(local_idx).map_err(re)?;

            if let Some(variant_plan) = variant_plans.get(&remote_disc) {
                // Per-variant plan (struct variant or tuple variant with translation)
                partial =
                    deserialize_struct_planned::<BORROW>(partial, cursor, variant_plan, registry)?;
            } else if let Some(inner_plan) = nested.get(&local_idx) {
                // Newtype variant with nested translation
                partial = partial.begin_nth_field(0).map_err(re)?;
                partial = deserialize_value::<BORROW>(partial, cursor, inner_plan, registry)?;
                partial = partial.end().map_err(re)?;
            } else {
                // Identity: read all fields in order
                for (i, _variant_field) in variant_fields.iter().enumerate() {
                    partial = partial.begin_nth_field(i).map_err(re)?;
                    let field_plan = build_identity_plan(partial.shape());
                    partial = deserialize_value::<BORROW>(partial, cursor, &field_plan, registry)?;
                    partial = partial.end().map_err(re)?;
                }
            }

            Ok(partial)
        }
        _ => {
            // Identity path — no enum plan, use shape directly
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
                partial = deserialize_value::<BORROW>(partial, cursor, &field_plan, registry)?;
                partial = partial.end().map_err(re)?;
            }
            Ok(partial)
        }
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
            let s = cursor.read_str()?;
            if !BORROW {
                return Err(DeserializeError::Custom(
                    "cannot deserialize borrowed &str with BORROW=false; \
                     use from_slice_borrowed or change the target type to String"
                        .into(),
                ));
            }
            // SAFETY: The caller of from_slice_borrowed guarantees 'input: 'facet,
            // so the cursor's borrowed data outlives the Partial.
            #[allow(unsafe_code)]
            let s: &'facet str = unsafe { std::mem::transmute(s) };
            partial.set(s).map_err(re)
        }
        ScalarType::CowStr => {
            let s = cursor.read_str()?;
            if BORROW {
                // SAFETY: The caller of from_slice_borrowed guarantees 'input: 'facet.
                #[allow(unsafe_code)]
                let s: &'facet str = unsafe { std::mem::transmute(s) };
                partial.set(std::borrow::Cow::Borrowed(s)).map_err(re)
            } else {
                partial
                    .set(std::borrow::Cow::<'facet, str>::Owned(s.to_owned()))
                    .map_err(re)
            }
        }
        _ => Err(DeserializeError::UnsupportedType(format!(
            "scalar {scalar_type:?}"
        ))),
    }
}

fn deserialize_option<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    inner_plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let tag = cursor.read_byte()?;
    match tag {
        0x00 => Ok(partial),
        0x01 => {
            let partial = partial.begin_some().map_err(re)?;
            let partial = deserialize_value::<BORROW>(partial, cursor, inner_plan, registry)?;
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
    plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());

    // Extract nested plans from Enum variant, or use Identity for both
    let nested = match plan {
        TranslationPlan::Enum { nested, .. } => nested,
        _ => &HashMap::new(),
    };
    let identity = TranslationPlan::Identity;

    let variant_index = cursor.read_varint()? as usize;
    match variant_index {
        0 => {
            let partial = partial.begin_ok().map_err(re)?;
            let ok_plan = nested.get(&0).unwrap_or(&identity);
            let partial = deserialize_value::<BORROW>(partial, cursor, ok_plan, registry)?;
            partial.end().map_err(re)
        }
        1 => {
            let partial = partial.begin_err().map_err(re)?;
            let err_plan = nested.get(&1).unwrap_or(&identity);
            let partial = deserialize_value::<BORROW>(partial, cursor, err_plan, registry)?;
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
    element_plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let len = cursor.read_varint()? as usize;
    let mut partial = partial.init_list_with_capacity(len).map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_list_item().map_err(re)?;
        partial = deserialize_value::<BORROW>(partial, cursor, element_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_array<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    n: usize,
    element_plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let mut partial = partial;
    for i in 0..n {
        partial = partial.begin_nth_field(i).map_err(re)?;
        partial = deserialize_value::<BORROW>(partial, cursor, element_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_map<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    key_plan: &TranslationPlan,
    value_plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let len = cursor.read_varint()? as usize;
    let mut partial = partial.init_map().map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_key().map_err(re)?;
        partial = deserialize_value::<BORROW>(partial, cursor, key_plan, registry)?;
        partial = partial.end().map_err(re)?;

        partial = partial.begin_value().map_err(re)?;
        partial = deserialize_value::<BORROW>(partial, cursor, value_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_set<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    element_plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let len = cursor.read_varint()? as usize;
    let mut partial = partial.init_set().map_err(re)?;
    for _ in 0..len {
        partial = partial.begin_set_item().map_err(re)?;
        partial = deserialize_value::<BORROW>(partial, cursor, element_plan, registry)?;
        partial = partial.end().map_err(re)?;
    }
    Ok(partial)
}

fn deserialize_pointer<'de, 'facet, const BORROW: bool>(
    partial: Partial<'facet, BORROW>,
    cursor: &mut Cursor<'de>,
    pointee_plan: &TranslationPlan,
    registry: &SchemaRegistry,
) -> Result<Partial<'facet, BORROW>, DeserializeError> {
    let re = |e: facet_reflect::ReflectError| DeserializeError::ReflectError(e.to_string());
    let shape = partial.shape();

    // Special case: &[u8] — borrowed byte slice reference.
    // We can't use begin_smart_ptr() for plain references. Instead, read
    // the bytes and set the fat pointer directly.
    if let Def::Pointer(ptr_def) = shape.def
        && let Some(facet_core::KnownPointer::SharedReference) = ptr_def.known
        && let Some(pointee) = ptr_def.pointee()
        && let Def::Slice(slice_def) = pointee.def
        && slice_def.t().is_type::<u8>()
    {
        let bytes = cursor.read_byte_slice()?;
        // SAFETY: from_slice_borrowed guarantees 'input: 'facet,
        // so borrowing from the cursor is valid.
        #[allow(unsafe_code)]
        let bytes: &'facet [u8] = unsafe { std::mem::transmute(bytes) };
        return partial.set(bytes).map_err(re);
    }

    let partial = partial.begin_smart_ptr().map_err(re)?;
    let partial = deserialize_value::<BORROW>(partial, cursor, pointee_plan, registry)?;
    partial.end().map_err(re)
}
