#![deny(unsafe_code)]

use facet::Facet;
use facet_core::{Def, ScalarType, Shape, StructKind, Type, UserType};
use roam_types::{is_rx, is_tx};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Tracks schema exchange state for one session.
// r[impl schema.tracking.sent]
// r[impl schema.tracking.received]
pub struct SchemaTracker {
    sent: Mutex<HashSet<TypeId>>,
    received: Mutex<HashSet<TypeId>>,
}

impl SchemaTracker {
    pub fn new() -> Self {
        SchemaTracker {
            sent: Mutex::new(HashSet::new()),
            received: Mutex::new(HashSet::new()),
        }
    }

    /// Given a Shape, compute all schemas needed and return the ones
    /// not yet sent. Marks them as sent atomically. Returns None if
    /// all schemas were already sent.
    // r[impl schema.tracking.transitive]
    // r[impl schema.exchange.idempotent]
    // r[impl schema.principles.once-per-type]
    pub fn prepare_send(&self, shape: &'static Shape) -> Option<Vec<Schema>> {
        let all_schemas = extract_schemas(shape);
        let mut sent = self.sent.lock().unwrap();
        let unsent: Vec<Schema> = all_schemas
            .into_iter()
            .filter(|s| !sent.contains(&s.type_id))
            .collect();
        if unsent.is_empty() {
            return None;
        }
        for s in &unsent {
            sent.insert(s.type_id);
        }
        Some(unsent)
    }

    /// Record that we received schemas for these type IDs.
    pub fn record_received(&self, type_ids: &[TypeId]) {
        let mut received = self.received.lock().unwrap();
        for id in type_ids {
            received.insert(*id);
        }
    }

    /// Check if we've received a schema for this type ID.
    pub fn has_received(&self, type_id: &TypeId) -> bool {
        self.received.lock().unwrap().contains(type_id)
    }
}

impl Default for SchemaTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute a TypeId from a Shape by hashing its canonical byte encoding with blake3.
// r[impl schema.type-id]
// r[impl schema.type-id.deterministic]
// r[impl schema.type-id.structural]
pub fn type_id_of(shape: &'static Shape) -> TypeId {
    let bytes = roam_hash::encode_shape_bytes(shape);
    let hash = blake3::hash(&bytes);
    let mut id = [0u8; 16];
    id.copy_from_slice(&hash.as_bytes()[..16]);
    TypeId(id)
}

/// Extract all schemas for a type and its transitive dependencies.
///
/// Returns schemas in dependency order: dependencies appear before dependents.
/// The root type's schema is last.
// r[impl schema.format]
pub fn extract_schemas(shape: &'static Shape) -> Vec<Schema> {
    let mut ctx = ExtractCtx {
        schemas: Vec::new(),
        seen: HashSet::new(),
        stack: Vec::new(),
    };
    ctx.extract(shape);
    ctx.schemas
}

struct ExtractCtx {
    schemas: Vec<Schema>,
    /// Shapes already fully processed (by pointer identity).
    seen: HashSet<usize>,
    /// Stack for cycle detection (by pointer identity).
    stack: Vec<usize>,
}

impl ExtractCtx {
    /// Extract a schema for the given shape, returning its TypeId.
    /// Recursively extracts dependencies first.
    fn extract(&mut self, shape: &'static Shape) -> TypeId {
        // Channel types: extract the element type, skip the channel wrapper.
        if is_tx(shape) || is_rx(shape) {
            if let Some(inner) = shape.type_params.first() {
                return self.extract(inner.shape);
            }
        }

        // Transparent wrappers: follow inner.
        if shape.is_transparent() {
            if let Some(inner) = shape.inner {
                return self.extract(inner);
            }
        }

        let type_id = type_id_of(shape);
        let ptr = shape as *const Shape as usize;

        // Already fully processed — just return its id.
        if self.seen.contains(&ptr) {
            return type_id;
        }

        // r[impl schema.format.recursive]
        // Cycle detection: if on the stack, return the id without re-entering.
        if self.stack.contains(&ptr) {
            return type_id;
        }

        // r[impl schema.format.primitive]
        // Scalars
        if let Some(scalar) = shape.scalar_type() {
            if self.seen.insert(ptr) {
                self.schemas.push(Schema {
                    type_id,
                    kind: SchemaKind::Primitive {
                        primitive_type: scalar_to_primitive(scalar),
                    },
                });
            }
            return type_id;
        }

        // r[impl schema.format.container]
        // Containers
        match shape.def {
            Def::List(list_def) => {
                if let Some(ScalarType::U8) = list_def.t().scalar_type() {
                    // Vec<u8> → Bytes
                    if self.seen.insert(ptr) {
                        self.schemas.push(Schema {
                            type_id,
                            kind: SchemaKind::Primitive {
                                primitive_type: PrimitiveType::Bytes,
                            },
                        });
                    }
                } else {
                    let elem_id = self.extract(list_def.t());
                    if self.seen.insert(ptr) {
                        self.schemas.push(Schema {
                            type_id,
                            kind: SchemaKind::List { element: elem_id },
                        });
                    }
                }
                return type_id;
            }
            Def::Array(array_def) => {
                let elem_id = self.extract(array_def.t());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::Array {
                            element: elem_id,
                            length: array_def.n as u64,
                        },
                    });
                }
                return type_id;
            }
            Def::Slice(slice_def) => {
                let elem_id = self.extract(slice_def.t());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::List { element: elem_id },
                    });
                }
                return type_id;
            }
            Def::Map(map_def) => {
                let key_id = self.extract(map_def.k());
                let val_id = self.extract(map_def.v());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::Map {
                            key: key_id,
                            value: val_id,
                        },
                    });
                }
                return type_id;
            }
            Def::Set(set_def) => {
                let elem_id = self.extract(set_def.t());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::Set { element: elem_id },
                    });
                }
                return type_id;
            }
            Def::Option(opt_def) => {
                let elem_id = self.extract(opt_def.t());
                if self.seen.insert(ptr) {
                    self.schemas.push(Schema {
                        type_id,
                        kind: SchemaKind::Option { element: elem_id },
                    });
                }
                return type_id;
            }
            Def::Pointer(ptr_def) => {
                if let Some(pointee) = ptr_def.pointee {
                    return self.extract(pointee);
                }
            }
            _ => {}
        }

        // User-defined types: push onto stack for cycle detection.
        self.stack.push(ptr);

        let kind = match shape.ty {
            // r[impl schema.format.struct]
            // r[impl schema.format.tuple]
            Type::User(UserType::Struct(struct_type)) => match struct_type.kind {
                StructKind::Unit => SchemaKind::Primitive {
                    primitive_type: PrimitiveType::Unit,
                },
                StructKind::TupleStruct | StructKind::Tuple => {
                    let elements: Vec<TypeId> = struct_type
                        .fields
                        .iter()
                        .map(|f| self.extract(f.shape()))
                        .collect();
                    SchemaKind::Tuple { elements }
                }
                StructKind::Struct => {
                    let fields: Vec<FieldSchema> = struct_type
                        .fields
                        .iter()
                        .map(|f| FieldSchema {
                            name: f.name.to_string(),
                            type_id: self.extract(f.shape()),
                            required: true,
                        })
                        .collect();
                    SchemaKind::Struct { fields }
                }
            },
            // r[impl schema.format.enum]
            Type::User(UserType::Enum(enum_type)) => {
                let variants: Vec<VariantSchema> = enum_type
                    .variants
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let payload = match v.data.kind {
                            StructKind::Unit => VariantPayload::Unit,
                            StructKind::TupleStruct | StructKind::Tuple => {
                                if v.data.fields.len() == 1 {
                                    VariantPayload::Newtype {
                                        type_id: self.extract(v.data.fields[0].shape()),
                                    }
                                } else {
                                    let fields: Vec<FieldSchema> = v
                                        .data
                                        .fields
                                        .iter()
                                        .enumerate()
                                        .map(|(j, f)| FieldSchema {
                                            name: j.to_string(),
                                            type_id: self.extract(f.shape()),
                                            required: true,
                                        })
                                        .collect();
                                    VariantPayload::Struct { fields }
                                }
                            }
                            StructKind::Struct => {
                                let fields: Vec<FieldSchema> = v
                                    .data
                                    .fields
                                    .iter()
                                    .map(|f| FieldSchema {
                                        name: f.name.to_string(),
                                        type_id: self.extract(f.shape()),
                                        required: true,
                                    })
                                    .collect();
                                VariantPayload::Struct { fields }
                            }
                        };
                        VariantSchema {
                            name: v.name.to_string(),
                            index: i as u32,
                            payload,
                        }
                    })
                    .collect();
                SchemaKind::Enum { variants }
            }
            Type::Pointer(_) => {
                // Follow pointer type params
                if let Some(inner) = shape.type_params.first() {
                    self.stack.pop();
                    return self.extract(inner.shape);
                }
                SchemaKind::Primitive {
                    primitive_type: PrimitiveType::Unit,
                }
            }
            _ => SchemaKind::Primitive {
                primitive_type: PrimitiveType::Unit,
            },
        };

        self.stack.pop();

        if self.seen.insert(ptr) {
            self.schemas.push(Schema { type_id, kind });
        }

        type_id
    }
}

fn scalar_to_primitive(scalar: ScalarType) -> PrimitiveType {
    match scalar {
        ScalarType::Unit => PrimitiveType::Unit,
        ScalarType::Bool => PrimitiveType::Bool,
        ScalarType::Char => PrimitiveType::Char,
        ScalarType::Str | ScalarType::String | ScalarType::CowStr => PrimitiveType::String,
        ScalarType::F32 => PrimitiveType::F32,
        ScalarType::F64 => PrimitiveType::F64,
        ScalarType::U8 => PrimitiveType::U8,
        ScalarType::U16 => PrimitiveType::U16,
        ScalarType::U32 => PrimitiveType::U32,
        ScalarType::U64 => PrimitiveType::U64,
        ScalarType::U128 => PrimitiveType::U128,
        ScalarType::USize => PrimitiveType::U64,
        ScalarType::I8 => PrimitiveType::I8,
        ScalarType::I16 => PrimitiveType::I16,
        ScalarType::I32 => PrimitiveType::I32,
        ScalarType::I64 => PrimitiveType::I64,
        ScalarType::I128 => PrimitiveType::I128,
        ScalarType::ISize => PrimitiveType::I64,
        ScalarType::ConstTypeId => PrimitiveType::U64,
        _ => PrimitiveType::Unit,
    }
}

/// A 16-byte identifier for a type.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TypeId(pub [u8; 16]);

/// The root schema type describing a single type.
#[derive(Facet, Clone, Debug)]
pub struct Schema {
    pub type_id: TypeId,
    pub kind: SchemaKind,
}

/// The structural kind of a type.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum SchemaKind {
    Struct { fields: Vec<FieldSchema> },
    Enum { variants: Vec<VariantSchema> },
    Tuple { elements: Vec<TypeId> },
    List { element: TypeId },
    Map { key: TypeId, value: TypeId },
    Set { element: TypeId },
    Array { element: TypeId, length: u64 },
    Option { element: TypeId },
    Primitive { primitive_type: PrimitiveType },
}

/// Describes a single field in a struct or struct variant.
#[derive(Facet, Clone, Debug)]
pub struct FieldSchema {
    pub name: String,
    pub type_id: TypeId,
    pub required: bool,
}

/// Describes a single variant in an enum.
#[derive(Facet, Clone, Debug)]
pub struct VariantSchema {
    pub name: String,
    pub index: u32,
    pub payload: VariantPayload,
}

/// The payload of an enum variant.
#[derive(Facet, Clone, Debug)]
#[repr(u8)]
pub enum VariantPayload {
    Unit,
    Newtype { type_id: TypeId },
    Struct { fields: Vec<FieldSchema> },
}

/// Primitive types supported by the wire format.
#[derive(Facet, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum PrimitiveType {
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
    Char,
    String,
    Unit,
    Bytes,
}

/// Lookup table mapping TypeId → Schema, used for resolving type references
/// during deserialization with translation plans.
pub type SchemaRegistry = HashMap<TypeId, Schema>;

/// Build a SchemaRegistry from a list of schemas.
pub fn build_registry(schemas: &[Schema]) -> SchemaRegistry {
    schemas.iter().map(|s| (s.type_id, s.clone())).collect()
}

/// A batch of schemas sent over the wire.
#[derive(Facet, Clone, Debug)]
pub struct SchemaMessage {
    pub schemas: Vec<Schema>,
}

/// Build a CBOR-encoded schema batch from a list of schemas.
// r[impl schema.format.self-contained]
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn build_schema_message(schemas: &[Schema]) -> Vec<u8> {
    facet_cbor::to_vec(&schemas).expect("schema CBOR serialization should not fail")
}

/// Parse a CBOR-encoded schema batch.
// r[impl schema.format.batch]
// r[impl schema.principles.cbor]
pub fn parse_schema_message(bytes: &[u8]) -> Result<Vec<Schema>, facet_cbor::CborError> {
    facet_cbor::from_slice(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    // r[verify schema.format.primitive]
    #[test]
    fn primitive_u32() {
        let schemas = extract_schemas(<u32 as Facet>::SHAPE);
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::U32
            }
        ));
    }

    #[test]
    fn primitive_string() {
        let schemas = extract_schemas(<String as Facet>::SHAPE);
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::String
            }
        ));
    }

    #[test]
    fn primitive_bool() {
        let schemas = extract_schemas(<bool as Facet>::SHAPE);
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Bool
            }
        ));
    }

    // r[verify schema.format.struct]
    #[test]
    fn simple_struct() {
        #[derive(Facet)]
        struct Point {
            x: f64,
            y: f64,
        }

        let schemas = extract_schemas(Point::SHAPE);
        // f64 schema + Point schema
        assert!(schemas.len() >= 2);

        let point_schema = schemas.last().unwrap();
        match &point_schema.kind {
            SchemaKind::Struct { fields } => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "x");
                assert_eq!(fields[1].name, "y");
                assert!(fields[0].required);
                // Both fields should reference the same f64 TypeId
                assert_eq!(fields[0].type_id, fields[1].type_id);
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    // r[verify schema.format.enum]
    #[test]
    fn simple_enum() {
        #[derive(Facet)]
        #[repr(u8)]
        enum Color {
            Red,
            Green,
            Blue,
        }

        let schemas = extract_schemas(Color::SHAPE);
        let color_schema = schemas.last().unwrap();
        match &color_schema.kind {
            SchemaKind::Enum { variants } => {
                assert_eq!(variants.len(), 3);
                assert_eq!(variants[0].name, "Red");
                assert_eq!(variants[1].name, "Green");
                assert_eq!(variants[2].name, "Blue");
                assert!(matches!(variants[0].payload, VariantPayload::Unit));
            }
            other => panic!("expected Enum, got {other:?}"),
        }
    }

    // r[verify schema.format.enum]
    #[test]
    fn enum_with_payloads() {
        #[derive(Facet)]
        #[repr(u8)]
        enum Shape {
            Circle(f64),
            Rect { w: f64, h: f64 },
            Empty,
        }

        let schemas = extract_schemas(Shape::SHAPE);
        let shape_schema = schemas.last().unwrap();
        match &shape_schema.kind {
            SchemaKind::Enum { variants } => {
                assert_eq!(variants.len(), 3);
                assert!(matches!(
                    variants[0].payload,
                    VariantPayload::Newtype { .. }
                ));
                match &variants[1].payload {
                    VariantPayload::Struct { fields } => {
                        assert_eq!(fields.len(), 2);
                        assert_eq!(fields[0].name, "w");
                        assert_eq!(fields[1].name, "h");
                    }
                    other => panic!("expected Struct variant, got {other:?}"),
                }
                assert!(matches!(variants[2].payload, VariantPayload::Unit));
            }
            other => panic!("expected Enum, got {other:?}"),
        }
    }

    // r[verify schema.format.container]
    #[test]
    fn container_vec() {
        let schemas = extract_schemas(<Vec<u32> as Facet>::SHAPE);
        // u32 schema + Vec<u32> schema
        assert_eq!(schemas.len(), 2);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::U32
            }
        ));
        assert!(matches!(schemas[1].kind, SchemaKind::List { .. }));
    }

    // r[verify schema.format.container]
    #[test]
    fn container_option() {
        let schemas = extract_schemas(<Option<String> as Facet>::SHAPE);
        assert_eq!(schemas.len(), 2);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::String
            }
        ));
        assert!(matches!(schemas[1].kind, SchemaKind::Option { .. }));
    }

    // r[verify schema.format.recursive]
    #[test]
    fn recursive_type_terminates() {
        #[derive(Facet)]
        struct Node {
            value: u32,
            next: Option<Box<Node>>,
        }

        let schemas = extract_schemas(Node::SHAPE);
        // Should not infinite loop. Should contain at least u32, Option<Box<Node>>, Node.
        assert!(schemas.len() >= 2);

        // The root schema should be Node (last in dependency order)
        let node_schema = schemas.last().unwrap();
        assert!(matches!(node_schema.kind, SchemaKind::Struct { .. }));
    }

    // r[verify schema.format.primitive]
    #[test]
    fn vec_u8_is_bytes() {
        let schemas = extract_schemas(<Vec<u8> as Facet>::SHAPE);
        assert_eq!(schemas.len(), 1);
        assert!(matches!(
            schemas[0].kind,
            SchemaKind::Primitive {
                primitive_type: PrimitiveType::Bytes
            }
        ));
    }

    // r[verify schema.principles.once-per-type]
    #[test]
    fn deduplication_two_u32_fields() {
        #[derive(Facet)]
        struct TwoU32 {
            a: u32,
            b: u32,
        }

        let schemas = extract_schemas(TwoU32::SHAPE);
        // u32 should appear exactly once, plus the struct itself
        let u32_count = schemas
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    SchemaKind::Primitive {
                        primitive_type: PrimitiveType::U32
                    }
                )
            })
            .count();
        assert_eq!(u32_count, 1, "u32 schema should appear exactly once");
        assert_eq!(schemas.len(), 2); // u32 + TwoU32
    }

    // r[verify schema.type-id]
    // r[verify schema.type-id.deterministic]
    #[test]
    fn type_id_is_stable() {
        let id1 = type_id_of(<u32 as Facet>::SHAPE);
        let id2 = type_id_of(<u32 as Facet>::SHAPE);
        assert_eq!(id1, id2);
    }

    // r[verify schema.type-id.structural]
    #[test]
    fn type_id_differs_for_different_types() {
        let id_u32 = type_id_of(<u32 as Facet>::SHAPE);
        let id_u64 = type_id_of(<u64 as Facet>::SHAPE);
        assert_ne!(id_u32, id_u64);
    }

    // r[verify schema.format.container]
    #[test]
    fn container_map() {
        let schemas = extract_schemas(<std::collections::HashMap<String, u32> as Facet>::SHAPE);
        let map_schema = schemas.last().unwrap();
        assert!(matches!(map_schema.kind, SchemaKind::Map { .. }));
    }

    // r[verify schema.format.container]
    #[test]
    fn container_array() {
        let schemas = extract_schemas(<[u32; 4] as Facet>::SHAPE);
        let arr_schema = schemas.last().unwrap();
        match &arr_schema.kind {
            SchemaKind::Array { length, .. } => assert_eq!(*length, 4),
            other => panic!("expected Array, got {other:?}"),
        }
    }

    // r[verify schema.format.tuple]
    #[test]
    fn tuple_type() {
        let schemas = extract_schemas(<(u32, String) as Facet>::SHAPE);
        let tuple_schema = schemas.last().unwrap();
        match &tuple_schema.kind {
            SchemaKind::Tuple { elements } => {
                assert_eq!(elements.len(), 2);
                assert_ne!(elements[0], elements[1]);
            }
            other => panic!("expected Tuple, got {other:?}"),
        }
    }

    // r[verify schema.format]
    #[test]
    fn extract_schemas_returns_all_kinds() {
        #[derive(Facet)]
        struct Mixed {
            count: u32,
            tags: Vec<String>,
            pair: (u8, u8),
        }

        let schemas = extract_schemas(Mixed::SHAPE);
        assert!(schemas.len() >= 4);
    }

    // r[verify schema.principles.once-per-type]
    // r[verify schema.exchange.idempotent]
    #[test]
    fn tracker_prepare_send_returns_some_then_none() {
        let tracker = SchemaTracker::new();
        let first = tracker.prepare_send(<u32 as Facet>::SHAPE);
        assert!(first.is_some(), "first prepare_send should return Some");
        let second = tracker.prepare_send(<u32 as Facet>::SHAPE);
        assert!(
            second.is_none(),
            "second prepare_send for same shape should return None"
        );
    }

    // r[verify schema.tracking.transitive]
    // r[verify schema.tracking.sent]
    #[test]
    fn tracker_prepare_send_includes_transitive_deps() {
        #[derive(Facet)]
        struct Outer {
            inner: u32,
            name: String,
        }

        let tracker = SchemaTracker::new();
        let schemas = tracker
            .prepare_send(Outer::SHAPE)
            .expect("should return schemas");
        // Should include u32, String, and Outer
        assert!(
            schemas.len() >= 3,
            "should include transitive deps, got {}",
            schemas.len()
        );

        // Sending u32 again should return None since it was already sent as a transitive dep
        let u32_again = tracker.prepare_send(<u32 as Facet>::SHAPE);
        assert!(
            u32_again.is_none(),
            "u32 was already sent as transitive dep"
        );
    }

    // r[verify schema.tracking.received]
    #[test]
    fn tracker_record_and_has_received() {
        let tracker = SchemaTracker::new();
        let id = type_id_of(<u32 as Facet>::SHAPE);
        assert!(!tracker.has_received(&id));
        tracker.record_received(&[id]);
        assert!(tracker.has_received(&id));
    }

    // r[verify schema.principles.cbor]
    // r[verify schema.format.self-contained]
    // r[verify schema.format.batch]
    #[test]
    fn cbor_round_trip() {
        #[derive(Facet)]
        struct Point {
            x: f64,
            y: f64,
        }

        let schemas = extract_schemas(Point::SHAPE);
        let bytes = build_schema_message(&schemas);
        let parsed = parse_schema_message(&bytes).expect("should parse CBOR");
        assert_eq!(parsed.len(), schemas.len());
        for (original, decoded) in schemas.iter().zip(parsed.iter()) {
            assert_eq!(original.type_id, decoded.type_id);
        }
    }
}
