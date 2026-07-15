# Vix Typed Primitives — Phase 02: Core Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Build `vix::runtime::primitive` — descriptor/identity, the object-safe `Primitive` trait with `EffectCtx`/tickets/completions, the taxon→vir type bridge, facet↔store value conversion, and the `PrimitiveSet::register_function::<Resp, Req>` typed adapter — fully unit-tested, with zero compiler/scheduler wiring (that is phases 03–05).

**Architecture:** New module `vix/src/runtime/primitive/` (six focused files) per the spec at `docs/superpowers/specs/2026-07-15-vix-typed-primitives-design.md`. Registration derives taxon schemas from facet shapes (`phon::derive::of_shape`), validates them into `vir::Type` (lossless subset only), and produces a content-derived `PrimitiveId`. Values convert structurally: Rust→`FramedNode`+`FrozenValue` trees mirroring `realize_structural_node`'s framing exactly, so ValueIds agree with vix-constructed values.

**Tech Stack:** Rust (edition 2024), facet reflection (`facet::Peek`/`facet::Partial`, re-exported from facet-reflect via `facet/src/lib.rs:96`), phon derive bridge, taxon, blake3 via existing `runtime::identity` helpers.

## Global Constraints

- Branch: `vix-prim-02-core`, created with `git town append vix-prim-02-core` from `vix-prim-01-spec`. All commits `git commit --no-verify` (owner instruction: the facet-dev hook is skipped).
- Zero changes outside `vix/src/runtime/` — and inside it, only the new `primitive/` module plus the two mechanical `semantic_schema_id` hoist edits named in Task 1.
- No `Result<_, String>` anywhere (`machine.error.typed`). No `RefCell` closure smuggling (`machine.abi.host-env-type`). No private result caches (`machine.cache.no-private-caches`).
- No per-primitive match arms/fields/variants anywhere (`machine.primitive.registered`) — everything keyed by descriptor data.
- Test runner: `cargo nextest run -p vix` (run from repo root; use `nix develop --command` if the toolchain isn't on PATH).
- Value framing MUST mirror `vix/src/runtime/scheduler.rs` `realize_structural_node`/`realize_structural_fields` (lines ~2228–2400): scalars are 8-byte LE leafs; tuples/records are `FramedNode::Variant { tag: 0 }`; enum tag = variant index; scalar fields inline as `FramedValue::Bytes`, all other fields as `FramedValue::Optional(Some(child.identity()))`; aggregates intern with EMPTY resident bytes and carry a `FrozenValue` replay tree via `attach_frozen`.

## File Structure

- `vix/src/runtime/primitive/mod.rs` — module root, re-exports, `PrimitiveSet`.
- `vix/src/runtime/primitive/descriptor.rs` — `PrimitiveId`, `PrimitiveName`, `MemoPolicy`, `RegisteredSchema`, `PrimitiveDescriptor`, `RegistrationError`.
- `vix/src/runtime/primitive/bridge.rs` — taxon→vir mapping + validation.
- `vix/src/runtime/primitive/convert.rs` — Rust value ↔ store value conversion.
- `vix/src/runtime/primitive/traits.rs` — `Primitive`, `EffectCtx`, `EffectTicket`, `Completion`, `PrimitiveFailure`.
- `vix/src/runtime/primitive/register.rs` — `register_function` typed adapter internals.
- Modify: `vix/src/runtime/mod.rs` (add `pub mod primitive;`), `vix/src/runtime/identity.rs` (hoisted `semantic_schema_id`), `vix/src/runtime/scheduler.rs` + `vix/src/lowering.rs` (delete their private `semantic_schema_id` copies, import the shared one).

---

### Task 1: Hoist `semantic_schema_id` + descriptor types and identity

**Files:**
- Modify: `vix/src/runtime/identity.rs` (append fn), `vix/src/runtime/scheduler.rs:~2761`, `vix/src/lowering.rs:~655` (remove local copies, import), `vix/src/runtime/mod.rs` (add `pub mod primitive;`)
- Create: `vix/src/runtime/primitive/mod.rs`, `vix/src/runtime/primitive/descriptor.rs`
- Test: unit tests inside `descriptor.rs`

**Interfaces:**
- Consumes: `crate::runtime::identity::{SchemaId, Digest, hash_framed}` (check `hash_framed`'s exact signature in identity.rs and match it), `crate::vir::Type`, `taxon::{Schema, SchemaId as TaxonSchemaId}`.
- Produces (later tasks rely on these exact names):
  - `pub(crate) fn semantic_schema_id(ty: &crate::vir::Type) -> SchemaId` in `identity.rs`
  - `pub struct PrimitiveName(String)` with `pub fn new(&str) -> Result<Self, RegistrationError>` (rules: non-empty, `[a-z_][a-z0-9_]*`, not in `RESERVED_NAMES`)
  - `pub const RESERVED_NAMES: &[&str] = &["None", "Some", "by_key", "range", "expect", "expect_eq", "expect_ne", "expect_some", "expect_none", "expect_snapshot", "demanded", "json_decode", "toml_decode", "scheduler_requests_at_most", "memo_entries_at_most", "store_interns_at_most"]`
  - `#[derive(Clone, Copy, PartialEq, Eq, ...)] pub struct PrimitiveId(pub Digest)`
  - `#[derive(Clone, Copy, ...)] pub enum MemoPolicy { Hermetic, Pinned, Observed, Volatile }`
  - `pub struct RegisteredSchema { pub taxon_root: TaxonSchemaId, pub taxon_schemas: Vec<taxon::Schema>, pub vix_type: crate::vir::Type, pub store_schema: SchemaId }`
  - `pub struct PrimitiveDescriptor { pub id: PrimitiveId, pub name: PrimitiveName, pub version: u32, pub protocol: u32, pub request: RegisteredSchema, pub response: RegisteredSchema, pub policy: MemoPolicy, pub capabilities: Vec<CapabilityRequirement> }` with `pub struct CapabilityRequirement { pub identity: String }` (placeholder shape, empty in v1, present per spec)
  - `pub enum RegistrationError { InvalidName { name: String }, ReservedName { name: String }, DuplicateName { name: String }, UnsupportedShape { path: String, kind: String }, Derive { message: String } }` (implement `Display` + `std::error::Error`)
  - `impl PrimitiveId { pub fn derive(name: &PrimitiveName, version: u32, protocol: u32, request: TaxonSchemaId, response: TaxonSchemaId) -> Self }` — `hash_framed(b"vix.primitive.v1", &[name.as_str().as_bytes(), &version.to_le_bytes(), &protocol.to_le_bytes(), &request.as_u64().to_le_bytes(), &response.as_u64().to_le_bytes()])`

- [x] **Step 1: Write failing tests** in `descriptor.rs` `#[cfg(test)]`:

```rust
#[test]
fn primitive_id_rekeys_on_every_descriptor_axis() {
    let name = PrimitiveName::new("probe_version").unwrap();
    let other = PrimitiveName::new("probe_other").unwrap();
    let req = taxon::SchemaId::from_raw(11);
    let resp = taxon::SchemaId::from_raw(22);
    let base = PrimitiveId::derive(&name, 1, 1, req, resp);
    assert_eq!(base, PrimitiveId::derive(&name, 1, 1, req, resp));
    assert_ne!(base, PrimitiveId::derive(&other, 1, 1, req, resp));
    assert_ne!(base, PrimitiveId::derive(&name, 2, 1, req, resp));
    assert_ne!(base, PrimitiveId::derive(&name, 1, 2, req, resp));
    assert_ne!(base, PrimitiveId::derive(&name, 1, 1, taxon::SchemaId::from_raw(12), resp));
    assert_ne!(base, PrimitiveId::derive(&name, 1, 1, req, taxon::SchemaId::from_raw(23)));
}

#[test]
fn names_are_validated() {
    assert!(PrimitiveName::new("probe_version").is_ok());
    assert!(matches!(PrimitiveName::new(""), Err(RegistrationError::InvalidName { .. })));
    assert!(matches!(PrimitiveName::new("9lives"), Err(RegistrationError::InvalidName { .. })));
    assert!(matches!(PrimitiveName::new("Probe"), Err(RegistrationError::InvalidName { .. })));
    assert!(matches!(PrimitiveName::new("range"), Err(RegistrationError::ReservedName { .. })));
}

#[test]
fn semantic_schema_id_matches_scheduler_format() {
    // Pins the format the scheduler/lowering used before the hoist. If this
    // breaks, value identities across the runtime change — do not "fix" the
    // test; investigate.
    let ty = crate::vir::Type::Int;
    assert_eq!(
        crate::runtime::identity::semantic_schema_id(&ty),
        crate::runtime::identity::SchemaId::named(&format!("vix.semantic.v1:{}", ty.name())),
    );
}
```

- [x] **Step 2: Run to verify failure** — `cargo nextest run -p vix primitive` → compile error (module missing). Expected.
- [x] **Step 3: Implement.** Move `fn semantic_schema_id` bodies: cut the private fn from `scheduler.rs` (~2761) and `lowering.rs` (~655), add to `identity.rs`:

```rust
/// The runtime store schema for a vir type: blake3 of the type's canonical
/// name under the semantic domain. One definition — scheduler, lowering, and
/// primitive registration must agree byte-for-byte.
pub(crate) fn semantic_schema_id(ty: &crate::vir::Type) -> SchemaId {
    SchemaId::named(&format!("vix.semantic.v1:{}", ty.name()))
}
```

Update the two former definition sites to `use super::identity::semantic_schema_id;` / `use crate::runtime::identity::semantic_schema_id;` (match each file's existing import style). Then write `descriptor.rs` with the exact interfaces above and `mod.rs`:

```rust
//! Registered Rust effect primitives (r[machine.primitive.trait] and family).
mod descriptor;
pub use descriptor::*;
```

Add `pub mod primitive;` to `runtime/mod.rs`.
- [x] **Step 4: Run** `cargo nextest run -p vix` (full crate — the hoist touches scheduler/lowering; everything must stay green). Expected: PASS.
- [x] **Step 5: Commit** — `git add -A && git commit --no-verify -m "vix: primitive descriptor identity + shared semantic_schema_id"`

---

### Task 2: taxon→vir bridge with lossless-subset validation

**Files:**
- Create: `vix/src/runtime/primitive/bridge.rs` (+ `mod bridge; pub use bridge::*;` in mod.rs)
- Test: unit tests inside `bridge.rs`

**Interfaces:**
- Consumes: `taxon::{Schema, SchemaId, SchemaRef, Kind, Primitive, VariantPayload as TaxonVariantPayload, Field, Variant}`; `crate::vir::{Type, RecordType, RecordField, EnumType, EnumVariant, VariantPayload}`.
- Produces: `pub fn vir_type_for(root: taxon::SchemaId, schemas: &[taxon::Schema]) -> Result<crate::vir::Type, RegistrationError>`.
- Naming rule (identity-bearing, do not vary): a taxon Struct/Enum named `Foo` with taxon id `id` becomes vir `RecordType`/`EnumType` with `name = format!("{Foo}@{:016x}", id.as_u64())`. `@` cannot appear in source-authored type names, so registered types can never collide with user types.
- Mapping table (verified against `phon/rust/taxon/src/lib.rs:76-325` and `vix/src/vir.rs:39-102`; everything not listed → `RegistrationError::UnsupportedShape { path, kind }` where `path` is a dotted field path like `"ProbeRequest.deep.0"` and `kind` names the taxon kind, e.g. `"F64"`):
  - `Kind::Primitive`: `Bool→Type::Bool`, `I64→Type::Int`, `String→Type::String`, `Unit→Type::Tuple(vec![])`. The other 17 primitives (`U8..U128`, `I8..I32`, `I128`, `F32`, `F64`, `Char`, `Bytes`, `DateTime`, `Uuid`, `QName`, `Never`) are rejected.
  - `Kind::Struct { name, fields }→Type::Record` — each `taxon::Field { name, schema, required }`; `required: false` is rejected (kind `"non-required field"`; optionality is expressed as `Kind::Option`, not field flags).
  - `Kind::Enum { name, variants }→Type::Enum` — payloads: taxon `VariantPayload::Unit→vir VariantPayload::Unit`, `Newtype(r)→vir Tuple(vec![t])`, `Tuple(refs)→vir Tuple`, `Struct(fields)→vir Record` (vir has all three, `vir.rs:41`). Variants must be dense and positional: reject when `variants[i].index != i as u32` (kind `"sparse variant indices"`; the runtime frames tags positionally).
  - `Kind::Tuple→Type::Tuple`, `Kind::List→Type::Array` (Vec = vix array), `Kind::Set→Type::Set`, `Kind::Map→Type::Map`, `Kind::Option { element }→Type::option(inner)`.
  - Explicitly rejected kinds: `Kind::Array` (fixed-size, kind `"fixed-size array"`), `Kind::Tensor`, `Kind::Channel`, `Kind::Dynamic`, `Kind::External`.
- Recursion through `SchemaRef::Concrete { id, args }` resolves `id` in `schemas`; non-empty `args` and `SchemaRef::Var` are rejected (kind `"generic"`), recursive cycles rejected (kind `"recursive"`; track a visiting `BTreeSet<taxon::SchemaId>`).

- [x] **Step 1: Write failing tests** (build small `taxon::Schema` batches by hand in the tests — see `phon/rust/taxon/src/lib.rs:64-176` for constructors; ids can be `SchemaId::from_raw(n)` since the bridge only resolves references, it never re-derives ids):

```rust
#[test]
fn maps_the_supported_subset() {
    // Struct { text: String, deep: Bool, count: I64, tags: List<String>,
    //          extra: Option<String> }  → Record with matching field types.
    let (root, schemas) = struct_fixture(); // helper built in this test module
    let ty = vir_type_for(root, &schemas).unwrap();
    let crate::vir::Type::Record(record) = ty else { panic!("expected record") };
    assert!(record.name.starts_with("ProbeRequest@"));
    assert_eq!(record.fields.len(), 5);
    assert_eq!(record.fields[0].ty, crate::vir::Type::String);
    assert_eq!(record.fields[1].ty, crate::vir::Type::Bool);
    assert_eq!(record.fields[2].ty, crate::vir::Type::Int);
    assert_eq!(record.fields[3].ty, crate::vir::Type::array(crate::vir::Type::String));
    assert_eq!(record.fields[4].ty, crate::vir::Type::option(crate::vir::Type::String));
}

#[test]
fn rejects_with_field_path() {
    // Struct { weight: F64 } → UnsupportedShape { path: "Bad.weight", kind: "F64" }
    let (root, schemas) = f64_fixture();
    let err = vir_type_for(root, &schemas).unwrap_err();
    let RegistrationError::UnsupportedShape { path, kind } = err else { panic!() };
    assert_eq!(path, "Bad.weight");
    assert_eq!(kind, "F64");
}

#[test]
fn rejects_every_unsupported_primitive() {
    use taxon::Primitive as P;
    for p in [P::U8, P::U16, P::U32, P::U64, P::U128, P::I8, P::I16, P::I32,
              P::I128, P::F32, P::F64, P::Char, P::Bytes, P::DateTime,
              P::Uuid, P::QName, P::Never] {
        let (root, schemas) = primitive_field_fixture(p);
        assert!(matches!(vir_type_for(root, &schemas),
            Err(RegistrationError::UnsupportedShape { .. })), "{p:?} must be rejected");
    }
}
```

- [x] **Step 2: Run to verify failure** — `cargo nextest run -p vix bridge` → fails (fn missing).
- [x] **Step 3: Implement** `vir_type_for`: index `schemas` by id into a `BTreeMap`, recursive `fn convert(id, ctx: &mut Ctx { by_id, visiting: BTreeSet<TaxonSchemaId>, path: Vec<String> })`. Push field/variant/element names onto `path` as you descend; render `path.join(".")` on error. Names per the naming rule.
- [x] **Step 4: Run** `cargo nextest run -p vix bridge` → PASS.
- [x] **Step 5: Commit** — `git commit --no-verify -am "vix: taxon-to-vir bridge with lossless-subset validation"`

---

### Task 3: Encode — Rust value → interned store value

**Files:**
- Create: `vix/src/runtime/primitive/convert.rs` (+ mod wiring)
- Test: unit tests inside `convert.rs`

**Interfaces:**
- Consumes: `facet::Peek` (walk any `&T where T: Facet`), `crate::runtime::identity::{FramedNode, FramedField, FramedValue, semantic_schema_id}`, `crate::runtime::store::{Store, Interned, FrozenValue}` (`FrozenValue` is `pub(crate)` — this module is inside `runtime`, so accessible), `crate::vir::Type`, `RegisteredSchema`.
- Produces:
  - `pub enum ConvertError { ShapeMismatch { path: String, expected: String, found: String } }` (+Display/Error)
  - `pub(crate) fn encode_value(peek: facet::Peek<'_, '_>, ty: &crate::vir::Type) -> Result<Encoded, ConvertError>` where `pub(crate) struct Encoded { pub node: FramedNode, pub frozen: FrozenValue, pub resident: Vec<u8> }`
  - `pub(crate) fn intern_rust_value<'f, T: facet::Facet<'f>>(value: &T, schema: &RegisteredSchema, store: &mut Store) -> Result<Interned, ConvertError>` — encodes, `store.intern_tree(&node, &resident)`, `store.attach_frozen(handle, frozen)`, returns the `Interned`.
- Framing rules (mirror `realize_structural_node`, scheduler.rs:2228 — parity is the whole point):
  - `Type::Bool`/`Type::Int`: `resident = (value as i64).to_le_bytes().to_vec()`; `node = FramedNode::leaf(semantic_schema_id(ty), resident.clone())`; `frozen = FrozenValue::Inline(resident.clone())`. Bool encodes as 0/1 i64.
  - `Type::String`: `resident = s.as_bytes().to_vec()`; leaf node; `frozen = FrozenValue::Opaque(bytes)`.
  - `Type::Tuple`/`Type::Record`: `node = FramedNode::Variant { schema: semantic_schema_id(ty), tag: 0, fields }`; per field: scalar (`Bool|Int`) → `FramedValue::Bytes(inline)`, else recurse, `FramedValue::Optional(Some(child_node.identity()))`; `frozen = FrozenValue::Product(child_frozen)`; `resident = Vec::new()`.
  - `Type::Enum`: tag = variant index from `peek.into_enum()`; fields from the payload exactly as records; `frozen = FrozenValue::Variant { tag, fields }`; empty resident. `Type::option(_)` is just an enum (Some = 0, None = 1) and needs no special case beyond facet's Option peek → map to the enum path.
  - `Type::Array`: children encoded recursively; `node = FramedNode::SeqChildren { schema, element_schema: semantic_schema_id(element), children: child_ids }`; `frozen = FrozenValue::DenseArray(child_frozen)`; empty resident.
  - `Type::Map`/`Type::Set`: encode entries, sort rows by the KEY's `ValueId` ordering to canonical order (mirror `realize_ordered` — read scheduler.rs `fn realize_ordered` first and copy its ordering rule exactly; if it orders by structural comparison rather than ValueId, do that), `node = FramedNode::OrderedMap { schema, rows } / OrderedSet { schema, elements }`; `frozen = FrozenValue::OrderedMap(pairs) / OrderedSet(items)`.
  - Facet-side walking: `let peek = facet::Peek::new(value);` then match on `ty` (the vir type drives the walk; facet shape was already validated by the bridge, so mismatches are `ShapeMismatch` bugs, not user errors).

- [x] **Step 1: Write failing tests**:

```rust
#[derive(facet::Facet)]
struct Sample {
    text: String,
    deep: bool,
    count: i64,
}

fn sample_type() -> crate::vir::Type {
    crate::vir::Type::Record(crate::vir::RecordType {
        name: "Sample@0000000000000001".into(),
        fields: vec![
            crate::vir::RecordField { name: "text".into(), ty: crate::vir::Type::String },
            crate::vir::RecordField { name: "deep".into(), ty: crate::vir::Type::Bool },
            crate::vir::RecordField { name: "count".into(), ty: crate::vir::Type::Int },
        ],
    })
}

#[test]
fn record_frames_exactly_like_the_scheduler_would() {
    let ty = sample_type();
    let value = Sample { text: "hi".into(), deep: true, count: 7 };
    let encoded = encode_value(facet::Peek::new(&value), &ty).unwrap();
    // Hand-build the FramedNode the scheduler's realize_structural_fields
    // would produce, and require identical identity.
    use crate::runtime::identity::{FramedField, FramedNode, FramedValue, semantic_schema_id};
    let text_leaf = FramedNode::leaf(semantic_schema_id(&crate::vir::Type::String), b"hi".to_vec());
    let expected = FramedNode::Variant {
        schema: semantic_schema_id(&ty),
        tag: 0,
        fields: vec![
            FramedField { schema: semantic_schema_id(&crate::vir::Type::String),
                          value: FramedValue::Optional(Some(text_leaf.identity())) },
            FramedField { schema: semantic_schema_id(&crate::vir::Type::Bool),
                          value: FramedValue::Bytes(1i64.to_le_bytes().to_vec()) },
            FramedField { schema: semantic_schema_id(&crate::vir::Type::Int),
                          value: FramedValue::Bytes(7i64.to_le_bytes().to_vec()) },
        ],
    };
    assert_eq!(encoded.node.identity(), expected.identity());
    assert!(encoded.resident.is_empty());
}

#[test]
fn interning_twice_dedupes() {
    let ty = sample_type();
    let schema = test_registered_schema(ty); // helper: RegisteredSchema w/ this vir type
    let mut store = crate::runtime::store::Store::default();
    let v = Sample { text: "hi".into(), deep: false, count: 1 };
    let first = intern_rust_value(&v, &schema, &mut store).unwrap();
    let second = intern_rust_value(&v, &schema, &mut store).unwrap();
    assert_eq!(first.identity, second.identity);
    assert!(second.deduped);
}
```

- [x] **Step 2: Run to verify failure**, **Step 3: Implement** per the framing rules (read `realize_ordered` before writing the map/set arm and mirror its ordering), **Step 4: Run** `cargo nextest run -p vix convert` → PASS, **Step 5: Commit** `--no-verify -am "vix: encode facet values into framed store values"`.

---

### Task 4: Decode — FrozenValue → Rust value

**Files:**
- Modify: `vix/src/runtime/primitive/convert.rs`
- Test: unit tests inside `convert.rs`

**Interfaces:**
- Consumes: `facet::Partial` (typed builder — study `facet-reflect/src/partial/` and an existing user, e.g. grep `Partial::alloc` in `facet-json`, before writing; use the real API), `FrozenValue`.
- Produces: `pub(crate) fn decode_value<'f, T: facet::Facet<'f>>(frozen: &FrozenValue, ty: &crate::vir::Type) -> Result<T, ConvertError>`.
- Rules are the encode rules inverted. `FrozenValue::Reference(_)` is a `ShapeMismatch` in v1 (decode input for a primitive request is always a fully-frozen tree; references appear only for store-resident strings — resolve those in phase 05 where a `&Store` is in hand; v1 decode takes the frozen tree only and the phase-05 wiring passes trees with references pre-resolved. Record this as a doc comment on `decode_value`).

- [x] **Step 1: Write failing round-trip tests**:

```rust
#[test]
fn round_trip_preserves_value_and_identity() {
    let ty = sample_type();
    let original = Sample { text: "round".into(), deep: true, count: -3 };
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

fn mixed_type() -> crate::vir::Type {
    use crate::vir::{EnumType, EnumVariant, RecordField, RecordType, Type, VariantPayload};
    Type::Record(RecordType {
        name: "Mixed@0000000000000002".into(),
        fields: vec![
            RecordField {
                name: "verdict".into(),
                ty: Type::Enum(EnumType {
                    name: "Verdict@0000000000000003".into(),
                    variants: vec![
                        EnumVariant { name: "Pass".into(), payload: VariantPayload::Unit },
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
            RecordField { name: "scores".into(), ty: Type::array(Type::Int) },
            RecordField { name: "note".into(), ty: Type::option(Type::String) },
            RecordField { name: "counts".into(), ty: Type::map(Type::String, Type::Int) },
        ],
    })
}

#[test]
fn round_trips_enums_options_lists_maps() {
    let ty = mixed_type();
    for original in [
        Mixed {
            verdict: Verdict::Fail { reason: "nope".into() },
            scores: vec![3, 1, 2],
            note: Some("hello".into()),
            counts: [("a".to_string(), 1i64), ("b".to_string(), 2i64)].into_iter().collect(),
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
    let ty = crate::vir::Type::option(crate::vir::Type::Int);
    let some = encode_value(facet::Peek::new(&Some(5i64)), &ty).unwrap();
    let none = encode_value(facet::Peek::new(&None::<i64>), &ty).unwrap();
    let FrozenValue::Variant { tag: some_tag, .. } = &some.frozen else { panic!() };
    let FrozenValue::Variant { tag: none_tag, .. } = &none.frozen else { panic!() };
    assert_eq!(*some_tag, crate::vir::OPTION_SOME_VARIANT);
    assert_ne!(some_tag, none_tag);
}
```

- [x] **Steps 2–4: red → implement → green** (`cargo nextest run -p vix convert`).
- [x] **Step 5: Commit** `--no-verify -am "vix: decode frozen store values into facet values"`.

---

### Task 5: Trait layer — Primitive, EffectCtx, tickets, completions

**Files:**
- Create: `vix/src/runtime/primitive/traits.rs` (+ mod wiring)
- Test: unit tests inside `traits.rs`

**Interfaces:**
- Consumes: `crate::runtime::model::{Receipt, ReadWitness}`, `crate::runtime::identity::ValueId`, `crate::runtime::store::{Store, Interned, FrozenValue}`, `crate::runtime::error::MachineError`, descriptor types.
- Produces (exact — phases 03/05 and register.rs compile against these):

```rust
/// Request handed to a primitive: the interned identity plus the frozen tree.
pub(crate) struct RequestRef<'a> {
    pub identity: ValueId,
    pub frozen: &'a FrozenValue,
}

/// One in-flight effect. Owned by the DEMAND, never the task
/// (r[machine.scheduler.tickets-outlive-tasks]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EffectTicket(pub u64);

/// Typed failure a primitive reports as a LANGUAGE result (memoizes under
/// policy). Carried as a registered-response-independent value in v1: a
/// rendered code + message pair interned as the failure payload; the typed
/// per-primitive failure schema axis is reserved for a later phase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveFailure { pub code: String, pub message: String }

pub enum Completion {
    Ok(Interned),
    Failed(PrimitiveFailure),
}

/// The primitive's ONLY machine window (r[machine.primitive.effectctx-witness-only]).
pub struct EffectCtx<'a> {
    store: &'a mut Store,
    witnessed: Vec<ReadWitness>,
    completion: Option<Completion>,
    events: Vec<EffectEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectEvent { pub primitive: PrimitiveId, pub message: String }

impl<'a> EffectCtx<'a> {
    pub(crate) fn new(store: &'a mut Store) -> Self { ... }
    pub fn witness_read(&mut self, source: ValueId, projection: &str) { ... }
    pub fn emit(&mut self, event: EffectEvent) { ... }
    pub fn complete(&mut self, completion: Completion) { ... } // second call: debug_assert + ignore in release? NO — make it a typed error state read by finish()
    pub(crate) fn store_mut(&mut self) -> &mut Store { ... } // pub(crate): register.rs interns through this; NOT visible to primitive impls outside the crate
    pub(crate) fn finish(self, demand: crate::runtime::identity::DemandKey)
        -> Result<(Completion, Receipt, Vec<EffectEvent>), Box<MachineError>> { ... }
    // finish() errors (typed, via a new RuntimeFault variant added in phase 05;
    // for now return the completion or a placeholder EffectProtocol error kind
    // defined locally) when complete() was never called or called twice.
}

pub trait Primitive {
    fn descriptor(&self) -> &PrimitiveDescriptor;
    /// Non-blocking begin (r[machine.primitive.trait]). v1 adapters complete
    /// inline before returning; the signature already permits async backends.
    fn begin(&self, request: RequestRef<'_>, ctx: &mut EffectCtx<'_>)
        -> Result<EffectTicket, Box<MachineError>>;
}
```

- Note the deliberate deviation-from-nothing: `EffectCtx::finish` consumes the ctx and produces `(Completion, Receipt, events)` — the scheduler (phase 05) is the only caller. Double-complete / no-complete is a protocol violation surfaced as a typed error, not a panic (`machine.error.structural-impossibility` still applies to impossible states, but a misbehaving registered primitive is EXPECTED fallibility).

- [x] **Step 1: failing tests** — ctx happy path (witness + complete + finish yields receipt with the witnessed reads and the demand key), no-complete → error, double-complete → error. Write them concretely against the API above.
- [x] **Steps 2–4: red → implement → green.**
- [x] **Step 5: Commit** `--no-verify -am "vix: primitive trait, EffectCtx, tickets, completions"`.

---

### Task 6: PrimitiveSet + register_function typed adapter

**Files:**
- Create: `vix/src/runtime/primitive/register.rs`; finalize `mod.rs` re-exports
- Test: unit tests inside `register.rs`

**Interfaces:**
- Consumes: everything above plus `phon::derive::of_shape` (`phon/rust/phon/src/derive.rs:102`, returns `Derived { root, schemas, .. }` or `DeriveError`), `facet::Facet` (`T::SHAPE`).
- Produces:

```rust
pub struct PrimitiveSet { entries: BTreeMap<String, Arc<dyn Primitive>> } // name-keyed

impl PrimitiveSet {
    pub fn new() -> Self;
    pub fn register(&mut self, primitive: Arc<dyn Primitive>) -> Result<(), RegistrationError>; // DuplicateName check
    pub fn register_function<'f, Resp, Req, F>(&mut self, name: &str, policy: MemoPolicy, f: F)
        -> Result<PrimitiveId, RegistrationError>
    where
        Req: facet::Facet<'f>,
        Resp: facet::Facet<'f>,
        F: Fn(Req) -> Result<Resp, PrimitiveFailure> + Send + Sync + 'static;
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Primitive>>;
    pub fn by_id(&self, id: PrimitiveId) -> Option<&Arc<dyn Primitive>>;
    pub fn descriptors(&self) -> impl Iterator<Item = &PrimitiveDescriptor>; // the compiler manifest (phase 03)
}
```

- `register_function` internals: `of_shape(Req::SHAPE)` + `of_shape(Resp::SHAPE)` (map `DeriveError` → `RegistrationError::Derive`), `vir_type_for` both, build `RegisteredSchema`s (`store_schema = semantic_schema_id(&vix_type)`), `PrimitiveId::derive(...)` with `version: u32 = 1` for the sugar path (full-control `register` path takes author-supplied versions via their own descriptor), wrap `f` in `struct FunctionPrimitive<Req, Resp, F> { descriptor, f, _marker }` whose `begin` does: `decode_value::<Req>(request.frozen, &descriptor.request.vix_type)` → run `f` → on Ok `intern_rust_value(&resp, &descriptor.response, ctx.store_mut())` → `ctx.complete(Completion::Ok(interned))`; on Err `ctx.complete(Completion::Failed(failure))`; decode `ConvertError` → `Box<MachineError>` (protocol violation — the compiler type-checked the call, so a mismatched request tree is machine-plane, not language-plane); return `EffectTicket(0)` (ticket ids become real in phase 05; the adapter's inline completion makes the id inert here — document this).

- [x] **Step 1: failing end-to-end unit test**:

```rust
#[derive(facet::Facet)]
struct AddRequest { left: i64, right: i64 }
#[derive(facet::Facet)]
struct AddResponse { sum: i64 }

#[test]
fn register_and_invoke_round_trip() {
    let mut set = PrimitiveSet::new();
    let id = set.register_function::<AddResponse, AddRequest, _>(
        "add_numbers", MemoPolicy::Hermetic,
        |req: AddRequest| Ok(AddResponse { sum: req.left + req.right }),
    ).unwrap();
    let primitive = set.by_id(id).unwrap().clone();
    let desc = primitive.descriptor();

    // Build the request the way vix will: encode a Rust value with the
    // REGISTERED vir type, then hand its frozen tree to begin().
    let mut store = crate::runtime::store::Store::default();
    let req = AddRequest { left: 40, right: 2 };
    let interned = crate::runtime::primitive::intern_rust_value(&req, &desc.request, &mut store).unwrap();
    // Re-encoding is the simplest way to hold the frozen request tree while
    // the store is mutably borrowed by the ctx below.
    let frozen =
        crate::runtime::primitive::encode_value(facet::Peek::new(&req), &desc.request.vix_type).unwrap().frozen;
    let response_type = desc.response.vix_type.clone();

    let mut ctx = EffectCtx::new(&mut store);
    primitive.begin(RequestRef { identity: interned.identity, frozen: &frozen }, &mut ctx).unwrap();
    let (completion, _receipt, _events) = ctx.finish(test_demand_key()).unwrap();
    let Completion::Ok(result) = completion else { panic!("expected ok") };
    let response_frozen = store.frozen_for(result.handle).expect("response carries a frozen tree").clone();
    let response: AddResponse =
        crate::runtime::primitive::decode_value(&response_frozen, &response_type).unwrap();
    assert_eq!(response.sum, 42);
}

#[test]
fn duplicate_and_unsupported_registrations_fail() {
    let mut set = PrimitiveSet::new();
    set.register_function::<AddResponse, AddRequest, _>("add_numbers", MemoPolicy::Volatile, |r| Ok(AddResponse { sum: r.left })).unwrap();
    assert!(matches!(
        set.register_function::<AddResponse, AddRequest, _>("add_numbers", MemoPolicy::Volatile, |r| Ok(AddResponse { sum: r.left })),
        Err(RegistrationError::DuplicateName { .. })));

    #[derive(facet::Facet)]
    struct BadRequest { weight: f64 }
    assert!(matches!(
        set.register_function::<AddResponse, BadRequest, _>("bad", MemoPolicy::Volatile, |_| Ok(AddResponse { sum: 0 })),
        Err(RegistrationError::UnsupportedShape { .. })));
}
```

Part of this task: the test needs `store.frozen_for(handle)`. `StoreEntry::frozen()` already exists (`store.rs:70`); add the thin accessor on `Store`:

```rust
pub(crate) fn frozen_for(&self, handle: Handle) -> Option<&FrozenValue> {
    self.entries.get(handle.0 as usize)?.frozen()
}
```

Also define the test helper `fn test_demand_key() -> crate::runtime::identity::DemandKey` by constructing the type the way existing runtime unit tests do — grep `DemandKey` in `vix/src/runtime/` tests and copy that construction.

- [x] **Steps 2–4: red → implement → green** (`cargo nextest run -p vix primitive`).
- [x] **Step 5: Commit** `--no-verify -am "vix: PrimitiveSet and register_function typed adapter"`.

---

### Task 7: Phase gate

- [x] Full suite: `cargo nextest run -p vix` → all green (nothing outside the module may regress; the only shared-code diff is the Task 1 hoist).
- [x] `cargo clippy -p vix -- -D warnings` clean on the new module.
- [x] Re-read the six new files against the Global Constraints (no strings-as-errors, no per-primitive arms, no private caches, spec rule comments `r[machine.primitive.*]` present on the trait/ctx/descriptor items).
- [x] Commit any fixups, then stop — phase 03 planning happens against this landed state.

## Self-review notes (already applied)

- Spec coverage: descriptor/trait/EffectCtx/adapter/bridge/conversion = spec §Components 1–4. Compiler/VIR/lowering/scheduler sections intentionally out of scope (phases 03–05); memo policy is carried as data only in this phase.
- Type consistency: `RegisteredSchema.store_schema` = `semantic_schema_id(vix_type)` everywhere; `Completion::Ok(Interned)` matches register.rs adapter and Task 5 finish() signature.
- Known unknowns called out to the executor: exact `facet::Partial` builder API (Task 4 step 1 requires reading facet-reflect/partial first), `realize_ordered` ordering rule (Task 3), vir `VariantPayload` record support (Task 2).
## Implementation notes (phase 02 as landed)

Deviations from the plan, all forced by the actual codebase — record for phase 03+:

- **facet `reflect` feature**: `facet::Peek`/`Partial` are behind the non-default
  `reflect` feature. Enabled it on vix's `facet` dep (`vix/Cargo.toml`). The plan
  assumed they were always available.
- **Arrays of scalar elements frame as `SeqInline`, not `SeqChildren`**. The
  scheduler's `realize_array` splits on `type_contains_handle`: scalar (`Bool`/
  `Int`) elements pack into `FramedNode::SeqInline` (8-byte words); handle-bearing
  elements use `SeqChildren`. `convert.rs` mirrors this for the primitive subset.
  Arrays of pure-scalar *composites* (tuple/record of ints) would frame inline in
  the scheduler too but are out of the phase-02 subset — noted in `encode_array`.
- **Trait layer returns a local `EffectProtocolError`, not `Box<MachineError>`**.
  The constraints forbid editing `error.rs`, and there's no existing `RuntimeFault`
  variant for effect-protocol violations. Per the plan's own Task 5 note ("a
  placeholder EffectProtocol error kind defined locally"), `begin`/`finish`/the
  adapter all return `EffectProtocolError`. Phase 05 lifts this into a real
  `RuntimeFault` variant on the machine-error plane.
- **Trait-layer types are `pub(crate)`** (`Primitive`, `RequestRef`, `EffectCtx`,
  `Completion`): they reference the `pub(crate)` `FrozenValue`/`Store`/`Interned`,
  so making them `pub` would be a private-interface leak. `PrimitiveSet`,
  `register_function`, descriptors, and the error/id/policy types stay `pub`.
- **`#![allow(dead_code)]`** on `convert.rs`/`traits.rs`/`register.rs` (+ item-level
  on `store::frozen_for`): the module is landed ahead of its phase-03 (compiler
  manifest) and phase-05 (scheduler) consumers; items are exercised by unit tests.
- **Commits**: Tasks 3 and 4 (encode/decode) landed as one commit (single file,
  `convert.rs`); every other task is its own commit.
