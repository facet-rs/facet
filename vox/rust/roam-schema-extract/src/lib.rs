#![deny(unsafe_code)]

use facet_core::{Def, ScalarType, Shape, StructKind, Type, UserType};
use roam_schema::{
    FieldSchema, PrimitiveType, Schema, SchemaKind, TypeId, VariantPayload, VariantSchema,
    type_id_of,
};
use roam_types::{is_rx, is_tx};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Tracks schema exchange state for one session.
///
/// Handles both outbound dedup (what we've sent) and inbound storage
/// (schemas received from the remote peer, used for building translation plans).
// r[impl schema.tracking.sent]
// r[impl schema.tracking.received]
pub struct SchemaTracker {
    sent: Mutex<HashSet<TypeId>>,
    received: Mutex<HashMap<TypeId, Schema>>,
}

impl SchemaTracker {
    pub fn new() -> Self {
        SchemaTracker {
            sent: Mutex::new(HashSet::new()),
            received: Mutex::new(HashMap::new()),
        }
    }

    /// Given a Shape, compute all schemas needed and return the ones
    /// not yet sent. Marks them as sent atomically. Returns None if
    /// all schemas were already sent.
    // r[impl schema.tracking.transitive]
    // r[impl schema.exchange.idempotent]
    // r[impl schema.principles.once-per-type]
    // r[impl schema.principles.sender-driven]
    // r[impl schema.principles.no-roundtrips]
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

    /// Record schemas received from the remote peer.
    pub fn record_received(&self, schemas: Vec<Schema>) {
        let mut received = self.received.lock().unwrap();
        for schema in schemas {
            received.insert(schema.type_id, schema);
        }
    }

    /// Look up a received schema by type ID.
    pub fn get_received(&self, type_id: &TypeId) -> Option<Schema> {
        self.received.lock().unwrap().get(type_id).cloned()
    }

    /// Get a snapshot of the received schema registry for building translation plans.
    pub fn received_registry(&self) -> roam_schema::SchemaRegistry {
        self.received.lock().unwrap().clone()
    }
}

impl Default for SchemaTracker {
    fn default() -> Self {
        Self::new()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    #[allow(unused_imports)]
    use roam_schema::*;

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
        assert!(schemas.len() >= 2);

        let point_schema = schemas.last().unwrap();
        match &point_schema.kind {
            SchemaKind::Struct { fields } => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].name, "x");
                assert_eq!(fields[1].name, "y");
                assert!(fields[0].required);
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
        #[allow(dead_code)]
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
        assert!(schemas.len() >= 2);

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
        assert_eq!(schemas.len(), 2);
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
        assert!(
            schemas.len() >= 3,
            "should include transitive deps, got {}",
            schemas.len()
        );

        let u32_again = tracker.prepare_send(<u32 as Facet>::SHAPE);
        assert!(
            u32_again.is_none(),
            "u32 was already sent as transitive dep"
        );
    }

    // r[verify schema.tracking.received]
    #[test]
    fn tracker_record_and_get_received() {
        let tracker = SchemaTracker::new();
        let schemas = extract_schemas(<u32 as Facet>::SHAPE);
        let id = schemas[0].type_id;
        assert!(tracker.get_received(&id).is_none());
        tracker.record_received(schemas);
        assert!(tracker.get_received(&id).is_some());
    }
}
