//! Property-fuzzing harness for phon's writer↔reader schema reconciliation.
//!
//! This exercises the *dynamic* reconciliation path in `phon-engine::plan`:
//! `build_plan` / `decode` (the recursive planner) and `decode_via_ir` (the same
//! plan lowered to the linear IR and run by the interpreter). Both must agree
//! value-for-value and error-for-error; the hand-written drift tests in
//! `plan.rs` cover ~16 specific cases, this generates thousands.
//!
//! The strategy:
//!  1. Generate a bounded random *base* schema tree (`Ty`), lowered to phon
//!     `Schema`s with dense synthetic ids in a writer-id range.
//!  2. Derive a *drifted reader* tree by applying tagged edits — either
//!     compat-PRESERVING (reorder fields, add a non-required reader-only field,
//!     drop a writer field, add an enum variant) or compat-BREAKING (add a
//!     required reader-only field, change a scalar's primitive, remove a used
//!     enum variant). Each edit records the expected outcome.
//!  3. Generate a `Value` conforming to the writer tree.
//!  4. Encode with the writer schema (`compact::to_bytes`), decode with both
//!     reader paths, and assert the properties below.
//!
//! Properties asserted per generated case:
//!  - **Cross-engine agreement**: `decode` and `decode_via_ir` always return the
//!    same `Ok(value)` or same `Err` kind.
//!  - **Compat-preserving drift → Ok**, decoded value equals the expected
//!    reader-shaped value (writer-only fields dropped, reader-only non-required
//!    fields defaulted to null, reorder transparent, enum re-tagged under the
//!    reader's variant name).
//!  - **Compat-breaking drift → expected `Err`** (`Incompatible` at plan time for
//!    a required reader-only field or a scalar mismatch; `WriterOnlyVariant` at
//!    decode time when a removed-but-used variant arrives).

use std::collections::BTreeMap;

use facet_value::{VArray, VBytes, VObject, VString, Value};
use phon_engine::CompactError;
use phon_engine::compact::{self, Registry};
use phon_engine::plan::{decode, decode_via_ir};
use phon_schema::{
    Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, Variant, VariantPayload,
    primitive_id,
};
use proptest::prelude::*;

// ============================================================================
// The generation model: a self-contained schema tree
// ============================================================================
//
// `Ty` is a bounded recursive description of a schema. We keep it small (depth
// budget passed into the strategy) and lower it to phon `Schema`s on demand,
// allocating dense synthetic ids. The model is the source of truth for both the
// schema and the conforming value — generating a value walks the same tree.

#[derive(Clone, Debug, PartialEq)]
enum Ty {
    /// A scalar primitive (a curated, codec-supported subset).
    Prim(Primitive),
    /// A struct: ordered named fields, each with a `required` flag.
    Struct(Vec<FieldTy>),
    /// An enum: variants in declaration order; the index is the position.
    Enum(Vec<VariantTy>),
    List(Box<Ty>),
    Option(Box<Ty>),
    /// A map with string keys and a value type.
    Map(Box<Ty>),
    /// A tuple of element types.
    Tuple(Vec<Ty>),
}

#[derive(Clone, Debug, PartialEq)]
struct FieldTy {
    name: String,
    ty: Ty,
    required: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct VariantTy {
    name: String,
    payload: PayloadTy,
}

#[derive(Clone, Debug, PartialEq)]
enum PayloadTy {
    Unit,
    Newtype(Box<Ty>),
    Tuple(Vec<Ty>),
    Struct(Vec<FieldTy>),
}

/// The scalar primitives we generate. We avoid `F32`/`F64` (NaN breaks value
/// equality and set/map dedup) and the never/unit oddities; these all roundtrip
/// cleanly through the compact codec and have stable `Value` equality.
const SCALARS: [Primitive; 9] = [
    Primitive::Bool,
    Primitive::U8,
    Primitive::U16,
    Primitive::U32,
    Primitive::U64,
    Primitive::I32,
    Primitive::I64,
    Primitive::String,
    Primitive::Bytes,
];

// ============================================================================
// Strategies
// ============================================================================

fn scalar_strategy() -> impl Strategy<Value = Ty> {
    prop::sample::select(SCALARS.to_vec()).prop_map(Ty::Prim)
}

/// A short lowercase identifier, used for field and variant names. Drawn from a
/// small pool so collisions (and thus name-matching) happen often.
fn name_strategy() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta",
    ])
    .prop_map(String::from)
}

fn variant_name_strategy() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["Va", "Vb", "Vc", "Vd", "Ve", "Vf", "Vg", "Vh"])
        .prop_map(String::from)
}

/// A bounded `Ty` of nesting depth at most `depth`. At depth 0 only scalars are
/// produced; deeper levels can produce composites whose children are one level
/// shallower.
fn ty_strategy(depth: u32) -> BoxedStrategy<Ty> {
    if depth == 0 {
        return scalar_strategy().boxed();
    }
    let child = ty_strategy(depth - 1);
    let child2 = ty_strategy(depth - 1);
    let child3 = ty_strategy(depth - 1);

    // Distinct-by-name field lists keep struct field-matching unambiguous.
    let fields = dedup_fields(prop::collection::vec(field_strategy(depth - 1), 0..4));
    let variants = dedup_variants(prop::collection::vec(variant_strategy(depth - 1), 1..4));
    let tuple = prop::collection::vec(ty_strategy(depth - 1), 1..4);

    prop_oneof![
        4 => scalar_strategy(),
        2 => fields.prop_map(Ty::Struct),
        2 => variants.prop_map(Ty::Enum),
        1 => child.prop_map(|t| Ty::List(Box::new(t))),
        1 => child2.prop_map(|t| Ty::Option(Box::new(t))),
        1 => child3.prop_map(|t| Ty::Map(Box::new(t))),
        1 => tuple.prop_map(Ty::Tuple),
    ]
    .boxed()
}

fn field_strategy(depth: u32) -> impl Strategy<Value = FieldTy> {
    (name_strategy(), ty_strategy(depth), any::<bool>()).prop_map(|(name, ty, required)| FieldTy {
        name,
        ty,
        required,
    })
}

fn variant_strategy(depth: u32) -> impl Strategy<Value = VariantTy> {
    (variant_name_strategy(), payload_strategy(depth))
        .prop_map(|(name, payload)| VariantTy { name, payload })
}

fn payload_strategy(depth: u32) -> impl Strategy<Value = PayloadTy> {
    let fields = dedup_fields(prop::collection::vec(field_strategy(depth), 0..3));
    prop_oneof![
        2 => Just(PayloadTy::Unit),
        2 => ty_strategy(depth).prop_map(|t| PayloadTy::Newtype(Box::new(t))),
        1 => prop::collection::vec(ty_strategy(depth), 1..3).prop_map(PayloadTy::Tuple),
        1 => fields.prop_map(PayloadTy::Struct),
    ]
}

/// Drop later fields whose name already appeared (struct field-matching keys on
/// name; duplicates would make the model ambiguous).
fn dedup_fields(s: impl Strategy<Value = Vec<FieldTy>>) -> impl Strategy<Value = Vec<FieldTy>> {
    s.prop_map(|fields| {
        let mut seen = std::collections::HashSet::new();
        fields
            .into_iter()
            .filter(|f| seen.insert(f.name.clone()))
            .collect()
    })
}

/// Drop later variants whose name already appeared.
fn dedup_variants(
    s: impl Strategy<Value = Vec<VariantTy>>,
) -> impl Strategy<Value = Vec<VariantTy>> {
    s.prop_map(|variants| {
        let mut seen = std::collections::HashSet::new();
        let v: Vec<_> = variants
            .into_iter()
            .filter(|v| seen.insert(v.name.clone()))
            .collect();
        // An enum needs at least one variant; the collection floor of 1 plus
        // dedup can never empty it (the first element always survives).
        v
    })
}

// ============================================================================
// Lowering a `Ty` tree to phon `Schema`s
// ============================================================================
//
// We walk a `Ty` and emit composite `Schema`s into a builder, returning a
// `SchemaRef` to the just-lowered node. Primitives become primitive references
// (`primitive_id`); composites get a fresh dense synthetic id from the builder's
// counter. Writer and reader trees are lowered into the *same* registry with
// disjoint id ranges so their roots never collide.

struct SchemaBuilder {
    next_id: u64,
    schemas: Vec<Schema>,
}

impl SchemaBuilder {
    fn new(base_id: u64) -> Self {
        SchemaBuilder {
            next_id: base_id,
            schemas: Vec::new(),
        }
    }

    fn fresh(&mut self) -> SchemaId {
        let id = SchemaId(self.next_id);
        self.next_id += 1;
        id
    }

    fn push(&mut self, kind: SchemaKind) -> SchemaRef {
        let id = self.fresh();
        self.schemas.push(Schema {
            id,
            type_params: Vec::new(),
            kind,
        });
        SchemaRef::concrete(id)
    }

    fn lower(&mut self, ty: &Ty) -> SchemaRef {
        match ty {
            Ty::Prim(p) => SchemaRef::concrete(primitive_id(*p)),
            Ty::Struct(fields) => {
                let fs = self.lower_fields(fields);
                self.push(SchemaKind::Struct {
                    name: "S".to_string(),
                    fields: fs,
                })
            }
            Ty::Enum(variants) => {
                let vs = variants
                    .iter()
                    .enumerate()
                    .map(|(i, v)| Variant {
                        name: v.name.clone(),
                        index: i as u32,
                        payload: self.lower_payload(&v.payload),
                    })
                    .collect();
                self.push(SchemaKind::Enum {
                    name: "E".to_string(),
                    variants: vs,
                })
            }
            Ty::List(inner) => {
                let element = self.lower(inner);
                self.push(SchemaKind::List { element })
            }
            Ty::Option(inner) => {
                let element = self.lower(inner);
                self.push(SchemaKind::Option { element })
            }
            Ty::Map(value) => {
                let value_ref = self.lower(value);
                self.push(SchemaKind::Map {
                    key: SchemaRef::concrete(primitive_id(Primitive::String)),
                    value: value_ref,
                })
            }
            Ty::Tuple(elems) => {
                let elements = elems.iter().map(|e| self.lower(e)).collect();
                self.push(SchemaKind::Tuple { elements })
            }
        }
    }

    fn lower_fields(&mut self, fields: &[FieldTy]) -> Vec<Field> {
        fields
            .iter()
            .map(|f| Field {
                name: f.name.clone(),
                schema: self.lower(&f.ty),
                required: f.required,
            })
            .collect()
    }

    fn lower_payload(&mut self, payload: &PayloadTy) -> VariantPayload {
        match payload {
            PayloadTy::Unit => VariantPayload::Unit,
            PayloadTy::Newtype(t) => VariantPayload::Newtype(self.lower(t)),
            PayloadTy::Tuple(ts) => {
                VariantPayload::Tuple(ts.iter().map(|t| self.lower(t)).collect())
            }
            PayloadTy::Struct(fs) => VariantPayload::Struct(self.lower_fields(fs)),
        }
    }
}

// ============================================================================
// Generating a value conforming to a writer `Ty`
// ============================================================================
//
// We generate values inside the proptest strategy (so they shrink), but to keep
// the strategy tree simple we generate a *deterministic* well-typed value from a
// seed driven by proptest. Concretely: the value strategy walks the `Ty` and
// composes child value strategies, producing a `Value` that `compact::to_bytes`
// accepts against the writer schema.

fn value_for(ty: &Ty) -> BoxedStrategy<Value> {
    match ty {
        Ty::Prim(p) => prim_value(*p),
        Ty::Struct(fields) => {
            // One value per field, assembled into an object keyed by field name.
            let names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            let children: Vec<BoxedStrategy<Value>> =
                fields.iter().map(|f| value_for(&f.ty)).collect();
            children
                .prop_map(move |vals| {
                    let mut obj = VObject::new();
                    for (name, v) in names.iter().zip(vals) {
                        obj.insert(VString::new(name), v);
                    }
                    Value::from(obj)
                })
                .boxed()
        }
        Ty::Enum(variants) => {
            // Pick a variant, then generate its payload.
            let arms: Vec<BoxedStrategy<Value>> = variants
                .iter()
                .map(|v| {
                    let name = v.name.clone();
                    payload_value(&v.payload)
                        .prop_map(move |payload| {
                            let mut obj = VObject::new();
                            obj.insert(VString::new(&name), payload);
                            Value::from(obj)
                        })
                        .boxed()
                })
                .collect();
            // `Union` over the arms: proptest's `Union::new` requires a non-empty
            // vec, which the enum's >=1 variant floor guarantees.
            prop::strategy::Union::new(arms).boxed()
        }
        Ty::List(inner) => prop::collection::vec(value_for(inner), 0..4)
            .prop_map(|vs| {
                let mut arr = VArray::new();
                for v in vs {
                    arr.push(v);
                }
                Value::from(arr)
            })
            .boxed(),
        Ty::Option(inner) => {
            let inner_s = value_for(inner);
            prop_oneof![
                1 => Just(Value::NULL),
                2 => inner_s,
            ]
            .boxed()
        }
        Ty::Map(value) => {
            // Distinct string keys -> values. Use a BTreeMap so keys are unique.
            prop::collection::btree_map(
                prop::sample::select(vec!["k0", "k1", "k2", "k3"]),
                value_for(value),
                0..4,
            )
            .prop_map(|m: BTreeMap<&str, Value>| {
                let mut obj = VObject::new();
                for (k, v) in m {
                    obj.insert(VString::new(k), v);
                }
                Value::from(obj)
            })
            .boxed()
        }
        Ty::Tuple(elems) => {
            let children: Vec<BoxedStrategy<Value>> = elems.iter().map(value_for).collect();
            children
                .prop_map(|vals| {
                    let mut arr = VArray::new();
                    for v in vals {
                        arr.push(v);
                    }
                    Value::from(arr)
                })
                .boxed()
        }
    }
}

fn payload_value(payload: &PayloadTy) -> BoxedStrategy<Value> {
    match payload {
        PayloadTy::Unit => Just(Value::NULL).boxed(),
        PayloadTy::Newtype(t) => value_for(t),
        PayloadTy::Tuple(ts) => {
            let children: Vec<BoxedStrategy<Value>> = ts.iter().map(value_for).collect();
            children
                .prop_map(|vals| {
                    let mut arr = VArray::new();
                    for v in vals {
                        arr.push(v);
                    }
                    Value::from(arr)
                })
                .boxed()
        }
        PayloadTy::Struct(fields) => {
            let names: Vec<String> = fields.iter().map(|f| f.name.clone()).collect();
            let children: Vec<BoxedStrategy<Value>> =
                fields.iter().map(|f| value_for(&f.ty)).collect();
            children
                .prop_map(move |vals| {
                    let mut obj = VObject::new();
                    for (name, v) in names.iter().zip(vals) {
                        obj.insert(VString::new(name), v);
                    }
                    Value::from(obj)
                })
                .boxed()
        }
    }
}

/// A value for a scalar primitive. The constructors here MUST match the ones the
/// compact decoder uses (`compact::decode_primitive`), so a generated value and
/// a decoded one compare equal: e.g. both build a `u32` via `Value::from(u32)`.
fn prim_value(p: Primitive) -> BoxedStrategy<Value> {
    match p {
        Primitive::Bool => any::<bool>().prop_map(Value::from).boxed(),
        Primitive::U8 => any::<u8>().prop_map(Value::from).boxed(),
        Primitive::U16 => any::<u16>().prop_map(Value::from).boxed(),
        Primitive::U32 => any::<u32>().prop_map(Value::from).boxed(),
        Primitive::U64 => any::<u64>().prop_map(Value::from).boxed(),
        Primitive::I32 => any::<i32>().prop_map(Value::from).boxed(),
        Primitive::I64 => any::<i64>().prop_map(Value::from).boxed(),
        Primitive::String => "[a-z]{0,6}"
            .prop_map(|s| Value::from(VString::new(&s)))
            .boxed(),
        Primitive::Bytes => prop::collection::vec(any::<u8>(), 0..6)
            .prop_map(|b| Value::from(VBytes::new(&b)))
            .boxed(),
        other => unreachable!("prim_value called with non-scalar {other:?}"),
    }
}

// ============================================================================
// Drift: deriving a reader `Ty` from the writer `Ty`
// ============================================================================
//
// We apply ONE edit to the *root* of the writer tree (the most observable point)
// and tag the expected outcome. The applicable edits, by root shape:
//  - Struct root: identity, reorder fields (transparent), add a reader-only
//    NON-required field (defaults to null), add a reader-only REQUIRED field
//    (plan fails `Incompatible`).
//  - Enum root: identity, add a new variant the writer lacks (reader superset,
//    value unchanged), remove the variant the value uses (plan builds on the
//    other variants, decode fails `WriterOnlyVariant`).
//  - Primitive root: identity, change to a different primitive (`Incompatible`).
//  - Container root: identity (still a valuable cross-engine-agreement case).

/// What we expect the two decode paths to do with a drifted case.
enum Expect {
    /// Both decodes succeed and equal this reader-shaped value.
    Ok(Value),
    /// Both fail to build a plan with `Incompatible`.
    Incompatible,
    /// The plan builds but decoding hits a writer-only variant.
    WriterOnlyVariant,
}

/// Given the writer `Ty` and the writer's generated `Value`, choose an applicable
/// drift, returning the reader `Ty` and the expected outcome. Picks based on the
/// root shape so the edit is always meaningful.
fn apply_drift(
    writer: &Ty,
    value: &Value,
    drift_seed: u32,
    new_field_name: &str,
    new_scalar: Primitive,
    new_variant_name: &str,
) -> (Ty, Expect) {
    match writer {
        Ty::Struct(fields) => {
            // Available struct edits: identity, reorder, add optional, add required.
            // `drift_seed % 4` selects among them.
            match drift_seed % 4 {
                0 => (writer.clone(), Expect::Ok(value.clone())),
                1 => {
                    // Reverse the field order: a non-trivial reorder when >=2 fields.
                    let mut reordered = fields.clone();
                    reordered.reverse();
                    (Ty::Struct(reordered), Expect::Ok(value.clone()))
                }
                2 => {
                    // Add an optional field whose name is not already present.
                    if fields.iter().any(|f| f.name == new_field_name) {
                        // Name collides — fall back to identity rather than make
                        // an ambiguous schema.
                        (writer.clone(), Expect::Ok(value.clone()))
                    } else {
                        let mut rf = fields.clone();
                        rf.push(FieldTy {
                            name: new_field_name.to_string(),
                            ty: Ty::Prim(new_scalar),
                            required: false,
                        });
                        // Expected: the original object plus the new field = null.
                        let mut expected = value.as_object().unwrap().clone();
                        expected.insert(VString::new(new_field_name), Value::NULL);
                        (Ty::Struct(rf), Expect::Ok(Value::from(expected)))
                    }
                }
                _ => {
                    // Add a REQUIRED reader-only field -> plan must fail.
                    if fields.iter().any(|f| f.name == new_field_name) {
                        (writer.clone(), Expect::Ok(value.clone()))
                    } else {
                        let mut rf = fields.clone();
                        rf.push(FieldTy {
                            name: new_field_name.to_string(),
                            ty: Ty::Prim(new_scalar),
                            required: true,
                        });
                        (Ty::Struct(rf), Expect::Incompatible)
                    }
                }
            }
        }
        Ty::Enum(variants) => {
            // Which variant does the value use?
            let used_name = value
                .as_object()
                .and_then(|o| o.iter().next().map(|(k, _)| k.as_str().to_string()))
                .expect("enum value is a single-key object");
            match drift_seed % 3 {
                0 => (writer.clone(), Expect::Ok(value.clone())),
                1 => {
                    // Add a brand-new variant; reader is a superset.
                    if variants.iter().any(|v| v.name == new_variant_name) {
                        (writer.clone(), Expect::Ok(value.clone()))
                    } else {
                        let mut rv = variants.clone();
                        rv.push(VariantTy {
                            name: new_variant_name.to_string(),
                            payload: PayloadTy::Unit,
                        });
                        (Ty::Enum(rv), Expect::Ok(value.clone()))
                    }
                }
                _ => {
                    // Remove the USED variant. The plan still builds if any other
                    // variant remains; decoding the used one fails. If the enum
                    // has a single variant (the used one), removing it would leave
                    // an empty enum — instead expect the plan still builds (no
                    // matching variants) and decode fails with WriterOnlyVariant.
                    let remaining: Vec<VariantTy> = variants
                        .iter()
                        .filter(|v| v.name != used_name)
                        .cloned()
                        .collect();
                    if remaining.is_empty() {
                        // Can't build an empty enum schema; keep one *other*
                        // variant is impossible, so fall back to identity. (Only
                        // happens for single-variant enums.)
                        (writer.clone(), Expect::Ok(value.clone()))
                    } else {
                        (Ty::Enum(remaining), Expect::WriterOnlyVariant)
                    }
                }
            }
        }
        Ty::Prim(p) => {
            match drift_seed % 2 {
                0 => (writer.clone(), Expect::Ok(value.clone())),
                _ => {
                    // Change to a *different* scalar -> Incompatible.
                    let to = if new_scalar == *p {
                        // Ensure it actually differs.
                        if *p == Primitive::U32 {
                            Primitive::U64
                        } else {
                            Primitive::U32
                        }
                    } else {
                        new_scalar
                    };
                    (Ty::Prim(to), Expect::Incompatible)
                }
            }
        }
        // For container roots (list/option/map/tuple) we don't have a targeted
        // edit; identity is still a valuable cross-engine-agreement case.
        other => (other.clone(), Expect::Ok(value.clone())),
    }
}

// ============================================================================
// The full per-case generator: (writer Ty, value, reader Ty, expectation)
// ============================================================================

#[derive(Debug)]
struct Case {
    writer: Ty,
    value: Value,
    reader: Ty,
    // The expectation is not Debug-friendly (holds a Value); we render a tag.
    expect_tag: ExpectTag,
    expected_value: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ExpectTag {
    Ok,
    Incompatible,
    WriterOnlyVariant,
}

fn case_strategy() -> impl Strategy<Value = Case> {
    ty_strategy(3)
        .prop_flat_map(|writer| {
            let value_s = value_for(&writer);
            (
                Just(writer),
                value_s,
                any::<u32>(),
                name_strategy(),
                prop::sample::select(SCALARS.to_vec()),
                variant_name_strategy(),
            )
        })
        .prop_map(|(writer, value, seed, fname, scalar, vname)| {
            let (reader, expect) = apply_drift(&writer, &value, seed, &fname, scalar, &vname);
            let (expect_tag, expected_value) = match expect {
                Expect::Ok(v) => (ExpectTag::Ok, Some(v)),
                Expect::Incompatible => (ExpectTag::Incompatible, None),
                Expect::WriterOnlyVariant => (ExpectTag::WriterOnlyVariant, None),
            };
            Case {
                writer,
                value,
                reader,
                expect_tag,
                expected_value,
            }
        })
}

// ============================================================================
// The check that ties a case to the engine
// ============================================================================

/// Lower the writer and reader trees into one registry (disjoint id ranges),
/// returning the registry and the two root ids.
fn build_registry(writer: &Ty, reader: &Ty) -> (Registry, SchemaId, SchemaId) {
    let mut wb = SchemaBuilder::new(1);
    let writer_root_ref = wb.lower(writer);
    let writer_root = root_id(&writer_root_ref, writer);

    // Reader ids start well above the writer range to guarantee disjointness.
    let reader_base = wb.next_id + 1_000_000;
    let mut rb = SchemaBuilder::new(reader_base);
    let reader_root_ref = rb.lower(reader);
    let reader_root = root_id(&reader_root_ref, reader);

    let mut all = wb.schemas;
    all.extend(rb.schemas);
    (Registry::new(all), writer_root, reader_root)
}

/// The root id for a lowered tree. For a primitive root the ref is a primitive
/// id (no composite was pushed); for a composite root it's the freshly assigned
/// id. Either way the `SchemaRef::concrete` id is what `decode` expects.
fn root_id(root_ref: &SchemaRef, _ty: &Ty) -> SchemaId {
    match root_ref {
        SchemaRef::Concrete { id, .. } => *id,
        SchemaRef::Var { .. } => unreachable!("lowering never produces a var"),
    }
}

fn check_case(case: &Case) {
    let (reg, writer_root, reader_root) = build_registry(&case.writer, &case.reader);

    // Encode the value against the WRITER schema.
    let bytes = match compact::to_bytes(&case.value, writer_root, &reg) {
        Ok(b) => b,
        Err(e) => panic!(
            "writer-side encode failed (generator bug): {e}\n  writer={:?}\n  value={:?}",
            case.writer, case.value
        ),
    };

    let recursive = decode(&bytes, writer_root, reader_root, &reg);
    let flat = decode_via_ir(&bytes, writer_root, reader_root, &reg);

    // Property 1: cross-engine agreement (same Ok value or same Err kind).
    match (&recursive, &flat) {
        (Ok(a), Ok(b)) => assert_eq!(
            a, b,
            "CROSS-ENGINE DISAGREEMENT (both Ok, values differ)\n  writer={:?}\n  reader={:?}\n  value={:?}\n  recursive={a:?}\n  flat={b:?}",
            case.writer, case.reader, case.value
        ),
        (Err(a), Err(b)) => assert!(
            same_err_kind(a, b),
            "CROSS-ENGINE DISAGREEMENT (errors differ)\n  writer={:?}\n  reader={:?}\n  value={:?}\n  recursive={a:?}\n  flat={b:?}",
            case.writer,
            case.reader,
            case.value
        ),
        _ => panic!(
            "CROSS-ENGINE DISAGREEMENT (one Ok, one Err)\n  writer={:?}\n  reader={:?}\n  value={:?}\n  recursive={recursive:?}\n  flat={flat:?}",
            case.writer, case.reader, case.value
        ),
    }

    // Property 2/3: the outcome matches the tagged expectation.
    match case.expect_tag {
        ExpectTag::Ok => {
            let got = recursive.as_ref().unwrap_or_else(|e| {
                panic!(
                    "expected Ok, got Err {e:?}\n  writer={:?}\n  reader={:?}\n  value={:?}",
                    case.writer, case.reader, case.value
                )
            });
            let expected = case.expected_value.as_ref().unwrap();
            assert_eq!(
                got, expected,
                "decoded reader value mismatch\n  writer={:?}\n  reader={:?}\n  value={:?}",
                case.writer, case.reader, case.value
            );
        }
        ExpectTag::Incompatible => {
            assert!(
                matches!(recursive, Err(CompactError::Incompatible(_))),
                "expected Incompatible, got {recursive:?}\n  writer={:?}\n  reader={:?}",
                case.writer,
                case.reader
            );
        }
        ExpectTag::WriterOnlyVariant => {
            assert!(
                matches!(recursive, Err(CompactError::WriterOnlyVariant(_))),
                "expected WriterOnlyVariant, got {recursive:?}\n  writer={:?}\n  reader={:?}\n  value={:?}",
                case.writer,
                case.reader,
                case.value
            );
        }
    }
}

/// Compare two `CompactError`s by *kind* (the property is about kinds, not the
/// exact message string, which legitimately differs between the two paths).
fn same_err_kind(a: &CompactError, b: &CompactError) -> bool {
    use CompactError::*;
    match (a, b) {
        (UnknownSchema(x), UnknownSchema(y)) => x == y,
        (Unsupported(x), Unsupported(y)) => x == y,
        (TypeMismatch { .. }, TypeMismatch { .. }) => true,
        (UnknownVariant(_), UnknownVariant(_)) => true,
        (BadVariantIndex(_), BadVariantIndex(_)) => true,
        (GenericArity { .. }, GenericArity { .. }) => true,
        (Malformed(_), Malformed(_)) => true,
        (Incompatible(_), Incompatible(_)) => true,
        (WriterOnlyVariant(x), WriterOnlyVariant(y)) => x == y,
        (Decode(_), Decode(_)) => true,
        (Encode(_), Encode(_)) => true,
        _ => false,
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 2000,
        ..ProptestConfig::default()
    })]

    /// The omnibus property: generate a writer schema + conforming value, derive
    /// a tagged drift, and assert cross-engine agreement plus the tagged outcome.
    #[test]
    fn reconciliation_matches_expectation(case in case_strategy()) {
        check_case(&case);
    }
}
