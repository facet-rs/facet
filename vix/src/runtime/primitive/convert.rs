//! Rust value ↔ interned store value conversion.
//!
//! The whole point of this module is *parity*: a Rust value encoded here frames
//! byte-for-byte the way the scheduler's `realize_structural_node` frames the
//! same value, so ValueIds computed here agree with values vix constructs
//! itself. Every framing rule mirrors `scheduler.rs::realize_structural_*`.
//!
//! Phase 02 lands conversion ahead of its consumers (the primitive adapter and,
//! in phase 05, the scheduler). The functions are exercised by unit tests here;
//! `dead_code` is expected until that wiring exists.
#![allow(dead_code)]

use std::cmp::Ordering;

use crate::runtime::identity::{FramedField, FramedNode, FramedValue, semantic_schema_id};
use crate::runtime::store::{FrozenValue, Interned, Store};
use crate::vir::{OPTION_NONE_VARIANT, OPTION_SOME_VARIANT, Type, VariantPayload};

use super::descriptor::RegisteredSchema;

/// A structurally-encoded value: the framed node (its identity), the frozen
/// replay tree, and the canonical resident bytes (empty for aggregates).
pub(crate) struct Encoded {
    pub node: FramedNode,
    pub frozen: FrozenValue,
    pub resident: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConvertError {
    ShapeMismatch {
        path: String,
        expected: String,
        found: String,
    },
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ShapeMismatch {
                path,
                expected,
                found,
            } => write!(
                f,
                "value shape mismatch at {path:?}: expected {expected}, found {found}"
            ),
        }
    }
}

impl std::error::Error for ConvertError {}

fn mismatch(ty: &Type, found: &str) -> ConvertError {
    ConvertError::ShapeMismatch {
        path: String::new(),
        expected: ty.name(),
        found: found.to_owned(),
    }
}

/// A scalar leaf frames inline (r[machine.value.scalar-inline]): the scheduler
/// inlines `Bool`/`Int` field payloads as `FramedValue::Bytes` rather than as a
/// child reference.
fn is_scalar(ty: &Type) -> bool {
    matches!(ty, Type::Bool | Type::Int)
}

/// Encode a facet value into a framed store value, driven by the vir type. The
/// bridge already validated the facet shape, so any divergence here is a
/// structural bug surfaced as [`ConvertError::ShapeMismatch`], not a user error.
pub(crate) fn encode_value(
    peek: facet::Peek<'_, '_>,
    ty: &Type,
) -> Result<Encoded, ConvertError> {
    match ty {
        Type::Bool => {
            let value = peek.get::<bool>().map_err(|_| mismatch(ty, "non-bool"))?;
            let word = i64::from(*value).to_le_bytes().to_vec();
            Ok(Encoded {
                node: FramedNode::leaf(semantic_schema_id(ty), word.clone()),
                frozen: FrozenValue::Inline(word.clone()),
                resident: word,
            })
        }
        Type::Int => {
            let value = peek.get::<i64>().map_err(|_| mismatch(ty, "non-i64"))?;
            let word = value.to_le_bytes().to_vec();
            Ok(Encoded {
                node: FramedNode::leaf(semantic_schema_id(ty), word.clone()),
                frozen: FrozenValue::Inline(word.clone()),
                resident: word,
            })
        }
        Type::String => {
            let text = peek.as_str().ok_or_else(|| mismatch(ty, "non-string"))?;
            let bytes = text.as_bytes().to_vec();
            Ok(Encoded {
                node: FramedNode::leaf(semantic_schema_id(ty), bytes.clone()),
                frozen: FrozenValue::Opaque(bytes.clone()),
                resident: bytes,
            })
        }
        Type::Tuple(elements) => {
            let tuple = peek.into_tuple().map_err(|_| mismatch(ty, "non-tuple"))?;
            let mut items = Vec::with_capacity(elements.len());
            for (index, element_ty) in elements.iter().enumerate() {
                let field = tuple.field(index).ok_or_else(|| mismatch(ty, "short tuple"))?;
                items.push((field, element_ty));
            }
            frame_product(ty, items)
        }
        Type::Record(record) => {
            let structure = peek.into_struct().map_err(|_| mismatch(ty, "non-struct"))?;
            let mut items = Vec::with_capacity(record.fields.len());
            for (index, field) in record.fields.iter().enumerate() {
                let peek = structure
                    .field(index)
                    .map_err(|_| mismatch(ty, "missing field"))?;
                items.push((peek, &field.ty));
            }
            frame_product(ty, items)
        }
        Type::Enum(enumeration) => {
            if let Some(inner) = ty.option_inner() {
                encode_option(peek, ty, inner)
            } else {
                let peek_enum = peek.into_enum().map_err(|_| mismatch(ty, "non-enum"))?;
                let tag = peek_enum
                    .variant_index()
                    .map_err(|_| mismatch(ty, "no active variant"))?;
                let variant = enumeration
                    .variants
                    .get(tag)
                    .ok_or_else(|| mismatch(ty, "variant index out of range"))?;
                let field_types = payload_field_types(&variant.payload);
                let mut items = Vec::with_capacity(field_types.len());
                for (index, field_ty) in field_types.into_iter().enumerate() {
                    let field = peek_enum
                        .field(index)
                        .map_err(|_| mismatch(ty, "variant field"))?
                        .ok_or_else(|| mismatch(ty, "variant field"))?;
                    items.push((field, field_ty));
                }
                frame_variant(ty, tag as u64, items)
            }
        }
        Type::Array(element) => encode_array(peek, ty, element),
        Type::Map { key, value } => encode_map(peek, ty, key, value),
        Type::Set(element) => encode_set(peek, ty, element),
        _ => Err(mismatch(ty, "unsupported type")),
    }
}

fn payload_field_types(payload: &VariantPayload) -> Vec<&Type> {
    match payload {
        VariantPayload::Unit => Vec::new(),
        VariantPayload::Tuple(elements) => elements.iter().collect(),
        VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
    }
}

/// Frame the shared field structure of a record/tuple (product) or an enum
/// variant. Scalar fields inline as bytes; every other field contributes its
/// child identity, exactly as `realize_structural_fields` does.
fn frame_fields(
    items: Vec<(facet::Peek<'_, '_>, &Type)>,
) -> Result<(Vec<FramedField>, Vec<FrozenValue>), ConvertError> {
    let mut fields = Vec::with_capacity(items.len());
    let mut frozen = Vec::with_capacity(items.len());
    for (peek, field_ty) in items {
        let child = encode_value(peek, field_ty)?;
        let value = if is_scalar(field_ty) {
            FramedValue::Bytes(child.resident)
        } else {
            FramedValue::Optional(Some(child.node.identity()))
        };
        fields.push(FramedField {
            schema: semantic_schema_id(field_ty),
            value,
        });
        frozen.push(child.frozen);
    }
    Ok((fields, frozen))
}

fn frame_product(
    ty: &Type,
    items: Vec<(facet::Peek<'_, '_>, &Type)>,
) -> Result<Encoded, ConvertError> {
    let (fields, frozen) = frame_fields(items)?;
    Ok(Encoded {
        node: FramedNode::Variant {
            schema: semantic_schema_id(ty),
            tag: 0,
            fields,
        },
        frozen: FrozenValue::Product(frozen),
        resident: Vec::new(),
    })
}

fn frame_variant(
    ty: &Type,
    tag: u64,
    items: Vec<(facet::Peek<'_, '_>, &Type)>,
) -> Result<Encoded, ConvertError> {
    let (fields, frozen) = frame_fields(items)?;
    Ok(Encoded {
        node: FramedNode::Variant {
            schema: semantic_schema_id(ty),
            tag,
            fields,
        },
        frozen: FrozenValue::Variant {
            tag: tag as u32,
            fields: frozen,
        },
        resident: Vec::new(),
    })
}

fn encode_option(
    peek: facet::Peek<'_, '_>,
    ty: &Type,
    inner: &Type,
) -> Result<Encoded, ConvertError> {
    let option = peek.into_option().map_err(|_| mismatch(ty, "non-option"))?;
    if let Some(value) = option.value() {
        frame_variant(ty, u64::from(OPTION_SOME_VARIANT), vec![(value, inner)])
    } else {
        frame_variant(ty, u64::from(OPTION_NONE_VARIANT), Vec::new())
    }
}

fn encode_array(
    peek: facet::Peek<'_, '_>,
    ty: &Type,
    element: &Type,
) -> Result<Encoded, ConvertError> {
    let list = peek.into_list().map_err(|_| mismatch(ty, "non-list"))?;
    let mut children = Vec::with_capacity(list.len());
    for element_peek in list.iter() {
        children.push(encode_value(element_peek, element)?);
    }
    let element_schema = semantic_schema_id(element);
    let schema = semantic_schema_id(ty);
    let frozen = FrozenValue::DenseArray(children.iter().map(|c| c.frozen.clone()).collect());
    // A scalar-element array frames inline (packed 8-byte words); anything whose
    // element carries a handle frames by child identity. This mirrors
    // `realize_array`'s `type_contains_handle` split for the cases a primitive
    // request/response can hold. (Arrays of pure-scalar *composites* would frame
    // inline in the scheduler too; those are out of the phase-02 subset.)
    let node = if is_scalar(element) {
        let mut canonical_bytes = Vec::with_capacity(children.len() * 8);
        for child in &children {
            canonical_bytes.extend_from_slice(&child.resident);
        }
        FramedNode::SeqInline {
            schema,
            element_schema,
            element_width: 8,
            canonical_bytes,
        }
    } else {
        FramedNode::SeqChildren {
            schema,
            element_schema,
            children: children.iter().map(|c| c.node.identity()).collect(),
        }
    };
    Ok(Encoded {
        node,
        frozen,
        resident: Vec::new(),
    })
}

fn encode_map(
    peek: facet::Peek<'_, '_>,
    ty: &Type,
    key_ty: &Type,
    value_ty: &Type,
) -> Result<Encoded, ConvertError> {
    let map = peek.into_map().map_err(|_| mismatch(ty, "non-map"))?;
    let mut rows = Vec::new();
    for (key, value) in map.iter() {
        rows.push((encode_value(key, key_ty)?, encode_value(value, value_ty)?));
    }
    rows.sort_by(|(ka, _), (kb, _)| structural_cmp(&ka.frozen, &kb.frozen));
    let schema = semantic_schema_id(ty);
    Ok(Encoded {
        node: FramedNode::OrderedMap {
            schema,
            rows: rows
                .iter()
                .map(|(k, v)| (k.node.identity(), v.node.identity()))
                .collect(),
        },
        frozen: FrozenValue::OrderedMap(
            rows.iter()
                .map(|(k, v)| (k.frozen.clone(), v.frozen.clone()))
                .collect(),
        ),
        resident: Vec::new(),
    })
}

fn encode_set(
    peek: facet::Peek<'_, '_>,
    ty: &Type,
    element: &Type,
) -> Result<Encoded, ConvertError> {
    let set = peek.into_set().map_err(|_| mismatch(ty, "non-set"))?;
    let mut elements = Vec::new();
    for element_peek in set.iter() {
        elements.push(encode_value(element_peek, element)?);
    }
    elements.sort_by(|a, b| structural_cmp(&a.frozen, &b.frozen));
    let schema = semantic_schema_id(ty);
    Ok(Encoded {
        node: FramedNode::OrderedSet {
            schema,
            elements: elements.iter().map(|e| e.node.identity()).collect(),
        },
        frozen: FrozenValue::OrderedSet(elements.iter().map(|e| e.frozen.clone()).collect()),
        resident: Vec::new(),
    })
}

/// Canonical key/element order for maps and sets, mirroring the ordered
/// machine's `lang.value.ordering`: `Int`/`Bool` compare numerically, strings
/// (opaque bytes) lexicographically, and aggregates compare component-wise.
/// Identity hashing does not re-sort, so encode MUST present rows in this order
/// for ValueIds to agree with a vix-constructed map/set.
fn structural_cmp(a: &FrozenValue, b: &FrozenValue) -> Ordering {
    use FrozenValue::{DenseArray, Inline, OrderedMap, OrderedSet, Product, Variant};
    match (a, b) {
        (Inline(x), Inline(y)) => read_word(x).cmp(&read_word(y)),
        (FrozenValue::Opaque(x), FrozenValue::Opaque(y)) => x.cmp(y),
        (Product(x), Product(y)) => cmp_frozen_slice(x, y),
        (
            Variant {
                tag: ta,
                fields: fa,
            },
            Variant {
                tag: tb,
                fields: fb,
            },
        ) => ta.cmp(tb).then_with(|| cmp_frozen_slice(fa, fb)),
        (DenseArray(x), DenseArray(y)) => cmp_frozen_slice(x, y),
        (OrderedSet(x), OrderedSet(y)) => cmp_frozen_slice(x, y),
        (OrderedMap(x), OrderedMap(y)) => {
            let mut xi = x.iter();
            let mut yi = y.iter();
            loop {
                match (xi.next(), yi.next()) {
                    (Some((xk, xv)), Some((yk, yv))) => {
                        let ordering = structural_cmp(xk, yk).then_with(|| structural_cmp(xv, yv));
                        if ordering != Ordering::Equal {
                            return ordering;
                        }
                    }
                    (Some(_), None) => return Ordering::Greater,
                    (None, Some(_)) => return Ordering::Less,
                    (None, None) => return Ordering::Equal,
                }
            }
        }
        _ => Ordering::Equal,
    }
}

fn cmp_frozen_slice(a: &[FrozenValue], b: &[FrozenValue]) -> Ordering {
    for (x, y) in a.iter().zip(b.iter()) {
        let ordering = structural_cmp(x, y);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    a.len().cmp(&b.len())
}

fn read_word(bytes: &[u8]) -> i64 {
    let mut word = [0u8; 8];
    let take = bytes.len().min(8);
    word[..take].copy_from_slice(&bytes[..take]);
    i64::from_le_bytes(word)
}

/// Encode `value` under `schema`'s vir type, intern the framed tree, and attach
/// its frozen replay tree. The returned [`Interned`] carries the same ValueId a
/// vix-constructed value of the same shape would.
pub(crate) fn intern_rust_value<'f, T: facet::Facet<'f>>(
    value: &T,
    schema: &RegisteredSchema,
    store: &mut Store,
) -> Result<Interned, ConvertError> {
    let encoded = encode_value(facet::Peek::new(value), &schema.vix_type)?;
    let interned = store.intern_tree(&encoded.node, &encoded.resident);
    store.attach_frozen(interned.handle, encoded.frozen);
    Ok(interned)
}

/// Resolve every [`FrozenValue::Reference`] in a request tree against the store,
/// substituting each referent's concrete frozen structure. A store-resident
/// string leaf resolves to [`FrozenValue::Opaque`] (its resident bytes); a nested
/// aggregate resolves to its own frozen tree, itself resolved so the result is
/// reference-free at every depth. `Inline`/`Opaque` pass through unchanged;
/// products/variants/collections recurse structurally.
///
/// The scheduler calls this on a request frozen taken from `&Store` BEFORE the
/// `&mut Store` window ([`EffectCtx`](super::EffectCtx)) opens, so
/// [`decode_value`] never sees a reference — its reference rejection stays the
/// honest backstop. Reads reuse the store's `by_identity` index via
/// `handle_for_identity` (the same O(log n) lookup `render_frozen`'s leaf/deref
/// helpers use), so no linear scan is introduced.
///
/// Returns `None` when a reference names a value with neither a frozen tree nor
/// resident bytes in the store — a machine invariant the caller lifts onto the
/// machine-error plane.
pub(crate) fn resolve_references(frozen: &FrozenValue, store: &Store) -> Option<FrozenValue> {
    Some(match frozen {
        FrozenValue::Inline(bytes) => FrozenValue::Inline(bytes.clone()),
        FrozenValue::Opaque(bytes) => FrozenValue::Opaque(bytes.clone()),
        FrozenValue::Reference(id) => {
            let entry = store.entry(store.handle_for_identity(*id)?)?;
            if let Some(inner) = entry.frozen() {
                // An aggregate (or a string that already carries an Opaque frozen)
                // resolves to its own structure — recursively reference-free.
                resolve_references(inner, store)?
            } else {
                // A resident leaf with no frozen tree is a string constant; its
                // canonical resident bytes are the opaque payload decode expects.
                FrozenValue::Opaque(entry.resident_bytes()?.to_vec())
            }
        }
        FrozenValue::Product(fields) => FrozenValue::Product(resolve_each(fields, store)?),
        FrozenValue::Variant { tag, fields } => FrozenValue::Variant {
            tag: *tag,
            fields: resolve_each(fields, store)?,
        },
        FrozenValue::DenseArray(items) => FrozenValue::DenseArray(resolve_each(items, store)?),
        FrozenValue::OrderedSet(items) => FrozenValue::OrderedSet(resolve_each(items, store)?),
        FrozenValue::OrderedMap(rows) => {
            let mut resolved = Vec::with_capacity(rows.len());
            for (key, value) in rows {
                resolved.push((
                    resolve_references(key, store)?,
                    resolve_references(value, store)?,
                ));
            }
            FrozenValue::OrderedMap(resolved)
        }
    })
}

fn resolve_each(items: &[FrozenValue], store: &Store) -> Option<Vec<FrozenValue>> {
    items
        .iter()
        .map(|item| resolve_references(item, store))
        .collect()
}

/// Decode a fully-frozen value tree back into a Rust value, driven by the vir
/// type through facet's typed [`facet::Partial`] builder.
///
/// v1 takes the frozen tree only: a [`FrozenValue::Reference`] (which appears for
/// store-resident strings in real scheduler output) is a [`ConvertError`] here.
/// Phase 05/06 wiring resolves references against the `&Store` (via
/// [`resolve_references`]) before calling this, so decode never sees one.
pub(crate) fn decode_value<'f, T: facet::Facet<'f>>(
    frozen: &FrozenValue,
    ty: &Type,
) -> Result<T, ConvertError> {
    let partial = facet::Partial::alloc::<T>().map_err(|_| ConvertError::ShapeMismatch {
        path: String::new(),
        expected: ty.name(),
        found: "allocation failed".to_owned(),
    })?;
    let partial = build_into(partial, frozen, ty)?;
    let heap = partial.build().map_err(|_| mismatch(ty, "incomplete value"))?;
    heap.materialize::<T>()
        .map_err(|_| mismatch(ty, "shape mismatch"))
}

fn frozen_kind(frozen: &FrozenValue) -> &'static str {
    match frozen {
        FrozenValue::Inline(_) => "inline",
        FrozenValue::Opaque(_) => "opaque",
        FrozenValue::Reference(_) => "reference",
        FrozenValue::Product(_) => "product",
        FrozenValue::Variant { .. } => "variant",
        FrozenValue::DenseArray(_) => "array",
        FrozenValue::OrderedMap(_) => "map",
        FrozenValue::OrderedSet(_) => "set",
    }
}

fn build_into<'f>(
    mut partial: facet::Partial<'f, true>,
    frozen: &FrozenValue,
    ty: &Type,
) -> Result<facet::Partial<'f, true>, ConvertError> {
    match ty {
        Type::Bool => {
            let FrozenValue::Inline(bytes) = frozen else {
                return Err(mismatch(ty, frozen_kind(frozen)));
            };
            partial
                .set(read_word(bytes) != 0)
                .map_err(|_| mismatch(ty, "set bool"))
        }
        Type::Int => {
            let FrozenValue::Inline(bytes) = frozen else {
                return Err(mismatch(ty, frozen_kind(frozen)));
            };
            partial
                .set(read_word(bytes))
                .map_err(|_| mismatch(ty, "set int"))
        }
        Type::String => {
            let FrozenValue::Opaque(bytes) = frozen else {
                return Err(mismatch(ty, frozen_kind(frozen)));
            };
            let text =
                String::from_utf8(bytes.clone()).map_err(|_| mismatch(ty, "invalid utf-8"))?;
            partial.set(text).map_err(|_| mismatch(ty, "set string"))
        }
        Type::Tuple(elements) => {
            let FrozenValue::Product(fields) = frozen else {
                return Err(mismatch(ty, frozen_kind(frozen)));
            };
            build_product(partial, ty, elements.iter(), fields)
        }
        Type::Record(record) => {
            let FrozenValue::Product(fields) = frozen else {
                return Err(mismatch(ty, frozen_kind(frozen)));
            };
            build_product(partial, ty, record.fields.iter().map(|f| &f.ty), fields)
        }
        Type::Enum(enumeration) => {
            if let Some(inner) = ty.option_inner() {
                return build_option(partial, ty, inner, frozen);
            }
            let FrozenValue::Variant { tag, fields } = frozen else {
                return Err(mismatch(ty, frozen_kind(frozen)));
            };
            let variant = enumeration
                .variants
                .get(*tag as usize)
                .ok_or_else(|| mismatch(ty, "variant index out of range"))?;
            partial = partial
                .select_nth_variant(*tag as usize)
                .map_err(|_| mismatch(ty, "select variant"))?;
            let field_types = payload_field_types(&variant.payload);
            for (index, field_ty) in field_types.into_iter().enumerate() {
                let field_frozen = fields
                    .get(index)
                    .ok_or_else(|| mismatch(ty, "short variant payload"))?;
                partial = partial
                    .begin_nth_field(index)
                    .map_err(|_| mismatch(ty, "variant field"))?;
                partial = build_into(partial, field_frozen, field_ty)?;
                partial = partial.end().map_err(|_| mismatch(ty, "end variant field"))?;
            }
            Ok(partial)
        }
        Type::Array(element) => {
            let FrozenValue::DenseArray(items) = frozen else {
                return Err(mismatch(ty, frozen_kind(frozen)));
            };
            partial = partial.init_list().map_err(|_| mismatch(ty, "init list"))?;
            for item in items {
                partial = partial
                    .begin_list_item()
                    .map_err(|_| mismatch(ty, "list item"))?;
                partial = build_into(partial, item, element)?;
                partial = partial.end().map_err(|_| mismatch(ty, "end list item"))?;
            }
            Ok(partial)
        }
        Type::Map { key, value } => {
            let FrozenValue::OrderedMap(pairs) = frozen else {
                return Err(mismatch(ty, frozen_kind(frozen)));
            };
            partial = partial.init_map().map_err(|_| mismatch(ty, "init map"))?;
            for (frozen_key, frozen_value) in pairs {
                partial = partial.begin_key().map_err(|_| mismatch(ty, "begin key"))?;
                partial = build_into(partial, frozen_key, key)?;
                partial = partial.end().map_err(|_| mismatch(ty, "end key"))?;
                partial = partial
                    .begin_value()
                    .map_err(|_| mismatch(ty, "begin value"))?;
                partial = build_into(partial, frozen_value, value)?;
                partial = partial.end().map_err(|_| mismatch(ty, "end value"))?;
            }
            Ok(partial)
        }
        Type::Set(element) => {
            let FrozenValue::OrderedSet(items) = frozen else {
                return Err(mismatch(ty, frozen_kind(frozen)));
            };
            partial = partial.init_set().map_err(|_| mismatch(ty, "init set"))?;
            for item in items {
                partial = partial
                    .begin_set_item()
                    .map_err(|_| mismatch(ty, "set item"))?;
                partial = build_into(partial, item, element)?;
                partial = partial.end().map_err(|_| mismatch(ty, "end set item"))?;
            }
            Ok(partial)
        }
        _ => Err(mismatch(ty, "unsupported type")),
    }
}

fn build_product<'f, 'a>(
    mut partial: facet::Partial<'f, true>,
    ty: &Type,
    field_types: impl Iterator<Item = &'a Type>,
    fields: &[FrozenValue],
) -> Result<facet::Partial<'f, true>, ConvertError> {
    for (index, field_ty) in field_types.enumerate() {
        let field_frozen = fields
            .get(index)
            .ok_or_else(|| mismatch(ty, "short product"))?;
        partial = partial
            .begin_nth_field(index)
            .map_err(|_| mismatch(ty, "field"))?;
        partial = build_into(partial, field_frozen, field_ty)?;
        partial = partial.end().map_err(|_| mismatch(ty, "end field"))?;
    }
    Ok(partial)
}

fn build_option<'f>(
    mut partial: facet::Partial<'f, true>,
    ty: &Type,
    inner: &Type,
    frozen: &FrozenValue,
) -> Result<facet::Partial<'f, true>, ConvertError> {
    let FrozenValue::Variant { tag, fields } = frozen else {
        return Err(mismatch(ty, frozen_kind(frozen)));
    };
    if *tag == OPTION_SOME_VARIANT {
        let inner_frozen = fields.first().ok_or_else(|| mismatch(ty, "some payload"))?;
        partial = partial.begin_some().map_err(|_| mismatch(ty, "begin some"))?;
        partial = build_into(partial, inner_frozen, inner)?;
        partial.end().map_err(|_| mismatch(ty, "end some"))
    } else {
        partial.set_default().map_err(|_| mismatch(ty, "none"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::identity::SchemaId;

    #[derive(facet::Facet)]
    struct Sample {
        text: String,
        deep: bool,
        count: i64,
    }

    fn sample_type() -> Type {
        Type::Record(crate::vir::RecordType {
            name: "Sample@0000000000000001".into(),
            fields: vec![
                crate::vir::RecordField {
                    name: "text".into(),
                    ty: Type::String,
                },
                crate::vir::RecordField {
                    name: "deep".into(),
                    ty: Type::Bool,
                },
                crate::vir::RecordField {
                    name: "count".into(),
                    ty: Type::Int,
                },
            ],
        })
    }

    fn test_registered_schema(ty: Type) -> RegisteredSchema {
        RegisteredSchema {
            taxon_root: taxon::SchemaId::from_raw(0),
            taxon_schemas: Vec::new(),
            store_schema: semantic_schema_id(&ty),
            vix_type: ty,
        }
    }

    #[test]
    fn record_frames_exactly_like_the_scheduler_would() {
        let ty = sample_type();
        let value = Sample {
            text: "hi".into(),
            deep: true,
            count: 7,
        };
        let encoded = encode_value(facet::Peek::new(&value), &ty).unwrap();
        let text_leaf =
            FramedNode::leaf(semantic_schema_id(&Type::String), b"hi".to_vec());
        let expected = FramedNode::Variant {
            schema: semantic_schema_id(&ty),
            tag: 0,
            fields: vec![
                FramedField {
                    schema: semantic_schema_id(&Type::String),
                    value: FramedValue::Optional(Some(text_leaf.identity())),
                },
                FramedField {
                    schema: semantic_schema_id(&Type::Bool),
                    value: FramedValue::Bytes(1i64.to_le_bytes().to_vec()),
                },
                FramedField {
                    schema: semantic_schema_id(&Type::Int),
                    value: FramedValue::Bytes(7i64.to_le_bytes().to_vec()),
                },
            ],
        };
        assert_eq!(encoded.node.identity(), expected.identity());
        assert!(encoded.resident.is_empty());
        let _ = SchemaId::named("unused");
    }

    #[test]
    fn interning_twice_dedupes() {
        let ty = sample_type();
        let schema = test_registered_schema(ty);
        let mut store = Store::default();
        let v = Sample {
            text: "hi".into(),
            deep: false,
            count: 1,
        };
        let first = intern_rust_value(&v, &schema, &mut store).unwrap();
        let second = intern_rust_value(&v, &schema, &mut store).unwrap();
        assert_eq!(first.identity, second.identity);
        assert!(second.deduped);
    }

    #[test]
    fn round_trip_preserves_value_and_identity() {
        let ty = sample_type();
        let original = Sample {
            text: "round".into(),
            deep: true,
            count: -3,
        };
        let encoded = encode_value(facet::Peek::new(&original), &ty).unwrap();
        let decoded: Sample = decode_value(&encoded.frozen, &ty).unwrap();
        assert_eq!(decoded.text, original.text);
        assert_eq!(decoded.deep, original.deep);
        assert_eq!(decoded.count, original.count);
        let re_encoded = encode_value(facet::Peek::new(&decoded), &ty).unwrap();
        assert_eq!(encoded.node.identity(), re_encoded.node.identity());
    }

    #[derive(facet::Facet, Debug, PartialEq)]
    #[repr(u8)]
    enum Verdict {
        Pass,
        Fail { reason: String },
    }

    #[derive(facet::Facet, Debug, PartialEq)]
    struct Mixed {
        verdict: Verdict,
        scores: Vec<i64>,
        note: Option<String>,
        counts: std::collections::BTreeMap<String, i64>,
    }

    fn mixed_type() -> Type {
        use crate::vir::{EnumType, EnumVariant, RecordField, RecordType, VariantPayload};
        Type::Record(RecordType {
            name: "Mixed@0000000000000002".into(),
            fields: vec![
                RecordField {
                    name: "verdict".into(),
                    ty: Type::Enum(EnumType {
                        name: "Verdict@0000000000000003".into(),
                        variants: vec![
                            EnumVariant {
                                name: "Pass".into(),
                                payload: VariantPayload::Unit,
                            },
                            EnumVariant {
                                name: "Fail".into(),
                                payload: VariantPayload::Record(vec![RecordField {
                                    name: "reason".into(),
                                    ty: Type::String,
                                }]),
                            },
                        ],
                    }),
                },
                RecordField {
                    name: "scores".into(),
                    ty: Type::array(Type::Int),
                },
                RecordField {
                    name: "note".into(),
                    ty: Type::option(Type::String),
                },
                RecordField {
                    name: "counts".into(),
                    ty: Type::map(Type::String, Type::Int),
                },
            ],
        })
    }

    #[test]
    fn round_trips_enums_options_lists_maps() {
        let ty = mixed_type();
        for original in [
            Mixed {
                verdict: Verdict::Fail {
                    reason: "nope".into(),
                },
                scores: vec![3, 1, 2],
                note: Some("hello".into()),
                counts: [("a".to_string(), 1i64), ("b".to_string(), 2i64)]
                    .into_iter()
                    .collect(),
            },
            Mixed {
                verdict: Verdict::Pass,
                scores: vec![],
                note: None,
                counts: std::collections::BTreeMap::new(),
            },
        ] {
            let encoded = encode_value(facet::Peek::new(&original), &ty).unwrap();
            let decoded: Mixed = decode_value(&encoded.frozen, &ty).unwrap();
            assert_eq!(decoded, original);
            let re_encoded = encode_value(facet::Peek::new(&decoded), &ty).unwrap();
            assert_eq!(encoded.node.identity(), re_encoded.node.identity());
        }
    }

    #[test]
    fn option_uses_the_vir_variant_tags() {
        let ty = Type::option(Type::Int);
        let some = encode_value(facet::Peek::new(&Some(5i64)), &ty).unwrap();
        let none = encode_value(facet::Peek::new(&None::<i64>), &ty).unwrap();
        let FrozenValue::Variant { tag: some_tag, .. } = &some.frozen else {
            panic!()
        };
        let FrozenValue::Variant { tag: none_tag, .. } = &none.frozen else {
            panic!()
        };
        assert_eq!(*some_tag, crate::vir::OPTION_SOME_VARIANT);
        assert_ne!(some_tag, none_tag);
    }
}
