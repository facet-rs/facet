//! Cross-language conformance corpus: the case definitions, plus the resolver
//! and paths shared by the generator and the loaders.
//!
//! Rust is the source of truth (see `conformance/README.md`). The generator
//! ([`main`](../main.rs)) writes each case's resolved schemas as self-describing
//! bytes under `conformance/cases/<case>/<label>.phon`. The expected
//! [`SchemaId`](phon_schema::SchemaId) is baked into those bytes, so any
//! implementation can read a case, recompute the id with its own identity hash,
//! and check it matches — that is the cross-language oracle.
//!
//! Spec: "Schema identity" is the linchpin this corpus protects.

use std::path::{Path, PathBuf};

use facet_value::{VArray, VBytes, VDateTime, VObject, VQName, VString, VUuid, Value};
use phon_schema::{
    ChannelDirection, Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, Variant,
    VariantPayload, primitive_id, resolve_ids,
};

/// The committed schema-case directory, relative to the repository root.
pub const CASES_DIR: &str = "conformance/cases";
/// The committed value-case directory, relative to the repository root.
pub const VALUES_DIR: &str = "conformance/values";

/// The repository root, derived from this crate's location
/// (`<repo>/rust/phon-conformance` -> `<repo>`).
fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate is two levels under the repo root")
}

/// The schema-case corpus directory.
#[must_use]
pub fn cases_dir() -> PathBuf {
    repo_root().join(CASES_DIR)
}

/// The value-case corpus directory.
#[must_use]
pub fn values_dir() -> PathBuf {
    repo_root().join(VALUES_DIR)
}

/// A schema within a case, with the filename label it is written under.
pub struct LabeledSchema {
    pub label: String,
    pub schema: Schema,
}

/// A named group of mutually-referential schemas, resolved together.
pub struct Case {
    pub name: String,
    pub schemas: Vec<LabeledSchema>,
}

/// Resolve a case's schemas to their real [`SchemaId`]s (the input carries
/// provisional keys). Output order matches input order, so labels stay aligned.
#[must_use]
pub fn resolve_case(case: &Case) -> Vec<LabeledSchema> {
    let batch: Vec<Schema> = case.schemas.iter().map(|ls| ls.schema.clone()).collect();
    let resolved = resolve_ids(batch);
    case.schemas
        .iter()
        .zip(resolved)
        .map(|(ls, schema)| LabeledSchema {
            label: ls.label.clone(),
            schema,
        })
        .collect()
}

// ============================================================================
// Construction helpers (provisional keys; primitives referenced as externals)
// ============================================================================

fn prim(p: Primitive) -> SchemaRef {
    SchemaRef::concrete(primitive_id(p))
}

fn schema(key: u64, kind: SchemaKind) -> Schema {
    Schema {
        id: SchemaId(key),
        type_params: Vec::new(),
        kind,
    }
}

fn parametric(key: u64, type_params: &[&str], kind: SchemaKind) -> Schema {
    Schema {
        id: SchemaId(key),
        type_params: type_params.iter().map(|s| (*s).to_string()).collect(),
        kind,
    }
}

fn field(name: &str, r: SchemaRef, required: bool) -> Field {
    Field {
        name: name.to_string(),
        schema: r,
        required,
    }
}

fn labeled(label: &str, schema: Schema) -> LabeledSchema {
    LabeledSchema {
        label: label.to_string(),
        schema,
    }
}

// ============================================================================
// The cases
// ============================================================================

/// The canonical corpus. Each case exercises corners that must agree across
/// implementations: names, field/variant order, recursion, generics, every
/// container kind, and the special kinds.
#[must_use]
pub fn cases() -> Vec<Case> {
    vec![
        point(),
        enum_shapes(),
        linked_list(),
        generics(),
        containers(),
        special(),
    ]
}

fn point() -> Case {
    Case {
        name: "point".to_string(),
        schemas: vec![labeled(
            "Point",
            schema(
                1,
                SchemaKind::Struct {
                    name: "Point".to_string(),
                    fields: vec![
                        field("x", prim(Primitive::U32), true),
                        field("y", prim(Primitive::F64), false),
                    ],
                },
            ),
        )],
    }
}

fn enum_shapes() -> Case {
    Case {
        name: "enum_shapes".to_string(),
        schemas: vec![labeled(
            "Shape",
            schema(
                1,
                SchemaKind::Enum {
                    name: "Shape".to_string(),
                    variants: vec![
                        Variant {
                            name: "Empty".to_string(),
                            index: 0,
                            payload: VariantPayload::Unit,
                        },
                        Variant {
                            name: "Id".to_string(),
                            index: 1,
                            payload: VariantPayload::Newtype(prim(Primitive::U32)),
                        },
                        Variant {
                            name: "Span".to_string(),
                            index: 2,
                            payload: VariantPayload::Tuple(vec![
                                prim(Primitive::U32),
                                prim(Primitive::U32),
                            ]),
                        },
                        Variant {
                            name: "Named".to_string(),
                            index: 3,
                            payload: VariantPayload::Struct(vec![
                                field("label", prim(Primitive::String), true),
                                field("active", prim(Primitive::Bool), false),
                            ]),
                        },
                    ],
                },
            ),
        )],
    }
}

/// `Node { value: u32, next: Option<Node> }` modelled as two mutually
/// referential schemas — the recursion/SCC case.
fn linked_list() -> Case {
    let node = schema(
        10,
        SchemaKind::Struct {
            name: "Node".to_string(),
            fields: vec![
                field("value", prim(Primitive::U32), true),
                field("next", SchemaRef::concrete(SchemaId(20)), true),
            ],
        },
    );
    let opt = schema(
        20,
        SchemaKind::Option {
            element: SchemaRef::concrete(SchemaId(10)),
        },
    );
    Case {
        name: "linked_list".to_string(),
        schemas: vec![labeled("Node", node), labeled("OptionNode", opt)],
    }
}

/// Generics: `Pair<A, B>` and a `Holder<T>` that uses `Pair<T, u32>`.
fn generics() -> Case {
    let pair = parametric(
        1,
        &["A", "B"],
        SchemaKind::Tuple {
            elements: vec![SchemaRef::var("A"), SchemaRef::var("B")],
        },
    );
    let holder = parametric(
        2,
        &["T"],
        SchemaKind::Struct {
            name: "Holder".to_string(),
            fields: vec![
                field(
                    "pair",
                    SchemaRef::generic(
                        SchemaId(1),
                        vec![SchemaRef::var("T"), prim(Primitive::U32)],
                    ),
                    true,
                ),
                field("tag", prim(Primitive::String), true),
            ],
        },
    );
    Case {
        name: "generics".to_string(),
        schemas: vec![labeled("Pair", pair), labeled("Holder", holder)],
    }
}

/// Every container kind, standalone.
fn containers() -> Case {
    Case {
        name: "containers".to_string(),
        schemas: vec![
            labeled(
                "list",
                schema(
                    1,
                    SchemaKind::List {
                        element: prim(Primitive::U32),
                    },
                ),
            ),
            labeled(
                "set",
                schema(
                    2,
                    SchemaKind::Set {
                        element: prim(Primitive::String),
                    },
                ),
            ),
            labeled(
                "map",
                schema(
                    3,
                    SchemaKind::Map {
                        key: prim(Primitive::String),
                        value: prim(Primitive::U32),
                    },
                ),
            ),
            labeled(
                "array",
                schema(
                    4,
                    SchemaKind::Array {
                        element: prim(Primitive::F64),
                        dimensions: vec![4, 4],
                    },
                ),
            ),
            labeled(
                "tensor_fixed",
                schema(
                    5,
                    SchemaKind::Tensor {
                        element: prim(Primitive::F32),
                        rank: Some(2),
                    },
                ),
            ),
            labeled(
                "tensor_dyn",
                schema(
                    6,
                    SchemaKind::Tensor {
                        element: prim(Primitive::F32),
                        rank: None,
                    },
                ),
            ),
        ],
    }
}

/// The special kinds: dynamic, external (with and without metadata), channel.
fn special() -> Case {
    Case {
        name: "special".to_string(),
        schemas: vec![
            labeled("dynamic", schema(1, SchemaKind::Dynamic)),
            labeled(
                "external_blob",
                schema(
                    2,
                    SchemaKind::External {
                        kind: "blob".to_string(),
                        metadata: Some(prim(Primitive::U64)),
                    },
                ),
            ),
            labeled(
                "external_fd",
                schema(
                    3,
                    SchemaKind::External {
                        kind: "fd".to_string(),
                        metadata: None,
                    },
                ),
            ),
            labeled(
                "channel_rx",
                schema(
                    4,
                    SchemaKind::Channel {
                        direction: ChannelDirection::Rx,
                        element: prim(Primitive::U32),
                    },
                ),
            ),
        ],
    }
}

// ============================================================================
// Value cases
// ============================================================================

/// A named sample [`Value`], written self-describing under `values/<name>.phon`.
pub struct ValueCase {
    pub name: String,
    pub value: Value,
}

fn value_case(name: &str, value: impl Into<Value>) -> ValueCase {
    ValueCase {
        name: name.to_string(),
        value: value.into(),
    }
}

/// Sample values exercising every `Value` case the codec emits — scalars, the
/// containers, and the extended kinds (uuid, qname, every datetime shape). The
/// oracle: decode these and re-encode to byte-identical output, with `SchemaId`s
/// untouched (values carry no schema).
#[must_use]
pub fn value_cases() -> Vec<ValueCase> {
    let mut array = VArray::new();
    array.push(Value::from(1i64));
    array.push(VString::new("x"));
    array.push(Value::NULL);
    array.push(Value::from(true));

    let mut object = VObject::new();
    object.insert(VString::new("a"), Value::from(1i64));
    object.insert(VString::new("b"), Value::from(true));
    let mut inner = VArray::new();
    inner.push(Value::from('z'));
    object.insert(VString::new("c"), Value::from(inner));

    vec![
        value_case("null", Value::NULL),
        value_case("bool_true", true),
        value_case("bool_false", false),
        value_case("int_small", 42i64),
        value_case("int_negative", -7i64),
        value_case("int_u64_max", u64::MAX),
        value_case("int_u128_max", u128::MAX),
        value_case("int_i128_min", i128::MIN),
        value_case("float", 2.5f64),
        value_case("string", VString::new("héllo λ 🌍")),
        value_case("bytes", VBytes::new(&[0, 1, 2, 254, 255])),
        value_case("char", 'λ'),
        value_case("array", array),
        value_case("object", object),
        value_case(
            "uuid",
            VUuid::from_u128(0x0123_4567_89ab_cdef_fedc_ba98_7654_3210),
        ),
        value_case(
            "qname_namespaced",
            VQName::new(VString::new("http://ex.com/ns"), VString::new("el")),
        ),
        value_case("qname_local", VQName::new_local(VString::new("el"))),
        value_case(
            "datetime_offset",
            VDateTime::new_offset(2026, 5, 29, 7, 32, 0, 123_456_789, 330),
        ),
        value_case(
            "datetime_utc",
            VDateTime::new_offset(2026, 5, 29, 7, 32, 0, 0, 0),
        ),
        value_case(
            "datetime_local",
            VDateTime::new_local_datetime(2026, 5, 29, 7, 32, 0, 0),
        ),
        value_case("datetime_date", VDateTime::new_local_date(2026, 5, 29)),
        value_case("datetime_time", VDateTime::new_local_time(7, 32, 0, 500)),
    ]
}
