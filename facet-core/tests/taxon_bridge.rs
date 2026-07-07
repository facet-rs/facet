use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use facet::Facet;
use taxon::{Kind, Primitive, Schema, SchemaId, SchemaRef, VariantPayload, resolve_ids};

fn resolved<T: facet::Facet<'static>>() -> Vec<Schema> {
    resolve_ids(facet_core::taxon_bridge::schemas_of(
        <T as facet::Facet>::SHAPE,
    ))
}

fn root<T: facet::Facet<'static>>() -> Schema {
    resolved::<T>().remove(0)
}

fn id_of(schema_ref: &SchemaRef) -> SchemaId {
    match schema_ref {
        SchemaRef::Concrete { id, args } => {
            assert!(args.is_empty());
            *id
        }
        SchemaRef::Var { name } => panic!("unexpected type variable {name}"),
    }
}

fn schema_for<'a>(schemas: &'a [Schema], schema_ref: &SchemaRef) -> &'a Schema {
    let id = id_of(schema_ref);
    schemas
        .iter()
        .find(|schema| schema.id == id)
        .expect("referenced schema should be in the resolved batch")
}

#[derive(Facet)]
struct Leaf {
    value: u32,
}

#[derive(Facet)]
#[repr(u8)]
enum Payload {
    Unit,
    Newtype(String),
    Tuple(u8, bool),
    Struct { ok: bool, label: String },
}

fn payload_weight(payload: Payload) -> usize {
    match payload {
        Payload::Unit => 0,
        Payload::Newtype(value) => value.len(),
        Payload::Tuple(value, flag) => usize::from(value) + usize::from(flag),
        Payload::Struct { ok, label } => label.len() + usize::from(ok),
    }
}

#[derive(Facet)]
struct Representative {
    leaf: Leaf,
    list: Vec<u32>,
    set: HashSet<u8>,
    map: HashMap<String, Option<Result<u8, String>>>,
    tuple: (u8, bool),
    array: [u16; 3],
    result: Result<u32, String>,
    payload: Payload,
    arc: Arc<Leaf>,
}

#[derive(Facet)]
struct Node {
    value: u32,
    #[facet(recursive_type)]
    next: Option<Box<Node>>,
}

#[derive(Facet)]
struct SameA {
    value: u32,
}

#[derive(Facet)]
struct SameB {
    value: u32,
}

#[test]
fn classifies_representative_shape() {
    let schemas = resolved::<Representative>();
    let Kind::Struct { name, fields } = &schemas[0].kind else {
        panic!("representative root should be a struct");
    };
    assert_eq!(name, "Representative");
    assert_eq!(fields.len(), 9);

    let leaf = schema_for(&schemas, &fields[0].schema);
    assert!(matches!(&leaf.kind, Kind::Struct { name, .. } if name == "Leaf"));

    let list = schema_for(&schemas, &fields[1].schema);
    assert!(
        matches!(&list.kind, Kind::List { element } if matches!(schema_for(&schemas, element).kind, Kind::Primitive(Primitive::U32)))
    );

    let set = schema_for(&schemas, &fields[2].schema);
    assert!(
        matches!(&set.kind, Kind::Set { element } if matches!(schema_for(&schemas, element).kind, Kind::Primitive(Primitive::U8)))
    );

    let map = schema_for(&schemas, &fields[3].schema);
    let Kind::Map { key, value } = &map.kind else {
        panic!("map field should classify as Kind::Map");
    };
    assert!(matches!(
        schema_for(&schemas, key).kind,
        Kind::Primitive(Primitive::String)
    ));
    let option = schema_for(&schemas, value);
    assert!(matches!(option.kind, Kind::Option { .. }));

    let tuple = schema_for(&schemas, &fields[4].schema);
    assert!(matches!(&tuple.kind, Kind::Tuple { elements } if elements.len() == 2));

    let array = schema_for(&schemas, &fields[5].schema);
    assert!(matches!(&array.kind, Kind::Array { dimensions, .. } if dimensions == &[3]));

    let result = schema_for(&schemas, &fields[6].schema);
    assert!(
        matches!(&result.kind, Kind::Enum { variants, .. } if variants.iter().map(|v| v.name.as_str()).eq(["Ok", "Err"]))
    );

    let payload = schema_for(&schemas, &fields[7].schema);
    let Kind::Enum { name, variants } = &payload.kind else {
        panic!("payload field should classify as Kind::Enum");
    };
    assert_eq!(name, "Payload");
    assert!(matches!(variants[0].payload, VariantPayload::Unit));
    assert!(matches!(variants[1].payload, VariantPayload::Newtype(_)));
    assert!(matches!(variants[2].payload, VariantPayload::Tuple(_)));
    assert!(matches!(variants[3].payload, VariantPayload::Struct(_)));

    assert_eq!(id_of(&fields[0].schema), id_of(&fields[8].schema));
}

#[test]
fn payload_fixture_fields_are_used() {
    assert_eq!(payload_weight(Payload::Unit), 0);
    assert_eq!(payload_weight(Payload::Newtype(String::from("abc"))), 3);
    assert_eq!(payload_weight(Payload::Tuple(2, true)), 3);
    assert_eq!(
        payload_weight(Payload::Struct {
            ok: true,
            label: String::from("ok"),
        }),
        3
    );
}

#[test]
fn recursive_shape_uses_provisional_back_edges() {
    let batch = facet_core::taxon_bridge::schemas_of(<Node as facet::Facet>::SHAPE);
    assert!(
        batch
            .iter()
            .enumerate()
            .all(|(index, schema)| schema.id.as_u64() == index as u64),
        "classifier should emit dense provisional ids"
    );

    let resolved = resolve_ids(batch);
    let Kind::Struct { fields, .. } = &resolved[0].kind else {
        panic!("node root should be a struct");
    };
    let option = schema_for(&resolved, &fields[1].schema);
    let Kind::Option { element } = &option.kind else {
        panic!("next field should classify as an option");
    };
    assert_eq!(id_of(element), resolved[0].id);
}

#[test]
fn resolved_identity_is_name_sensitive_and_stable() {
    let same_a = root::<SameA>().id;
    assert_eq!(same_a, root::<SameA>().id);
    assert_ne!(same_a, root::<SameB>().id);
}
