//! The inverse of [`super::decode_primitive`]'s `decoded_value`: instead of
//! walking a `Type` + dynamic value to BUILD a [`PrimitiveValue`], this walks a
//! target Rust type's `facet::Shape` and a wire `PrimitiveValue` in PARALLEL,
//! driving a [`facet_reflect::Partial`] to construct the typed value.
//!
//! `value` is not trusted: it is a runtime wire value that may have been built
//! by arbitrary vix source (or handed to a registered primitive by a hostile
//! caller), so every shape/wire mismatch here is a defensive `Err`, never a
//! panic — wrong `Product`/`Variant` arity, an out-of-range enum tag, the wrong
//! `PrimitiveValueBody` variant for the expected shape, a truncated or
//! oversized scalar leaf, invalid UTF-8. `unwrap`/indexing-without-a-check on
//! `value`'s contents is not acceptable anywhere in this module.
//!
//! The leaf-override types this decoder must special-case mirror
//! `crate::vir::Type::from_facet`'s `facet_leaf_override` table exactly, and
//! must decode the same bytes the hand-written parsers
//! (`fetch_primitive::parse_blob_id`/`parse_origins`/`parse_upstream`,
//! `observe_primitive::parse_request`, `decode_primitive::decode_request`,
//! `tree_read_primitive::parse_request`) already read: fixed 32-byte digests
//! are a hex `String`; a [`SchemaRef`] is its `canonical_bytes`; a
//! [`RegistryHandle`] is not decoded from bytes at all but from the *identity*
//! of the wire child it names (`machine.identity.handle-by-referent`).

use facet_reflect::Partial;

use crate::schema::SchemaRef;

use super::{
    Digest, PrimitiveField, PrimitiveFieldValue, PrimitiveMachineError, PrimitiveValue,
    PrimitiveValueBody, RegistryHandle, UpstreamDigest, ValueId,
};

/// Decode a runtime wire [`PrimitiveValue`] into a typed `T` — the mirror of
/// whatever built `value` from a `T` on the encode side. Never panics: every
/// shape/wire mismatch is an `Err`, because `value` may originate from
/// arbitrary vix source.
pub fn decode_primitive_value<T: facet::Facet<'static>>(
    value: &PrimitiveValue,
) -> Result<T, PrimitiveMachineError> {
    let root = value.identity();
    let partial = Partial::alloc_owned::<T>().map_err(|_| invalid(&root))?;
    let partial = decode_shape(partial, T::SHAPE, value, &root)?;
    let built = partial.build().map_err(|_| invalid(&root))?;
    built.materialize::<T>().map_err(|_| invalid(&root))
}

/// `value`'s own identity stands in for "the malformed request" in every error
/// this decoder returns — the closest available analogue to how the
/// hand-written parsers thread a `request: &ValueId` through every structural
/// check they perform (`fetch_primitive::parse_request`,
/// `observe_primitive::parse_request`).
fn invalid(root: &ValueId) -> PrimitiveMachineError {
    PrimitiveMachineError::InvalidRequest {
        request: root.clone(),
    }
}

fn expect_bytes<'a>(
    value: &'a PrimitiveValue,
    root: &ValueId,
) -> Result<&'a [u8], PrimitiveMachineError> {
    match &value.body {
        PrimitiveValueBody::Bytes(bytes) => Ok(bytes),
        PrimitiveValueBody::Product(_)
        | PrimitiveValueBody::Sequence { .. }
        | PrimitiveValueBody::Variant { .. } => Err(invalid(root)),
    }
}

/// Fixed 32-byte digests wire-encode as a hex `String`
/// (`fetch_primitive::parse_blob_id`/`parse_upstream`: `hex::decode`).
fn decode_hex_digest(
    value: &PrimitiveValue,
    root: &ValueId,
) -> Result<[u8; 32], PrimitiveMachineError> {
    let bytes = expect_bytes(value, root)?;
    let text = core::str::from_utf8(bytes).map_err(|_| invalid(root))?;
    let decoded = hex::decode(text).map_err(|_| invalid(root))?;
    <[u8; 32]>::try_from(decoded).map_err(|_| invalid(root))
}

fn decode_i64_leaf(bytes: &[u8], root: &ValueId) -> Result<i64, PrimitiveMachineError> {
    Ok(i64::from_le_bytes(
        bytes.try_into().map_err(|_| invalid(root))?,
    ))
}

/// Whether `shape`'s wire encoding as a [`PrimitiveField`] is the raw
/// `Inline` bytes form or a `Child` (nested [`PrimitiveValue`]).
///
/// Mirrors `decode_primitive::field_value`'s rule exactly: only `Int`/`Bool`
/// scalars are ever inlined; everything else — `String`, the leaf-override
/// types, records, enums, options, sequences — is a `Child`.
fn field_is_inline(shape: &'static facet::Shape) -> bool {
    if leaf_override_kind(shape).is_some() {
        return false;
    }
    matches!(
        shape.scalar_type(),
        Some(facet::ScalarType::Bool | facet::ScalarType::I64)
    )
}

/// Resolve a wire [`PrimitiveField`] against the target `shape`, producing the
/// owned [`PrimitiveValue`] `decode_shape` recurses into. `Inline` fields carry
/// no schema-wrapped value on the wire (just raw bytes), so this synthesizes
/// one; `Child` fields already are one and are cloned.
fn field_value(
    field: &PrimitiveField,
    shape: &'static facet::Shape,
    root: &ValueId,
) -> Result<PrimitiveValue, PrimitiveMachineError> {
    if field_is_inline(shape) {
        let PrimitiveFieldValue::Inline(bytes) = &field.value else {
            return Err(invalid(root));
        };
        Ok(PrimitiveValue::bytes(field.schema.clone(), bytes.clone()))
    } else {
        let PrimitiveFieldValue::Child(child) = &field.value else {
            return Err(invalid(root));
        };
        Ok((**child).clone())
    }
}

/// The handful of Rust types whose wire meaning cannot be read off their
/// `Facet` shape structurally — see `crate::vir::facet_leaf_override`, which
/// this must track exactly.
enum LeafOverride {
    Digest,
    UpstreamDigest,
    SchemaRef,
    RegistryHandle,
}

fn leaf_override_kind(shape: &'static facet::Shape) -> Option<LeafOverride> {
    if shape.id == <Digest as facet::Facet>::SHAPE.id {
        return Some(LeafOverride::Digest);
    }
    if shape.id == <UpstreamDigest as facet::Facet>::SHAPE.id {
        return Some(LeafOverride::UpstreamDigest);
    }
    if shape.id == <SchemaRef as facet::Facet>::SHAPE.id {
        return Some(LeafOverride::SchemaRef);
    }
    if shape.id == <RegistryHandle as facet::Facet>::SHAPE.id {
        return Some(LeafOverride::RegistryHandle);
    }
    None
}

/// Populate the Partial's current frame (already positioned at `shape`) from
/// `value`. Enters and exits at the same frame-stack depth: leaves either call
/// `.set(...)` directly on the current frame, or push/pop child frames in
/// exactly matched pairs.
fn decode_shape(
    partial: Partial<'static, false>,
    shape: &'static facet::Shape,
    value: &PrimitiveValue,
    root: &ValueId,
) -> Result<Partial<'static, false>, PrimitiveMachineError> {
    match leaf_override_kind(shape) {
        Some(LeafOverride::Digest) => {
            let digest = decode_hex_digest(value, root)?;
            return partial.set(Digest(digest)).map_err(|_| invalid(root));
        }
        Some(LeafOverride::UpstreamDigest) => {
            let digest = decode_hex_digest(value, root)?;
            return partial
                .set(UpstreamDigest(digest))
                .map_err(|_| invalid(root));
        }
        Some(LeafOverride::SchemaRef) => {
            let bytes = expect_bytes(value, root)?;
            let schema = SchemaRef::from_canonical_bytes(bytes).map_err(|_| invalid(root))?;
            return partial.set(schema).map_err(|_| invalid(root));
        }
        Some(LeafOverride::RegistryHandle) => {
            // Not decoded from bytes: a capability handle names its wire
            // child by referent identity (`fetch_primitive::parse_origins`:
            // `RegistryHandle(child(capability)?.identity())`).
            return partial
                .set(RegistryHandle(value.identity()))
                .map_err(|_| invalid(root));
        }
        None => {}
    }

    if let Some(scalar) = shape.scalar_type() {
        return decode_scalar(partial, scalar, value, root);
    }

    match shape.def {
        facet::Def::List(list) => decode_list(partial, list.t(), value, root),
        facet::Def::Slice(slice) => decode_list(partial, slice.t(), value, root),
        facet::Def::Option(option) => decode_option(partial, option.t(), value, root),
        facet::Def::Scalar | facet::Def::Undefined => decode_user(partial, shape, value, root),
        _ => Err(invalid(root)),
    }
}

fn decode_scalar(
    partial: Partial<'static, false>,
    scalar: facet::ScalarType,
    value: &PrimitiveValue,
    root: &ValueId,
) -> Result<Partial<'static, false>, PrimitiveMachineError> {
    let bytes = expect_bytes(value, root)?;
    match scalar {
        facet::ScalarType::Bool => {
            let n = decode_i64_leaf(bytes, root)?;
            partial.set(n != 0).map_err(|_| invalid(root))
        }
        facet::ScalarType::I64 => {
            let n = decode_i64_leaf(bytes, root)?;
            partial.set(n).map_err(|_| invalid(root))
        }
        facet::ScalarType::String => {
            let text = core::str::from_utf8(bytes).map_err(|_| invalid(root))?;
            partial.set(text.to_owned()).map_err(|_| invalid(root))
        }
        // `ISize`/`Str`/`CowStr` and anything else `facet::ScalarType` may add
        // are not produced by any of today's request types
        // (`crate::vir::facet_scalar` only ever emits `Type::Int`/`Type::Bool`
        // from `I64`/`ISize` and `Type::String` from `Str`/`String`/`CowStr`,
        // and every request type in this crate uses `i64`/`bool`/`String`
        // exclusively) — a defensive `Err` rather than a guess at their wire
        // encoding.
        _ => Err(invalid(root)),
    }
}

fn decode_list(
    mut partial: Partial<'static, false>,
    element_shape: &'static facet::Shape,
    value: &PrimitiveValue,
    root: &ValueId,
) -> Result<Partial<'static, false>, PrimitiveMachineError> {
    let PrimitiveValueBody::Sequence { elements, .. } = &value.body else {
        return Err(invalid(root));
    };
    partial = partial
        .init_list_with_capacity(elements.len())
        .map_err(|_| invalid(root))?;
    for element in elements {
        partial = partial.begin_list_item().map_err(|_| invalid(root))?;
        partial = decode_shape(partial, element_shape, element, root)?;
        partial = partial.end().map_err(|_| invalid(root))?;
    }
    Ok(partial)
}

fn decode_option(
    mut partial: Partial<'static, false>,
    inner_shape: &'static facet::Shape,
    value: &PrimitiveValue,
    root: &ValueId,
) -> Result<Partial<'static, false>, PrimitiveMachineError> {
    let PrimitiveValueBody::Variant { tag, fields } = &value.body else {
        return Err(invalid(root));
    };
    match (*tag, fields.as_slice()) {
        (crate::vir::OPTION_SOME_VARIANT, [field]) => {
            let inner = field_value(field, inner_shape, root)?;
            partial = partial.begin_some().map_err(|_| invalid(root))?;
            partial = decode_shape(partial, inner_shape, &inner, root)?;
            partial.end().map_err(|_| invalid(root))
        }
        (crate::vir::OPTION_NONE_VARIANT, []) => {
            // `Option<T>: Default` unconditionally (`None`), regardless of
            // `T`, so this sets the frame directly rather than needing to
            // enumerate a "no value" case per inner shape.
            partial.set_default().map_err(|_| invalid(root))
        }
        _ => Err(invalid(root)),
    }
}

fn decode_user(
    mut partial: Partial<'static, false>,
    shape: &'static facet::Shape,
    value: &PrimitiveValue,
    root: &ValueId,
) -> Result<Partial<'static, false>, PrimitiveMachineError> {
    match shape.ty {
        facet::Type::User(facet::UserType::Struct(struct_type)) => {
            let PrimitiveValueBody::Product(wire_fields) = &value.body else {
                return Err(invalid(root));
            };
            if wire_fields.len() != struct_type.fields.len() {
                return Err(invalid(root));
            }
            for (idx, (field_meta, wire_field)) in
                struct_type.fields.iter().zip(wire_fields).enumerate()
            {
                let field_shape = field_meta.shape();
                let resolved = field_value(wire_field, field_shape, root)?;
                partial = partial.begin_nth_field(idx).map_err(|_| invalid(root))?;
                partial = decode_shape(partial, field_shape, &resolved, root)?;
                partial = partial.end().map_err(|_| invalid(root))?;
            }
            Ok(partial)
        }
        facet::Type::User(facet::UserType::Enum(enum_type)) => {
            let PrimitiveValueBody::Variant {
                tag,
                fields: wire_fields,
            } = &value.body
            else {
                return Err(invalid(root));
            };
            let variant_idx = usize::try_from(*tag).map_err(|_| invalid(root))?;
            let variant = enum_type
                .variants
                .get(variant_idx)
                .ok_or_else(|| invalid(root))?;
            let variant_fields = variant.data.fields;
            if wire_fields.len() != variant_fields.len() {
                return Err(invalid(root));
            }
            partial = partial
                .select_nth_variant(variant_idx)
                .map_err(|_| invalid(root))?;
            for (idx, (field_meta, wire_field)) in
                variant_fields.iter().zip(wire_fields).enumerate()
            {
                let field_shape = field_meta.shape();
                let resolved = field_value(wire_field, field_shape, root)?;
                partial = partial.begin_nth_field(idx).map_err(|_| invalid(root))?;
                partial = decode_shape(partial, field_shape, &resolved, root)?;
                partial = partial.end().map_err(|_| invalid(root))?;
            }
            Ok(partial)
        }
        _ => Err(invalid(root)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vir::{ExternKind, Type};

    #[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
    struct Pair {
        left: i64,
        right: bool,
    }

    #[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
    struct Nested {
        name: String,
        pair: Pair,
        items: Vec<i64>,
        note: Option<String>,
    }

    #[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
    #[repr(u8)]
    enum Choice {
        Zero,
        One(i64),
        Two { a: bool, b: String },
    }

    fn inline_i64(n: i64) -> PrimitiveField {
        PrimitiveField {
            schema: Type::Int.schema_ref(),
            value: PrimitiveFieldValue::Inline(n.to_le_bytes().to_vec()),
        }
    }

    fn inline_bool(b: bool) -> PrimitiveField {
        PrimitiveField {
            schema: Type::Bool.schema_ref(),
            value: PrimitiveFieldValue::Inline(i64::from(b).to_le_bytes().to_vec()),
        }
    }

    fn child_string(s: &str) -> PrimitiveField {
        PrimitiveField {
            schema: Type::String.schema_ref(),
            value: PrimitiveFieldValue::Child(Box::new(PrimitiveValue::bytes(
                Type::String.schema_ref(),
                s.as_bytes().to_vec(),
            ))),
        }
    }

    fn child(value: PrimitiveValue) -> PrimitiveField {
        PrimitiveField {
            schema: value.schema.clone(),
            value: PrimitiveFieldValue::Child(Box::new(value)),
        }
    }

    fn string_value(s: &str) -> PrimitiveValue {
        PrimitiveValue::bytes(Type::String.schema_ref(), s.as_bytes().to_vec())
    }

    fn int_value(n: i64) -> PrimitiveValue {
        PrimitiveValue::bytes(Type::Int.schema_ref(), n.to_le_bytes().to_vec())
    }

    // -- round trip: nested struct/list/option/product ----------------------

    #[test]
    fn decodes_a_nested_struct_with_list_and_present_option() {
        let wire = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                child_string("hello"),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Product(vec![inline_i64(7), inline_bool(true)]),
                }),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Sequence {
                        element_schema: Type::Int.schema_ref(),
                        elements: vec![int_value(1), int_value(2), int_value(3)],
                    },
                }),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Variant {
                        tag: 0,
                        fields: vec![child_string("noted")],
                    },
                }),
            ]),
        };

        let decoded: Nested = decode_primitive_value(&wire).expect("well-formed wire value");
        assert_eq!(
            decoded,
            Nested {
                name: "hello".to_owned(),
                pair: Pair {
                    left: 7,
                    right: true,
                },
                items: vec![1, 2, 3],
                note: Some("noted".to_owned()),
            }
        );
    }

    #[test]
    fn decodes_a_missing_option_as_none() {
        let wire = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                child_string("hi"),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Product(vec![inline_i64(0), inline_bool(false)]),
                }),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Sequence {
                        element_schema: Type::Int.schema_ref(),
                        elements: vec![],
                    },
                }),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Variant {
                        tag: 1,
                        fields: vec![],
                    },
                }),
            ]),
        };

        let decoded: Nested = decode_primitive_value(&wire).expect("well-formed wire value");
        assert_eq!(
            decoded,
            Nested {
                name: "hi".to_owned(),
                pair: Pair { left: 0, right: false },
                items: vec![],
                note: None,
            }
        );
    }

    // -- round trip: enum variants of every kind -----------------------------

    #[test]
    fn decodes_every_enum_variant_kind() {
        let unit = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Variant {
                tag: 0,
                fields: vec![],
            },
        };
        assert_eq!(
            decode_primitive_value::<Choice>(&unit).expect("well-formed"),
            Choice::Zero
        );

        let tuple = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Variant {
                tag: 1,
                fields: vec![inline_i64(42)],
            },
        };
        assert_eq!(
            decode_primitive_value::<Choice>(&tuple).expect("well-formed"),
            Choice::One(42)
        );

        let record = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Variant {
                tag: 2,
                fields: vec![inline_bool(true), child_string("yo")],
            },
        };
        assert_eq!(
            decode_primitive_value::<Choice>(&record).expect("well-formed"),
            Choice::Two {
                a: true,
                b: "yo".to_owned(),
            }
        );
    }

    // -- adversarial: never panic, always Err --------------------------------

    #[test]
    fn rejects_wrong_product_arity_too_few() {
        let wire = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Product(vec![inline_i64(1)]),
        };
        assert!(decode_primitive_value::<Pair>(&wire).is_err());
    }

    #[test]
    fn rejects_wrong_product_arity_too_many() {
        let wire = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                inline_i64(1),
                inline_bool(true),
                inline_i64(2),
            ]),
        };
        assert!(decode_primitive_value::<Pair>(&wire).is_err());
    }

    #[test]
    fn rejects_out_of_range_variant_tag() {
        let wire = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Variant {
                tag: 99,
                fields: vec![],
            },
        };
        assert!(decode_primitive_value::<Choice>(&wire).is_err());
    }

    #[test]
    fn rejects_wrong_body_variant_bytes_instead_of_product() {
        let wire = PrimitiveValue::bytes(Type::Int.schema_ref(), vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(decode_primitive_value::<Pair>(&wire).is_err());
    }

    #[test]
    fn rejects_wrong_body_variant_product_instead_of_bytes() {
        let wire = PrimitiveValue {
            schema: Type::Int.schema_ref(),
            body: PrimitiveValueBody::Product(vec![]),
        };
        assert!(decode_primitive_value::<i64>(&wire).is_err());
    }

    #[test]
    fn rejects_truncated_int_leaf() {
        let wire = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                PrimitiveField {
                    schema: Type::Int.schema_ref(),
                    value: PrimitiveFieldValue::Inline(vec![1, 2, 3]),
                },
                inline_bool(true),
            ]),
        };
        assert!(decode_primitive_value::<Pair>(&wire).is_err());
    }

    #[test]
    fn rejects_invalid_utf8_string_leaf() {
        let wire = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                PrimitiveField {
                    schema: Type::String.schema_ref(),
                    value: PrimitiveFieldValue::Child(Box::new(PrimitiveValue::bytes(
                        Type::String.schema_ref(),
                        vec![0xff, 0xfe, 0xfd],
                    ))),
                },
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Product(vec![inline_i64(0), inline_bool(false)]),
                }),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Sequence {
                        element_schema: Type::Int.schema_ref(),
                        elements: vec![],
                    },
                }),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Variant {
                        tag: 1,
                        fields: vec![],
                    },
                }),
            ]),
        };
        assert!(decode_primitive_value::<Nested>(&wire).is_err());
    }

    #[test]
    fn rejects_deeply_nested_wrong_shape() {
        // `pair` (a `Pair` record) is given a `Bytes` body instead of `Product`.
        let wire = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                child_string("hello"),
                child(PrimitiveValue::bytes(
                    Type::Extern(ExternKind::Schema).schema_ref(),
                    vec![9, 9, 9, 9, 9, 9, 9, 9],
                )),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Sequence {
                        element_schema: Type::Int.schema_ref(),
                        elements: vec![],
                    },
                }),
                child(PrimitiveValue {
                    schema: Type::Extern(ExternKind::Schema).schema_ref(),
                    body: PrimitiveValueBody::Variant {
                        tag: 1,
                        fields: vec![],
                    },
                }),
            ]),
        };
        assert!(decode_primitive_value::<Nested>(&wire).is_err());
    }

    #[test]
    fn rejects_option_with_wrong_field_count() {
        let wire = PrimitiveValue {
            schema: Type::Extern(ExternKind::Schema).schema_ref(),
            body: PrimitiveValueBody::Variant {
                tag: 0,
                fields: vec![child_string("a"), child_string("b")],
            },
        };
        assert!(decode_primitive_value::<Option<String>>(&wire).is_err());
    }
}
