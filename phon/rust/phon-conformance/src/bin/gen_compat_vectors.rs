//! Generates the cross-implementation **compat** conformance corpus at
//! `conformance/compat/vectors.json`.
//!
//! Each case pairs a *writer* schema graph with a *reader* schema graph plus a
//! sample writer [`Value`]. The Rust reference implementation is the oracle:
//!
//! - `writer_hex` = compact wire bytes of the writer value against the writer
//!   root (`phon_engine::compact::to_bytes`);
//! - `reader_hex` = compact wire bytes of the value that
//!   `phon_engine::plan::decode` reconciles for the reader (when the plan
//!   builds and the payload decodes);
//! - `error_kind` = the [`CompactError`] variant name when decode is expected to
//!   fail.
//!
//! Every schema reachable across all cases is resolved together into one batch
//! (so content-derived [`SchemaId`]s are stable and shared), written as
//! self-describing bytes, and collected into one registry. The TypeScript port
//! rebuilds that registry from the `schemas` array and replays each case.
//!
//! Run with `cargo run -p phon-conformance --bin gen_compat_vectors`. The file
//! is emitted via `facet-json` — never hand-written JSON.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

use facet::Facet;
use facet_value::{VArray, VBytes, VDateTime, VObject, VQName, VString, VUuid, Value};
use phon_engine::compact::{self, CompactError, Registry};
use phon_engine::plan;
use phon_schema::{
    Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, Variant, VariantPayload,
    primitive_id, resolve_ids, schema_to_bytes,
};

// ============================================================================
// Output shape (facet-derived; serialized with facet-json)
// ============================================================================

#[derive(Facet)]
struct VectorFile {
    /// Self-describing schema bytes (hex) for EVERY composite schema in the
    /// registry. The TS side parses each via its `schema_from_bytes` port and
    /// keys a registry by the parsed schema id. Primitive schemas are intrinsic
    /// and never appear here (they are recognized by their canonical id).
    schemas: Vec<String>,
    /// Every primitive's canonical id (hex) paired with its tag string. The TS
    /// side cannot recompute the content-derived primitive ids (no blake3 port),
    /// so it resolves primitive `SchemaRef`s through this table; composite refs
    /// resolve through `schemas`.
    primitives: Vec<PrimEntry>,
    cases: Vec<Case>,
}

#[derive(Facet)]
struct PrimEntry {
    /// Canonical `primitive_id` as a 16-char lowercase hex string.
    id: String,
    /// The primitive's tag string (`Primitive::tag`), e.g. `"u32"`, `"string"`.
    tag: String,
}

#[derive(Facet)]
struct Case {
    name: String,
    /// u64 SchemaId as a 16-char lowercase hex string (JSON numbers lose
    /// precision above 2^53).
    writer_root: String,
    reader_root: String,
    /// Compact wire bytes of the writer value, lowercase hex.
    writer_hex: String,
    /// `Some` => expect `plan::decode` to succeed with these reconciled reader
    /// bytes.
    reader_hex: Option<String>,
    /// `Some` => expect `plan::decode` to fail with this `CompactError` variant
    /// name (`reader_hex` is then `None`).
    error_kind: Option<String>,
}

// ============================================================================
// Schema-graph builder
// ============================================================================
//
// Provisional keys must be unique across the *entire* batch and must not collide
// with any primitive id. We hand out dense keys from a counter starting well
// above 0; primitive ids are content-derived blake3 values, so a small dense
// counter cannot collide with them.

/// Accumulates every composite schema across all cases (with provisional keys)
/// so they resolve to content-derived ids together in one batch.
struct Batch {
    schemas: Vec<Schema>,
    next_key: u64,
}

impl Batch {
    fn new() -> Self {
        Batch {
            schemas: Vec::new(),
            next_key: 1,
        }
    }

    /// Register a composite schema with the next provisional key, returning a
    /// concrete ref to it (carrying that provisional key — rewritten to the real
    /// id by `resolve_ids`).
    fn add(&mut self, kind: SchemaKind) -> SchemaRef {
        self.add_parametric(&[], kind)
    }

    fn add_parametric(&mut self, type_params: &[&str], kind: SchemaKind) -> SchemaRef {
        let key = self.next_key;
        self.next_key += 1;
        self.schemas.push(Schema {
            id: SchemaId(key),
            type_params: type_params.iter().map(|s| (*s).to_string()).collect(),
            kind,
        });
        SchemaRef::concrete(SchemaId(key))
    }
}

fn prim(p: Primitive) -> SchemaRef {
    SchemaRef::concrete(primitive_id(p))
}

/// Every primitive phon defines — the source for the `primitives` id->tag table.
fn all_primitives() -> Vec<Primitive> {
    use Primitive::*;
    vec![
        Bool, U8, U16, U32, U64, U128, I8, I16, I32, I64, I128, F32, F64, Char, String, Bytes,
        DateTime, Uuid, QName, Unit, Never,
    ]
}

fn field(name: &str, schema: SchemaRef, required: bool) -> Field {
    Field {
        name: name.to_string(),
        schema,
        required,
    }
}

fn obj(entries: &[(&str, Value)]) -> Value {
    let mut o = VObject::new();
    for (k, v) in entries {
        o.insert(VString::new(k), v.clone());
    }
    o.into()
}

fn arr(items: Vec<Value>) -> Value {
    let mut a = VArray::new();
    for it in items {
        a.push(it);
    }
    a.into()
}

fn enum_val(variant: &str, payload: Value) -> Value {
    obj(&[(variant, payload)])
}

// ============================================================================
// A case-in-progress: writer/reader roots + sample value
// ============================================================================

/// One planned compat case before it is run through the oracle. `writer_root`
/// and `reader_root` carry *provisional* refs while building; they are rewritten
/// to real ids once the whole batch resolves.
struct PlannedCase {
    name: String,
    writer_root: SchemaRef,
    reader_root: SchemaRef,
    value: Value,
}

/// The CompactError variant name we expect a failing decode to surface. Matched
/// against `plan::decode`'s error so the recorded `error_kind` stays a stable,
/// short string the TS side can compare.
fn error_kind_name(e: &CompactError) -> &'static str {
    match e {
        CompactError::UnknownSchema(_) => "UnknownSchema",
        CompactError::Unsupported(_) => "Unsupported",
        CompactError::TypeMismatch { .. } => "TypeMismatch",
        CompactError::UnknownVariant(_) => "UnknownVariant",
        CompactError::BadVariantIndex(_) => "BadVariantIndex",
        CompactError::GenericArity { .. } => "GenericArity",
        CompactError::Malformed(_) => "Malformed",
        CompactError::Incompatible(_) => "Incompatible",
        CompactError::WriterOnlyVariant(_) => "WriterOnlyVariant",
        CompactError::Decode(_) => "Decode",
        CompactError::Encode(_) => "Encode",
        // `CompactError` is `#[non_exhaustive]`; a future variant should not
        // silently masquerade as a known one.
        _ => "Unknown",
    }
}

// ============================================================================
// Building all cases
// ============================================================================

/// Build every case, registering all composite schemas into `b`. The returned
/// `PlannedCase`s still hold provisional refs.
fn build_cases(b: &mut Batch) -> Vec<PlannedCase> {
    let mut cases = Vec::new();

    let mut push = |name: &str, writer_root: SchemaRef, reader_root: SchemaRef, value: Value| {
        cases.push(PlannedCase {
            name: name.to_string(),
            writer_root,
            reader_root,
            value,
        });
    };

    // 1. scalar_u32 — same schema, root = u32 primitive (no composite).
    push(
        "scalar_u32",
        prim(Primitive::U32),
        prim(Primitive::U32),
        Value::from(123_456u32),
    );

    // 2. struct_mixed_align — heavy alignment padding, same writer/reader.
    {
        let fields = vec![
            field("a", prim(Primitive::U8), true),
            field("b", prim(Primitive::U32), true),
            field("c", prim(Primitive::U64), true),
            field("d", prim(Primitive::U128), true),
            field("e", prim(Primitive::F64), true),
            field("f", prim(Primitive::Bool), true),
            field("g", prim(Primitive::I16), true),
            field("h", prim(Primitive::String), true),
        ];
        let root = b.add(SchemaKind::Struct {
            name: "Mixed".to_string(),
            fields,
        });
        let value = obj(&[
            ("a", Value::from(7u8)),
            ("b", Value::from(70_000u32)),
            ("c", Value::from(5_000_000_000u64)),
            ("d", Value::from(1u128 << 70)),
            ("e", Value::from(2.5f64)),
            ("f", Value::from(true)),
            ("g", Value::from(-3i16)),
            ("h", Value::from(VString::new("hi"))),
        ]);
        push("struct_mixed_align", root.clone(), root, value);
    }

    // 3. string_and_bytes.
    {
        let root = b.add(SchemaKind::Struct {
            name: "Blob".to_string(),
            fields: vec![
                field("s", prim(Primitive::String), true),
                field("b", prim(Primitive::Bytes), true),
            ],
        });
        let value = obj(&[
            ("s", Value::from(VString::new("héllo λ 🌍"))),
            ("b", VBytes::new(&[0u8, 1, 2, 254, 255]).into()),
        ]);
        push("string_and_bytes", root.clone(), root, value);
    }

    // 4. list_u64 — element-run alignment.
    {
        let root = b.add(SchemaKind::List {
            element: prim(Primitive::U64),
        });
        let value = arr(vec![
            Value::from(1u64),
            Value::from(2u64),
            Value::from(0xdead_beef_cafeu64),
        ]);
        push("list_u64", root.clone(), root, value);
    }

    // 5. list_empty — list<u32>, 0 elements.
    {
        let root = b.add(SchemaKind::List {
            element: prim(Primitive::U32),
        });
        push("list_empty", root.clone(), root, arr(vec![]));
    }

    // 6a. list_zst — list of an EMPTY STRUCT (zero-sized element), 3 elements.
    {
        let empty = b.add(SchemaKind::Struct {
            name: "Empty".to_string(),
            fields: vec![],
        });
        let root = b.add(SchemaKind::List { element: empty });
        let value = arr(vec![obj(&[]), obj(&[]), obj(&[])]);
        push("list_zst", root.clone(), root, value);
    }

    // 6b. list_unit — list<unit>, 3 elements.
    {
        let root = b.add(SchemaKind::List {
            element: prim(Primitive::Unit),
        });
        let value = arr(vec![Value::NULL, Value::NULL, Value::NULL]);
        push("list_unit", root.clone(), root, value);
    }

    // 7. set_u32 — a few unique elements.
    {
        let root = b.add(SchemaKind::Set {
            element: prim(Primitive::U32),
        });
        let value = arr(vec![
            Value::from(3u32),
            Value::from(1u32),
            Value::from(2u32),
        ]);
        push("set_u32", root.clone(), root, value);
    }

    // 8. map_string_u32 — 2 entries.
    {
        let root = b.add(SchemaKind::Map {
            key: prim(Primitive::String),
            value: prim(Primitive::U32),
        });
        let value = obj(&[("alpha", Value::from(1u32)), ("beta", Value::from(2u32))]);
        push("map_string_u32", root.clone(), root, value);
    }

    // 9a. option_some — option<u32> = Some(7).
    {
        let root = b.add(SchemaKind::Option {
            element: prim(Primitive::U32),
        });
        push("option_some", root.clone(), root, Value::from(7u32));
    }
    // 9b. option_none — option<u32> = None.
    {
        let root = b.add(SchemaKind::Option {
            element: prim(Primitive::U32),
        });
        push("option_none", root.clone(), root, Value::NULL);
    }

    // 10. tuple_mixed — tuple(u32, bool, string).
    {
        let root = b.add(SchemaKind::Tuple {
            elements: vec![
                prim(Primitive::U32),
                prim(Primitive::Bool),
                prim(Primitive::String),
            ],
        });
        let value = arr(vec![
            Value::from(42u32),
            Value::from(true),
            Value::from(VString::new("tup")),
        ]);
        push("tuple_mixed", root.clone(), root, value);
    }

    // 11. nested_struct — struct { inner: struct{x,y}, tag }.
    {
        let inner = b.add(SchemaKind::Struct {
            name: "Inner".to_string(),
            fields: vec![
                field("x", prim(Primitive::U32), true),
                field("y", prim(Primitive::U32), true),
            ],
        });
        let root = b.add(SchemaKind::Struct {
            name: "Outer".to_string(),
            fields: vec![
                field("inner", inner, true),
                field("tag", prim(Primitive::U32), true),
            ],
        });
        let value = obj(&[
            (
                "inner",
                obj(&[("x", Value::from(11u32)), ("y", Value::from(22u32))]),
            ),
            ("tag", Value::from(99u32)),
        ]);
        push("nested_struct", root.clone(), root, value);
    }

    // 12. enum_same — enum { A (unit), B(u32 newtype), C(u8,u8 tuple) }.
    // Three sample values share one schema.
    {
        let variants = vec![
            Variant {
                name: "A".to_string(),
                index: 0,
                payload: VariantPayload::Unit,
            },
            Variant {
                name: "B".to_string(),
                index: 1,
                payload: VariantPayload::Newtype(prim(Primitive::U32)),
            },
            Variant {
                name: "C".to_string(),
                index: 2,
                payload: VariantPayload::Tuple(vec![prim(Primitive::U8), prim(Primitive::U8)]),
            },
        ];
        let root = b.add(SchemaKind::Enum {
            name: "E".to_string(),
            variants,
        });
        push(
            "enum_same",
            root.clone(),
            root.clone(),
            enum_val("B", Value::from(42u32)),
        );
        push(
            "enum_unit_variant",
            root.clone(),
            root.clone(),
            enum_val("A", Value::NULL),
        );
        push(
            "enum_tuple_variant",
            root.clone(),
            root,
            enum_val("C", arr(vec![Value::from(1u8), Value::from(2u8)])),
        );
    }

    // 13. struct_field_skip — writer {x, gone, y}; reader {x, y}.
    {
        let writer = b.add(SchemaKind::Struct {
            name: "S".to_string(),
            fields: vec![
                field("x", prim(Primitive::U32), true),
                field("gone", prim(Primitive::String), true),
                field("y", prim(Primitive::U32), true),
            ],
        });
        let reader = b.add(SchemaKind::Struct {
            name: "S".to_string(),
            fields: vec![
                field("x", prim(Primitive::U32), true),
                field("y", prim(Primitive::U32), true),
            ],
        });
        let value = obj(&[
            ("x", Value::from(1u32)),
            ("gone", Value::from(VString::new("bye"))),
            ("y", Value::from(2u32)),
        ]);
        push("struct_field_skip", writer, reader, value);
    }

    // 14. struct_field_default — writer {x}; reader {x, extra: option<u32> (non-required)}.
    // The reconciled reader value fills `extra` with its default — Value::NULL
    // (see plan.rs `exec_struct` defaults). `extra` is an option<u32> so its
    // null default round-trips through the reader encode (an option encodes
    // null as the absent presence byte).
    {
        let writer = b.add(SchemaKind::Struct {
            name: "D".to_string(),
            fields: vec![field("x", prim(Primitive::U32), true)],
        });
        let opt_u32 = b.add(SchemaKind::Option {
            element: prim(Primitive::U32),
        });
        let reader = b.add(SchemaKind::Struct {
            name: "D".to_string(),
            fields: vec![
                field("x", prim(Primitive::U32), true),
                field("extra", opt_u32, false),
            ],
        });
        let value = obj(&[("x", Value::from(7u32))]);
        push("struct_field_default", writer, reader, value);
    }

    // 15. struct_reorder — writer {x:u32, y:u64}; reader {y:u64, x:u32}.
    {
        let writer = b.add(SchemaKind::Struct {
            name: "R".to_string(),
            fields: vec![
                field("x", prim(Primitive::U32), true),
                field("y", prim(Primitive::U64), true),
            ],
        });
        let reader = b.add(SchemaKind::Struct {
            name: "R".to_string(),
            fields: vec![
                field("y", prim(Primitive::U64), true),
                field("x", prim(Primitive::U32), true),
            ],
        });
        let value = obj(&[("x", Value::from(5u32)), ("y", Value::from(9u64))]);
        push("struct_reorder", writer, reader, value);
    }

    // 16. enum_variant_remap — writer A=0,B=1; reader B=0,A=1 (indices swapped).
    // Sample = B. Reconciled reader value is still B; reader_bytes encode the
    // reader's index for B (0).
    {
        let writer = b.add(SchemaKind::Enum {
            name: "Rm".to_string(),
            variants: vec![
                Variant {
                    name: "A".to_string(),
                    index: 0,
                    payload: VariantPayload::Unit,
                },
                Variant {
                    name: "B".to_string(),
                    index: 1,
                    payload: VariantPayload::Newtype(prim(Primitive::U32)),
                },
            ],
        });
        let reader = b.add(SchemaKind::Enum {
            name: "Rm".to_string(),
            variants: vec![
                Variant {
                    name: "B".to_string(),
                    index: 0,
                    payload: VariantPayload::Newtype(prim(Primitive::U32)),
                },
                Variant {
                    name: "A".to_string(),
                    index: 1,
                    payload: VariantPayload::Unit,
                },
            ],
        });
        push(
            "enum_variant_remap",
            writer,
            reader,
            enum_val("B", Value::from(77u32)),
        );
    }

    // 17. enum_writer_only_variant — writer {A,B,Z(u32)}; reader {A,B}.
    // Sample = Z(99). plan::decode must ERROR (WriterOnlyVariant).
    {
        let writer = b.add(SchemaKind::Enum {
            name: "Wo".to_string(),
            variants: vec![
                Variant {
                    name: "A".to_string(),
                    index: 0,
                    payload: VariantPayload::Unit,
                },
                Variant {
                    name: "B".to_string(),
                    index: 1,
                    payload: VariantPayload::Unit,
                },
                Variant {
                    name: "Z".to_string(),
                    index: 2,
                    payload: VariantPayload::Newtype(prim(Primitive::U32)),
                },
            ],
        });
        let reader = b.add(SchemaKind::Enum {
            name: "Wo".to_string(),
            variants: vec![
                Variant {
                    name: "A".to_string(),
                    index: 0,
                    payload: VariantPayload::Unit,
                },
                Variant {
                    name: "B".to_string(),
                    index: 1,
                    payload: VariantPayload::Unit,
                },
            ],
        });
        push(
            "enum_writer_only_variant",
            writer,
            reader,
            enum_val("Z", Value::from(99u32)),
        );
    }

    // 17b. enum_struct_variant — an enum carrying a struct variant (named fields),
    // so the typed front door's `{ tag, ...fields }` inlining is round-tripped.
    {
        let variants = vec![
            Variant {
                name: "Move".to_string(),
                index: 0,
                payload: VariantPayload::Struct(vec![
                    field("x", prim(Primitive::U32), true),
                    field("y", prim(Primitive::U32), true),
                ]),
            },
            Variant {
                name: "Stop".to_string(),
                index: 1,
                payload: VariantPayload::Unit,
            },
        ];
        let root = b.add(SchemaKind::Enum {
            name: "Cmd".to_string(),
            variants,
        });
        push(
            "enum_struct_variant",
            root.clone(),
            root,
            enum_val(
                "Move",
                obj(&[("x", Value::from(3u32)), ("y", Value::from(4u32))]),
            ),
        );
    }

    // 17c. enum_struct_variant_payload_drift — an enum variant matches by name,
    // then its struct payload applies ordinary field compatibility. The writer
    // carries a transient field the reader skips, while the reader has an
    // optional payload field that defaults to null.
    {
        let writer = b.add(SchemaKind::Enum {
            name: "CmdCompat".to_string(),
            variants: vec![
                Variant {
                    name: "Move".to_string(),
                    index: 3,
                    payload: VariantPayload::Struct(vec![
                        field("x", prim(Primitive::U32), true),
                        field("transient", prim(Primitive::U64), true),
                        field("y", prim(Primitive::U32), true),
                    ]),
                },
                Variant {
                    name: "Stop".to_string(),
                    index: 4,
                    payload: VariantPayload::Unit,
                },
            ],
        });
        let option_u32 = b.add(SchemaKind::Option {
            element: prim(Primitive::U32),
        });
        let reader = b.add(SchemaKind::Enum {
            name: "CmdCompat".to_string(),
            variants: vec![
                Variant {
                    name: "Move".to_string(),
                    index: 0,
                    payload: VariantPayload::Struct(vec![
                        field("y", prim(Primitive::U32), true),
                        field("x", prim(Primitive::U32), true),
                        field("extra", option_u32, false),
                    ]),
                },
                Variant {
                    name: "Stop".to_string(),
                    index: 1,
                    payload: VariantPayload::Unit,
                },
            ],
        });
        push(
            "enum_struct_variant_payload_drift",
            writer,
            reader,
            enum_val(
                "Move",
                obj(&[
                    ("x", Value::from(3u32)),
                    ("transient", Value::from(999u64)),
                    ("y", Value::from(4u32)),
                ]),
            ),
        );
    }

    // 18. char_value — primitive char = 'λ'.
    push(
        "char_value",
        prim(Primitive::Char),
        prim(Primitive::Char),
        Value::from('λ'),
    );

    // 19. extended_kinds — struct { dt: datetime, id: uuid, qn: qname }.
    {
        let root = b.add(SchemaKind::Struct {
            name: "Ext".to_string(),
            fields: vec![
                field("dt", prim(Primitive::DateTime), true),
                field("id", prim(Primitive::Uuid), true),
                field("qn", prim(Primitive::QName), true),
            ],
        });
        let value = obj(&[
            (
                "dt",
                VDateTime::new_offset(2026, 5, 29, 7, 32, 0, 123_456_789, 330).into(),
            ),
            (
                "id",
                VUuid::from_u128(0x0123_4567_89ab_cdef_fedc_ba98_7654_3210).into(),
            ),
            (
                "qn",
                VQName::new(VString::new("http://ex.com/ns"), VString::new("el")).into(),
            ),
        ]);
        push("extended_kinds", root.clone(), root, value);
    }

    // 20. generic_pair — a parametric Pair<A,B> instantiated as Pair<u32, string>
    // inside a Holder struct. Exercises eager per-reference type substitution.
    {
        let pair_ref = b.add_parametric(
            &["A", "B"],
            SchemaKind::Struct {
                name: "Pair".to_string(),
                fields: vec![
                    field("a", SchemaRef::var("A"), true),
                    field("b", SchemaRef::var("B"), true),
                ],
            },
        );
        let SchemaRef::Concrete { id: pair_id, .. } = pair_ref else {
            unreachable!("add_parametric returns a concrete ref")
        };
        let root = b.add(SchemaKind::Struct {
            name: "Holder".to_string(),
            fields: vec![field(
                "pair",
                SchemaRef::generic(pair_id, vec![prim(Primitive::U32), prim(Primitive::String)]),
                true,
            )],
        });
        let value = obj(&[(
            "pair",
            obj(&[
                ("a", Value::from(5u32)),
                ("b", Value::from(VString::new("x"))),
            ]),
        )]);
        push("generic_pair", root.clone(), root, value);
    }

    // 21. dynamic_field — a struct with a `dynamic` field (self-describing on the
    // wire). The reconciled value round-trips through read_value/write_value.
    {
        let dyn_ref = b.add(SchemaKind::Dynamic);
        let root = b.add(SchemaKind::Struct {
            name: "Dyn".to_string(),
            fields: vec![
                field("tag", prim(Primitive::U32), true),
                field("payload", dyn_ref, true),
            ],
        });
        let value = obj(&[
            ("tag", Value::from(7u32)),
            ("payload", Value::from(VString::new("dyn"))),
        ]);
        push("dynamic_field", root.clone(), root, value);
    }

    // 22. channel_item_schema_compat — channel roots are transport capabilities,
    // but stream items are ordinary per-message payloads. This Dodeca-shaped item
    // proves a writer-only field is skipped by the shared compat corpus.
    {
        let writer = b.add(SchemaKind::Struct {
            name: "DodecaTunnelItem".to_string(),
            fields: vec![
                field("seq", prim(Primitive::U64), true),
                field("chunk_len", prim(Primitive::U32), true),
                field("transient_id", prim(Primitive::U64), true),
            ],
        });
        let reader = b.add(SchemaKind::Struct {
            name: "DodecaTunnelItem".to_string(),
            fields: vec![
                field("seq", prim(Primitive::U64), true),
                field("chunk_len", prim(Primitive::U32), true),
            ],
        });
        let value = obj(&[
            ("seq", Value::from(7u64)),
            ("chunk_len", Value::from(128u32)),
            ("transient_id", Value::from(99u64)),
        ]);
        push("channel_item_schema_compat", writer, reader, value);
    }

    // 23. external_metadata_schema_compat — external roots are transport-owned
    // capabilities, but their metadata payload schema is planned and decoded
    // normally beside the transport handle.
    {
        let writer = b.add(SchemaKind::Struct {
            name: "StaxFdMetadata".to_string(),
            fields: vec![
                field("path", prim(Primitive::String), true),
                field("flags", prim(Primitive::U32), true),
                field("probe_id", prim(Primitive::U64), true),
            ],
        });
        let reader = b.add(SchemaKind::Struct {
            name: "StaxFdMetadata".to_string(),
            fields: vec![
                field("path", prim(Primitive::String), true),
                field("flags", prim(Primitive::U32), true),
            ],
        });
        let value = obj(&[
            ("path", Value::from(VString::new("/proc/self/fd/7"))),
            ("flags", Value::from(0x800u32)),
            ("probe_id", Value::from(44u64)),
        ]);
        push("external_metadata_schema_compat", writer, reader, value);
    }

    cases
}

// ============================================================================
// Resolution + ref rewriting
// ============================================================================
//
// `resolve_ids` rewrites every in-batch provisional key — both each schema's id
// and every in-batch `SchemaRef::Concrete.id` — to its real, content-derived id.
// A root ref the cases hold is either a primitive (already a real id) or an
// in-batch provisional key; we map provisional keys through the same table the
// resolver produced.

/// Resolve a root ref to a real `SchemaId`, given the provisional-key -> real-id
/// map. Primitive roots carry a real id already and pass through.
fn resolve_root(r: &SchemaRef, key_to_real: &BTreeMap<u64, SchemaId>) -> SchemaId {
    match r {
        SchemaRef::Concrete { id, .. } => key_to_real.get(&id.0).copied().unwrap_or(*id),
        SchemaRef::Var { .. } => panic!("a case root cannot be a type variable"),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn id_hex(id: SchemaId) -> String {
    format!("{:016x}", id.0)
}

// ============================================================================
// Entry point
// ============================================================================

fn out_path() -> PathBuf {
    // <repo>/rust/phon-conformance -> <repo>/conformance/compat/vectors.json
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo = manifest
        .ancestors()
        .nth(2)
        .expect("crate is two levels under the repo root");
    repo.join("conformance").join("compat").join("vectors.json")
}

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let mut batch = Batch::new();
    let planned = build_cases(&mut batch);

    // Resolve every composite schema together so ids are content-derived and
    // shared. We capture the provisional-key -> real-id map by zipping the input
    // and resolved batches (resolve_ids preserves order).
    let provisional_keys: Vec<u64> = batch.schemas.iter().map(|s| s.id.0).collect();
    let resolved = resolve_ids(batch.schemas);
    let mut key_to_real: BTreeMap<u64, SchemaId> = BTreeMap::new();
    for (key, schema) in provisional_keys.iter().zip(&resolved) {
        key_to_real.insert(*key, schema.id);
    }

    // One registry holding every resolved composite schema. Primitives are
    // intrinsic.
    let reg = Registry::new(resolved.iter().cloned());

    // Self-describing bytes for every distinct composite schema (hex). Some
    // cases build the same logical schema independently (e.g. `option<u32>`),
    // which content-resolves to one id; emit each id once so the TS side's
    // id-keyed registry has no redundant entries.
    let mut seen_ids = std::collections::HashSet::new();
    let schemas: Vec<String> = resolved
        .iter()
        .filter(|s| seen_ids.insert(s.id))
        .map(|s| hex(&schema_to_bytes(s)))
        .collect();

    // Run each case through the oracle.
    let mut cases = Vec::with_capacity(planned.len());
    for pc in &planned {
        let writer_root = resolve_root(&pc.writer_root, &key_to_real);
        let reader_root = resolve_root(&pc.reader_root, &key_to_real);

        let writer_bytes = compact::to_bytes(&pc.value, writer_root, &reg)
            .unwrap_or_else(|e| panic!("case {}: writer encode failed: {e}", pc.name));

        let (reader_hex, error_kind) =
            match plan::decode(&writer_bytes, writer_root, reader_root, &reg) {
                Ok(reader_value) => {
                    let reader_bytes = compact::to_bytes(&reader_value, reader_root, &reg)
                        .unwrap_or_else(|e| {
                            panic!("case {}: reader re-encode failed: {e}", pc.name)
                        });
                    tracing::debug!(
                        case = %pc.name,
                        writer = %id_hex(writer_root),
                        reader = %id_hex(reader_root),
                        writer_len = writer_bytes.len(),
                        reader_len = reader_bytes.len(),
                        "decoded ok"
                    );
                    (Some(hex(&reader_bytes)), None)
                }
                Err(e) => {
                    let kind = error_kind_name(&e);
                    tracing::debug!(
                        case = %pc.name,
                        writer = %id_hex(writer_root),
                        reader = %id_hex(reader_root),
                        error = %kind,
                        "decode errored (expected for negative cases)"
                    );
                    (None, Some(kind.to_string()))
                }
            };

        cases.push(Case {
            name: pc.name.clone(),
            writer_root: id_hex(writer_root),
            reader_root: id_hex(reader_root),
            writer_hex: hex(&writer_bytes),
            reader_hex,
            error_kind,
        });
    }

    let primitives: Vec<PrimEntry> = all_primitives()
        .into_iter()
        .map(|p| PrimEntry {
            id: id_hex(primitive_id(p)),
            tag: p.tag().to_string(),
        })
        .collect();

    let file = VectorFile {
        schemas,
        primitives,
        cases,
    };
    let json = facet_json::to_string_pretty(&file)
        .map_err(|e| format!("facet-json serialize failed: {e}"))?;

    let path = out_path();
    fs::create_dir_all(path.parent().expect("path has a parent"))?;
    fs::write(&path, json.as_bytes())?;

    tracing::info!(
        cases = file.cases.len(),
        schemas = file.schemas.len(),
        path = %path.display(),
        "wrote compat vectors"
    );
    Ok(())
}
