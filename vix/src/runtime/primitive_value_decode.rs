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
    use crate::runtime::fetch_primitive::{
        parse_blob_id, parse_origins, parse_request as fetch_parse_request, parse_upstream,
    };
    use crate::runtime::observe_primitive::parse_request as observe_parse_request;
    use crate::runtime::{
        BlobId, ObserveCoordinate, ObserveRequest, OriginHint, PinnedBlobRef, PinnedFetchRequest,
    };
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

    // -- agreement with the real request types and their hand-written parsers --
    //
    // Every generic test above only ever drives synthetic shapes (`Pair`,
    // `Nested`, `Choice`). None of it exercises the leaf-override paths
    // (`Digest`/`UpstreamDigest`/`SchemaRef`/`RegistryHandle`) or validates
    // this decoder against the actual request types it exists to decode. The
    // wire values below are built field-by-field to exactly match what
    // `fetch_primitive::parse_request`/`parse_blob_id`/`parse_origins`/
    // `parse_upstream` and `observe_primitive::parse_request` already read, so
    // a disagreement here is a real decoder bug, not a test artifact.

    fn product(schema: SchemaRef, fields: Vec<PrimitiveField>) -> PrimitiveValue {
        PrimitiveValue {
            schema,
            body: PrimitiveValueBody::Product(fields),
        }
    }

    fn sequence(
        schema: SchemaRef,
        element_schema: SchemaRef,
        elements: Vec<PrimitiveValue>,
    ) -> PrimitiveValue {
        PrimitiveValue {
            schema,
            body: PrimitiveValueBody::Sequence {
                element_schema,
                elements,
            },
        }
    }

    fn variant(schema: SchemaRef, tag: u32, fields: Vec<PrimitiveField>) -> PrimitiveValue {
        PrimitiveValue {
            schema,
            body: PrimitiveValueBody::Variant { tag, fields },
        }
    }

    fn hex_digest_wire(digest: [u8; 32]) -> PrimitiveValue {
        PrimitiveValue::bytes(
            Type::from_facet::<Digest>().schema_ref(),
            hex::encode(digest).into_bytes(),
        )
    }

    fn hex_upstream_digest_wire(digest: [u8; 32]) -> PrimitiveValue {
        PrimitiveValue::bytes(
            Type::from_facet::<UpstreamDigest>().schema_ref(),
            hex::encode(digest).into_bytes(),
        )
    }

    fn schema_ref_wire(schema: &SchemaRef) -> PrimitiveValue {
        PrimitiveValue::bytes(
            Type::from_facet::<SchemaRef>().schema_ref(),
            schema.canonical_bytes(),
        )
    }

    /// A capability's wire form: a `Bytes` leaf tagged `Extern(Registry)`. Its
    /// wire *identity* — never its bytes — is what a `RegistryHandle` decodes
    /// to (`machine.identity.handle-by-referent`), matching
    /// `fetch_primitive::parse_origins`: `RegistryHandle(child(capability)?.identity())`.
    fn registry_capability_wire(label: &str) -> PrimitiveValue {
        PrimitiveValue::bytes(
            Type::from_facet::<RegistryHandle>().schema_ref(),
            label.as_bytes().to_vec(),
        )
    }

    fn blob_id_wire(blob_id: &BlobId) -> PrimitiveValue {
        product(
            Type::from_facet::<BlobId>().schema_ref(),
            vec![
                child(schema_ref_wire(&blob_id.schema)),
                child(hex_digest_wire(blob_id.content.0)),
            ],
        )
    }

    fn origin_hint_wire(origin: &OriginHint, capability_wire: PrimitiveValue) -> PrimitiveValue {
        product(
            Type::from_facet::<OriginHint>().schema_ref(),
            vec![
                child(capability_wire),
                child_string(&origin.coordinate),
            ],
        )
    }

    fn origins_wire(origins: &[(OriginHint, PrimitiveValue)]) -> PrimitiveValue {
        sequence(
            Type::from_facet::<Vec<OriginHint>>().schema_ref(),
            Type::from_facet::<OriginHint>().schema_ref(),
            origins
                .iter()
                .map(|(origin, capability_wire)| origin_hint_wire(origin, capability_wire.clone()))
                .collect(),
        )
    }

    fn upstream_wire(upstream: Option<[u8; 32]>) -> PrimitiveValue {
        match upstream {
            Some(digest) => variant(
                Type::from_facet::<Option<UpstreamDigest>>().schema_ref(),
                crate::vir::OPTION_SOME_VARIANT,
                vec![child(hex_upstream_digest_wire(digest))],
            ),
            None => variant(
                Type::from_facet::<Option<UpstreamDigest>>().schema_ref(),
                crate::vir::OPTION_NONE_VARIANT,
                vec![],
            ),
        }
    }

    fn pinned_blob_ref_wire(
        blob_wire: PrimitiveValue,
        origins_wire: PrimitiveValue,
        upstream_wire: PrimitiveValue,
    ) -> PrimitiveValue {
        product(
            Type::from_facet::<PinnedBlobRef>().schema_ref(),
            vec![child(blob_wire), child(origins_wire), child(upstream_wire)],
        )
    }

    fn pinned_fetch_request_wire(pin_wire: PrimitiveValue) -> PrimitiveValue {
        product(
            Type::from_facet::<PinnedFetchRequest>().schema_ref(),
            vec![child(pin_wire)],
        )
    }

    /// One capability, one origin, an upstream digest present: exercises every
    /// leaf override plus `Vec`/`Option`/nested records at once.
    #[test]
    fn round_trips_pinned_fetch_request_against_hand_parsers() {
        let blob_id = BlobId {
            schema: Type::Extern(ExternKind::Blob).schema_ref(),
            content: Digest([0xAB; 32]),
        };
        let blob_wire = blob_id_wire(&blob_id);

        let capability_a = registry_capability_wire("registry:primary");
        let origin_a = OriginHint {
            capability: RegistryHandle(capability_a.identity()),
            coordinate: "artifacts/a.bin".to_owned(),
        };
        let capability_b = registry_capability_wire("registry:mirror");
        let origin_b = OriginHint {
            capability: RegistryHandle(capability_b.identity()),
            coordinate: "artifacts/b.bin".to_owned(),
        };
        let origins = vec![(origin_a.clone(), capability_a), (origin_b.clone(), capability_b)];
        let origins_wire_value = origins_wire(&origins);

        let upstream_bytes = [0xCD; 32];
        let upstream_wire_value = upstream_wire(Some(upstream_bytes));

        let pin_wire = pinned_blob_ref_wire(
            blob_wire.clone(),
            origins_wire_value.clone(),
            upstream_wire_value.clone(),
        );
        let wire = pinned_fetch_request_wire(pin_wire.clone());

        let expected = PinnedBlobRef {
            value: blob_id.clone(),
            origins: vec![origin_a.clone(), origin_b.clone()],
            upstream: Some(UpstreamDigest(upstream_bytes)),
        };

        // (1) `decode_primitive_value` reconstructs the exact typed value.
        let decoded: PinnedFetchRequest =
            decode_primitive_value(&wire).expect("well-formed pinned-fetch wire value");
        assert_eq!(decoded.pin, expected);

        // (2) It agrees with the hand-written parser on the SAME wire value —
        // top-level `parse_request` and every sub-parser it delegates to.
        let hand_parsed = fetch_parse_request(wire.clone(), wire.identity())
            .expect("hand parser accepts the same wire value");
        assert_eq!(
            decoded.pin, hand_parsed,
            "decode_primitive_value disagrees with fetch_primitive::parse_request"
        );

        let hand_blob_id = parse_blob_id(&pin_wire_field(&pin_wire, 0), &wire.identity())
            .expect("parse_blob_id accepts the same wire value");
        assert_eq!(
            decoded.pin.value, hand_blob_id,
            "decode_primitive_value disagrees with fetch_primitive::parse_blob_id"
        );

        let hand_origins = parse_origins(&pin_wire_field(&pin_wire, 1))
            .expect("parse_origins accepts the same wire value");
        assert_eq!(
            decoded.pin.origins, hand_origins,
            "decode_primitive_value disagrees with fetch_primitive::parse_origins"
        );

        let hand_upstream = parse_upstream(&pin_wire_field(&pin_wire, 2))
            .expect("parse_upstream accepts the same wire value");
        assert_eq!(
            decoded.pin.upstream, hand_upstream,
            "decode_primitive_value disagrees with fetch_primitive::parse_upstream"
        );
    }

    /// The empty/absent edge: no origins, no upstream digest — exercises the
    /// `None` variant and a zero-length `Sequence` against the same parsers.
    #[test]
    fn round_trips_pinned_fetch_request_with_no_origins_and_no_upstream() {
        let blob_id = BlobId {
            schema: Type::Extern(ExternKind::Blob).schema_ref(),
            content: Digest([0x11; 32]),
        };
        let blob_wire = blob_id_wire(&blob_id);
        let origins_wire_value = origins_wire(&[]);
        let upstream_wire_value = upstream_wire(None);

        let pin_wire = pinned_blob_ref_wire(blob_wire, origins_wire_value, upstream_wire_value);
        let wire = pinned_fetch_request_wire(pin_wire);

        let expected = PinnedBlobRef {
            value: blob_id,
            origins: vec![],
            upstream: None,
        };

        let decoded: PinnedFetchRequest =
            decode_primitive_value(&wire).expect("well-formed pinned-fetch wire value");
        assert_eq!(decoded.pin, expected);

        let hand_parsed = fetch_parse_request(wire.clone(), wire.identity())
            .expect("hand parser accepts the same wire value");
        assert_eq!(
            decoded.pin, hand_parsed,
            "decode_primitive_value disagrees with fetch_primitive::parse_request"
        );
    }

    /// Pull the nth `Child` field back out of a `Product` wire value — used to
    /// hand the sub-parsers (`parse_blob_id`/`parse_origins`/`parse_upstream`)
    /// exactly the same nested wire value `decode_primitive_value` walked.
    fn pin_wire_field(pin_wire: &PrimitiveValue, index: usize) -> PrimitiveValue {
        let PrimitiveValueBody::Product(fields) = &pin_wire.body else {
            panic!("pin_wire is always a Product in these tests");
        };
        let PrimitiveFieldValue::Child(child) = &fields[index].value else {
            panic!("pin_wire fields are always Child in these tests");
        };
        (**child).clone()
    }

    #[test]
    fn round_trips_observe_request_against_hand_parser() {
        let capability = registry_capability_wire("registry:primary");
        let origin = OriginHint {
            capability: RegistryHandle(capability.identity()),
            coordinate: "coordinate/path".to_owned(),
        };
        let origin_wire_value = origin_hint_wire(&origin, capability);

        let wire = product(
            Type::from_facet::<ObserveRequest>().schema_ref(),
            vec![child(origin_wire_value), inline_bool(true)],
        );

        let expected = ObserveRequest {
            origin: origin.clone(),
            refresh: true,
        };

        let decoded: ObserveRequest =
            decode_primitive_value(&wire).expect("well-formed observe wire value");
        assert_eq!(decoded, expected);

        let (hand_coordinate, hand_refresh) = observe_parse_request(wire.clone(), wire.identity())
            .expect("hand parser accepts the same wire value");
        assert_eq!(hand_refresh, expected.refresh);
        assert_eq!(
            hand_coordinate,
            ObserveCoordinate {
                capability: origin.capability.0,
                coordinate: origin.coordinate,
            },
            "decode_primitive_value disagrees with observe_primitive::parse_request"
        );
    }

    #[test]
    fn round_trips_observe_request_refresh_false() {
        let capability = registry_capability_wire("registry:mirror");
        let origin = OriginHint {
            capability: RegistryHandle(capability.identity()),
            coordinate: "coordinate/other".to_owned(),
        };
        let origin_wire_value = origin_hint_wire(&origin, capability);

        let wire = product(
            Type::from_facet::<ObserveRequest>().schema_ref(),
            vec![child(origin_wire_value), inline_bool(false)],
        );

        let expected = ObserveRequest {
            origin: origin.clone(),
            refresh: false,
        };

        let decoded: ObserveRequest =
            decode_primitive_value(&wire).expect("well-formed observe wire value");
        assert_eq!(decoded, expected);

        let (_, hand_refresh) = observe_parse_request(wire.clone(), wire.identity())
            .expect("hand parser accepts the same wire value");
        assert_eq!(hand_refresh, expected.refresh);
    }

    // -- adversarial: leaf-override paths, never panic, always Err -----------

    #[test]
    fn rejects_digest_with_non_hex_bytes() {
        let wire = blob_id_wire(&BlobId {
            schema: Type::Extern(ExternKind::Blob).schema_ref(),
            content: Digest([0; 32]),
        });
        let PrimitiveValueBody::Product(mut fields) = wire.body else {
            unreachable!("blob_id_wire is always Product");
        };
        fields[1] = child(PrimitiveValue::bytes(
            Type::from_facet::<Digest>().schema_ref(),
            b"not-hex-at-all-zz".to_vec(),
        ));
        let wire = product(wire.schema, fields);
        assert!(decode_primitive_value::<BlobId>(&wire).is_err());
    }

    #[test]
    fn rejects_upstream_digest_with_non_hex_bytes() {
        let wire = variant(
            Type::from_facet::<Option<UpstreamDigest>>().schema_ref(),
            crate::vir::OPTION_SOME_VARIANT,
            vec![child(PrimitiveValue::bytes(
                Type::from_facet::<UpstreamDigest>().schema_ref(),
                b"zz-not-hex".to_vec(),
            ))],
        );
        assert!(decode_primitive_value::<Option<UpstreamDigest>>(&wire).is_err());
    }

    #[test]
    fn rejects_hex_digest_that_decodes_to_wrong_length() {
        // Valid hex, but only 16 bytes once decoded — not the required 32.
        let short_hex = hex::encode([0xAA; 16]);
        let wire = blob_id_wire(&BlobId {
            schema: Type::Extern(ExternKind::Blob).schema_ref(),
            content: Digest([0; 32]),
        });
        let PrimitiveValueBody::Product(mut fields) = wire.body else {
            unreachable!("blob_id_wire is always Product");
        };
        fields[1] = child(PrimitiveValue::bytes(
            Type::from_facet::<Digest>().schema_ref(),
            short_hex.into_bytes(),
        ));
        let wire = product(wire.schema, fields);
        assert!(decode_primitive_value::<BlobId>(&wire).is_err());
    }

    #[test]
    fn rejects_hex_digest_that_decodes_to_too_many_bytes() {
        let long_hex = hex::encode([0xAA; 33]);
        let wire = blob_id_wire(&BlobId {
            schema: Type::Extern(ExternKind::Blob).schema_ref(),
            content: Digest([0; 32]),
        });
        let PrimitiveValueBody::Product(mut fields) = wire.body else {
            unreachable!("blob_id_wire is always Product");
        };
        fields[1] = child(PrimitiveValue::bytes(
            Type::from_facet::<Digest>().schema_ref(),
            long_hex.into_bytes(),
        ));
        let wire = product(wire.schema, fields);
        assert!(decode_primitive_value::<BlobId>(&wire).is_err());
    }

    #[test]
    fn rejects_schema_ref_with_bytes_that_are_not_valid_canonical_schema_bytes() {
        // Too short to even carry a `SchemaId` + arg count (needs >= 16 bytes).
        let wire = blob_id_wire(&BlobId {
            schema: Type::Extern(ExternKind::Blob).schema_ref(),
            content: Digest([0; 32]),
        });
        let PrimitiveValueBody::Product(mut fields) = wire.body else {
            unreachable!("blob_id_wire is always Product");
        };
        fields[0] = child(PrimitiveValue::bytes(
            Type::from_facet::<SchemaRef>().schema_ref(),
            vec![1, 2, 3],
        ));
        let wire = product(wire.schema, fields);
        assert!(decode_primitive_value::<BlobId>(&wire).is_err());
    }

    #[test]
    fn rejects_schema_ref_with_trailing_garbage_bytes() {
        let mut bytes = Type::Extern(ExternKind::Blob).schema_ref().canonical_bytes();
        bytes.push(0xFF);
        let wire = blob_id_wire(&BlobId {
            schema: Type::Extern(ExternKind::Blob).schema_ref(),
            content: Digest([0; 32]),
        });
        let PrimitiveValueBody::Product(mut fields) = wire.body else {
            unreachable!("blob_id_wire is always Product");
        };
        fields[0] = child(PrimitiveValue::bytes(
            Type::from_facet::<SchemaRef>().schema_ref(),
            bytes,
        ));
        let wire = product(wire.schema, fields);
        assert!(decode_primitive_value::<BlobId>(&wire).is_err());
    }

    #[test]
    fn rejects_schema_ref_child_with_product_body_instead_of_bytes() {
        let wire = blob_id_wire(&BlobId {
            schema: Type::Extern(ExternKind::Blob).schema_ref(),
            content: Digest([0; 32]),
        });
        let PrimitiveValueBody::Product(mut fields) = wire.body else {
            unreachable!("blob_id_wire is always Product");
        };
        fields[0] = child(PrimitiveValue {
            schema: Type::from_facet::<SchemaRef>().schema_ref(),
            body: PrimitiveValueBody::Product(vec![]),
        });
        let wire = product(wire.schema, fields);
        assert!(decode_primitive_value::<BlobId>(&wire).is_err());
    }

    #[test]
    fn rejects_registry_handle_field_given_inline_instead_of_child() {
        // `RegistryHandle` (like every leaf override) is never `Inline` on the
        // wire — only `Int`/`Bool` scalars are.
        let wire = PrimitiveValue {
            schema: Type::from_facet::<OriginHint>().schema_ref(),
            body: PrimitiveValueBody::Product(vec![inline_i64(0), child_string("coordinate")]),
        };
        assert!(decode_primitive_value::<OriginHint>(&wire).is_err());
    }

    #[test]
    fn rejects_origin_hint_with_too_few_fields() {
        let capability = registry_capability_wire("registry:primary");
        let wire = PrimitiveValue {
            schema: Type::from_facet::<OriginHint>().schema_ref(),
            body: PrimitiveValueBody::Product(vec![child(capability)]),
        };
        assert!(decode_primitive_value::<OriginHint>(&wire).is_err());
    }

    #[test]
    fn rejects_origin_hint_with_too_many_fields() {
        let capability = registry_capability_wire("registry:primary");
        let wire = PrimitiveValue {
            schema: Type::from_facet::<OriginHint>().schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                child(capability),
                child_string("coordinate"),
                child_string("unexpected"),
            ]),
        };
        assert!(decode_primitive_value::<OriginHint>(&wire).is_err());
    }

    #[test]
    fn rejects_bytes_leaf_where_a_real_request_expects_a_child_product() {
        // `PinnedBlobRef.value: BlobId` is a struct; give it `Bytes` instead of
        // `Product`.
        let wire = pinned_blob_ref_wire(
            PrimitiveValue::bytes(Type::from_facet::<BlobId>().schema_ref(), vec![1, 2, 3]),
            origins_wire(&[]),
            upstream_wire(None),
        );
        assert!(decode_primitive_value::<PinnedBlobRef>(&wire).is_err());
    }

    #[test]
    fn rejects_product_where_a_real_request_expects_a_bytes_leaf() {
        // `OriginHint.coordinate: String` is a scalar; give it a `Product`
        // instead of `Bytes`.
        let capability = registry_capability_wire("registry:primary");
        let wire = PrimitiveValue {
            schema: Type::from_facet::<OriginHint>().schema_ref(),
            body: PrimitiveValueBody::Product(vec![
                child(capability),
                child(PrimitiveValue {
                    schema: Type::String.schema_ref(),
                    body: PrimitiveValueBody::Product(vec![]),
                }),
            ]),
        };
        assert!(decode_primitive_value::<OriginHint>(&wire).is_err());
    }
}
